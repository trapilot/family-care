use rusqlite::Connection;
use rust_decimal::Decimal;

use crate::error::{AppError, Result};
use crate::models::{
    decimal_to_f64, Currency,
    transaction::{NewTransaction, Transaction, TransactionType},
};
use crate::repository::{HoldingRepo, TransactionRepo, TxnImageRepo, WalletRepo};
use crate::services::HoldingService;

pub struct TransactionService;

impl TransactionService {
    /// Single entry point for all transaction types.
    /// Wraps everything in a SQLite transaction — either all succeeds or nothing changes.
    pub fn execute(conn: &Connection, input: &NewTransaction) -> Result<Transaction> {
        Self::execute_with_photos(conn, None, input, &[])
    }

    /// Replace transaction `old_id` with `input` (an edit): delete + insert in ONE SQL transaction,
    /// so a failing insert leaves the original untouched.
    pub fn replace(conn: &Connection, old_id: &str, input: &NewTransaction) -> Result<Transaction> {
        Self::execute_with_photos(conn, Some(old_id), input, &[])
    }

    /// Save a transaction with its photos atomically. With `old_id`, the old transaction (and its
    /// photos) is reversed and removed first; everything commits or rolls back together.
    pub fn execute_with_photos(
        conn: &Connection,
        old_id: Option<&str>,
        input: &NewTransaction,
        photos: &[Vec<u8>],
    ) -> Result<Transaction> {
        Self::execute_atomic(conn, old_id, input, None, photos)
    }

    /// Buy by typed code: the (wallet, code) holding is found or created inside the same SQL transaction,
    /// so a failed save leaves no empty holding behind. `input.holding_id` is ignored.
    pub fn execute_buy_by_symbol(
        conn: &Connection,
        old_id: Option<&str>,
        input: &NewTransaction,
        symbol: &str,
        photos: &[Vec<u8>],
    ) -> Result<Transaction> {
        let symbol = symbol.trim().to_uppercase();
        if symbol.is_empty() {
            return Err(AppError::Validation("Code is required".into()));
        }
        if input.txn_type != TransactionType::InvestBuy {
            return Err(AppError::Validation("Only a buy can be saved by code".into()));
        }
        Self::execute_atomic(conn, old_id, input, Some(&symbol), photos)
    }

    fn execute_atomic(
        conn: &Connection,
        old_id: Option<&str>,
        input: &NewTransaction,
        buy_symbol: Option<&str>,
        photos: &[Vec<u8>],
    ) -> Result<Transaction> {
        let old = match old_id {
            Some(id) => Some(
                TransactionRepo::find_by_id(conn, id)?
                    .ok_or_else(|| AppError::NotFound(format!("transaction '{id}'")))?,
            ),
            None => None,
        };

        conn.execute("BEGIN IMMEDIATE", [])?;

        let result = (|| -> Result<Transaction> {
            // Resolve the holding first (inside the SQL transaction, so a rollback removes it too)
            let resolved;
            let input = match buy_symbol {
                Some(sym) => {
                    let mut i = input.clone();
                    // A code with '-' (e.g. BTC-USD) is a crypto pair; anything else is a stock
                    let asset_type = if sym.contains('-') {
                        crate::models::AssetType::Crypto
                    } else {
                        crate::models::AssetType::Stock
                    };
                    i.holding_id = Some(HoldingService::get_or_create(
                        conn, &i.wallet_id, sym, None, asset_type,
                    )?);
                    resolved = i;
                    &resolved
                }
                None => input,
            };
            // Read-only checks; an error rolls everything back
            Self::validate(conn, input)?;
            if let Some(ref old) = old {
                Self::reverse_inner(conn, old)?;
            }
            let saved = Self::execute_inner(conn, input)?;
            for jpeg in photos {
                TxnImageRepo::add(conn, &saved.id, jpeg)?;
            }
            // Re-applying an edited buy must not leave the old holding short of shares
            if let Some(ref old) = old {
                if matches!(old.txn_type, TransactionType::InvestBuy | TransactionType::InvestSell) {
                    if let Some(ref hid) = old.holding_id {
                        Self::ensure_not_negative(conn, hid)?;
                    }
                }
            }
            Ok(saved)
        })();

        match result {
            Ok(txn) => {
                conn.execute("COMMIT", [])?;
                Ok(txn)
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                Err(e)
            }
        }
    }

    fn execute_inner(conn: &Connection, input: &NewTransaction) -> Result<Transaction> {
        match input.txn_type {
            TransactionType::Income | TransactionType::OpeningBalance => Self::process_income(conn, input),
            TransactionType::Expense   => Self::process_expense(conn, input),
            TransactionType::Transfer  => Self::process_transfer(conn, input),
            TransactionType::InvestBuy => Self::process_invest_buy(conn, input),
            TransactionType::InvestSell => Self::process_invest_sell(conn, input),
        }
    }

