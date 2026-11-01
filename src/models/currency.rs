use rusqlite::Row;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use super::{wallet::to_sqlite_err, Currency};

/// One money type with its user-editable display details (table `currencies`)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurrencyInfo {
    pub code:       Currency,
    pub name:       String,
    pub unit:       String,
    pub is_active:  bool,
    pub sort_order: i32,
}

impl CurrencyInfo {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let code: String = row.get("code")?;
        let is_active_i: i32 = row.get("is_active")?;
        Ok(CurrencyInfo {
            code:       Currency::from_str(&code).map_err(to_sqlite_err)?,
            name:       row.get("name")?,
            unit:       row.get("unit")?,
            is_active:  is_active_i != 0,
            sort_order: row.get("sort_order")?,
        })
    }
}

// ─── Process-wide cache ───────────────────────────────────────────────────────

/// Plain formatting functions (also used outside components, e.g. notification text)
/// read the edited unit from here instead of the database.
static CACHE: std::sync::RwLock<Vec<CurrencyInfo>> = std::sync::RwLock::new(Vec::new());

/// Replace the cached money types
pub fn set_cache(list: Vec<CurrencyInfo>) {
    *CACHE.write().unwrap_or_else(|e| e.into_inner()) = list;
}

/// Unit symbol of a money type: the cached (user-edited) one, else the default
pub fn cached_unit(currency: &Currency) -> String {
    let cache = CACHE.read().unwrap_or_else(|e| e.into_inner());
    if let Some(info) = cache.iter().find(|i| &i.code == currency) {
        return info.unit.clone();
    }
    match currency {
        Currency::VND  => "₫",
        Currency::USD  => "$",
        Currency::GOLD => "chỉ",
    }.to_string()
}
