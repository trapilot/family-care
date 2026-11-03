use chrono::{Duration, Local, NaiveDate, TimeZone};
use rusqlite::Connection;
use rust_decimal::Decimal;

use crate::error::{AppError, Result};
use crate::models::{CardSummary, CreditCard, Currency};
use crate::notify;
use crate::repository::{CreditCardRepo, WalletRepo};
use crate::services::ReminderService;

pub struct CreditCardService;

impl CreditCardService {
    /// What is owed on a card as of `today`, in VND. The statement balance is rolled back
    /// from the current balance using the transactions dated after the statement date, so
    /// editing an old transaction re-computes everything; nothing is stored.
    pub fn summary(conn: &Connection, card: &CreditCard, today: NaiveDate) -> Result<CardSummary> {
        let statement = card.last_statement(today);
        // Transactions are stored at local noon, so one dated on the statement day belongs to
        // that statement and only the next local midnight onwards counts as "since"
        let next_day = (statement + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap_or_default();
        let from_ts = Local.from_local_datetime(&next_day).single()
            .or_else(|| Local.from_local_datetime(&next_day).earliest())
            .map(|dt| dt.timestamp())
            .unwrap_or_default();

        let balance = WalletRepo::get_balance(conn, &card.wallet_id, &Currency::VND)?;
        let (inflows, outflows) = CreditCardRepo::flows_since(conn, &card.wallet_id, from_ts)?;

        let balance_at_statement = balance - inflows + outflows;
        let statement_balance = (-balance_at_statement).max(Decimal::ZERO);
        let remaining_due = (statement_balance - inflows).max(Decimal::ZERO);
        let owed_total = (-balance).max(Decimal::ZERO);

        Ok(CardSummary {
            statement_date: statement,
            due_date: card.due_date(statement),
            statement_balance,
            paid_since: inflows,
            remaining_due,
            new_charges: outflows,
            owed_total,
            available: card.credit_limit.map(|limit| limit - owed_total),
            is_paid: card.is_paid(today),
        })
    }

    /// Mark the statement open on `today` paid
    pub fn mark_paid(conn: &Connection, wallet_id: &str, today: NaiveDate) -> Result<()> {
        let card = CreditCardRepo::get(conn, wallet_id)?
            .ok_or_else(|| AppError::NotFound(format!("credit card '{wallet_id}'")))?;
        Self::mark_month_paid(conn, wallet_id, &card.cycle_key(today), today)
    }

    /// Mark the statement `month` ("YYYY-MM", from `CreditCard::cycle_key`) paid and stop its
    /// notifications, including the ones already shown in Notification Center.
    pub fn mark_month_paid(conn: &Connection, wallet_id: &str, month: &str, today: NaiveDate) -> Result<()> {
        CreditCardRepo::set_paid_month(conn, wallet_id, Some(month))?;
        if let Some(card) = CreditCardRepo::get(conn, wallet_id)? {
            let slots = ReminderService::day_slots(conn)?;
            let delivered: Vec<String> = card.last_statement(today).iter_days()
                .take_while(|d| *d <= today)
                .flat_map(|d| slots.iter().map(move |&s| ReminderService::card_notification_id(wallet_id, month, d, s)))
                .collect();
            notify::clear_delivered(&delivered);
        }
        ReminderService::resync(conn)
    }

    pub fn unmark_paid(conn: &Connection, wallet_id: &str) -> Result<()> {
        CreditCardRepo::set_paid_month(conn, wallet_id, None)?;
        ReminderService::resync(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NewTransaction, NewWallet, WalletType};
    use crate::repository::{CategoryRepo, TransactionRepo};

    fn noon(day: NaiveDate) -> i64 {
        Local.from_local_datetime(&day.and_hms_opt(12, 0, 0).unwrap()).single().unwrap().timestamp()
    }

    #[test]
    fn statement_balance_is_rolled_back_from_the_current_balance() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        CategoryRepo::seed_defaults(&conn).unwrap();
        let category = CategoryRepo::list(&conn).unwrap()[0].id.clone();
        let mk = |name: &str, t: WalletType| WalletRepo::create(&conn, &NewWallet {
            name: name.into(), wallet_type: t, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap().id;
        let (card_id, bank) = (mk("Visa", WalletType::CreditCard), mk("Bank", WalletType::Bank));

        let today = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
        let day = |m, d| NaiveDate::from_ymd_opt(2026, m, d).unwrap();
        CreditCardRepo::upsert(&conn, &card_id, 10, 28, Some(Decimal::from(5_000_000))).unwrap();

        // A credit card goes below zero, which `TransactionService::execute` refuses for
        // an expense, so the rows and balances are written directly
        let spend = |amount: i64, date: NaiveDate| {
            TransactionRepo::insert(&conn, &NewTransaction::expense(&card_id, Decimal::from(amount), Currency::VND, &category, noon(date))).unwrap();
            WalletRepo::add_balance(&conn, &card_id, &Currency::VND, -(amount as f64)).unwrap();
        };
        spend(1_000_000, day(11, 9));   // before the 10 Nov statement
        spend(200_000, day(11, 12));    // after it
        // Payment from the bank after the statement
        TransactionRepo::insert(&conn, &NewTransaction::transfer(
            &bank, Decimal::from(300_000), Currency::VND, &card_id, Decimal::from(300_000), Currency::VND, noon(day(11, 15)),
        )).unwrap();
        WalletRepo::add_balance(&conn, &card_id, &Currency::VND, 300_000.0).unwrap();

        let card = CreditCardRepo::get(&conn, &card_id).unwrap().unwrap();
        let s = CreditCardService::summary(&conn, &card, today).unwrap();
        assert_eq!(s.statement_date, day(11, 10));
        assert_eq!(s.due_date, day(11, 28));
        assert_eq!(s.statement_balance, Decimal::from(1_000_000));
        assert_eq!(s.new_charges, Decimal::from(200_000));
        assert_eq!(s.paid_since, Decimal::from(300_000));
        assert_eq!(s.remaining_due, Decimal::from(700_000));
        assert_eq!(s.owed_total, Decimal::from(900_000));
        assert_eq!(s.available, Some(Decimal::from(4_100_000)));
        assert!(!s.is_paid);

        // A charge dated on the statement day itself still belongs to that statement
        spend(50_000, day(11, 10));
        let s = CreditCardService::summary(&conn, &card, today).unwrap();
        assert_eq!(s.statement_balance, Decimal::from(1_050_000));
        assert_eq!(s.new_charges, Decimal::from(200_000));
    }
}