    /// The wallet's opening-balance records (earliest first), every money type
    fn opening_balances_sorted(conn: &Connection, wallet_id: &str) -> Result<Vec<Transaction>> {
        let mut found = TransactionRepo::list(conn, &crate::repository::transaction_repo::TxnFilter {
            wallet_id: Some(wallet_id.to_string()),
            txn_type:  Some(TransactionType::OpeningBalance),
            ..Default::default()
        })?;
        found.sort_by_key(|t| (t.txn_date, t.created_at));
        Ok(found)
    }

    /// The wallet's opening-balance record in one money type (the earliest one), if any
    pub fn find_opening_balance(conn: &Connection, wallet_id: &str, currency: &Currency) -> Result<Option<Transaction>> {
        Ok(Self::opening_balances_sorted(conn, wallet_id)?.into_iter().find(|t| &t.currency == currency))
    }

    /// The wallet's opening-balance records: the earliest one per money type
    pub fn find_opening_balances(conn: &Connection, wallet_id: &str) -> Result<Vec<Transaction>> {
        let mut seen: Vec<Currency> = vec![];
        let mut result = vec![];
        for t in Self::opening_balances_sorted(conn, wallet_id)? {
            if !seen.contains(&t.currency) {
                seen.push(t.currency.clone());
                result.push(t);
            }
        }
        Ok(result)
    }

    /// Set the wallet's opening balance in one money type (0 allowed): replaces the existing
    /// record, keeping its date and note, so the wallet balance moves by the difference;
    /// creates one if missing (but no empty record for a type the wallet never had).
    pub fn set_opening_balance(
        conn: &Connection,
        wallet_id: &str,
        amount: Decimal,
        currency: Currency,
        default_note: &str,
    ) -> Result<()> {
        let existing = Self::find_opening_balance(conn, wallet_id, &currency)?;
        if existing.is_none() && amount.is_zero() {
            return Ok(());
        }
        if existing.as_ref().is_some_and(|t| t.amount == amount) {
            return Ok(());
        }
        let (date, note) = match &existing {
            Some(old) => (old.txn_date, old.note.clone()),
            None      => (crate::views::now_ts(), Some(default_note.to_string())),
        };
        let mut opening = NewTransaction::opening_balance(wallet_id, amount, currency, date);
        opening.note = note;
        Self::validate(conn, &opening)?;
        if let Some(old) = existing {
            Self::delete(conn, &old.id)?;
        }
        Self::execute(conn, &opening)?;
        Ok(())
    }

    // ─── Income ───────────────────────────────────────────────────────────────

    fn process_income(conn: &Connection, input: &NewTransaction) -> Result<Transaction> {
        WalletRepo::add_balance(conn, &input.wallet_id, &input.currency, decimal_to_f64(input.amount))?;
        TransactionRepo::insert(conn, input)
    }

    // ─── Expense ─────────────────────────────────────────────────────────────

    fn process_expense(conn: &Connection, input: &NewTransaction) -> Result<Transaction> {
        Self::check_balance(conn, &input.wallet_id, &input.currency, input.amount)?;
        WalletRepo::add_balance(conn, &input.wallet_id, &input.currency, -decimal_to_f64(input.amount))?;
        TransactionRepo::insert(conn, input)
    }

    // ─── Transfer ────────────────────────────────────────────────────────────

    fn process_transfer(conn: &Connection, input: &NewTransaction) -> Result<Transaction> {
        let to_wallet_id = input.to_wallet_id.as_ref()
            .ok_or_else(|| AppError::Validation("Transfer requires to_wallet_id".into()))?;
        let to_amount = input.to_amount
            .ok_or_else(|| AppError::Validation("Transfer requires to_amount".into()))?;

        let to_currency = input.to_currency.as_ref()
            .ok_or_else(|| AppError::Validation("Transfer requires to_currency".into()))?;

        Self::check_balance(conn, &input.wallet_id, &input.currency, input.amount)?;
        WalletRepo::add_balance(conn, &input.wallet_id, &input.currency, -decimal_to_f64(input.amount))?;
        WalletRepo::add_balance(conn, to_wallet_id, to_currency, decimal_to_f64(to_amount))?;
        TransactionRepo::insert(conn, input)
    }

    // ─── Invest Buy ──────────────────────────────────────────────────────────

