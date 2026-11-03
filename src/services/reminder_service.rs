use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone};
use rusqlite::Connection;

use crate::error::Result;
use crate::i18n::{translate, Lang};
use crate::models::{Bill, CardSummary, CreditCard, Reminder, TransactionType, Wallet};
use crate::notify::{self, ActionTitles, Category, ScheduledNotification};
use crate::repository::{BillRepo, CreditCardRepo, ReminderRepo, SettingsRepo, WalletRepo};
use crate::services::CreditCardService;
use crate::views::{fmt_currency, fmt_vnd};

/// How far ahead notifications are scheduled (re-synced every time the app opens)
const SCHEDULE_DAYS: i64 = 14;
/// iOS keeps at most 64 pending notifications per app
const MAX_PENDING: usize = 60;

pub struct ReminderService;

impl ReminderService {
    /// Active (not paused) reminders due on `date`, including ones already marked done
    pub fn due_on(conn: &Connection, date: NaiveDate) -> Result<Vec<Reminder>> {
        Ok(ReminderRepo::list(conn)?.into_iter().filter(|r| !r.is_paused && r.is_due(date)).collect())
    }

    /// Notification times of a due day as minutes after midnight:
    /// the configured first time, then every `interval_hours` until midnight.
    pub fn day_slots(conn: &Connection) -> Result<Vec<u32>> {
        let (hour, minute) = SettingsRepo::get_reminder_start_time(conn)?;
        let interval = SettingsRepo::get_reminder_interval_hours(conn)?;
        Ok((hour * 60 + minute..24 * 60).step_by((interval * 60) as usize).collect())
    }

    fn notification_id(reminder_id: &str, day: NaiveDate, slot: u32) -> String {
        format!("reminder:{reminder_id}:{day}:{slot}")
    }

    /// Stop the rest of `day`'s notifications for one reminder ("Done for today").
    pub fn mark_done(conn: &Connection, reminder_id: &str, day: NaiveDate) -> Result<()> {
        ReminderRepo::mark_done(conn, reminder_id, &day.format("%Y-%m-%d").to_string())?;
        // Also clear the ones already shown in Notification Center
        let delivered: Vec<String> = Self::day_slots(conn)?.into_iter()
            .map(|slot| Self::notification_id(reminder_id, day, slot))
            .collect();
        notify::clear_delivered(&delivered);
        Self::resync(conn)
    }

    /// Handle the notification button: "Done for today" (`reminder:<id>:<day>:<slot>`)
    /// or "Paid this month" (`bill:<id>:<YYYY-MM>:<day>:<slot>`).
    pub fn handle_done_action(conn: &Connection, notification_id: &str) -> Result<()> {
        let mut parts = notification_id.split(':');
        match (parts.next(), parts.next(), parts.next()) {
            (Some("reminder"), Some(id), Some(day)) => match NaiveDate::parse_from_str(day, "%Y-%m-%d") {
                Ok(day) => Self::mark_done(conn, id, day),
                Err(_)  => Ok(()),
            },
            (Some("bill"), Some(id), Some(month)) => Self::mark_bill_paid(conn, id, month),
            (Some("card"), Some(id), Some(month)) => {
                CreditCardService::mark_month_paid(conn, id, month, Local::now().date_naive())
            }
            _ => Ok(()),
        }
    }

    // ─── Monthly bills ───

    fn bill_notification_id(bill_id: &str, month: &str, day: NaiveDate, slot: u32) -> String {
        format!("bill:{bill_id}:{month}:{day}:{slot}")
    }

    // ─── Credit cards ───

    pub(crate) fn card_notification_id(wallet_id: &str, month: &str, day: NaiveDate, slot: u32) -> String {
        format!("card:{wallet_id}:{month}:{day}:{slot}")
    }

    /// Notification title and body for a card, e.g. ("💳 Card payment due", "Visa · 1.200.000 ₫ · due 05/11")
    pub fn describe_card(wallet_name: &str, due: NaiveDate, owed: rust_decimal::Decimal, lang: Lang) -> (String, String) {
        let due = due.format("%d/%m");
        let body = format!("{wallet_name} · {} · {} {due}", fmt_vnd(owed), translate(lang, "due"));
        (translate(lang, "💳 Card payment due").to_string(), body)
    }

    /// Bills that notify on `date` (due and not yet paid this month)
    pub fn unpaid_bills_on(conn: &Connection, date: NaiveDate) -> Result<Vec<Bill>> {
        Ok(BillRepo::list(conn)?.into_iter().filter(|b| b.is_unpaid_on(date)).collect())
    }

