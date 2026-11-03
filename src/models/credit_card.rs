use chrono::{Datelike, Duration, Local, Months, NaiveDate, TimeZone};
use rusqlite::Row;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::f64_to_decimal;

// ─── CreditCard ───────────────────────────────────────────────────────────────

/// Statement settings of a credit-card wallet. Amounts are computed in VND only; the
/// statement balance is derived from transaction dates, nothing is moved or stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreditCard {
    pub wallet_id:     String,
    /// 1–31; short months use their last day
    pub statement_day: u32,
    /// 1–31; the first such day after the statement date
    pub due_day:       u32,
    pub credit_limit:  Option<Decimal>,
    /// Month ("YYYY-MM") of the statement marked paid
    pub paid_month:    Option<String>,
    pub created_at:    i64,
}

/// How much is owed on a card, split by the last statement
#[derive(Debug, Clone, PartialEq)]
pub struct CardSummary {
    pub statement_date:    NaiveDate,
    pub due_date:          NaiveDate,
    /// Owed at the end of the statement date
    pub statement_balance: Decimal,
    /// Money put on the card after the statement date
    pub paid_since:        Decimal,
    /// Statement balance not yet covered by `paid_since`
    pub remaining_due:     Decimal,
    /// Spent after the statement date
    pub new_charges:       Decimal,
    /// Everything owed right now
    pub owed_total:        Decimal,
    /// Limit minus `owed_total`; negative when over the limit
    pub available:         Option<Decimal>,
    pub is_paid:           bool,
}

/// The `day` of the month of `any_day`, e.g. day 31 → 30 Nov / 28 Feb
fn day_in_month(day: u32, any_day: NaiveDate) -> NaiveDate {
    let first = any_day.with_day(1).unwrap_or(any_day);
    let last = (first + Months::new(1)).pred_opt().unwrap_or(first);
    first.with_day(day.min(last.day())).unwrap_or(last)
}