    fn process_invest_buy(conn: &Connection, input: &NewTransaction) -> Result<Transaction> {
        let holding_id = input.holding_id.as_ref()
            .ok_or_else(|| AppError::Validation("invest_buy requires holding_id".into()))?;
        let qty = input.asset_quantity
            .ok_or_else(|| AppError::Validation("invest_buy requires asset_quantity".into()))?;
        let price = input.asset_price
            .ok_or_else(|| AppError::Validation("invest_buy requires asset_price".into()))?;

        let holding = HoldingRepo::find_by_id(conn, holding_id)?
            .ok_or_else(|| AppError::NotFound(format!("holding '{holding_id}'")))?;
        Self::check_whole_shares(&holding, qty)?;

        // Deduct cost from wallet
        let total_cost = qty * price;
        Self::check_balance(conn, &input.wallet_id, &input.currency, total_cost)?;
        WalletRepo::add_balance(conn, &input.wallet_id, &input.currency, -decimal_to_f64(total_cost))?;

        // Update holding: quantity += qty, recalculate avg price
        HoldingService::apply_buy(conn, holding_id, qty, price)?;

        TransactionRepo::insert(conn, input)
    }

    // ─── Invest Sell ─────────────────────────────────────────────────────────

    fn process_invest_sell(conn: &Connection, input: &NewTransaction) -> Result<Transaction> {
        let holding_id = input.holding_id.as_ref()
            .ok_or_else(|| AppError::Validation("invest_sell requires holding_id".into()))?;
        let qty = input.asset_quantity
            .ok_or_else(|| AppError::Validation("invest_sell requires asset_quantity".into()))?;
        let price = input.asset_price
            .ok_or_else(|| AppError::Validation("invest_sell requires asset_price".into()))?;

        // Check sufficient holding quantity
        let holding = HoldingRepo::find_by_id(conn, holding_id)?
            .ok_or_else(|| AppError::NotFound(format!("holding '{holding_id}'")))?;
        Self::check_whole_shares(&holding, qty)?;
        let current_qty = HoldingRepo::get_quantity(conn, holding_id)?;
        if qty > current_qty {
            return Err(AppError::Validation(format!(
                "Not enough quantity: you hold {} {}", current_qty.normalize(), holding.symbol
            )));
        }

        // Reduce holding quantity
        HoldingService::apply_sell(conn, holding_id, qty)?;

        // Add proceeds to wallet
        let total_proceeds = qty * price;
        WalletRepo::add_balance(conn, &input.wallet_id, &input.currency, decimal_to_f64(total_proceeds))?;

        TransactionRepo::insert(conn, input)
    }

    // ─── Undo / Delete ───────────────────────────────────────────────────────

    /// Reverse a transaction — restores all balances/positions then deletes the record.
    pub fn delete(conn: &Connection, txn_id: &str) -> Result<()> {
        let txn = TransactionRepo::find_by_id(conn, txn_id)?
            .ok_or_else(|| AppError::NotFound(format!("transaction '{txn_id}'")))?;

        conn.execute("BEGIN IMMEDIATE", [])?;

        // `reverse_inner` is the delete body that assumes an open SQL transaction
        let result = Self::reverse_inner(conn, &txn).and_then(|_| match (&txn.txn_type, &txn.holding_id) {
            (TransactionType::InvestBuy | TransactionType::InvestSell, Some(hid)) => Self::ensure_not_negative(conn, hid),
            _ => Ok(()),
        });
        match result {
            Ok(_) => {
                conn.execute("COMMIT", [])?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                Err(e)
            }
        }
    }

