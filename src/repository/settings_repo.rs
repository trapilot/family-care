use crate::error::Result;
use bcrypt::{hash, verify, DEFAULT_COST};
use rusqlite::{Connection, OptionalExtension};

pub struct SettingsRepo;

impl SettingsRepo {
    const KEY_PIN_HASH: &'static str = "pin_hash";
    const KEY_LANGUAGE: &'static str = "language";
    const KEY_APP_VERSION: &'static str = "app_version";
    const KEY_REMINDER_INTERVAL: &'static str = "reminder_interval_hours";
    const KEY_REMINDER_START: &'static str = "reminder_start_time";
    const KEY_DEFAULT_WALLET: &'static str = "default_wallet_created";
    const KEY_DEFAULT_WALLET_ID: &'static str = "default_wallet_id";
    const KEY_DEFAULT_CATEGORY_ID: &'static str = "default_category_id";

    fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
        Ok(conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        ).optional()?)
    }

    fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    /// Whether the default CASH wallet was already offered (so deleting it doesn't bring it back)
    pub fn is_default_wallet_created(conn: &Connection) -> Result<bool> {
        Ok(Self::get(conn, Self::KEY_DEFAULT_WALLET)?.is_some())
    }

    pub fn set_default_wallet_created(conn: &Connection) -> Result<()> {
        Self::set(conn, Self::KEY_DEFAULT_WALLET, "1")
    }

    fn get_non_empty(conn: &Connection, key: &str) -> Result<Option<String>> {
        Ok(Self::get(conn, key)?.filter(|v| !v.is_empty()))
    }

    fn set_or_clear(conn: &Connection, key: &str, id: Option<&str>) -> Result<()> {
        match id.filter(|v| !v.is_empty()) {
            Some(v) => Self::set(conn, key, v),
            None => {
                conn.execute("DELETE FROM settings WHERE key = ?1", rusqlite::params![key])?;
                Ok(())
            }
        }
    }

    /// Wallet pre-selected in the new-transaction form (None when unset).
    pub fn get_default_wallet_id(conn: &Connection) -> Result<Option<String>> {
        Self::get_non_empty(conn, Self::KEY_DEFAULT_WALLET_ID)
    }

    pub fn set_default_wallet_id(conn: &Connection, id: Option<&str>) -> Result<()> {
        Self::set_or_clear(conn, Self::KEY_DEFAULT_WALLET_ID, id)
    }

    /// Category pre-selected in the new-transaction form (None when unset).
    pub fn get_default_category_id(conn: &Connection) -> Result<Option<String>> {
        Self::get_non_empty(conn, Self::KEY_DEFAULT_CATEGORY_ID)
    }

    pub fn set_default_category_id(conn: &Connection, id: Option<&str>) -> Result<()> {
        Self::set_or_clear(conn, Self::KEY_DEFAULT_CATEGORY_ID, id)
    }

    /// Forget the default wallet when it is the given wallet (it was deleted or hidden).
    pub fn clear_default_wallet_if(conn: &Connection, wallet_id: &str) -> Result<()> {
        conn.execute(
            "DELETE FROM settings WHERE key = ?1 AND value = ?2",
            rusqlite::params![Self::KEY_DEFAULT_WALLET_ID, wallet_id],
        )?;
        Ok(())
    }

    /// Forget the default category when it is the given category (it was deleted).
    pub fn clear_default_category_if(conn: &Connection, category_id: &str) -> Result<()> {
        conn.execute(
            "DELETE FROM settings WHERE key = ?1 AND value = ?2",
            rusqlite::params![Self::KEY_DEFAULT_CATEGORY_ID, category_id],
        )?;
        Ok(())
    }

    /// UI language code ("en" / "vi").
    pub fn get_language(conn: &Connection) -> Result<Option<String>> {
        Self::get(conn, Self::KEY_LANGUAGE)
    }

    pub fn set_language(conn: &Connection, code: &str) -> Result<()> {
        Self::set(conn, Self::KEY_LANGUAGE, code)
    }

    /// Version of the app that last opened this database.
    pub fn get_app_version(conn: &Connection) -> Result<Option<String>> {
        Self::get(conn, Self::KEY_APP_VERSION)
    }

    /// Called on startup so the stored version always matches the running build.
    pub fn set_app_version(conn: &Connection, version: &str) -> Result<()> {
        Self::set(conn, Self::KEY_APP_VERSION, version)
    }

    /// On a day with due reminders, notify again every N hours (1–24, default 4).
    pub fn get_reminder_interval_hours(conn: &Connection) -> Result<u32> {
        Ok(Self::get(conn, Self::KEY_REMINDER_INTERVAL)?
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(4)
            .clamp(1, 24))
    }

    pub fn set_reminder_interval_hours(conn: &Connection, hours: u32) -> Result<()> {
        Self::set(conn, Self::KEY_REMINDER_INTERVAL, &hours.clamp(1, 24).to_string())
    }

    /// Time of the first notification on a due day, as (hour, minute). Default 08:00.
    pub fn get_reminder_start_time(conn: &Connection) -> Result<(u32, u32)> {
        Ok(Self::get(conn, Self::KEY_REMINDER_START)?
            .and_then(|v| parse_hh_mm(&v))
            .unwrap_or((8, 0)))
    }

    pub fn set_reminder_start_time(conn: &Connection, hour: u32, minute: u32) -> Result<()> {
        Self::set(conn, Self::KEY_REMINDER_START, &format!("{:02}:{:02}", hour.min(23), minute.min(59)))
    }

    pub fn set_pin(conn: &Connection, pin: &str) -> Result<()> {
        let hashed_pin = hash(pin, DEFAULT_COST)?;
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            rusqlite::params![Self::KEY_PIN_HASH, &hashed_pin],
        )?;
        Ok(())
    }

    pub fn verify_pin(conn: &Connection, pin: &str) -> Result<bool> {
        let hash: Option<String> = conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            rusqlite::params![Self::KEY_PIN_HASH],
            |row| row.get(0),
        ).optional()?;

        match hash {
            Some(hash_str) => Ok(verify(pin, &hash_str)?),
            None => Ok(false),
        }
    }

    pub fn is_pin_set(conn: &Connection) -> Result<bool> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM settings WHERE key = ?1",
            rusqlite::params![Self::KEY_PIN_HASH],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn clear_pin(conn: &Connection) -> Result<()> {
        conn.execute(
            "DELETE FROM settings WHERE key = ?1",
            rusqlite::params![Self::KEY_PIN_HASH],
        )?;
        Ok(())
    }
}

/// "08:30" → (8, 30)
pub fn parse_hh_mm(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}
