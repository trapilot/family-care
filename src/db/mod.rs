pub mod migrations;

use once_cell::sync::OnceCell;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

static DB: OnceCell<Arc<Mutex<Connection>>> = OnceCell::new();
/// File path given to `init`, needed by `reset`
static DB_PATH: OnceCell<String> = OnceCell::new();

pub fn init(path: &str) -> Arc<Mutex<Connection>> {
    let conn = Connection::open(path).expect("Failed to open SQLite database");
    migrations::run(&conn).expect("Failed to run migrations");
    let _ = crate::repository::CurrencyRepo::refresh_cache(&conn);
    let db = Arc::new(Mutex::new(conn));
    DB.set(db.clone()).ok();
    DB_PATH.set(path.to_string()).ok();
    db
}

/// Folder holding the database, also used for app JSON files (e.g. bank statements).
pub fn data_dir() -> std::path::PathBuf {
    let path = DB_PATH.get().map(String::as_str).unwrap_or("");
    match std::path::Path::new(path).parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => std::path::PathBuf::from("."),
    }
}

pub fn get() -> Arc<Mutex<Connection>> {
    DB.get().expect("Database not initialized — call db::init() first").clone()
}

/// Data every database needs: system categories, a default CASH wallet and the running app version
pub fn seed(conn: &Connection) -> crate::error::Result<()> {
    use crate::repository::{CategoryRepo, SettingsRepo};
    CategoryRepo::seed_defaults(conn)?;
    seed_default_wallet(conn)?;
    SettingsRepo::set_app_version(conn, env!("CARGO_PKG_VERSION"))
}

/// Once per database: a CASH wallet (VND, opening balance 0) if there are no wallets yet.
/// It's an ordinary wallet — editable and deletable — and never re-created afterwards.
fn seed_default_wallet(conn: &Connection) -> crate::error::Result<()> {
    use crate::models::{Currency, NewTransaction, NewWallet, WalletType};
    use crate::repository::{SettingsRepo, WalletRepo};
    use rust_decimal::Decimal;

    if SettingsRepo::is_default_wallet_created(conn)? {
        return Ok(());
    }
    let no_wallets: bool = conn.query_row("SELECT COUNT(*) = 0 FROM wallets", [], |r| r.get(0))?;
    if no_wallets {
        let wallet = WalletRepo::create(conn, &NewWallet {
            name: "CASH".into(), wallet_type: WalletType::Cash, broker: None, icon: None, color: None, sort_order: 0,
        })?;
        // Same as the wallet form: every wallet starts with an (editable) opening balance record
        let mut opening = NewTransaction::opening_balance(&wallet.id, Decimal::ZERO, Currency::VND, crate::views::now_ts());
        opening.note = Some("Opening Balance".into());
        crate::services::TransactionService::execute(conn, &opening)?;
    }
    SettingsRepo::set_default_wallet_created(conn)
}

/// "Forgot PIN": close the database, delete its files and start over with an empty,
/// seeded one (no PIN). Backup files live elsewhere and are kept.
pub fn reset() -> crate::error::Result<()> {
    let path = DB_PATH.get()
        .ok_or_else(|| crate::error::AppError::NotFound("database path".into()))?;
    let db = get();
    let mut conn = db.lock().expect("DB lock poisoned");

    // Swap in a throwaway connection so the file is closed before deleting it
    let old = std::mem::replace(&mut *conn, Connection::open_in_memory()?);
    old.close().map_err(|(_, e)| e)?;
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let file = format!("{path}{suffix}");
        if std::path::Path::new(&file).exists() {
            std::fs::remove_file(&file)?;
        }
    }

    let fresh = Connection::open(path)?;
    migrations::run(&fresh)?;
    seed(&fresh)?;
    // Reminders are gone: cancel their scheduled notifications
    crate::services::ReminderService::resync(&fresh)?;
    *conn = fresh;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::repository::{CategoryRepo, SettingsRepo, WalletRepo};

    #[test]
    fn default_cash_wallet_is_created_once() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::migrations::run(&conn).unwrap();
        super::seed(&conn).unwrap();
        let wallets = WalletRepo::list_active(&conn).unwrap();
        assert_eq!(wallets.len(), 1);
        assert_eq!(wallets[0].name, "CASH");
        assert!(crate::services::TransactionService::find_opening_balance(&conn, &wallets[0].id, &crate::models::Currency::VND).unwrap().is_some());

        // Deleted by the user: the next launch doesn't bring it back
        conn.execute_batch("DELETE FROM transactions; DELETE FROM wallet_balances; DELETE FROM wallets;").unwrap();
        super::seed(&conn).unwrap();
        assert!(WalletRepo::list_active(&conn).unwrap().is_empty());
    }

    #[test]
    fn reset_deletes_everything_and_reseeds() {
        let dir = std::env::temp_dir().join(format!("ff-reset-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let db = super::init(path.to_str().unwrap());
        {
            let conn = db.lock().unwrap();
            super::seed(&conn).unwrap();
            SettingsRepo::set_pin(&conn, "1234").unwrap();
            conn.execute("INSERT INTO families (id, name, created_at) VALUES ('f', 'Nguyen', 0)", []).unwrap();
        }

        super::reset().unwrap();

        let conn = db.lock().unwrap();
        assert!(!SettingsRepo::is_pin_set(&conn).unwrap());
        let families: i64 = conn.query_row("SELECT COUNT(*) FROM families", [], |r| r.get(0)).unwrap();
        assert_eq!(families, 0);
        assert!(!CategoryRepo::list(&conn).unwrap().is_empty(), "default categories re-seeded");
        let wallets: Vec<String> = crate::repository::WalletRepo::list_active(&conn).unwrap().into_iter().map(|w| w.name).collect();
        assert_eq!(wallets, ["CASH"], "default wallet re-created");
        assert!(path.exists(), "a fresh file is in place");
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
