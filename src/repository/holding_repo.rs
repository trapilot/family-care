use rusqlite::Connection;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::{
    decimal_to_f64, f64_to_decimal,
    holding::{Holding, NewHolding},
};
use rust_decimal::Decimal;

pub struct HoldingRepo;

impl HoldingRepo {
    pub fn create(conn: &Connection, input: &NewHolding) -> Result<Holding> {
        let id = Uuid::new_v4().to_string();
        let now = now_ts();

        conn.execute(
            "INSERT INTO holdings
             (id, wallet_id, symbol, name, asset_type, quantity, avg_buy_price, last_price, last_price_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, NULL, NULL, ?6)",
            rusqlite::params![
                id,
                input.wallet_id,
                input.symbol.to_uppercase(),
                input.name,
                input.asset_type.to_string(),
                now,
            ],
        )?;

        Self::find_by_id(conn, &id)?.ok_or_else(|| AppError::NotFound(id))
    }

    pub fn find_by_id(conn: &Connection, id: &str) -> Result<Option<Holding>> {
        let mut stmt = conn.prepare(
            "SELECT id, wallet_id, symbol, name, asset_type, quantity, avg_buy_price, last_price, last_price_at, created_at
             FROM holdings WHERE id = ?1",
        )?;
        match stmt.query_row([id], Holding::from_row) {
            Ok(h)                                     => Ok(Some(h)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e)                                    => Err(e.into()),
        }
    }

    pub fn find_by_wallet_and_symbol(conn: &Connection, wallet_id: &str, symbol: &str) -> Result<Option<Holding>> {
        let mut stmt = conn.prepare(
            "SELECT id, wallet_id, symbol, name, asset_type, quantity, avg_buy_price, last_price, last_price_at, created_at
             FROM holdings WHERE wallet_id = ?1 AND symbol = ?2",
        )?;
        match stmt.query_row(rusqlite::params![wallet_id, symbol.to_uppercase()], Holding::from_row) {
            Ok(h)                                     => Ok(Some(h)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e)                                    => Err(e.into()),
        }
    }

    pub fn list_by_wallet(conn: &Connection, wallet_id: &str) -> Result<Vec<Holding>> {
        let mut stmt = conn.prepare(
            "SELECT id, wallet_id, symbol, name, asset_type, quantity, avg_buy_price, last_price, last_price_at, created_at
             FROM holdings
             WHERE wallet_id = ?1
             ORDER BY symbol ASC",
        )?;
        let rows = stmt.query_map([wallet_id], Holding::from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// Every holding, including sold-out ones (for backups: transactions may reference them)
    pub fn list_all(conn: &Connection) -> Result<Vec<Holding>> {
        let mut stmt = conn.prepare(
            "SELECT id, wallet_id, symbol, name, asset_type, quantity, avg_buy_price, last_price, last_price_at, created_at
             FROM holdings
             ORDER BY symbol ASC",
        )?;
        let rows = stmt.query_map([], Holding::from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn list_all_active(conn: &Connection) -> Result<Vec<Holding>> {
        let mut stmt = conn.prepare(
            "SELECT id, wallet_id, symbol, name, asset_type, quantity, avg_buy_price, last_price, last_price_at, created_at
             FROM holdings
             WHERE quantity > 0
             ORDER BY symbol ASC",
        )?;
        let rows = stmt.query_map([], Holding::from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// Update quantity and recalculated avg price atomically — called only by HoldingService
    pub fn update_position(
        conn: &Connection,
        holding_id: &str,
        new_quantity: Decimal,
        new_avg_price: Decimal,
    ) -> Result<()> {
        conn.execute(
            "UPDATE holdings SET quantity = ?1, avg_buy_price = ?2 WHERE id = ?3",
            rusqlite::params![
                decimal_to_f64(new_quantity),
                decimal_to_f64(new_avg_price),
                holding_id,
            ],
        )?;
        Ok(())
    }

    pub fn update_last_price(conn: &Connection, holding_id: &str, price: Decimal) -> Result<()> {
        let now = now_ts();
        conn.execute(
            "UPDATE holdings SET last_price = ?1, last_price_at = ?2 WHERE id = ?3",
            rusqlite::params![decimal_to_f64(price), now, holding_id],
        )?;
        Ok(())
    }

    pub fn update_name(conn: &Connection, holding_id: &str, name: &str) -> Result<()> {
        conn.execute(
            "UPDATE holdings SET name = ?1 WHERE id = ?2",
            rusqlite::params![name, holding_id],
        )?;
        Ok(())
    }

    pub fn get_quantity(conn: &Connection, holding_id: &str) -> Result<Decimal> {
        let qty_f64: f64 = conn.query_row(
            "SELECT quantity FROM holdings WHERE id = ?1",
            [holding_id],
            |row| row.get(0),
        ).map_err(|_| AppError::NotFound(format!("holding '{holding_id}'")))?;
        Ok(f64_to_decimal(qty_f64))
    }

    pub fn get_avg_price(conn: &Connection, holding_id: &str) -> Result<Decimal> {
        let avg_f64: f64 = conn.query_row(
            "SELECT avg_buy_price FROM holdings WHERE id = ?1",
            [holding_id],
            |row| row.get(0),
        ).map_err(|_| AppError::NotFound(format!("holding '{holding_id}'")))?;
        Ok(f64_to_decimal(avg_f64))
    }
}

fn now_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
