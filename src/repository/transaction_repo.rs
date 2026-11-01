use rusqlite::Connection;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::{decimal_to_f64, transaction::NewTransaction, Transaction, TransactionType};

/// Filter options for listing transactions
#[derive(Debug, Default)]
pub struct TxnFilter {
    pub wallet_id:    Option<String>,
    pub category_id:  Option<String>,
    pub txn_type:     Option<TransactionType>,
    /// Any of these types (ignored when empty)
    pub txn_types:    Vec<TransactionType>,
    pub date_from:    Option<i64>,
    pub date_to:      Option<i64>,
    /// Leave out opening balances of 0 (records kept only so they can be edited later)
    pub skip_zero_opening: bool,
    pub limit:        Option<u32>,
    pub offset:       Option<u32>,
}

pub struct TransactionRepo;

impl TransactionRepo {
    const SELECT_COLS: &'static str =
        "id, txn_type, wallet_id, amount, currency,
         income_type, category_id,
         to_wallet_id, to_amount, to_currency,
         holding_id, asset_quantity, asset_price,
         note, txn_date, created_at";

    pub fn insert(conn: &Connection, input: &NewTransaction) -> Result<Transaction> {
        let id = Uuid::new_v4().to_string();
        let now = now_ts();

        conn.execute(
            "INSERT INTO transactions
             (id, txn_type, wallet_id, amount, currency,
              income_type, category_id,
              to_wallet_id, to_amount, to_currency,
              holding_id, asset_quantity, asset_price,
              note, txn_date, created_at)
             VALUES
             (?1,  ?2,  ?3,  ?4,  ?5,
              ?6,  ?7,
              ?8,  ?9,  ?10,
              ?11, ?12, ?13,
              ?14, ?15, ?16)",
            rusqlite::params![
                id,
                input.txn_type.to_string(),
                input.wallet_id,
                decimal_to_f64(input.amount),
                input.currency.to_string(),
                input.income_type.as_ref().map(|t| t.to_string()),
                input.category_id,
                input.to_wallet_id,
                input.to_amount.map(decimal_to_f64),
                input.to_currency.as_ref().map(|c| c.to_string()),
                input.holding_id,
                input.asset_quantity.map(decimal_to_f64),
                input.asset_price.map(decimal_to_f64),
                input.note,
                input.txn_date,
                now,
            ],
        )?;

        Self::find_by_id(conn, &id)?.ok_or_else(|| AppError::NotFound(id))
    }

    pub fn find_by_id(conn: &Connection, id: &str) -> Result<Option<Transaction>> {
        let sql = format!("SELECT {} FROM transactions WHERE id = ?1", Self::SELECT_COLS);
        let mut stmt = conn.prepare(&sql)?;
        match stmt.query_row([id], Transaction::from_row) {
            Ok(t)                                     => Ok(Some(t)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e)                                    => Err(e.into()),
        }
    }