impl CreditCard {
    /// Notifications start this many days before the due date
    pub const REMIND_DAYS_BEFORE: i64 = 3;

    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let statement_day: i64     = row.get("statement_day")?;
        let due_day: i64           = row.get("due_day")?;
        let credit_limit: Option<f64> = row.get("credit_limit")?;
        Ok(CreditCard {
            wallet_id:     row.get("wallet_id")?,
            statement_day: statement_day.clamp(1, 31) as u32,
            due_day:       due_day.clamp(1, 31) as u32,
            credit_limit:  credit_limit.map(f64_to_decimal),
            paid_month:    row.get("paid_month")?,
            created_at:    row.get("created_at")?,
        })
    }

    /// Latest statement date on or before `today`
    pub fn last_statement(&self, today: NaiveDate) -> NaiveDate {
        let this_month = day_in_month(self.statement_day, today);
        if today >= this_month {
            this_month
        } else {
            day_in_month(self.statement_day, today - Months::new(1))
        }
    }

    /// Due date of the statement dated `statement`: the first due day strictly after it
    pub fn due_date(&self, statement: NaiveDate) -> NaiveDate {
        let this_month = day_in_month(self.due_day, statement);
        if this_month > statement {
            this_month
        } else {
            day_in_month(self.due_day, statement + Months::new(1))
        }
    }

    /// "YYYY-MM" of the statement open on `today` (the value stored in `paid_month`)
    pub fn cycle_key(&self, today: NaiveDate) -> String {
        self.last_statement(today).format("%Y-%m").to_string()
    }

    pub fn is_paid(&self, today: NaiveDate) -> bool {
        self.paid_month.as_deref() == Some(self.cycle_key(today).as_str())
    }

    pub fn created_day(&self) -> NaiveDate {
        Local.timestamp_opt(self.created_at, 0).single()
            .map(|dt| dt.date_naive())
            .unwrap_or_default()
    }

    /// Whether to notify on `day`: the statement is unpaid with money owed, it closed after the
    /// card was added, and the due date is at most `REMIND_DAYS_BEFORE` days away (or past).
    pub fn reminds_on(&self, day: NaiveDate, owed: Decimal) -> bool {
        let statement = self.last_statement(day);
        !self.is_paid(day)
            && owed > Decimal::ZERO
            && statement >= self.created_day()
            && day >= self.due_date(statement) - Duration::days(Self::REMIND_DAYS_BEFORE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate { NaiveDate::from_ymd_opt(y, m, day).unwrap() }

    fn card(statement_day: u32, due_day: u32, created: NaiveDate) -> CreditCard {
        let ts = Local.from_local_datetime(&created.and_hms_opt(9, 0, 0).unwrap()).single().unwrap().timestamp();
        CreditCard { wallet_id: "w".into(), statement_day, due_day, credit_limit: None, paid_month: None, created_at: ts }
    }

    #[test]
    fn last_statement_before_on_and_after_the_day() {
        let c = card(15, 5, d(2026, 1, 1));
        assert_eq!(c.last_statement(d(2026, 11, 14)), d(2026, 10, 15));
        assert_eq!(c.last_statement(d(2026, 11, 15)), d(2026, 11, 15));
        assert_eq!(c.last_statement(d(2026, 11, 20)), d(2026, 11, 15));
        assert_eq!(c.last_statement(d(2026, 1, 3)), d(2025, 12, 15));
        assert_eq!(c.cycle_key(d(2026, 11, 14)), "2026-10");
    }

    #[test]
    fn day_31_uses_last_day_of_short_months() {
        let c = card(31, 31, d(2026, 1, 1));
        assert_eq!(c.last_statement(d(2026, 2, 28)), d(2026, 2, 28));
        assert_eq!(c.last_statement(d(2026, 3, 15)), d(2026, 2, 28));
        assert_eq!(c.last_statement(d(2026, 4, 30)), d(2026, 4, 30));
        assert_eq!(c.last_statement(d(2026, 5, 10)), d(2026, 4, 30));
        // Statement on the last day of Feb, due "31st" -> 31 Mar
        assert_eq!(c.due_date(d(2026, 2, 28)), d(2026, 3, 31));
    }

    #[test]
    fn due_date_is_the_first_matching_day_after_the_statement() {
        assert_eq!(card(5, 25, d(2026, 1, 1)).due_date(d(2026, 11, 5)), d(2026, 11, 25));
        assert_eq!(card(20, 5, d(2026, 1, 1)).due_date(d(2026, 11, 20)), d(2026, 12, 5));
        assert_eq!(card(10, 10, d(2026, 1, 1)).due_date(d(2026, 11, 10)), d(2026, 12, 10));
    }

    #[test]
    fn reminds_from_three_days_before_due_until_paid() {
        let owed = Decimal::from(1_000_000);
        let mut c = card(5, 25, d(2026, 10, 1));
        // Statement 5 Nov, due 25 Nov
        assert!(!c.reminds_on(d(2026, 11, 21), owed));
        assert!(c.reminds_on(d(2026, 11, 22), owed));
        assert!(c.reminds_on(d(2026, 11, 30), owed), "still unpaid after the due date");
        assert!(!c.reminds_on(d(2026, 11, 22), Decimal::ZERO));

        c.paid_month = Some("2026-11".into());
        assert!(c.is_paid(d(2026, 12, 4)));
        assert!(!c.reminds_on(d(2026, 11, 22), owed));
        assert!(!c.reminds_on(d(2026, 11, 30), owed));
    }

    #[test]
    fn no_reminder_for_a_statement_before_the_card_was_added() {
        let owed = Decimal::from(1_000_000);
        // Added 10 Nov: the 5 Nov statement is not tracked, the 5 Dec one is
        let c = card(5, 25, d(2026, 11, 10));
        assert!(!c.reminds_on(d(2026, 11, 24), owed));
        assert!(!c.reminds_on(d(2026, 12, 4), owed));
        assert!(c.reminds_on(d(2026, 12, 22), owed));
    }
}
