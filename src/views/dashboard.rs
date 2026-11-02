use dioxus::prelude::*;
use crate::i18n::use_i18n;
use chrono::Datelike;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

use crate::app::{AppState, SelectedMonth};
use crate::services::report_service::ExpenseByCategoryItem;
use crate::services::{ReminderService, ReportService};
use crate::views::fmt_vnd;
use crate::views::icons::IconName;
use crate::views::ui::{AppHeader, IconCircle, IconText, SectionTitle};
use crate::views::theme;

// ─── Dashboard ────────────────────────────────────────────────────────────────

#[component]
pub fn Dashboard() -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let SelectedMonth(mut selected) = use_context::<SelectedMonth>();

    // ── Data signals ──
    let summary   = use_signal(|| crate::services::report_service::MonthlySummary {
        year: 0, month: 0,
        income: Decimal::ZERO, expense: Decimal::ZERO,
        savings: Decimal::ZERO, savings_rate: Decimal::ZERO,
        net_transfers: Decimal::ZERO,
    });
    let alerts    = use_signal(|| vec![]);
    // Expense per category of the selected month
    let by_category = use_signal(Vec::<ExpenseByCategoryItem>::new);
    let mut show_all = use_signal(|| false);
    // Reminders due today, as (id, title, body, done)
    let mut due_today = use_signal(Vec::<(String, String, String, bool)>::new);
    // Unpaid monthly bills, as (id, month, title, body)
    let mut unpaid_bills = use_signal(Vec::<(String, String, String, String)>::new);
    let load_due = {
        let state = state.clone();
        move |lang| {
            let today = chrono::Local::now().date_naive();
            state.with_conn(|conn| {
                if let Ok(bills) = ReminderService::unpaid_bills_on(conn, today) {
                    unpaid_bills.set(bills.iter().map(|b| {
                        let (title, body) = ReminderService::describe_bill(b, today, lang);
                        (b.id.clone(), b.cycle_key(today), title, body)
                    }).collect());
                }
                if let (Ok(due), Ok(wallets)) = (ReminderService::due_on(conn, today), crate::repository::WalletRepo::list_active(conn)) {
                    due_today.set(due.iter().map(|r| {
                        let (title, body) = ReminderService::describe(r, &wallets, lang);
                        (r.id.clone(), title, body, r.is_done_on(today))
                    }).collect());
                }
            });
        }
    };

    // ── Load on mount and whenever the month changes ──
    use_effect({
        let state = state.clone();
        let mut summary   = summary.clone();
        let mut alerts    = alerts.clone();
        let mut by_category = by_category;
        let mut load_due = load_due.clone();
        move || {
            let (year, month) = selected();
            load_due(t.lang());
            state.with_conn(|conn| {
                if let Ok(list) = ReportService::expense_by_category(conn, year, month) {
                    by_category.set(list);
                }
                if let Ok(s) = ReportService::monthly_summary(conn, year, month) {
                    summary.set(s);
                }
                if let Ok(a) = ReportService::overspending_alerts(conn, year, month) {
                    alerts.set(a);
                }
            });
        }
    });

    let (year, month) = selected();
    let s = summary.read().clone();
    let balance = s.income - s.expense;
    let balance_color = if balance >= Decimal::ZERO { theme::POSITIVE } else { theme::NEGATIVE };

    // Average tiles
    let days = {
        let first = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap_or_default();
        let next = if month == 12 { chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1) } else { chrono::NaiveDate::from_ymd_opt(year, month + 1, 1) };
        next.map(|n| (n - first).num_days() as usize).unwrap_or(30)
    };
    let today = chrono::Local::now().date_naive();
    let days_elapsed = if (year, month) == (today.year(), today.month()) {
        today.day() as usize
    } else if (year, month) < (today.year(), today.month()) {
        days
    } else {
        0
    };
    let day_avg = if days_elapsed == 0 { Decimal::ZERO } else { s.expense / Decimal::from(days_elapsed as u64) };

    let cat_items = by_category.read().clone();
    let shown_cats = if show_all() { cat_items.len() } else { 3.min(cat_items.len()) };
    let more_rotate = if show_all() { "rotate(180deg)" } else { "none" };

    rsx! {
        div {
            style: "display:flex; flex-direction:column;",
            AppHeader {
                title: t.t("Overview").to_string(),
                month: Some((year, month)),
                on_month: Some(EventHandler::new(move |ym| selected.set(ym))),
            }

            div {
                style: "padding:16px; display:flex; flex-direction:column; gap:16px;",

                // ── Balance ──
                div {
                    div { style: "text-align:center; font-size:13px; color:{theme::TEXT};", {t.t("Balance")} }
                    div { style: "text-align:center; font-size:19px; font-weight:600; color:{balance_color}; margin-bottom:10px;", "{fmt_vnd(balance)}" }
                    div {
                        style: "display:grid; grid-template-columns:1fr 1fr; gap:8px;",
                        div {
                            style: "background:{theme::NEGATIVE}; color:#fff; border-radius:{theme::CARD_RADIUS}; padding:12px; text-align:center;",
                            div { style: "font-size:13px;", {t.t("Expense")} }
                            div { style: "font-size:19px; font-weight:600; overflow-wrap:anywhere;", "{fmt_vnd(s.expense)}" }
                        }
                        div {
                            style: "background:{theme::POSITIVE_SOFT}; color:{theme::TEXT}; border-radius:{theme::CARD_RADIUS}; padding:12px; text-align:center;",
                            div { style: "font-size:13px;", {t.t("Income")} }
                            div { style: "font-size:19px; font-weight:600; color:{theme::POSITIVE}; overflow-wrap:anywhere;", "{fmt_vnd(s.income)}" }
                        }
                    }
                }

                // ── Averages ──
                div {
                    style: "display:grid; grid-template-columns:repeat(2,1fr); gap:6px;",
                    for (label, value) in [
                        (t.t("Day (avg.)"), day_avg),
                        (t.t("Month"), s.expense),
                    ] {
                        div {
                            style: "background:#fbeef2; border-radius:14px; padding:10px 6px; text-align:center;",
                            div { style: "font-size:12px; color:{theme::TEXT_MUTED};", "{label}" }
                            div {
                                style: if value.is_zero() { "font-size:14px; font-weight:600; color:#8a8d9f; overflow-wrap:anywhere;" } else { "font-size:14px; font-weight:600; color:#1f2133; overflow-wrap:anywhere;" },
                                "{fmt_vnd(value)}"
                            }
                        }
                    }
                }

                // ── Top categories ──
                div {
                    style: "display:flex; flex-direction:column; gap:14px;",
                    SectionTitle { title: t.t("Expense").to_string() }
                    if cat_items.is_empty() {
                        div { style: "text-align:center; color:{theme::TEXT_MUTED}; font-size:14px; padding:8px 0;", {t.t("No expenses this month")} }
                    }
                    for item in cat_items.iter().take(shown_cats) {
                        CategoryRow { item: item.clone() }
                    }
                    if cat_items.len() > 3 {
                        div {
                            style: "display:flex; align-items:center; gap:12px; cursor:pointer;",
                            onclick: move |_| show_all.set(!show_all()),
                            span {
                                style: "display:inline-flex; transform:{more_rotate};",
                                IconCircle { icon: None, fallback: IconName::ChevronDown, color: theme::TEXT_MUTED.to_string(), size: 48 }
                            }
                            span { style: "font-size:15px; color:{theme::TEXT};",
                                if show_all() { {t.t("Less")} } else { {t.t("More…")} }
                            }
                        }
                    }
                }

                // ── Reminders due today ──
                if !due_today.read().is_empty() || !unpaid_bills.read().is_empty() {
                    div {
                        style: "{theme::CARD} display:flex; flex-direction:column; gap:8px;",
                        h2 {
                            style: "margin:0; font-size:14px; font-weight:600; color:{theme::PRIMARY}; display:flex; align-items:center; gap:6px;",
                            IconText { text: t.t("🔔 Due today").to_string(), size: 18 }
                        }
                        for (id, title, body, done) in due_today.read().iter().cloned() {
                            div { style: "display:flex; gap:10px; align-items:center;",
                                div { style: if done { "flex:1; min-width:0; opacity:.55;" } else { "flex:1; min-width:0;" },
                                    div { style: "font-size:13px; font-weight:600; color:{theme::TEXT};", IconText { text: title, size: 14 } }
                                    div { style: "font-size:13px; color:{theme::TEXT}; overflow-wrap:anywhere;", IconText { text: body, size: 14 } }
                                }
                                if done {
                                    span { style: "font-size:12px; color:{theme::POSITIVE}; font-weight:600; flex-shrink:0;", IconText { text: t.t("✓ Done").to_string(), size: 14, gap: 4 } }
                                } else {
                                    button {
                                        style: "border:none; background:{theme::PRIMARY}; color:#fff; border-radius:999px; padding:6px 12px; font-size:12px; font-weight:600; cursor:pointer; flex-shrink:0;",
                                        onclick: {
                                            let state = state.clone();
                                            let mut load_due = load_due.clone();
                                            move |_| {
                                                let today = chrono::Local::now().date_naive();
                                                let _ = state.with_conn(|conn| ReminderService::mark_done(conn, &id, today));
                                                load_due(t.lang());
                                            }
                                        },
                                        IconText { text: t.t("✓ Done for today").to_string(), size: 14, gap: 4 }
                                    }
                                }
                            }
                        }
                        for (id, month, title, body) in unpaid_bills.read().iter().cloned() {
                            div { style: "display:flex; gap:10px; align-items:center;",
                                div { style: "flex:1; min-width:0;",
                                    div { style: "font-size:13px; font-weight:600; color:{theme::NEGATIVE};", IconText { text: title, size: 14 } }
                                    div { style: "font-size:13px; color:{theme::TEXT}; overflow-wrap:anywhere;", IconText { text: body, size: 14 } }
                                }
                                button {
                                    style: "border:none; background:{theme::POSITIVE}; color:#fff; border-radius:999px; padding:6px 12px; font-size:12px; font-weight:600; cursor:pointer; flex-shrink:0;",
                                    onclick: {
                                        let state = state.clone();
                                        let mut load_due = load_due.clone();
                                        move |_| {
                                            let _ = state.with_conn(|conn| ReminderService::mark_bill_paid(conn, &id, &month));
                                            load_due(t.lang());
                                        }
                                    },
                                    IconText { text: t.t("✓ Paid").to_string(), size: 14, gap: 4 }
                                }
                            }
                        }
                    }
                }

                // ── Overspending Alerts ──
                if !alerts.read().is_empty() {
                    div {
                        style: "{theme::CARD}",
                        h2 {
                            style: "margin:0 0 10px; font-size:14px; font-weight:600; color:{theme::NEGATIVE}; display:flex; align-items:center; gap:6px;",
                            IconText { text: t.t("⚠️ Overspending").to_string(), size: 18 }
                        }
                        for alert in alerts.read().iter() {
                            div {
                                style: "display:flex; justify-content:space-between; align-items:center; padding:6px 0; border-bottom:1px solid {theme::BORDER};",
                                span { style: "font-size:13px; color:{theme::TEXT};", {t.tr(&alert.category_name)} }
                                div {
                                    style: "text-align:right;",
                                    div { style: "font-size:12px; color:{theme::NEGATIVE}; font-weight:600;", "+{fmt_vnd(alert.overage)}" }
                                    div { style: "font-size:11px; color:{theme::TEXT_MUTED};", "{fmt_vnd(alert.spent)} / {fmt_vnd(alert.budget)}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── Sub-components ───────────────────────────────────────────────────────────

/// Category with its amount, share of the month and a progress bar
#[component]
pub fn CategoryRow(item: ExpenseByCategoryItem) -> Element {
    let t = use_i18n();
    let pct = item.share_pct.to_f64().unwrap_or(0.0).clamp(0.0, 100.0);
    let color = item.color.clone().unwrap_or_else(|| theme::TEXT_MUTED.to_string());
    let bar_color: String = color.chars().take(7).collect();
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:12px;",
            IconCircle { icon: item.icon.clone(), fallback: IconName::Package, color, size: 48 }
            div { style: "flex:1; min-width:0;",
                div { style: "display:flex; justify-content:space-between; gap:8px; align-items:baseline;",
                    span { style: "font-size:15px; color:{theme::TEXT};", {t.tr(&item.category_name)} }
                    span { style: "font-size:15px; color:{theme::TEXT}; white-space:nowrap;", "{fmt_vnd(item.amount)}" }
                }
                div { style: "display:flex; align-items:center; gap:8px; margin-top:4px;",
                    span { style: "font-size:12px; color:{theme::TEXT_MUTED}; min-width:36px;", "{pct:.0}%" }
                    div { style: "flex:1; height:6px; background:#eeeef4; border-radius:3px; overflow:hidden;",
                        div { style: "height:100%; width:{pct}%; background:{bar_color}; border-radius:3px;" }
                    }
                }
            }
        }
    }
}