    pub fn list(conn: &Connection, filter: &TxnFilter) -> Result<Vec<Transaction>> {
        let mut conditions: Vec<String> = vec![];
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        if let Some(wid) = &filter.wallet_id {
            conditions.push(format!("(wallet_id = ?{} OR to_wallet_id = ?{})", params.len() + 1, params.len() + 1));
            params.push(Box::new(wid.clone()));
        }
        if let Some(cat) = &filter.category_id {
            conditions.push(format!("category_id = ?{}", params.len() + 1));
            params.push(Box::new(cat.clone()));
        }
        if let Some(tt) = &filter.txn_type {
            conditions.push(format!("txn_type = ?{}", params.len() + 1));
            params.push(Box::new(tt.to_string()));
        }
        if !filter.txn_types.is_empty() {
            let placeholders: Vec<String> = filter.txn_types.iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", params.len() + 1 + i))
                .collect();
            conditions.push(format!("txn_type IN ({})", placeholders.join(", ")));
            for tt in &filter.txn_types {
                params.push(Box::new(tt.to_string()));
            }
        }
        if let Some(from) = filter.date_from {
            conditions.push(format!("txn_date >= ?{}", params.len() + 1));
            params.push(Box::new(from));
        }
        if let Some(to) = filter.date_to {
            conditions.push(format!("txn_date <= ?{}", params.len() + 1));
            params.push(Box::new(to));
        }

        if filter.skip_zero_opening {
            conditions.push("NOT (txn_type = 'opening_balance' AND amount = 0)".to_string());
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let limit_clause = match (filter.limit, filter.offset) {
            (Some(l), Some(o)) => format!("LIMIT {l} OFFSET {o}"),
            (Some(l), None)    => format!("LIMIT {l}"),
            _                  => String::new(),
        };

        let sql = format!(
            "SELECT {} FROM transactions {} ORDER BY txn_date DESC, created_at DESC {}",
            Self::SELECT_COLS, where_clause, limit_clause
        );

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), Transaction::from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn list_by_month(conn: &Connection, year: i32, month: u32) -> Result<Vec<Transaction>> {
        let (from, to) = month_bounds(year, month);
        Self::list(conn, &TxnFilter {
            date_from: Some(from),
            date_to:   Some(to),
            ..Default::default()
        })
    }

    pub fn sum_by_type_in_month(conn: &Connection, txn_type: TransactionType, year: i32, month: u32) -> Result<f64> {
        let (from, to) = month_bounds(year, month);
        let result: f64 = conn.query_row(
            "SELECT COALESCE(SUM(amount), 0.0)
             FROM transactions
             WHERE txn_type = ?1 AND txn_date >= ?2 AND txn_date <= ?3",
            rusqlite::params![txn_type.to_string(), from, to],
            |row| row.get(0),
        )?;
        Ok(result)
    }

    /// Sum expenses grouped by category for a given month
    pub fn expense_sum_by_category_in_month(
        conn: &Connection,
        year: i32,
        month: u32,
    ) -> Result<Vec<(String, f64)>> {
        let (from, to) = month_bounds(year, month);
        let mut stmt = conn.prepare(
            "SELECT category_id, SUM(amount) as total
             FROM transactions
             WHERE txn_type = 'expense'
               AND category_id IS NOT NULL
               AND txn_date >= ?1
               AND txn_date <= ?2
             GROUP BY category_id
             ORDER BY total DESC",
        )?;
        let rows = stmt.query_map(rusqlite::params![from, to], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn delete(conn: &Connection, id: &str) -> Result<()> {
        conn.execute("DELETE FROM transaction_images WHERE transaction_id = ?1", [id])?;
        let affected = conn.execute("DELETE FROM transactions WHERE id = ?1", [id])?;
        if affected == 0 {
            return Err(AppError::NotFound(format!("transaction '{id}'")));
        }
        Ok(())
    }

    pub fn list_all(conn: &Connection) -> Result<Vec<Transaction>> {
        Self::list(conn, &TxnFilter::default())
    }
}

fn now_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Returns (start_ts, end_ts) in Unix seconds for the given month
pub fn month_bounds(year: i32, month: u32) -> (i64, i64) {
    use chrono::{NaiveDate, TimeZone, Utc};
    let start = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let end_month = if month == 12 { 1 } else { month + 1 };
    let end_year  = if month == 12 { year + 1 } else { year };
    let end = NaiveDate::from_ymd_opt(end_year, end_month, 1).unwrap();

    let start_ts = Utc.from_utc_datetime(&start.and_hms_opt(0, 0, 0).unwrap()).timestamp();
    let end_ts   = Utc.from_utc_datetime(&end.and_hms_opt(0, 0, 0).unwrap()).timestamp() - 1;
    (start_ts, end_ts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Currency, IncomeType, NewWallet, WalletType};
    use crate::repository::{CategoryRepo, WalletRepo};
    use crate::services::TransactionService;
    use rust_decimal::Decimal;

    #[test]
    fn filters_by_any_of_several_types() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        CategoryRepo::seed_defaults(&conn).unwrap();
        let wallet = |name: &str| {
            let id = WalletRepo::create(&conn, &NewWallet {
                name: name.into(), wallet_type: WalletType::Bank, broker: None, icon: None, color: None, sort_order: 0,
            }).unwrap().id;
            TransactionService::execute(&conn, &NewTransaction::opening_balance(&id, Decimal::from(1_000_000), Currency::VND, 0)).unwrap();
            id
        };
        let (a, b) = (wallet("A"), wallet("B"));
        let category = CategoryRepo::list(&conn).unwrap()[0].id.clone();
        let amount = Decimal::from(1_000);

        TransactionService::execute(&conn, &NewTransaction::income(&a, amount, Currency::VND, IncomeType::Salary, 1)).unwrap();
        TransactionService::execute(&conn, &NewTransaction::expense(&a, amount, Currency::VND, &category, 2)).unwrap();
        TransactionService::execute(&conn, &NewTransaction::transfer(&a, amount, Currency::VND, &b, amount, Currency::VND, 3)).unwrap();

        let types = |filter: Vec<TransactionType>| -> Vec<TransactionType> {
            let mut got: Vec<_> = TransactionRepo::list(&conn, &TxnFilter { txn_types: filter, ..Default::default() })
                .unwrap().into_iter().map(|t| t.txn_type).collect();
            got.sort_by_key(|t| t.to_string());
            got
        };
        assert_eq!(types(vec![TransactionType::Income, TransactionType::Transfer]),
                   vec![TransactionType::Income, TransactionType::Transfer]);
        assert_eq!(types(vec![TransactionType::Expense]), vec![TransactionType::Expense]);
        assert_eq!(types(vec![]).len(), 5, "empty = no type filter (3 above + 2 opening balances)");
    }

    #[test]
    fn zero_opening_balance_can_be_left_out() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let wallet = |name: &str| WalletRepo::create(&conn, &NewWallet {
            name: name.into(), wallet_type: WalletType::Cash, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap().id;
        let (empty, funded) = (wallet("EMPTY"), wallet("FUNDED"));
        TransactionService::execute(&conn, &NewTransaction::opening_balance(&empty, Decimal::ZERO, Currency::VND, 1)).unwrap();
        TransactionService::execute(&conn, &NewTransaction::opening_balance(&funded, Decimal::from(500), Currency::VND, 2)).unwrap();

        assert_eq!(TransactionRepo::list(&conn, &TxnFilter::default()).unwrap().len(), 2);
        let shown = TransactionRepo::list(&conn, &TxnFilter { skip_zero_opening: true, ..Default::default() }).unwrap();
        assert_eq!(shown.iter().map(|t| t.wallet_id.as_str()).collect::<Vec<_>>(), [funded.as_str()]);
    }

    #[test]
    fn opening_balance_moves_balance_but_is_not_income() {
        use crate::services::ReportService;
        use chrono::{Datelike, Local};

        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let wallet = WalletRepo::create(&conn, &NewWallet {
            name: "Cash".into(), wallet_type: WalletType::Cash, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        let now = Local::now();
        let opening = TransactionService::execute(&conn,
            &NewTransaction::opening_balance(&wallet.id, Decimal::from(5_000_000), Currency::VND, now.timestamp())).unwrap();

        let balance = || WalletRepo::find_by_id(&conn, &wallet.id).unwrap().unwrap().balance(&Currency::VND);
        assert_eq!(balance(), Decimal::from(5_000_000));
        let summary = ReportService::monthly_summary(&conn, now.year(), now.month()).unwrap();
        assert_eq!(summary.income, Decimal::ZERO, "opening balance is not income");

        TransactionService::delete(&conn, &opening.id).unwrap();
        assert_eq!(balance(), Decimal::ZERO, "deleting reverses it");
    }
}
