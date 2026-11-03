use chrono::{Datelike, Local};
use rust_decimal::Decimal;
use dioxus::prelude::*;

use crate::app::AppState;
use crate::i18n::use_i18n;
use crate::models::{Bill, NewBill, Reminder, Wallet};
use crate::repository::settings_repo::parse_hh_mm;
use crate::repository::{BillRepo, ReminderRepo, SettingsRepo, WalletRepo};
use crate::services::ReminderService;
use crate::views::icons::{Icon, IconName};
use crate::views::theme;
use crate::views::ui::IconText;
use crate::views::{fmt_vnd, repeat_unit_label, ts_to_date_str, AmountInput};

#[component]
pub fn RemindersSettings() -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();

    let mut reminders = use_signal(Vec::<Reminder>::new);
    let mut wallets   = use_signal(Vec::<Wallet>::new);
    let mut interval  = use_signal(|| "4".to_string());
    let mut start     = use_signal(|| "08:00".to_string());

    let reload = {
        let state = state.clone();
        move || {
            state.with_conn(|conn| {
                reminders.set(ReminderRepo::list(conn).unwrap_or_default());
                wallets.set(WalletRepo::list_active(conn).unwrap_or_default());
                interval.set(SettingsRepo::get_reminder_interval_hours(conn).unwrap_or(4).to_string());
                let (h, m) = SettingsRepo::get_reminder_start_time(conn).unwrap_or((8, 0));
                start.set(format!("{h:02}:{m:02}"));
            });
        }
    };
    use_hook({
        let mut reload = reload.clone();
        move || reload()
    });

    let save_interval = {
        let state = state.clone();
        let mut reload = reload.clone();
        move |_| {
            let hours = interval.read().parse::<u32>().unwrap_or(4);
            let _ = state.with_conn(|conn| {
                SettingsRepo::set_reminder_interval_hours(conn, hours)?;
                ReminderService::resync(conn)
            });
            reload();
        }
    };

    let save_start = {
        let state = state.clone();
        let mut reload = reload.clone();
        move |e: FormEvent| {
            if let Some((h, m)) = parse_hh_mm(&e.value()) {
                let _ = state.with_conn(|conn| {
                    SettingsRepo::set_reminder_start_time(conn, h, m)?;
                    ReminderService::resync(conn)
                });
            }
            reload();
        }
    };

    let today = Local::now().date_naive();

    rsx! {
        div { style: "display:flex; flex-direction:column; gap:16px;",

            // ── Interval ──
            div {
                style: "background:#fff; border-radius:0; padding:16px;",
                h2 { style: "margin:0 0 6px; font-size:14px; font-weight:600; color:{theme::TEXT};", {t.t("Notification frequency")} }
                p { style: "margin:0 0 12px; font-size:13px; color:{theme::TEXT_MUTED}; line-height:1.5;",
                    {t.t("On a day with due income, transfers or unpaid bills, notify from the first notification time, then every N hours until midnight.")}
                }
                div { style: "display:flex; align-items:center; gap:8px; font-size:14px; color:{theme::TEXT}; margin-bottom:10px;",
                    {t.t("First notification at")}
                    input {
                        style: "{theme::INPUT} width:auto;",
                        r#type: "time",
                        value: "{start.read()}",
                        oninput: move |e| start.set(e.value()),
                        onchange: save_start,
                    }
                }
                div { style: "display:flex; align-items:center; gap:8px; font-size:14px; color:{theme::TEXT};",
                    {t.t("Every")}
                    input {
                        style: "{theme::INPUT} width:64px; text-align:center;",
                        r#type: "text",
                        inputmode: "numeric",
                        value: "{interval.read()}",
                        oninput: move |e| interval.set(e.value().chars().filter(|c| c.is_ascii_digit()).take(2).collect()),
                        onchange: save_interval,
                    }
                    {t.t("hours (1–24)")}
                }
            }

            BillsSettings {}

            // ── List ──
            div {
                style: "background:#fff; border-radius:0; padding:16px; display:flex; flex-direction:column; gap:12px;",
                h2 { style: "margin:0; font-size:14px; font-weight:600; color:{theme::TEXT};", {t.t("Recurring reminders")} }
                if reminders.read().is_empty() {
                    p { style: "margin:0; font-size:13px; color:{theme::TEXT_MUTED}; line-height:1.5;",
                        {t.t("No reminders yet. Turn on \"Remind me when this repeats\" when adding an Income or Transfer.")}
                    }
                }
                for reminder in reminders.read().iter().cloned() {
                    {
                        let (title, body) = ReminderService::describe(&reminder, &wallets.read(), t.lang());
                        let schedule = t.t("Every {n} {unit} · from {date}")
                            .replace("{n}", &reminder.repeat_every.to_string())
                            .replace("{unit}", t.t(repeat_unit_label(reminder.repeat_unit)))
                            .replace("{date}", &ts_to_date_str(reminder.start_date));
                        let next = reminder.next_due(today)
                            .map(crate::views::fmt_date)
                            .unwrap_or_else(|| "—".into());
                        let paused = reminder.is_paused;
                        let state_pause = state.clone();
                        let mut reload_pause = reload.clone();
                        let id_pause = reminder.id.clone();
                        let state = state.clone();
                        let mut reload = reload.clone();
                        let id = reminder.id.clone();
                        rsx! {
                            div {
                                key: "{reminder.id}",
                                style: "display:flex; gap:10px; align-items:flex-start; border-top:1px solid #e6e6ef; padding-top:12px;",
                                div { style: "flex:1; min-width:0;",
                                    div { style: "font-size:14px; font-weight:600; color:{theme::TEXT};", IconText { text: title.clone(), size: 14 } }
                                    div { style: "font-size:13px; color:{theme::TEXT}; overflow-wrap:anywhere;", IconText { text: body.clone(), size: 14 } }
                                    div { style: "font-size:12px; color:{theme::TEXT_MUTED}; margin-top:2px;", "{schedule}" }
                                    if paused {
                                        div { style: "font-size:12px; color:#f59e0b; margin-top:2px; font-weight:600;", IconText { text: t.t("⏸ Paused — no notifications").to_string(), size: 14, gap: 4 } }
                                    } else {
                                        div { style: "font-size:12px; color:{theme::PRIMARY}; margin-top:2px;", {t.t("Next")} ": {next}" }
                                    }
                                }
                                button {
                                    style: "border:none; background:{theme::BG}; color:{theme::TEXT}; border-radius:999px; padding:6px 12px; font-size:13px; cursor:pointer; flex-shrink:0;",
                                    onclick: move |_| {
                                        let _ = state_pause.with_conn(|conn| {
                                            ReminderRepo::set_paused(conn, &id_pause, !paused)?;
                                            ReminderService::resync(conn)
                                        });
                                        reload_pause();
                                    },
                                    if paused {
                                        IconText { text: t.t("▶ Resume").to_string(), size: 14, gap: 4 }
                                    } else {
                                        IconText { text: t.t("⏸ Pause").to_string(), size: 14, gap: 4 }
                                    }
                                }
                                button {
                                    style: "border:none; background:{theme::NEGATIVE_SOFT}; color:{theme::DANGER}; border-radius:999px; padding:6px 12px; font-size:13px; cursor:pointer; flex-shrink:0;",
                                    onclick: move |_| {
                                        let _ = state.with_conn(|conn| {
                                            ReminderRepo::delete(conn, &id)?;
                                            ReminderService::resync(conn)
                                        });
                                        reload();
                                    },
                                    {t.t("Delete")}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── Monthly bills ────────────────────────────────────────────────────────────

const INPUT_STYLE: &str = "border:1px solid #e6e6ef; border-radius:0; padding:10px 12px; font-size:15px; outline:none; background:#fff; color:#1f2133;";

/// Bills paid every month: notify from the due day, every day, until marked paid for that month
#[component]
pub fn BillsSettings() -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();

    let mut bills   = use_signal(Vec::<Bill>::new);
    let mut name    = use_signal(String::new);
    let mut amount  = use_signal(String::new);
    let mut due_day = use_signal(|| Local::now().day().to_string());
    let mut error   = use_signal(String::new);
    // Bill being edited in the form; None = the form adds a new bill
    let mut editing = use_signal(|| None::<Bill>);

    let reload = {
        let state = state.clone();
        move || bills.set(state.with_conn(|conn| BillRepo::list(conn).unwrap_or_default()))
    };
    use_hook({
        let mut reload = reload.clone();
        move || reload()
    });

    // Change a bill, then reschedule notifications and refresh the list
    let update = {
        let state = state.clone();
        let mut reload = reload.clone();
        move |f: Box<dyn FnOnce(&rusqlite::Connection) -> crate::error::Result<()>>| {
            let _ = state.with_conn(|conn| {
                f(conn)?;
                ReminderService::resync(conn)
            });
            reload();
        }
    };

    let mut reset_form = move || {
        name.set(String::new());
        amount.set(String::new());
        due_day.set(Local::now().day().to_string());
        error.set(String::new());
        editing.set(None);
    };

    let save = {
        let mut update = update.clone();
        move |_| {
            let bill_name = name.read().trim().to_string();
            if bill_name.is_empty() {
                error.set(t.t("Enter the bill name").into());
                return;
            }
            let Some(day) = due_day.read().parse::<u32>().ok().filter(|d| (1..=31).contains(d)) else {
                error.set(t.t("Day must be 1–31").into());
                return;
            };
            let input = NewBill {
                name:    bill_name,
                amount:  amount.read().parse::<Decimal>().ok().filter(|a| !a.is_zero()),
                due_day: day,
            };
            match editing() {
                Some(old) => {
                    let bill = old.edited(&input, Local::now().date_naive());
                    update(Box::new(move |conn| BillRepo::update(conn, &bill)));
                }
                None => update(Box::new(move |conn| BillRepo::create(conn, &input))),
            }
            reset_form();
        }
    };

    let today = Local::now().date_naive();

    rsx! {
        div {
            style: "background:#fff; border-radius:0; padding:16px; display:flex; flex-direction:column; gap:12px;",
            h2 { style: "margin:0; font-size:14px; font-weight:600; color:{theme::TEXT};", {t.t("Monthly bills")} }
            p { style: "margin:0; font-size:13px; color:{theme::TEXT_MUTED}; line-height:1.5;",
                {t.t("From the due day, notify every day until you mark the bill paid for that month. Next month it starts again.")}
            }

            // ── Add / edit ──
            div {
                id: "bill-form",
                style: if editing.read().is_some() {
                    "display:flex; flex-direction:column; gap:8px; background:#e2e5f7; border-radius:0; padding:12px;"
                } else {
                    "display:flex; flex-direction:column; gap:8px; background:#f7f7fc; border-radius:0; padding:12px;"
                },
                if let Some(b) = editing.read().as_ref() {
                    div { style: "font-size:13px; font-weight:600; color:#3f51b5;", IconText { text: format!("{} {}", t.t("✏️ Editing"), b.name), size: 14, gap: 4 } }
                }
                input {
                    style: INPUT_STYLE,
                    r#type: "text",
                    placeholder: t.t("Bill name (e.g. Electricity)"),
                    value: "{name.read()}",
                    oninput: move |e| name.set(e.value()),
                }
                AmountInput {
                    value: amount.read().clone(),
                    oninput: move |v| amount.set(v),
                    placeholder: t.t("Amount (optional)").to_string(),
                    style: INPUT_STYLE.to_string(),
                }
                div { style: "display:flex; align-items:center; gap:8px; font-size:14px; color:{theme::TEXT};",
                    {t.t("Due on day")}
                    input {
                        style: "width:64px; {INPUT_STYLE} text-align:center;",
                        r#type: "text",
                        inputmode: "numeric",
                        value: "{due_day.read()}",
                        oninput: move |e| due_day.set(e.value().chars().filter(|c| c.is_ascii_digit()).take(2).collect()),
                    }
                    {t.t("of every month")}
                }
                if !error.read().is_empty() {
                    div { style: "font-size:12px; color:{theme::DANGER};", "{error}" }
                }
                div { style: "display:flex; gap:8px;",
                    if editing.read().is_some() {
                        button {
                            style: "{theme::BTN_SECONDARY} flex:1;",
                            onclick: move |_| reset_form(),
                            {t.t("Cancel")}
                        }
                    }
                    button {
                        style: "{theme::BTN_PRIMARY} flex:2;",
                        onclick: save,
                        if editing.read().is_some() { {t.t("Save")} } else { {t.t("+ Add bill")} }
                    }
                }
            }

            // ── List ──
            for bill in bills.read().iter().cloned() {
                {
                    let unpaid = bill.is_unpaid_on(today);
                    let paid_now = !bill.is_paused && !unpaid && bill.paid_month.as_deref() == Some(bill.cycle_key(today).as_str());
                    let status = if bill.is_paused {
                        t.t("⏸ Paused — no notifications").to_string()
                    } else if unpaid {
                        t.t("🔔 Unpaid since {date}").replace("{date}", &bill.cycle_due(today).format("%d/%m").to_string())
                    } else {
                        let next = bill.next_due(today).map(crate::views::fmt_date).unwrap_or_else(|| "—".into());
                        format!("{}{}: {next}", if paid_now { t.t("✓ Paid this month · ") } else { "" }, t.t("Next"))
                    };
                    let status_color = if bill.is_paused { "#f59e0b" } else if unpaid { theme::DANGER } else { theme::POSITIVE };
                    let schedule = t.t("Day {n} of every month").replace("{n}", &bill.due_day.to_string());
                    let amount_label = bill.amount.map(fmt_vnd);
                    let paused = bill.is_paused;
                    let cycle = bill.cycle_key(today);
                    let (id_paid, id_pause, id_delete) = (bill.id.clone(), bill.id.clone(), bill.id.clone());
                    let bill_edit = bill.clone();
                    let (mut update_paid, mut update_pause, mut update_delete) = (update.clone(), update.clone(), update.clone());
                    rsx! {
                        div {
                            key: "{bill.id}",
                            style: "display:flex; gap:8px; align-items:flex-start; border-top:1px solid #e6e6ef; padding-top:12px;",
                            div { style: "flex:1; min-width:0;",
                                div { style: "font-size:15px; font-weight:600; color:{theme::TEXT}; overflow-wrap:anywhere; display:flex; align-items:center; gap:6px;",
                                    Icon { name: IconName::Receipt, size: 16 }
                                    "{bill.name}"
                                }
                                if let Some(a) = amount_label {
                                    div { style: "font-size:13px; color:{theme::TEXT};", "{a}" }
                                }
                                div { style: "font-size:12px; color:{theme::TEXT_MUTED}; margin-top:2px;", "{schedule}" }
                                div { style: "font-size:12px; color:{status_color}; margin-top:2px; font-weight:600;", IconText { text: status, size: 14, gap: 4 } }
                            }
                            div { style: "display:flex; flex-direction:column; gap:6px; flex-shrink:0;",
                                if unpaid {
                                    button {
                                        style: "border:none; background:{theme::POSITIVE}; color:#fff; border-radius:999px; padding:6px 12px; font-size:13px; font-weight:600; cursor:pointer;",
                                        onclick: move |_| {
                                            let (id, cycle) = (id_paid.clone(), cycle.clone());
                                            update_paid(Box::new(move |conn| ReminderService::mark_bill_paid(conn, &id, &cycle)));
                                        },
                                        IconText { text: t.t("✓ Paid").to_string(), size: 14, gap: 4 }
                                    }
                                } else if paid_now {
                                    button {
                                        style: "border:none; background:{theme::BG}; color:{theme::TEXT}; border-radius:999px; padding:6px 12px; font-size:13px; cursor:pointer;",
                                        onclick: move |_| {
                                            let id = id_paid.clone();
                                            update_paid(Box::new(move |conn| BillRepo::set_paid_month(conn, &id, None)));
                                        },
                                        IconText { text: t.t("↩ Not paid").to_string(), size: 14, gap: 4 }
                                    }
                                }
                                button {
                                    style: "border:none; background:{theme::PRIMARY_SOFT}; color:{theme::PRIMARY}; border-radius:999px; padding:6px 12px; font-size:13px; cursor:pointer;",
                                    onclick: move |_| {
                                        name.set(bill_edit.name.clone());
                                        amount.set(bill_edit.amount.map(|a| a.normalize().to_string()).unwrap_or_default());
                                        due_day.set(bill_edit.due_day.to_string());
                                        error.set(String::new());
                                        editing.set(Some(bill_edit.clone()));
                                        // The form is above the list: bring it into view
                                        let _ = document::eval("document.getElementById('bill-form')?.scrollIntoView({behavior:'smooth', block:'center'})");
                                    },
                                    {t.t("Edit")}
                                }
                                button {
                                    style: "border:none; background:{theme::BG}; color:{theme::TEXT}; border-radius:999px; padding:6px 12px; font-size:13px; cursor:pointer;",
                                    onclick: move |_| {
                                        let id = id_pause.clone();
                                        update_pause(Box::new(move |conn| BillRepo::set_paused(conn, &id, !paused)));
                                    },
                                    if paused {
                                        IconText { text: t.t("▶ Resume").to_string(), size: 14, gap: 4 }
                                    } else {
                                        IconText { text: t.t("⏸ Pause").to_string(), size: 14, gap: 4 }
                                    }
                                }
                                button {
                                    style: "border:none; background:{theme::NEGATIVE_SOFT}; color:{theme::DANGER}; border-radius:999px; padding:6px 12px; font-size:13px; cursor:pointer;",
                                    onclick: move |_| {
                                        let id = id_delete.clone();
                                        if editing.read().as_ref().is_some_and(|b| b.id == id) { reset_form(); }
                                        update_delete(Box::new(move |conn| BillRepo::delete(conn, &id)));
                                    },
                                    {t.t("Delete")}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