    fn reverse_inner(conn: &Connection, txn: &Transaction) -> Result<()> {
        match txn.txn_type {
            TransactionType::Income | TransactionType::OpeningBalance => {
                WalletRepo::add_balance(conn, &txn.wallet_id, &txn.currency, -txn.amount_f64())?;
            }
            TransactionType::Expense => {
                WalletRepo::add_balance(conn, &txn.wallet_id, &txn.currency, txn.amount_f64())?;
            }
            TransactionType::Transfer => {
                let to_id = txn.to_wallet_id.as_ref()
                    .ok_or_else(|| AppError::Validation("Corrupt transfer: missing to_wallet_id".into()))?;
                let to_amount = txn.to_amount
                    .ok_or_else(|| AppError::Validation("Corrupt transfer: missing to_amount".into()))?;
                // Old rows may lack to_currency: they were same-currency transfers
                let to_currency = txn.to_currency.clone().unwrap_or(txn.currency.clone());
                WalletRepo::add_balance(conn, &txn.wallet_id, &txn.currency, txn.amount_f64())?;
                WalletRepo::add_balance(conn, to_id, &to_currency, -decimal_to_f64(to_amount))?;
            }
            TransactionType::InvestBuy => {
                let holding_id = txn.holding_id.as_ref()
                    .ok_or_else(|| AppError::Validation("Corrupt invest_buy: missing holding_id".into()))?;
                let qty = txn.asset_quantity
                    .ok_or_else(|| AppError::Validation("Corrupt invest_buy: missing asset_quantity".into()))?;
                let total = txn.amount;
                WalletRepo::add_balance(conn, &txn.wallet_id, &txn.currency, decimal_to_f64(total))?;
                // Reduce holding quantity; avg price recalculation is approximate on reversal
                let current_qty = HoldingRepo::get_quantity(conn, holding_id)?;
                let new_qty = current_qty - qty;
                let avg = HoldingRepo::get_avg_price(conn, holding_id)?;
                HoldingRepo::update_position(conn, holding_id, new_qty, avg)?;
            }
            TransactionType::InvestSell => {
                let holding_id = txn.holding_id.as_ref()
                    .ok_or_else(|| AppError::Validation("Corrupt invest_sell: missing holding_id".into()))?;
                let qty = txn.asset_quantity
                    .ok_or_else(|| AppError::Validation("Corrupt invest_sell: missing asset_quantity".into()))?;
                let price = txn.asset_price
                    .ok_or_else(|| AppError::Validation("Corrupt invest_sell: missing asset_price".into()))?;
                let total = txn.amount;
                WalletRepo::add_balance(conn, &txn.wallet_id, &txn.currency, -decimal_to_f64(total))?;
                // Restore holding quantity (avg price unchanged — best approximation)
                let current_qty = HoldingRepo::get_quantity(conn, holding_id)?;
                let avg = HoldingRepo::get_avg_price(conn, holding_id)?;
                // On reversal we also need to rebuild avg — using the old sell price as a proxy
                let new_qty = current_qty + qty;
                let new_avg = if new_qty > Decimal::ZERO {
                    (avg * current_qty + price * qty) / new_qty
                } else {
                    avg
                };
                HoldingRepo::update_position(conn, holding_id, new_qty, new_avg)?;
            }
        }

        TransactionRepo::delete(conn, &txn.id)?;
        Ok(())
    }

    /// Stocks trade in whole shares only; crypto may be fractional.
    fn check_whole_shares(holding: &crate::models::Holding, qty: Decimal) -> Result<()> {
        if holding.asset_type == crate::models::AssetType::Stock && qty.fract() != Decimal::ZERO {
            return Err(AppError::Validation("Stock quantity must be a whole number".into()));
        }
        Ok(())
    }

    /// Fails (so the surrounding SQL transaction rolls back) when a holding's quantity went below zero.
    fn ensure_not_negative(conn: &Connection, holding_id: &str) -> Result<()> {
        if HoldingRepo::get_quantity(conn, holding_id)? < Decimal::ZERO {
            return Err(AppError::Validation(
                "Some of these shares were already sold — edit or delete the sell first".into(),
            ));
        }
        Ok(())
    }

    // ─── Validation ──────────────────────────────────────────────────────────

    fn validate(conn: &Connection, input: &NewTransaction) -> Result<()> {
        // A wallet may start empty, so only its opening balance can be 0
        let zero_ok = input.txn_type == TransactionType::OpeningBalance;
        if input.amount < Decimal::ZERO || (input.amount.is_zero() && !zero_ok) {
            return Err(AppError::Validation("Amount must be greater than zero".into()));
        }

        // An empty id would violate the foreign keys: refuse it with a clear message
        let blank = |id: &Option<String>| id.as_deref().map_or(true, |v| v.trim().is_empty());
        if input.wallet_id.trim().is_empty() {
            return Err(AppError::Validation("wallet_id is required".into()));
        }
        match input.txn_type {
            TransactionType::Expense if blank(&input.category_id) =>
                return Err(AppError::Validation("expense requires category_id".into())),
            TransactionType::Transfer if blank(&input.to_wallet_id) =>
                return Err(AppError::Validation("transfer requires to_wallet_id".into())),
            TransactionType::InvestBuy | TransactionType::InvestSell if blank(&input.holding_id) =>
                return Err(AppError::Validation("invest transaction requires holding_id".into())),
            _ => {}
        }

        // Wallet must exist; it can hold any money type
        let wallet = WalletRepo::find_by_id(conn, &input.wallet_id)?
            .ok_or_else(|| AppError::NotFound(format!("wallet '{}'", input.wallet_id)))?;
        if !wallet.is_active {
            return Err(AppError::Validation(format!("Wallet '{}' is disabled", wallet.name)));
        }

        match input.txn_type {
            TransactionType::Income => {
                if input.income_type.is_none() {
                    return Err(AppError::Validation("income requires income_type".into()));
                }
            }
            TransactionType::Expense => {
                if input.category_id.is_none() {
                    return Err(AppError::Validation("expense requires category_id".into()));
                }
            }
            TransactionType::Transfer => {
                let to_id = input.to_wallet_id.as_ref()
                    .ok_or_else(|| AppError::Validation("transfer requires to_wallet_id".into()))?;
                let to_currency = input.to_currency.as_ref()
                    .ok_or_else(|| AppError::Validation("transfer requires to_currency".into()))?;
                // Inside one wallet only a currency exchange makes sense
                if to_id == &input.wallet_id && *to_currency == input.currency {
                    return Err(AppError::Validation("Cannot transfer to the same wallet".into()));
                }
                if input.to_amount.is_none() {
                    return Err(AppError::Validation("transfer requires to_amount".into()));
                }
                let to_wallet = WalletRepo::find_by_id(conn, to_id)?
                    .ok_or_else(|| AppError::NotFound(format!("wallet '{to_id}'")))?;
                if !to_wallet.is_active {
                    return Err(AppError::Validation(format!("Wallet '{}' is disabled", to_wallet.name)));
                }
            }
            TransactionType::OpeningBalance => {}
            TransactionType::InvestBuy | TransactionType::InvestSell => {
                if input.holding_id.is_none() {
                    return Err(AppError::Validation("invest transaction requires holding_id".into()));
                }
                if input.asset_quantity.map_or(true, |q| q <= Decimal::ZERO) {
                    return Err(AppError::Validation("asset_quantity must be > 0".into()));
                }
                if input.asset_price.map_or(true, |p| p <= Decimal::ZERO) {
                    return Err(AppError::Validation("asset_price must be > 0".into()));
                }
            }
        }

        Ok(())
    }

