use rusqlite::{Connection, OptionalExtension};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::str::FromStr;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::{
    f64_to_decimal,
    wallet::{to_sqlite_err, NewWallet, Wallet},
    Currency, WalletBalance,
};

pub struct WalletRepo;

const WALLET_COLUMNS: &str = "id, name, wallet_type, broker, icon, color, is_active, sort_order, created_at";

impl WalletRepo {
    pub fn create(conn: &Connection, input: &NewWallet) -> Result<Wallet> {
        let id = Uuid::new_v4().to_string();
        let now = now_ts();

        conn.execute(
            "INSERT INTO wallets
             (id, name, wallet_type, broker, icon, color, is_active, sort_order, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8)",
            rusqlite::params![
                id,
                input.name,
                input.wallet_type.to_string(),
                input.broker,
                input.icon,
                input.color,
                input.sort_order,
                now,
            ],
        )?;

        Self::find_by_id(conn, &id)?.ok_or_else(|| AppError::NotFound(id))
    }

    pub fn find_by_id(conn: &Connection, id: &str) -> Result<Option<Wallet>> {
        let mut stmt = conn.prepare(&format!("SELECT {WALLET_COLUMNS} FROM wallets WHERE id = ?1"))?;

        let result = stmt.query_row([id], Wallet::from_row);
        let mut wallet = match result {
            Ok(w)                                => w,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e)                               => return Err(e.into()),
        };

