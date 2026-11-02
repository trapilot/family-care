pub mod ocr;
pub mod parser;
pub mod store;

use std::collections::HashMap;

use rust_decimal::Decimal;

use crate::models::{CounterpartyAccount, Statement, StatementTxn};

/// Bank whose statement layout is used to read photos
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Bank {
    #[default]
    Techcombank,
}

impl Bank {
    pub const ALL: [Bank; 1] = [Bank::Techcombank];

    pub fn code(self) -> &'static str {
        match self {
            Bank::Techcombank => "tcb",
        }
    }

    /// Unknown codes fall back to the default bank
    pub fn from_code(code: &str) -> Self {
        Self::ALL.into_iter().find(|b| b.code() == code).unwrap_or_default()
    }

    pub fn name(self) -> &'static str {
        match self {
            Bank::Techcombank => "Techcombank",
        }
    }

    /// Parse one recognized page with this bank's layout
    pub fn parse(self, page: &ocr::OcrPage) -> crate::error::Result<parser::ParsedPage> {
        match self {
            Bank::Techcombank => parser::parse(page),
        }
    }
}

/// Grouping of imported statement transactions by counterparty account.
pub struct StatementService;

impl StatementService {
    /// Key identifying a transaction across overlapping photos
    /// (own account, date, reference, debit, credit, fee, balance).
    pub(crate) fn unique_key(statement: &Statement, t: &StatementTxn) -> String {
        format!(
            "{:?}|{:?}|{:?}|{}|{}|{}|{:?}",
            statement.account_no,
            t.date,
            t.reference,
            t.debit.normalize(),
            t.credit.normalize(),
            t.fee.normalize(),
            t.balance.map(|b| b.normalize()),
        )
    }

    /// Group key of a transaction's counterparty; `None` when neither name nor account is known.
    pub fn key_of(txn: &StatementTxn) -> Option<String> {
        if let Some(acc) = txn.counterparty_account.as_deref().filter(|a| !a.is_empty()) {
            return Some(format!("acc:{acc}"));
        }
        let name = txn.counterparty_name.as_deref().map(str::trim).filter(|n| !n.is_empty())?;
        let bank = txn
            .counterparty_bank
            .as_deref()
            .and_then(|b| b.split_whitespace().next())
            .unwrap_or("");
        Some(format!("name:{}|{}", name.to_uppercase(), bank.to_uppercase()))
    }

    /// Unique transactions across all statements (overlapping photos are merged).
    fn unique_txns(statements: &[Statement]) -> Vec<(&Statement, &StatementTxn)> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for s in statements {
            for t in &s.transactions {
                if seen.insert(Self::unique_key(s, t)) {
                    out.push((s, t));
                }
            }
        }
        out
    }

    /// Every counterparty across all statements, most recent transfer first.
    pub fn counterparties(statements: &[Statement]) -> Vec<CounterpartyAccount> {
        struct Group {
            names: HashMap<String, usize>,
            account_no: Option<String>,
            bank: Option<String>,
            sent: Decimal,
            received: Decimal,
            count: usize,
            last_date: Option<chrono::NaiveDate>,
        }
        let mut groups: HashMap<String, Group> = HashMap::new();
        for (_, t) in Self::unique_txns(statements) {
            let Some(key) = Self::key_of(t) else { continue };
            let g = groups.entry(key).or_insert_with(|| Group {
                names: HashMap::new(),
                account_no: None,
                bank: None,
                sent: Decimal::ZERO,
                received: Decimal::ZERO,
                count: 0,
                last_date: None,
            });
            if let Some(n) = t.counterparty_name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                *g.names.entry(n.to_string()).or_insert(0) += 1;
            }
            if g.account_no.is_none() {
                g.account_no = t.counterparty_account.clone().filter(|a| !a.is_empty());
            }
            if g.bank.is_none() {
                g.bank = t.counterparty_bank.clone().filter(|b| !b.is_empty());
            }
            g.sent += t.debit;
            g.received += t.credit;
            g.count += 1;
            g.last_date = g.last_date.max(t.date);
        }
        let mut out: Vec<CounterpartyAccount> = groups
            .into_iter()
            .map(|(key, g)| {
                // Most frequent name; ties resolved alphabetically so the result is stable
                let name = g
                    .names
                    .into_iter()
                    .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
                    .map(|(n, _)| n);
                CounterpartyAccount {
                    key,
                    name,
                    account_no: g.account_no,
                    bank: g.bank,
                    sent: g.sent,
                    received: g.received,
                    count: g.count,
                    last_date: g.last_date,
                }
            })
            .collect();
        // Most recent transfer first (undated last), then most frequent
        out.sort_by(|a, b| {
            b.last_date
                .cmp(&a.last_date)
                .then_with(|| b.count.cmp(&a.count))
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.key.cmp(&b.key))
        });
        out
    }

    /// Lowercase and strip Vietnamese diacritics, collapsing whitespace runs.
    fn fold(s: &str) -> String {
        let mapped: String = s
            .to_lowercase()
            .chars()
            .map(|c| match c {
                'à' | 'á' | 'ả' | 'ã' | 'ạ' | 'ă' | 'ằ' | 'ắ' | 'ẳ' | 'ẵ' | 'ặ' | 'â' | 'ầ' | 'ấ' | 'ẩ' | 'ẫ' | 'ậ' => 'a',
                'è' | 'é' | 'ẻ' | 'ẽ' | 'ẹ' | 'ê' | 'ề' | 'ế' | 'ể' | 'ễ' | 'ệ' => 'e',
                'ì' | 'í' | 'ỉ' | 'ĩ' | 'ị' => 'i',
                'ò' | 'ó' | 'ỏ' | 'õ' | 'ọ' | 'ô' | 'ồ' | 'ố' | 'ổ' | 'ỗ' | 'ộ' | 'ơ' | 'ờ' | 'ớ' | 'ở' | 'ỡ' | 'ợ' => 'o',
                'ù' | 'ú' | 'ủ' | 'ũ' | 'ụ' | 'ư' | 'ừ' | 'ứ' | 'ử' | 'ữ' | 'ự' => 'u',
                'ỳ' | 'ý' | 'ỷ' | 'ỹ' | 'ỵ' => 'y',
                'đ' => 'd',
                other => other,
            })
            .collect();
        mapped.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Unique transactions, newest first. Empty `keys` means no account filter; otherwise only
    /// transactions whose counterparty matches any of `keys`. A non-empty `query` further keeps
    /// transactions whose details, reference or counterparty name contain it (case- and
    /// diacritic-insensitive). Each item carries the own account number of its statement.
    pub fn transactions_for(statements: &[Statement], keys: &[String], query: &str) -> Vec<(Option<String>, StatementTxn)> {
        let q = Self::fold(query);
        let mut out: Vec<(Option<String>, StatementTxn)> = Self::unique_txns(statements)
            .into_iter()
            .filter(|(_, t)| keys.is_empty() || Self::key_of(t).is_some_and(|k| keys.contains(&k)))
            .filter(|(_, t)| {
                q.is_empty()
                    || Self::fold(&t.details).contains(&q)
                    || t.reference.as_deref().is_some_and(|r| Self::fold(r).contains(&q))
                    || t.counterparty_name.as_deref().is_some_and(|n| Self::fold(n).contains(&q))
            })
            .map(|(s, t)| (s.account_no.clone(), t.clone()))
            .collect();
        // Stable sort: date desc, undated last
        out.sort_by(|a, b| match (a.1.date, b.1.date) {
            (Some(x), Some(y)) => y.cmp(&x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        out
    }
}