    // ─── Balance check ────────────────────────────────────────────────────────

    fn check_balance(conn: &Connection, wallet_id: &str, currency: &Currency, amount: Decimal) -> Result<()> {
        // Credit cards spend on credit: their balance is allowed to go negative (amount owed)
        let is_credit_card = WalletRepo::find_by_id(conn, wallet_id)?
            .is_some_and(|w| w.wallet_type == crate::models::WalletType::CreditCard);
        if !is_credit_card && amount > WalletRepo::get_balance(conn, wallet_id, currency)? {
            return Err(AppError::InsufficientBalance(wallet_id.to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NewWallet, WalletType};
    use crate::repository::CategoryRepo;

    #[test]
    fn credit_card_can_spend_below_zero_but_cash_cannot() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        CategoryRepo::seed_defaults(&conn).unwrap();
        let category = CategoryRepo::list(&conn).unwrap()[0].id.clone();
        let new = |name: &str, wallet_type| WalletRepo::create(&conn, &NewWallet {
            name: name.into(), wallet_type, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        let card = new("VISA", WalletType::CreditCard);
        let cash = new("Cash", WalletType::Cash);
        let spend = |id: &str| TransactionService::execute(&conn, &NewTransaction::expense(id, Decimal::from(500_000), Currency::VND, &category, 1_000));

        spend(&card.id).unwrap();
        assert_eq!(WalletRepo::get_balance(&conn, &card.id, &Currency::VND).unwrap(), Decimal::from(-500_000));
        assert!(spend(&cash.id).is_err());
    }

    #[test]
    fn opening_balance_can_be_zero_and_edited_later() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        CategoryRepo::seed_defaults(&conn).unwrap();
        let wallet = WalletRepo::create(&conn, &NewWallet {
            name: "Cash".into(), wallet_type: WalletType::Cash, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        let balance = || WalletRepo::find_by_id(&conn, &wallet.id).unwrap().unwrap().balance(&Currency::VND);

        // Created with 0: the record still exists
        let mut opening = NewTransaction::opening_balance(&wallet.id, Decimal::ZERO, Currency::VND, 1_000);
        opening.note = Some("Opening Balance".into());
        TransactionService::execute(&conn, &opening).unwrap();
        let first = TransactionService::find_opening_balance(&conn, &wallet.id, &Currency::VND).unwrap().unwrap();
        assert_eq!((first.amount, balance()), (Decimal::ZERO, Decimal::ZERO));

        // Raised later; date and note kept, still one record
        TransactionService::set_opening_balance(&conn, &wallet.id, Decimal::from(5_000_000), Currency::VND, "x").unwrap();
        assert_eq!(balance(), Decimal::from(5_000_000));

        // Spending is kept when the opening balance changes again
        let category = CategoryRepo::list(&conn).unwrap()[0].id.clone();
        TransactionService::execute(&conn, &NewTransaction::expense(&wallet.id, Decimal::from(1_000_000), Currency::VND, &category, 2_000)).unwrap();
        TransactionService::set_opening_balance(&conn, &wallet.id, Decimal::from(3_000_000), Currency::VND, "x").unwrap();
        assert_eq!(balance(), Decimal::from(2_000_000));

        let all = TransactionRepo::list(&conn, &crate::repository::transaction_repo::TxnFilter {
            txn_type: Some(TransactionType::OpeningBalance), ..Default::default()
        }).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!((all[0].txn_date, all[0].note.as_deref()), (1_000, Some("Opening Balance")));

        // Same value again: nothing changes
        TransactionService::set_opening_balance(&conn, &wallet.id, Decimal::from(3_000_000), Currency::VND, "x").unwrap();
        assert_eq!(TransactionService::find_opening_balance(&conn, &wallet.id, &Currency::VND).unwrap().unwrap().id, all[0].id);

        // Other types still need more than 0
        let zero_expense = NewTransaction::expense(&wallet.id, Decimal::ZERO, Currency::VND, &category, 3_000);
        assert!(TransactionService::execute(&conn, &zero_expense).is_err());

        // A wallet holds any money type: GOLD lands in its own balance, VND is untouched
        let before = balance();
        let gold = NewTransaction::income(&wallet.id, Decimal::from(17), Currency::GOLD, crate::models::IncomeType::Other, 4_000);
        TransactionService::execute(&conn, &gold).unwrap();
        let w = WalletRepo::find_by_id(&conn, &wallet.id).unwrap().unwrap();
        assert_eq!((w.balance(&Currency::VND), w.balance(&Currency::GOLD)), (before, Decimal::from(17)));

        // Spending more GOLD than held is refused
        let too_much = NewTransaction::expense(&wallet.id, Decimal::from(20), Currency::GOLD, &category, 5_000);
        let err = TransactionService::execute(&conn, &too_much).unwrap_err().to_string();
        assert!(err.contains("Insufficient balance"), "{err}");

        // No empty opening record for a type the wallet never had
        TransactionService::set_opening_balance(&conn, &wallet.id, Decimal::ZERO, Currency::USD, "x").unwrap();
        assert!(TransactionService::find_opening_balance(&conn, &wallet.id, &Currency::USD).unwrap().is_none());

        // Same-wallet exchange VND -> GOLD works, VND -> VND is refused
        let exchange = NewTransaction::transfer(&wallet.id, Decimal::from(1_000), Currency::VND, &wallet.id, Decimal::from(1), Currency::GOLD, 6_000);
        TransactionService::execute(&conn, &exchange).unwrap();
        let w = WalletRepo::find_by_id(&conn, &wallet.id).unwrap().unwrap();
        assert_eq!((w.balance(&Currency::VND), w.balance(&Currency::GOLD)), (before - Decimal::from(1_000), Decimal::from(18)));
        let same = NewTransaction::transfer(&wallet.id, Decimal::from(1_000), Currency::VND, &wallet.id, Decimal::from(1_000), Currency::VND, 7_000);
        assert!(TransactionService::execute(&conn, &same).is_err());
    }

    #[test]
    fn failed_edit_keeps_the_original_transaction_balance_and_photo() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        CategoryRepo::seed_defaults(&conn).unwrap();
        let category = CategoryRepo::list(&conn).unwrap()[0].id.clone();
        let wallet = WalletRepo::create(&conn, &NewWallet {
            name: "Cash".into(), wallet_type: WalletType::Cash, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        let balance = || WalletRepo::get_balance(&conn, &wallet.id, &Currency::VND).unwrap();
        TransactionService::execute(&conn, &NewTransaction::opening_balance(&wallet.id, Decimal::from(1_000), Currency::VND, 1)).unwrap();
        let original = TransactionService::execute_with_photos(
            &conn, None,
            &NewTransaction::expense(&wallet.id, Decimal::from(300), Currency::VND, &category, 2),
            &[vec![1, 2, 3]],
        ).unwrap();
        assert_eq!(balance(), Decimal::from(700));

        // Empty category: refused, nothing changes
        let bad = NewTransaction::expense(&wallet.id, Decimal::from(100), Currency::VND, "", 3);
        assert!(TransactionService::replace(&conn, &original.id, &bad).is_err());
        // Passes validation but fails inside the SQL transaction (more than the balance): rolled back
        let too_much = NewTransaction::expense(&wallet.id, Decimal::from(5_000), Currency::VND, &category, 3);
        assert!(TransactionService::replace(&conn, &original.id, &too_much).is_err());
        // Unknown category: FK failure after the old one was reversed: rolled back
        let fk = NewTransaction::expense(&wallet.id, Decimal::from(100), Currency::VND, "nope", 3);
        assert!(TransactionService::replace(&conn, &original.id, &fk).is_err());

        assert!(TransactionRepo::find_by_id(&conn, &original.id).unwrap().is_some());
        assert_eq!(balance(), Decimal::from(700));
        assert_eq!(crate::repository::TxnImageRepo::list(&conn, &original.id).unwrap().len(), 1);

        // A valid edit still works
        let ok = NewTransaction::expense(&wallet.id, Decimal::from(100), Currency::VND, &category, 3);
        let saved = TransactionService::execute_with_photos(&conn, Some(&original.id), &ok, &[vec![9]]).unwrap();
        assert_eq!(balance(), Decimal::from(900));
        assert!(TransactionRepo::find_by_id(&conn, &original.id).unwrap().is_none());
        assert_eq!(crate::repository::TxnImageRepo::list(&conn, &saved.id).unwrap().len(), 1);
    }

    fn broker_conn(wallet_type: WalletType) -> (Connection, String) {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        CategoryRepo::seed_defaults(&conn).unwrap();
        let wallet = WalletRepo::create(&conn, &NewWallet {
            name: "Broker".into(), wallet_type, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        TransactionService::execute(&conn, &NewTransaction::opening_balance(&wallet.id, Decimal::from(1_000_000), Currency::VND, 1_000)).unwrap();
        (conn, wallet.id)
    }

    #[test]
    fn buy_by_symbol_creates_then_reuses_the_holding() {
        let (conn, wid) = broker_conn(WalletType::Investment);
        let buy = |sym: &str, qty: i64, price: i64| TransactionService::execute_buy_by_symbol(
            &conn, None,
            &NewTransaction::invest_buy(&wid, "", Decimal::from(qty), Decimal::from(price), Currency::VND, 2_000),
            sym, &[],
        ).unwrap();

        buy(" vnm ", 10, 1000);
        let first = HoldingRepo::find_by_wallet_and_symbol(&conn, &wid, "VNM").unwrap().unwrap();
        assert_eq!(first.quantity, Decimal::from(10));

        buy("VNM", 5, 2000);
        let second = HoldingRepo::find_by_wallet_and_symbol(&conn, &wid, "VNM").unwrap().unwrap();
        assert_eq!((second.id, second.quantity), (first.id, Decimal::from(15)));
        let balance = WalletRepo::get_balance(&conn, &wid, &Currency::VND).unwrap();
        assert_eq!(balance, Decimal::from(1_000_000 - 20_000));
    }

    #[test]
    fn failed_buy_by_symbol_leaves_no_holding() {
        let (conn, wid) = broker_conn(WalletType::Investment);
        let too_big = NewTransaction::invest_buy(&wid, "", Decimal::from(1_000), Decimal::from(5_000), Currency::VND, 2_000);
        assert!(TransactionService::execute_buy_by_symbol(&conn, None, &too_big, "FPT", &[]).is_err());
        assert!(HoldingRepo::find_by_wallet_and_symbol(&conn, &wid, "FPT").unwrap().is_none());
    }

    #[test]
    fn buy_by_symbol_rejects_blank_code_and_non_investment_wallet() {
        let (conn, wid) = broker_conn(WalletType::Investment);
        let buy = |w: &str| NewTransaction::invest_buy(w, "", Decimal::from(1), Decimal::from(1_000), Currency::VND, 2_000);
        assert!(TransactionService::execute_buy_by_symbol(&conn, None, &buy(&wid), "  ", &[]).is_err());

        let (conn, cash) = broker_conn(WalletType::Cash);
        assert!(TransactionService::execute_buy_by_symbol(&conn, None, &buy(&cash), "VNM", &[]).is_err());
    }

    #[test]
    fn buy_by_symbol_with_dash_creates_crypto_holding() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        CategoryRepo::seed_defaults(&conn).unwrap();
        let wallet = WalletRepo::create(&conn, &NewWallet {
            name: "Invest".into(), wallet_type: WalletType::Investment, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        let open = NewTransaction::opening_balance(&wallet.id, Decimal::from(100_000), Currency::USD, 1_000);
        TransactionService::execute(&conn, &open).unwrap();
        let buy = NewTransaction::invest_buy(&wallet.id, "", Decimal::from(1), Decimal::from(100), Currency::USD, 2_000);
        let txn = TransactionService::execute_buy_by_symbol(&conn, None, &buy, "btc-usd", &[]).unwrap();
        let holding = crate::repository::HoldingRepo::find_by_id(&conn, txn.holding_id.as_deref().unwrap()).unwrap().unwrap();
        assert_eq!(holding.symbol, "BTC-USD");
        assert_eq!(holding.asset_type, crate::models::AssetType::Crypto);
    }

    fn invest_conn() -> (Connection, String) {
        broker_conn(WalletType::Investment)
    }

    #[test]
    fn stock_quantity_must_be_whole_but_crypto_may_be_fractional() {
        let (conn, wid) = invest_conn();
        let buy = |qty: &str, price: i64| NewTransaction::invest_buy(
            &wid, "", qty.parse().unwrap(), Decimal::from(price), Currency::VND, 2_000,
        );
        assert!(TransactionService::execute_buy_by_symbol(&conn, None, &buy("1.5", 1000), "VNM.VN", &[]).is_err());
        assert!(TransactionService::execute_buy_by_symbol(&conn, None, &buy("0.5", 1000), "BTC-USD", &[]).is_ok());
    }

    #[test]
    fn selling_more_than_held_is_refused() {
        let (conn, wid) = invest_conn();
        let buy = NewTransaction::invest_buy(&wid, "", Decimal::from(3), Decimal::from(1000), Currency::VND, 2_000);
        let txn = TransactionService::execute_buy_by_symbol(&conn, None, &buy, "VNM", &[]).unwrap();
        let hid = txn.holding_id.unwrap();
        let sell = NewTransaction::invest_sell(&wid, &hid, Decimal::from(4), Decimal::from(1000), Currency::VND, 3_000);
        let err = TransactionService::execute(&conn, &sell).unwrap_err().to_string();
        assert!(err.contains("Not enough quantity"), "{err}");
    }

    #[test]
    fn editing_or_deleting_a_buy_cannot_leave_negative_shares() {
        let (conn, wid) = invest_conn();
        let buy = |qty: i64| NewTransaction::invest_buy(&wid, "", Decimal::from(qty), Decimal::from(1000), Currency::VND, 2_000);
        let bought = TransactionService::execute_buy_by_symbol(&conn, None, &buy(10), "VNM", &[]).unwrap();
        let hid = bought.holding_id.clone().unwrap();
        let sell = NewTransaction::invest_sell(&wid, &hid, Decimal::from(5), Decimal::from(1000), Currency::VND, 3_000);
        TransactionService::execute(&conn, &sell).unwrap();
        let held = || HoldingRepo::get_quantity(&conn, &hid).unwrap();
        assert_eq!(held(), Decimal::from(5));

        assert!(TransactionService::delete(&conn, &bought.id).is_err());
        assert_eq!(held(), Decimal::from(5));

        TransactionService::execute_buy_by_symbol(&conn, Some(&bought.id), &buy(12), "VNM", &[]).unwrap();
        assert_eq!(held(), Decimal::from(7));
        let edited = TransactionRepo::list(&conn, &crate::repository::transaction_repo::TxnFilter {
            txn_type: Some(TransactionType::InvestBuy), ..Default::default()
        }).unwrap().remove(0);

        assert!(TransactionService::execute_buy_by_symbol(&conn, Some(&edited.id), &buy(3), "VNM", &[]).is_err());
        assert_eq!(held(), Decimal::from(7));
    }

    #[test]
    fn disabled_wallet_accepts_no_transactions_until_enabled() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        CategoryRepo::seed_defaults(&conn).unwrap();
        let category = CategoryRepo::list(&conn).unwrap()[0].id.clone();
        let new = |name: &str| WalletRepo::create(&conn, &NewWallet {
            name: name.into(), wallet_type: WalletType::Cash, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        let (a, b) = (new("A"), new("B"));
        TransactionService::execute(&conn, &NewTransaction::opening_balance(&a.id, Decimal::from(1_000), Currency::VND, 1)).unwrap();
        TransactionService::execute(&conn, &NewTransaction::opening_balance(&b.id, Decimal::from(1_000), Currency::VND, 1)).unwrap();
        let spent = TransactionService::execute(&conn, &NewTransaction::expense(&a.id, Decimal::from(100), Currency::VND, &category, 2)).unwrap();

        WalletRepo::set_active(&conn, &a.id, false).unwrap();
        let expense = NewTransaction::expense(&a.id, Decimal::from(100), Currency::VND, &category, 3);
        let err = TransactionService::execute(&conn, &expense).unwrap_err().to_string();
        assert!(err.contains("disabled"), "{err}");
        assert!(TransactionService::delete(&conn, &spent.id).is_err());
        assert!(TransactionRepo::find_by_id(&conn, &spent.id).unwrap().is_some());
        let into_a = NewTransaction::transfer(&b.id, Decimal::from(50), Currency::VND, &a.id, Decimal::from(50), Currency::VND, 4);
        assert!(TransactionService::execute(&conn, &into_a).is_err());
        assert!(TransactionService::set_opening_balance(&conn, &a.id, Decimal::from(5), Currency::VND, "x").is_err());

        WalletRepo::set_active(&conn, &a.id, true).unwrap();
        TransactionService::execute(&conn, &expense).unwrap();
        TransactionService::execute(&conn, &into_a).unwrap();
        TransactionService::delete(&conn, &spent.id).unwrap();
    }
}
