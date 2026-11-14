use rusqlite::Row;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{decimal_to_f64, f64_to_decimal};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub id:            String,
    pub name:          String,
    pub icon:          Option<String>,
    pub color:         Option<String>,
    pub budget_amount: Option<Decimal>,
    pub parent_id:     Option<String>,
    pub sort_order:    i32,
    pub is_system:     bool,
    pub created_at:    i64,
}

impl Category {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let budget_f64: Option<f64> = row.get("budget_amount")?;
        let is_system_i: i32 = row.get("is_system")?;

        Ok(Category {
            id:            row.get("id")?,
            name:          row.get("name")?,
            icon:          row.get("icon")?,
            color:         row.get("color")?,
            budget_amount: budget_f64.map(f64_to_decimal),
            parent_id:     row.get("parent_id")?,
            sort_order:    row.get("sort_order")?,
            is_system:     is_system_i != 0,
            created_at:    row.get("created_at")?,
        })
    }

    pub fn budget_f64(&self) -> Option<f64> {
        self.budget_amount.map(decimal_to_f64)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewCategory {
    pub name:          String,
    pub icon:          Option<String>,
    pub color:         Option<String>,
    pub budget_amount: Option<Decimal>,
    pub parent_id:     Option<String>,
    pub sort_order:    i32,
}

/// System default categories seeded on first launch
pub fn default_categories() -> Vec<NewCategory> {
    let cat = |name: &str, icon: &str, color: &str, sort_order: i32| NewCategory {
        name: name.into(),
        icon: Some(icon.into()),
        color: Some(color.into()),
        budget_amount: None,
        parent_id: None,
        sort_order,
    };
    vec![
        cat("Food & Drinks",   "🍜",  "#FF6B6B", 1),
        cat("Transport",       "🚗",  "#4ECDC4", 2),
        cat("Shopping",        "🛍️", "#45B7D1", 3),
        cat("Health",          "💊",  "#96CEB4", 4),
        cat("Entertainment",   "🎮",  "#FFEAA7", 5),
        cat("Education",       "📚",  "#DDA0DD", 6),
        cat("Cosmetics",       "💄",  "#FFB6C1", 7),
        cat("Household Goods", "🧺",  "#C3B091", 8),
        cat("Travel",          "✈️", "#87CEEB", 9),
        cat("Family Support",  "🤲",  "#F4A261", 10),
        cat("Gift",            "🎁",  "#E91E63", 11),
        // Fixed monthly costs: named "Tiền …" in Vietnamese and kept last
        cat("Electric bill",   "⚡",  "#f59e0b", 21),
        cat("Water bill",      "💧",  "#06b6d4", 22),
        cat("Rent",            "🏠",  "#8b5cf6", 23),
        cat("Internet bill",   "📶",  "#3f51b5", 24),
        cat("School fees",     "🎓",  "#DDA0DD", 25),
        cat("Bills",           "🧾",  "#F0E68C", 26),
    ]
}