    /// Stop a bill's notifications until next month's due day ("Paid this month").
    /// `month` is the cycle key ("YYYY-MM") from `Bill::cycle_key`.
    pub fn mark_bill_paid(conn: &Connection, bill_id: &str, month: &str) -> Result<()> {
        BillRepo::set_paid_month(conn, bill_id, Some(month))?;
        // Clear this cycle's notifications already in Notification Center
        if let Some(bill) = BillRepo::list(conn)?.into_iter().find(|b| b.id == bill_id) {
            let today = Local::now().date_naive();
            let slots = Self::day_slots(conn)?;
            let delivered: Vec<String> = bill.cycle_due(today).iter_days()
                .take_while(|d| *d <= today)
                .flat_map(|d| slots.iter().map(move |&s| Self::bill_notification_id(bill_id, month, d, s)))
                .collect();
            notify::clear_delivered(&delivered);
        }
        Self::resync(conn)
    }

    /// Notification title and body for a bill, e.g. ("🧾 Bill to pay", "Electricity · 1.200.000 ₫ · due 05/11")
    pub fn describe_bill(bill: &Bill, today: NaiveDate, lang: Lang) -> (String, String) {
        let mut body = bill.name.clone();
        if let Some(amount) = bill.amount {
            body.push_str(&format!(" · {}", fmt_vnd(amount)));
        }
        let due = bill.cycle_due(today).format("%d/%m");
        body.push_str(&format!(" · {} {due}", translate(lang, "due")));
        (translate(lang, "🧾 Bill to pay").to_string(), body)
    }

    /// Notification title and body for a reminder, e.g. ("💰 Expected income today", "Vietcombank · +15.000.000 ₫ — Salary")
    pub fn describe(reminder: &Reminder, wallets: &[Wallet], lang: Lang) -> (String, String) {
        let wallet_name = |id: &str| wallets.iter().find(|w| w.id == id).map(|w| w.name.clone()).unwrap_or_default();
        let amount = fmt_currency(reminder.amount, &reminder.currency);

        let (title, mut body) = match reminder.txn_type {
            TransactionType::Transfer => {
                let to = reminder.to_wallet_id.as_deref().map(wallet_name).unwrap_or_default();
                (translate(lang, "⇄ Transfer due today"), format!("{} → {to} · {amount}", wallet_name(&reminder.wallet_id)))
            }
            _ => (translate(lang, "💰 Expected income today"), format!("{} · +{amount}", wallet_name(&reminder.wallet_id))),
        };
        if let Some(note) = reminder.note.as_deref().filter(|n| !n.is_empty()) {
            body.push_str(&format!(" — {note}"));
        }
        (title.to_string(), body)
    }

    /// Every notification for the next `SCHEDULE_DAYS` days: on each due day, one per reminder
    /// (and per unpaid bill) at the first notification time and then every `interval_hours`
    /// until midnight. Paused reminders/bills, days marked done and months paid are skipped.
    pub fn build_schedule(conn: &Connection, now: DateTime<Local>) -> Result<Vec<ScheduledNotification>> {
        let reminders = ReminderRepo::list(conn)?;
        let bills     = BillRepo::list(conn)?;
        let cards     = CreditCardRepo::list(conn)?;
        if reminders.is_empty() && bills.is_empty() && cards.is_empty() {
            return Ok(vec![]);
        }
        let wallets  = WalletRepo::list_active(conn)?;
        let slots    = Self::day_slots(conn)?;
        let lang     = SettingsRepo::get_language(conn)?.map(|c| Lang::from_code(&c)).unwrap_or_default();

        let mut items = vec![];
        let today = now.date_naive();
        // One summary per card; days after the next statement fall back to the total owed
        let mut card_summaries: Vec<(&CreditCard, CardSummary)> = vec![];
        for card in &cards {
            card_summaries.push((card, CreditCardService::summary(conn, card, today)?));
        }
        for offset in 0..SCHEDULE_DAYS {
            let day = today + Duration::days(offset);
            let due: Vec<&Reminder> = reminders.iter()
                .filter(|r| !r.is_paused && r.is_due(day) && !r.is_done_on(day))
                .collect();
            let unpaid: Vec<&Bill> = bills.iter().filter(|b| b.is_unpaid_on(day)).collect();
            // (wallet id, cycle key, title, body) of every card to pay on `day`
            let mut card_due: Vec<(&str, String, String, String)> = vec![];
            for (card, summary) in &card_summaries {
                let statement = card.last_statement(day);
                let owed = if statement == summary.statement_date { summary.remaining_due } else { summary.owed_total };
                if !card.reminds_on(day, owed) {
                    continue;
                }
                let name = wallets.iter().find(|w| w.id == card.wallet_id).map(|w| w.name.as_str()).unwrap_or_default();
                let (title, body) = Self::describe_card(name, card.due_date(statement), owed, lang);
                card_due.push((&card.wallet_id, card.cycle_key(day), title, body));
            }
            if due.is_empty() && unpaid.is_empty() && card_due.is_empty() {
                continue;
            }
            for &slot in &slots {
                let Some(at) = day.and_hms_opt(slot / 60, slot % 60, 0).and_then(|t| Local.from_local_datetime(&t).single()) else { continue };
                if at <= now {
                    continue;
                }
                for reminder in &due {
                    let (title, body) = Self::describe(reminder, &wallets, lang);
                    items.push(ScheduledNotification {
                        id: Self::notification_id(&reminder.id, day, slot),
                        title,
                        body,
                        fire_at: at.timestamp(),
                        category: Category::Reminder,
                    });
                }
                for bill in &unpaid {
                    let (title, body) = Self::describe_bill(bill, day, lang);
                    items.push(ScheduledNotification {
                        id: Self::bill_notification_id(&bill.id, &bill.cycle_key(day), day, slot),
                        title,
                        body,
                        fire_at: at.timestamp(),
                        category: Category::Bill,
                    });
                }
                for (wallet_id, month, title, body) in &card_due {
                    items.push(ScheduledNotification {
                        id: Self::card_notification_id(wallet_id, month, day, slot),
                        title: title.clone(),
                        body: body.clone(),
                        fire_at: at.timestamp(),
                        category: Category::Bill,
                    });
                }
            }
        }
        items.sort_by_key(|n| n.fire_at);
        items.truncate(MAX_PENDING);
        Ok(items)
    }

