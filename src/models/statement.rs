use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Content of `statements.json`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatementFile {
    pub version: u32,
    #[serde(default)]
    pub statements: Vec<Statement>,
}

impl Default for StatementFile {
    fn default() -> Self {
        Self { version: 1, statements: Vec::new() }
    }
}

/// One imported statement page (one photo)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Statement {
    pub id: String,
    pub imported_at: i64,
    #[serde(default)]
    pub account_no: Option<String>,
    #[serde(default)]
    pub account_name: Option<String>,
    #[serde(default)]
    pub period_from: Option<NaiveDate>,
    #[serde(default)]
    pub period_to: Option<NaiveDate>,
    #[serde(default)]
    pub page: Option<String>,
    /// Balance before the first listed row (pages after the first)
    #[serde(default)]
    pub opening_balance: Option<Decimal>,
    #[serde(default)]
    pub transactions: Vec<StatementTxn>,
    /// English keys, translated at display time where possible
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatementTxn {
    #[serde(default)]
    pub date: Option<NaiveDate>,
    #[serde(default)]
    pub counterparty_name: Option<String>,
    #[serde(default)]
    pub counterparty_account: Option<String>,
    #[serde(default)]
    pub counterparty_bank: Option<String>,
    #[serde(default)]
    pub details: String,
    /// First token of the "Transaction No." cell
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub debit: Decimal,
    #[serde(default)]
    pub credit: Decimal,
    #[serde(default)]
    pub fee: Decimal,
    #[serde(default)]
    pub tax: Decimal,
    #[serde(default)]
    pub balance: Option<Decimal>,
    #[serde(default)]
    pub needs_review: bool,
}

impl StatementTxn {
    /// Effect on the own account: credit - debit - fee - tax
    pub fn signed_amount(&self) -> Decimal {
        self.credit - self.debit - self.fee - self.tax
    }
}

/// A person/account the user sent money to or received money from
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CounterpartyAccount {
    pub key: String,
    pub name: Option<String>,
    pub account_no: Option<String>,
    pub bank: Option<String>,
    /// Sum of debit
    pub sent: Decimal,
    /// Sum of credit
    pub received: Decimal,
    pub count: usize,
    /// Date of the most recent transaction with this counterparty
    pub last_date: Option<NaiveDate>,
}
