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
        Self::validate(conn, input)?;

        let old = match old_id {
            Some(id) => Some(
                TransactionRepo::find_by_id(conn, id)?
                    .ok_or_else(|| AppError::NotFound(format!("transaction '{id}'")))?,
            ),
            None => None,
        };

        conn.execute("BEGIN IMMEDIATE", [])?;

        let result = (|| -> Result<Transaction> {
            if let Some(ref old) = old {
                Self::reverse_inner(conn, old)?;
            }
            let saved = Self::execute_inner(conn, input)?;
            for jpeg in photos {
                TxnImageRepo::add(conn, &saved.id, jpeg)?;
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
        let current_qty = HoldingRepo::get_quantity(conn, holding_id)?;
        if qty > current_qty {
            return Err(AppError::InsufficientQuantity(holding_id.clone()));
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
        let result = Self::reverse_inner(conn, &txn);
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
                let new_qty = (current_qty - qty).max(Decimal::ZERO);
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
        WalletRepo::find_by_id(conn, &input.wallet_id)?
            .ok_or_else(|| AppError::NotFound(format!("wallet '{}'", input.wallet_id)))?;

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
                WalletRepo::find_by_id(conn, to_id)?
                    .ok_or_else(|| AppError::NotFound(format!("wallet '{to_id}'")))?;
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
}
