use rusqlite::Connection;
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::Result;

/// Photos attached to transactions (JPEG bytes)
pub struct TxnImageRepo;

impl TxnImageRepo {
    pub fn add(conn: &Connection, transaction_id: &str, jpeg: &[u8]) -> Result<()> {
        conn.execute(
            "INSERT INTO transaction_images (id, transaction_id, jpeg, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![Uuid::new_v4().to_string(), transaction_id, jpeg, crate::views::now_ts()],
        )?;
        Ok(())
    }

    /// A transaction's photos, oldest first
    pub fn list(conn: &Connection, transaction_id: &str) -> Result<Vec<Vec<u8>>> {
        let mut stmt = conn.prepare(
            "SELECT jpeg FROM transaction_images WHERE transaction_id = ?1 ORDER BY created_at, rowid",
        )?;
        let rows = stmt.query_map([transaction_id], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// Number of photos per transaction (for the list), without loading them
    pub fn counts(conn: &Connection) -> Result<HashMap<String, i64>> {
        let mut stmt = conn.prepare("SELECT transaction_id, COUNT(*) FROM transaction_images GROUP BY transaction_id")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<rusqlite::Result<HashMap<_, _>>>().map_err(Into::into)
    }

    /// (number of photos, total bytes)
    pub fn stats(conn: &Connection) -> Result<(i64, i64)> {
        Ok(conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(LENGTH(jpeg)), 0) FROM transaction_images",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    /// Delete every transaction photo, then compact the database file so it really shrinks
    pub fn clear_all(conn: &Connection) -> Result<()> {
        conn.execute("DELETE FROM transaction_images", [])?;
        conn.execute_batch("VACUUM;")?;
        // WAL mode: fold the log back into the (now smaller) file
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Currency, NewTransaction, NewWallet, WalletType};
    use crate::repository::WalletRepo;
    use crate::services::TransactionService;
    use rust_decimal::Decimal;

    #[test]
    fn photos_follow_their_transaction_and_can_be_cleared() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let wallet = WalletRepo::create(&conn, &NewWallet {
            name: "CASH".into(), wallet_type: WalletType::Cash, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        let txn = |amount: i64| TransactionService::execute(&conn,
            &NewTransaction::opening_balance(&wallet.id, Decimal::from(amount), Currency::VND, 1)).unwrap().id;
        let (a, b) = (txn(1), txn(2));

        TxnImageRepo::add(&conn, &a, &[1, 2, 3]).unwrap();
        TxnImageRepo::add(&conn, &a, &[4, 5]).unwrap();
        TxnImageRepo::add(&conn, &b, &[6]).unwrap();
        assert_eq!(TxnImageRepo::list(&conn, &a).unwrap(), vec![vec![1, 2, 3], vec![4, 5]]);
        assert_eq!(TxnImageRepo::counts(&conn).unwrap()[&a], 2);
        assert_eq!(TxnImageRepo::stats(&conn).unwrap(), (3, 6));

        // Deleting a transaction takes its photos with it
        TransactionService::delete(&conn, &a).unwrap();
        assert_eq!(TxnImageRepo::stats(&conn).unwrap(), (1, 1));

        TxnImageRepo::clear_all(&conn).unwrap();
        assert_eq!(TxnImageRepo::stats(&conn).unwrap(), (0, 0));
        assert!(crate::repository::TransactionRepo::find_by_id(&conn, &b).unwrap().is_some(), "transactions stay");
    }
}