    /// Replace the app's pending system notifications with the current schedule.
    /// Call after reminders, the interval or the language change, and when the app opens.
    pub fn resync(conn: &Connection) -> Result<()> {
        let now = Local::now();
        let items = Self::build_schedule(conn, now)?;
        let lang = SettingsRepo::get_language(conn)?.map(|c| Lang::from_code(&c)).unwrap_or_default();
        notify::replace_all(&items, now.timestamp(), &ActionTitles {
            done_for_today:  translate(lang, "✓ Done for today"),
            paid_this_month: translate(lang, "✓ Paid this month"),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Currency, NewReminder, NewWallet, RepeatUnit, WalletType};
    use crate::repository::CreditCardRepo;
    use rust_decimal::Decimal;

    fn setup(interval: u32) -> (Connection, String) {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let wallet = WalletRepo::create(&conn, &NewWallet {
            name: "Vietcombank".into(), wallet_type: WalletType::Bank, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        SettingsRepo::set_reminder_interval_hours(&conn, interval).unwrap();
        (conn, wallet.id)
    }

    fn add_daily(conn: &Connection, wallet_id: &str, start: DateTime<Local>) {
        ReminderRepo::create(conn, &NewReminder {
            txn_type: TransactionType::Income, wallet_id: wallet_id.into(), to_wallet_id: None,
            amount: Decimal::from(15_000_000), currency: Currency::VND, note: Some("Salary".into()),
            start_date: start.timestamp(), repeat_unit: RepeatUnit::Day, repeat_every: 1,
        }).unwrap();
    }

    fn at(day: NaiveDate, hour: u32) -> DateTime<Local> {
        Local.from_local_datetime(&day.and_hms_opt(hour, 0, 0).unwrap()).single().unwrap()
    }

    #[test]
    fn notifies_from_8am_every_interval_until_midnight() {
        let (conn, wallet) = setup(4);
        let day = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        add_daily(&conn, &wallet, at(day, 12));

        // At 06:00 on the start day: 08, 12, 16, 20 today
        let items = ReminderService::build_schedule(&conn, at(day, 6)).unwrap();
        let today: Vec<i64> = items.iter().filter(|n| n.fire_at < at(day, 23).timestamp()).map(|n| n.fire_at).collect();
        assert_eq!(today, [8, 12, 16, 20].map(|h| at(day, h).timestamp()));
        assert_eq!(items[0].title, "💰 Expected income today");
        assert_eq!(items[0].body, "Vietcombank · +15.000.000 ₫ — Salary");
    }

    #[test]
    fn skips_past_slots_and_caps_at_ios_limit() {
        let (conn, wallet) = setup(1);
        let day = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        add_daily(&conn, &wallet, at(day, 12));

        let now = at(day, 9) + chrono::Duration::minutes(30);
        let items = ReminderService::build_schedule(&conn, now).unwrap();
        assert!(items.iter().all(|n| n.fire_at > now.timestamp()));
        assert_eq!(items.len(), MAX_PENDING);
        assert_eq!(items[0].fire_at, at(day, 10).timestamp());
    }

    #[test]
    fn custom_start_time_done_and_paused() {
        let (conn, wallet) = setup(6);
        SettingsRepo::set_reminder_start_time(&conn, 7, 30).unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        add_daily(&conn, &wallet, at(day, 12));
        let now = at(day, 6);

        let first_day = |items: &[ScheduledNotification]| -> Vec<i64> {
            items.iter().filter(|n| n.fire_at < at(day, 23).timestamp() + 3600).map(|n| n.fire_at).collect()
        };
        let items = ReminderService::build_schedule(&conn, now).unwrap();
        let expect: Vec<i64> = [(7, 30), (13, 30), (19, 30)].iter()
            .map(|(h, m)| (at(day, *h) + chrono::Duration::minutes(*m)).timestamp()).collect();
        assert_eq!(first_day(&items), expect);

        // "Done for today" from the notification stops the rest of that day only
        ReminderService::handle_done_action(&conn, &items[0].id).unwrap();
        let items = ReminderService::build_schedule(&conn, now).unwrap();
        assert!(first_day(&items).is_empty());
        assert!(!items.is_empty(), "next days still notify");

        // Pausing stops everything
        let id = ReminderRepo::list(&conn).unwrap()[0].id.clone();
        ReminderRepo::set_paused(&conn, &id, true).unwrap();
        assert!(ReminderService::build_schedule(&conn, now).unwrap().is_empty());
    }

    #[test]
    fn bill_notifies_daily_from_due_day_until_paid_from_the_notification() {
        let (conn, _) = setup(12);
        let day = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        crate::repository::BillRepo::create(&conn, &crate::models::NewBill {
            name: "Electricity".into(), amount: Some(Decimal::from(1_200_000)), due_day: 5,
        }).unwrap();
        let now = at(day, 6);

        // Bills added "now" only start next cycle; pretend it was added last month
        conn.execute("UPDATE bills SET created_at = ?1", [at(day, 6).timestamp() - 40 * 86_400]).unwrap();

        let items = ReminderService::build_schedule(&conn, now).unwrap();
        // 08:00 and 20:00 every day for the whole 14-day window
        assert_eq!(items.len(), 28);
        assert_eq!(items[0].fire_at, at(day, 8).timestamp());
        assert_eq!(items[0].category, Category::Bill);
        assert_eq!(items[0].title, "🧾 Bill to pay");
        assert_eq!(items[0].body, "Electricity · 1.200.000 ₫ · due 05/11");
        assert!(items[0].id.starts_with("bill:") && items[0].id.contains(":2026-11:"));

        // "Paid this month" from a notification stops it until 5 December
        ReminderService::handle_done_action(&conn, &items[3].id).unwrap();
        assert!(ReminderService::build_schedule(&conn, now).unwrap().is_empty());
        let next = crate::repository::BillRepo::list(&conn).unwrap()[0].next_due(day);
        assert_eq!(next, NaiveDate::from_ymd_opt(2026, 12, 5));
    }

    #[test]
    fn card_notifies_from_three_days_before_due_until_paid() {
        let (conn, _) = setup(12);
        let card = WalletRepo::create(&conn, &NewWallet {
            name: "Visa".into(), wallet_type: WalletType::CreditCard, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap().id;
        // Statement on the 5th, due on the 25th; owes 1.500.000 since an earlier statement
        CreditCardRepo::upsert(&conn, &card, 5, 25, None).unwrap();
        conn.execute("UPDATE credit_cards SET created_at = ?1", [at(NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(), 6).timestamp()]).unwrap();
        WalletRepo::add_balance(&conn, &card, &Currency::VND, -1_500_000.0).unwrap();

        // Today 20 Nov: the 5 Nov statement is due 25 Nov, so notifications start on 22 Nov
        let today = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
        let now = at(today, 6);
        let items = ReminderService::build_schedule(&conn, now).unwrap();
        // 08:00 and 20:00 from 22 Nov to 3 Dec (the window ends 3 Dec)
        assert_eq!(items.len(), 24);
        assert_eq!(items[0].fire_at, at(today + Duration::days(2), 8).timestamp());
        assert_eq!(items[0].category, Category::Bill);
        assert_eq!(items[0].title, "💳 Card payment due");
        assert_eq!(items[0].body, "Visa · 1.500.000 ₫ · due 25/11");
        assert!(items.iter().all(|n| n.id.starts_with(&format!("card:{card}:2026-11:"))));

        // "Paid" from the notification stops them
        ReminderService::handle_done_action(&conn, &items[0].id).unwrap();
        assert!(ReminderService::build_schedule(&conn, now).unwrap().is_empty());
    }

    #[test]
    fn nothing_scheduled_without_reminders() {
        let (conn, _) = setup(4);
        assert!(ReminderService::build_schedule(&conn, Local::now()).unwrap().is_empty());
    }
}