        let mut stmt = conn.prepare(
            "SELECT b.wallet_id, b.currency, b.balance
             FROM wallet_balances b JOIN currencies c ON c.code = b.currency
             WHERE b.wallet_id = ?1
             ORDER BY c.sort_order",
        )?;
        let rows = stmt.query_map([id], balance_from_row)?;
        wallet.balances = rows.collect::<rusqlite::Result<Vec<_>>>()?.into_iter().map(|(_, b)| b).collect();
        Ok(Some(wallet))
    }

    pub fn list_active(conn: &Connection) -> Result<Vec<Wallet>> {
        let mut stmt = conn.prepare(&format!(
            "SELECT {WALLET_COLUMNS}
             FROM wallets
             WHERE is_active = 1
             ORDER BY sort_order ASC, created_at ASC",
        ))?;
        let rows = stmt.query_map([], Wallet::from_row)?;
        let mut wallets = rows.collect::<rusqlite::Result<Vec<_>>>()?;

        // One query for every wallet's balances, grouped by wallet
        let mut stmt = conn.prepare(
            "SELECT b.wallet_id, b.currency, b.balance
             FROM wallet_balances b JOIN currencies c ON c.code = b.currency
             ORDER BY c.sort_order",
        )?;
        let rows = stmt.query_map([], balance_from_row)?;
        let mut by_wallet: HashMap<String, Vec<WalletBalance>> = HashMap::new();
        for (wallet_id, balance) in rows.collect::<rusqlite::Result<Vec<_>>>()? {
            by_wallet.entry(wallet_id).or_default().push(balance);
        }
        for w in &mut wallets {
            w.balances = by_wallet.remove(&w.id).unwrap_or_default();
        }
        Ok(wallets)
    }

    pub fn update(conn: &Connection, wallet: &Wallet) -> Result<()> {
        conn.execute(
            "UPDATE wallets
             SET name = ?1, wallet_type = ?2,
                 broker = ?3, icon = ?4, color = ?5, sort_order = ?6
             WHERE id = ?7",
            rusqlite::params![
                wallet.name,
                wallet.wallet_type.to_string(),
                wallet.broker,
                wallet.icon,
                wallet.color,
                wallet.sort_order,
                wallet.id,
            ],
        )?;
        Ok(())
    }

    /// Atomic balance delta in one money type — used exclusively by TransactionService
    pub fn add_balance(conn: &Connection, wallet_id: &str, currency: &Currency, delta_f64: f64) -> Result<()> {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM wallets WHERE id = ?1 AND is_active = 1",
            [wallet_id],
            |row| row.get(0),
        )?;
        if exists == 0 {
            return Err(AppError::NotFound(format!("wallet '{wallet_id}'")));
        }
        conn.execute(
            "INSERT INTO wallet_balances (wallet_id, currency, balance) VALUES (?1, ?2, ?3)
             ON CONFLICT(wallet_id, currency) DO UPDATE SET balance = balance + excluded.balance",
            rusqlite::params![wallet_id, currency.to_string(), delta_f64],
        )?;
        Ok(())
    }

    /// Balance of a wallet in one money type (0 when it never held it)
    pub fn get_balance(conn: &Connection, wallet_id: &str, currency: &Currency) -> Result<Decimal> {
        let balance: Option<f64> = conn
            .query_row(
                "SELECT balance FROM wallet_balances WHERE wallet_id = ?1 AND currency = ?2",
                rusqlite::params![wallet_id, currency.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(balance.map(f64_to_decimal).unwrap_or(Decimal::ZERO))
    }

    pub fn soft_delete(conn: &Connection, id: &str) -> Result<()> {
        conn.execute("UPDATE wallets SET is_active = 0 WHERE id = ?1", [id])?;
        Ok(())
    }

    /// A wallet can be deleted when nothing but its own opening balance uses it:
    /// no other transactions (from or to it), holdings or reminders.
    pub fn can_delete(conn: &Connection, id: &str) -> Result<bool> {
        let used: i64 = conn.query_row(
            "SELECT
               (SELECT COUNT(*) FROM transactions
                 WHERE (wallet_id = ?1 OR to_wallet_id = ?1) AND txn_type <> 'opening_balance')
             + (SELECT COUNT(*) FROM holdings  WHERE wallet_id = ?1)
             + (SELECT COUNT(*) FROM reminders WHERE wallet_id = ?1 OR to_wallet_id = ?1)",
            [id],
            |row| row.get(0),
        )?;
        Ok(used == 0)
    }

    /// Permanently delete an unused wallet (see `can_delete`) with its opening balance record
    pub fn delete(conn: &Connection, id: &str) -> Result<()> {
        if !Self::can_delete(conn, id)? {
            return Err(AppError::Validation("This wallet has transactions and can't be deleted".into()));
        }
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = conn.execute(
                "DELETE FROM transaction_images WHERE transaction_id IN
                   (SELECT id FROM transactions WHERE wallet_id = ?1 AND txn_type = 'opening_balance')", [id])
            .and_then(|_| conn.execute("DELETE FROM transactions WHERE wallet_id = ?1 AND txn_type = 'opening_balance'", [id]))
            .and_then(|_| conn.execute("DELETE FROM wallet_balances WHERE wallet_id = ?1", [id]))
            .and_then(|_| conn.execute("DELETE FROM wallets WHERE id = ?1", [id]));
        match result {
            Ok(_)  => { conn.execute_batch("COMMIT")?; Ok(()) }
            Err(e) => { let _ = conn.execute_batch("ROLLBACK"); Err(e.into()) }
        }
    }

    pub fn reorder(conn: &Connection, ordered_ids: &[String]) -> Result<()> {
        for (i, id) in ordered_ids.iter().enumerate() {
            conn.execute(
                "UPDATE wallets SET sort_order = ?1 WHERE id = ?2",
                rusqlite::params![i as i32, id],
            )?;
        }
        Ok(())
    }
}

/// (wallet_id, balance) from a `wallet_balances` row
fn balance_from_row(row: &rusqlite::Row) -> rusqlite::Result<(String, WalletBalance)> {
    let cur: String = row.get(1)?;
    let balance: f64 = row.get(2)?;
    Ok((row.get(0)?, WalletBalance {
        currency: Currency::from_str(&cur).map_err(to_sqlite_err)?,
        balance:  f64_to_decimal(balance),
    }))
}

fn now_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Currency, IncomeType, NewTransaction, WalletType};
    use crate::services::TransactionService;
    use rust_decimal::Decimal;

    #[test]
    fn only_wallets_with_just_an_opening_balance_can_be_deleted() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let wallet = |name: &str| {
            let w = WalletRepo::create(&conn, &NewWallet {
                name: name.into(), wallet_type: WalletType::Cash, broker: None, icon: None, color: None, sort_order: 0,
            }).unwrap();
            TransactionService::execute(&conn, &NewTransaction::opening_balance(&w.id, Decimal::from(500), Currency::VND, 1)).unwrap();
            w.id
        };
        let (unused, used) = (wallet("UNUSED"), wallet("USED"));
        TransactionService::execute(&conn, &NewTransaction::income(&used, Decimal::from(10), Currency::VND, IncomeType::Salary, 2)).unwrap();

        assert!(WalletRepo::can_delete(&conn, &unused).unwrap());
        assert!(!WalletRepo::can_delete(&conn, &used).unwrap());
        assert!(WalletRepo::delete(&conn, &used).is_err());

        WalletRepo::delete(&conn, &unused).unwrap();
        assert!(WalletRepo::find_by_id(&conn, &unused).unwrap().is_none());
        let left: i64 = conn.query_row("SELECT COUNT(*) FROM transactions WHERE wallet_id = ?1", [&unused], |r| r.get(0)).unwrap();
        assert_eq!(left, 0, "its opening balance record went with it");
    }
}
