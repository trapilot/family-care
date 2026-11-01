use rusqlite::Row;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::error::{AppError, Result};

// ─── Currency ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    VND,
    USD,
    GOLD,
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Currency::VND  => write!(f, "VND"),
            Currency::USD  => write!(f, "USD"),
            Currency::GOLD => write!(f, "GOLD"),
        }
    }
}

impl FromStr for Currency {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "VND"  => Ok(Currency::VND),
            "USD"  => Ok(Currency::USD),
            "GOLD" => Ok(Currency::GOLD),
            _      => Err(AppError::Parse(format!("Unknown currency: {s}"))),
        }
    }
}

// ─── WalletType ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WalletType {
    Cash,
    Bank,
    Loan,
    EWallet,
    Investment,
}

impl fmt::Display for WalletType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WalletType::Cash       => write!(f, "cash"),
            WalletType::Bank       => write!(f, "bank"),
            WalletType::Loan       => write!(f, "loan"),
            WalletType::EWallet    => write!(f, "e_wallet"),
            WalletType::Investment => write!(f, "investment"),
        }
    }
}

impl FromStr for WalletType {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "cash"       => Ok(WalletType::Cash),
            "bank"       => Ok(WalletType::Bank),
            "loan"       => Ok(WalletType::Loan),
            "e_wallet"   => Ok(WalletType::EWallet),
            "investment" => Ok(WalletType::Investment),
            _            => Err(AppError::Parse(format!("Unknown wallet type: {s}"))),
        }
    }
}

// ─── Wallet ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wallet {
    pub id:          String,
    pub name:        String,
    pub wallet_type: WalletType,
    pub broker:      Option<String>,
    pub icon:        Option<String>,
    pub color:       Option<String>,
    pub is_active:   bool,
    pub sort_order:  i32,
    pub created_at:  i64,
    /// Balance per money type, in money-type sort order (filled by WalletRepo)
    #[serde(default)]
    pub balances:    Vec<WalletBalance>,
}

/// Balance of a wallet in one money type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WalletBalance {
    pub currency: Currency,
    pub balance:  Decimal,
}

impl Wallet {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let is_active_i: i32 = row.get("is_active")?;
        let wt: String = row.get("wallet_type")?;

        Ok(Wallet {
            id:          row.get("id")?,
            name:        row.get("name")?,
            wallet_type: WalletType::from_str(&wt).map_err(to_sqlite_err)?,
            broker:      row.get("broker")?,
            icon:        row.get("icon")?,
            color:       row.get("color")?,
            is_active:   is_active_i != 0,
            sort_order:  row.get("sort_order")?,
            created_at:  row.get("created_at")?,
            balances:    vec![],
        })
    }

    /// Balance in one money type (0 if the wallet never held it)
    pub fn balance(&self, currency: &Currency) -> Decimal {
        self.balances.iter().find(|b| &b.currency == currency).map(|b| b.balance).unwrap_or(Decimal::ZERO)
    }

    /// The non-zero balances; if there are none, one zero entry in the wallet's first
    /// money type (VND when it has no balance rows at all)
    pub fn display_balances(&self) -> Vec<WalletBalance> {
        let non_zero: Vec<WalletBalance> = self.balances.iter().filter(|b| !b.balance.is_zero()).cloned().collect();
        if !non_zero.is_empty() {
            return non_zero;
        }
        let currency = self.balances.first().map(|b| b.currency.clone()).unwrap_or(Currency::VND);
        vec![WalletBalance { currency, balance: Decimal::ZERO }]
    }
}

// ─── NewWallet (for inserts) ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWallet {
    pub name:        String,
    pub wallet_type: WalletType,
    pub broker:      Option<String>,
    pub icon:        Option<String>,
    pub color:       Option<String>,
    pub sort_order:  i32,
}

// ─── Helper ───────────────────────────────────────────────────────────────────

pub(crate) fn to_sqlite_err(e: AppError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())),
    )
}
