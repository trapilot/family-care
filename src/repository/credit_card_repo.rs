use rusqlite::{Connection, OptionalExtension};
use rust_decimal::Decimal;

use crate::error::Result;
use crate::models::{decimal_to_f64, f64_to_decimal, CreditCard};

pub struct CreditCardRepo;

impl CreditCardRepo {
    pub fn get(conn: &Connection, wallet_id: &str) -> Result<Option<CreditCard>> {
        conn.query_row(
            "SELECT * FROM credit_cards WHERE wallet_id = ?1",
            [wallet_id],
            CreditCard::from_row,
        ).optional().map_err(Into::into)
    }

    /// Settings of credit-card wallets (disabled ones included: they stay tracked)
    pub fn list(conn: &Connection) -> Result<Vec<CreditCard>> {
        let mut stmt = conn.prepare(
            "SELECT c.* FROM credit_cards c
             JOIN wallets w ON w.id = c.wallet_id
             WHERE w.wallet_type = 'credit_card'
             ORDER BY w.sort_order, w.created_at",
        )?;
        let rows = stmt.query_map([], CreditCard::from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// Insert or update the days and limit; the paid month and creation time are kept
    pub fn upsert(
        conn: &Connection,
        wallet_id: &str,
        statement_day: u32,
        due_day: u32,
        credit_limit: Option<Decimal>,
    ) -> Result<()> {
        conn.execute(
            "INSERT INTO credit_cards (wallet_id, statement_day, due_day, credit_limit, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(wallet_id) DO UPDATE SET
                statement_day = excluded.statement_day,
                due_day       = excluded.due_day,
                credit_limit  = excluded.credit_limit",
            rusqlite::params![
                wallet_id,
                statement_day.clamp(1, 31),
                due_day.clamp(1, 31),
                credit_limit.map(decimal_to_f64),
                crate::views::now_ts(),
            ],
        )?;
        Ok(())
    }

    /// Mark a statement month ("YYYY-MM") paid, or `None` to undo
    pub fn set_paid_month(conn: &Connection, wallet_id: &str, month: Option<&str>) -> Result<()> {
        conn.execute(
            "UPDATE credit_cards SET paid_month = ?2 WHERE wallet_id = ?1",
            rusqlite::params![wallet_id, month],
        )?;
        Ok(())
    }

    pub fn delete(conn: &Connection, wallet_id: &str) -> Result<()> {
        conn.execute("DELETE FROM credit_cards WHERE wallet_id = ?1", [wallet_id])?;
        Ok(())
    }

    /// (inflows, outflows) in VND on the card from transactions dated `from_ts` or later.
    /// The effects match `TransactionService::reverse_inner`.
    pub fn flows_since(conn: &Connection, wallet_id: &str, from_ts: i64) -> Result<(Decimal, Decimal)> {
        let inflow: f64 = conn.query_row(
            "SELECT
               COALESCE((SELECT SUM(amount) FROM transactions
                          WHERE wallet_id = ?1 AND currency = 'VND' AND txn_date >= ?2
                            AND txn_type IN ('income', 'opening_balance', 'invest_sell')), 0)
             + COALESCE((SELECT SUM(COALESCE(to_amount, amount)) FROM transactions
                          WHERE to_wallet_id = ?1 AND to_currency = 'VND' AND txn_date >= ?2
                            AND txn_type = 'transfer'), 0)",
            rusqlite::params![wallet_id, from_ts],
            |row| row.get(0),
        )?;
        let outflow: f64 = conn.query_row(
            "SELECT COALESCE(SUM(amount), 0) FROM transactions
              WHERE wallet_id = ?1 AND currency = 'VND' AND txn_date >= ?2
                AND txn_type IN ('expense', 'transfer', 'invest_buy')",
            rusqlite::params![wallet_id, from_ts],
            |row| row.get(0),
        )?;
        Ok((f64_to_decimal(inflow), f64_to_decimal(outflow)))
    }
}
