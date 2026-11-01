use rusqlite::Connection;

use crate::error::{AppError, Result};
use crate::models::{currency, Currency, CurrencyInfo};

pub struct CurrencyRepo;

impl CurrencyRepo {
    pub fn list(conn: &Connection) -> Result<Vec<CurrencyInfo>> {
        let mut stmt = conn.prepare(
            "SELECT code, name, unit, is_active, sort_order FROM currencies ORDER BY sort_order, code",
        )?;
        let rows = stmt.query_map([], CurrencyInfo::from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// Money types shown in pickers
    pub fn list_active(conn: &Connection) -> Result<Vec<CurrencyInfo>> {
        let mut stmt = conn.prepare(
            "SELECT code, name, unit, is_active, sort_order FROM currencies
             WHERE is_active = 1 ORDER BY sort_order, code",
        )?;
        let rows = stmt.query_map([], CurrencyInfo::from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// Reload the process-wide cache used by the plain formatting functions
    pub fn refresh_cache(conn: &Connection) -> Result<()> {
        currency::set_cache(Self::list(conn)?);
        Ok(())
    }

    /// True when an active wallet holds a non-zero balance in this money type
    pub fn in_use(conn: &Connection, currency: &Currency) -> Result<bool> {
        let used: i64 = conn.query_row(
            "SELECT COUNT(*) FROM wallet_balances b JOIN wallets w ON w.id = b.wallet_id
             WHERE b.currency = ?1 AND w.is_active = 1 AND ABS(b.balance) > 1e-9",
            [currency.to_string()],
            |row| row.get(0),
        )?;
        Ok(used > 0)
    }

    /// Edit name / unit / shown flag. Hiding is refused while money is left in the type
    /// and for the last shown type.
    pub fn update(conn: &Connection, info: &CurrencyInfo) -> Result<()> {
        let name = info.name.trim();
        let unit = info.unit.trim();
        if name.is_empty() || unit.is_empty() {
            return Err(AppError::Validation("Name and unit are required".into()));
        }
        if !info.is_active {
            if Self::in_use(conn, &info.code)? {
                return Err(AppError::Validation("This money type still has a balance in a wallet".into()));
            }
            let other_active: i64 = conn.query_row(
                "SELECT COUNT(*) FROM currencies WHERE is_active = 1 AND code <> ?1",
                [info.code.to_string()],
                |row| row.get(0),
            )?;
            if other_active == 0 {
                return Err(AppError::Validation("At least one money type must stay shown".into()));
            }
        }
        conn.execute(
            "UPDATE currencies SET name = ?1, unit = ?2, is_active = ?3 WHERE code = ?4",
            rusqlite::params![name, unit, info.is_active as i32, info.code.to_string()],
        )?;
        Self::refresh_cache(conn)
    }

    pub fn reorder(conn: &Connection, ordered: &[Currency]) -> Result<()> {
        for (i, code) in ordered.iter().enumerate() {
            conn.execute(
                "UPDATE currencies SET sort_order = ?1 WHERE code = ?2",
                rusqlite::params![i as i32, code.to_string()],
            )?;
        }
        Self::refresh_cache(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NewTransaction, NewWallet, WalletType};
    use crate::repository::WalletRepo;
    use crate::services::TransactionService;
    use rust_decimal::Decimal;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn codes(list: &[CurrencyInfo]) -> Vec<Currency> {
        list.iter().map(|i| i.code.clone()).collect()
    }

    #[test]
    fn seeds_the_three_money_types_in_order_with_only_vnd_shown() {
        let conn = conn();
        let list = CurrencyRepo::list(&conn).unwrap();
        assert_eq!(codes(&list), [Currency::VND, Currency::USD, Currency::GOLD]);
        assert_eq!(codes(&CurrencyRepo::list_active(&conn).unwrap()), [Currency::VND]);
    }

    /// Show a seeded (hidden) money type
    fn show(conn: &Connection, code: Currency) -> CurrencyInfo {
        let mut info = CurrencyRepo::list(conn).unwrap().into_iter().find(|i| i.code == code).unwrap();
        info.is_active = true;
        CurrencyRepo::update(conn, &info).unwrap();
        info
    }

    #[test]
    fn cannot_hide_a_type_that_still_has_a_balance() {
        let conn = conn();
        show(&conn, Currency::USD);
        show(&conn, Currency::GOLD);
        let w = WalletRepo::create(&conn, &NewWallet {
            name: "W".into(), wallet_type: WalletType::Cash, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        TransactionService::execute(&conn, &NewTransaction::opening_balance(&w.id, Decimal::from(5), Currency::USD, 1)).unwrap();
        assert!(CurrencyRepo::in_use(&conn, &Currency::USD).unwrap());
        assert!(!CurrencyRepo::in_use(&conn, &Currency::GOLD).unwrap());

        let mut usd = CurrencyRepo::list(&conn).unwrap().into_iter().find(|i| i.code == Currency::USD).unwrap();
        usd.is_active = false;
        assert!(CurrencyRepo::update(&conn, &usd).is_err());

        let mut gold = CurrencyRepo::list(&conn).unwrap().into_iter().find(|i| i.code == Currency::GOLD).unwrap();
        gold.is_active = false;
        CurrencyRepo::update(&conn, &gold).unwrap();
        assert_eq!(codes(&CurrencyRepo::list_active(&conn).unwrap()), [Currency::VND, Currency::USD]);
    }

    #[test]
    fn cannot_hide_the_last_active_type() {
        let conn = conn();
        // Only VND is shown by default: it can't be hidden until another type is shown
        let mut vnd = CurrencyRepo::list(&conn).unwrap().into_iter().find(|i| i.code == Currency::VND).unwrap();
        vnd.is_active = false;
        assert!(CurrencyRepo::update(&conn, &vnd).is_err());

        show(&conn, Currency::GOLD);
        CurrencyRepo::update(&conn, &vnd).unwrap();
        assert_eq!(codes(&CurrencyRepo::list_active(&conn).unwrap()), [Currency::GOLD]);
    }

    #[test]
    fn reorder_changes_list_order() {
        let conn = conn();
        CurrencyRepo::reorder(&conn, &[Currency::GOLD, Currency::VND, Currency::USD]).unwrap();
        assert_eq!(codes(&CurrencyRepo::list(&conn).unwrap()), [Currency::GOLD, Currency::VND, Currency::USD]);
    }
}
