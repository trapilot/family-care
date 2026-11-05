use dioxus::prelude::*;
use crate::i18n::use_i18n;
use chrono::Datelike;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

use crate::app::{ActiveTab, AppState, SelectedMonth, Tab};
use crate::models::TransactionType;
use crate::views::reports::type_transactions::{TypeDrill, TypeTransactions};
use crate::services::report_service::ExpenseByCategoryItem;
use crate::services::{ReminderService, ReportService};
use crate::views::fmt_vnd;
use crate::views::icons::{Icon, IconName};
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
    // Transaction list opened by tapping a number (income / expense / category)
    let mut drill = use_signal(|| None::<TypeDrill>);
    let ActiveTab(mut active_tab) = use_context::<ActiveTab>();
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
    let balance_color = theme::signed_on_platinum(balance);

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

    rsx! {
        if let Some(d) = drill() {
            TypeTransactions { drill: d, on_back: move |_| drill.set(None) }
        }
        // Kept mounted while the list is open, so Back returns to the same scroll position / state
        div {
            style: if drill.read().is_some() { "display:none;" } else { "display:flex; flex-direction:column;" },
            AppHeader {
                title: t.t("Overview").to_string(),
                month: Some((year, month)),
                on_month: Some(EventHandler::new(move |ym| selected.set(ym))),
            }

            div {
                style: "padding:16px; display:flex; flex-direction:column; gap:16px;",

                // ── Hero money box ──
                div {
                    class: "money-box",
                    style: "{theme::PLATINUM_BG} color:{theme::TEXT}; padding:16px;",
                    div {
                        style: "cursor:pointer;",
                        onclick: move |_| active_tab.set(Tab::Transactions),
                        div { style: "font-size:13px; font-weight:600; color:{theme::MUTED_ON_PLATINUM};", {t.t("Balance")} }
                        div {
                            style: "font-size:28px; font-weight:700; overflow-wrap:anywhere; color:{balance_color};",
                            "{fmt_vnd(balance)}"
                        }
                    }
                    div { style: "border-top:1px solid rgba(31,33,51,.15); margin:14px 0 12px;" }
                    div {
                        style: "display:grid; grid-template-columns:1fr 1fr; gap:12px;",
                        div {
                            style: "cursor:pointer;",
                            onclick: move |_| drill.set(Some(TypeDrill::month(TransactionType::Income, year, month))),
                            div {
                                style: "font-size:12px; font-weight:600; color:{theme::MUTED_ON_PLATINUM}; display:flex; align-items:center; gap:2px;",
                                {t.t("Income")}
                                Icon { name: IconName::ChevronRight, size: 14, color: theme::MUTED_ON_PLATINUM.to_string() }
                            }
                            div { style: "font-size:17px; font-weight:700; color:{theme::signed_on_platinum(s.income)}; overflow-wrap:anywhere;", "{fmt_vnd(s.income)}" }
                        }
                        div {
                            style: "text-align:right; cursor:pointer;",
                            onclick: move |_| drill.set(Some(TypeDrill::month(TransactionType::Expense, year, month))),
                            div {
                                style: "font-size:12px; font-weight:600; color:{theme::MUTED_ON_PLATINUM}; display:flex; align-items:center; justify-content:flex-end; gap:2px;",
                                {t.t("Expense")}
                                Icon { name: IconName::ChevronRight, size: 14, color: theme::MUTED_ON_PLATINUM.to_string() }
                            }
                            div { style: "font-size:17px; font-weight:700; color:{theme::signed_on_platinum(-s.expense)}; overflow-wrap:anywhere;", "{fmt_vnd(s.expense)}" }
                        }
                    }
                    div {
                        style: "margin-top:12px; background:rgba(255,255,255,.5); color:{theme::TEXT}; padding:10px 12px; display:flex; justify-content:space-between; align-items:center; gap:8px; cursor:pointer;",
                        onclick: move |_| drill.set(Some(TypeDrill::month(TransactionType::Expense, year, month))),
                        span { style: "font-size:13px; font-weight:600;", {t.t("Average day")} }
                        span {
                            style: if day_avg.is_zero() { "font-size:16px; font-weight:700; opacity:.6;" } else { "font-size:16px; font-weight:700;" },
                            "{fmt_vnd(day_avg)}"
                        }
                    }
                }

                // ── Reminders due today ──
                if !due_today.read().is_empty() || !unpaid_bills.read().is_empty() {
                    div {
                        style: "background:#fff; border:1px solid {theme::BORDER}; border-left:3px solid {theme::PRIMARY}; padding:12px 14px; display:flex; flex-direction:column;",
                        h2 {
                            style: "margin:0; font-size:14px; font-weight:600; color:{theme::PRIMARY}; display:flex; align-items:center; gap:6px; padding-bottom:8px;",
                            IconText { text: t.t("🔔 Due today").to_string(), size: 18 }
                        }
                        for (id, title, body, done) in due_today.read().iter().cloned() {
                            div { style: "display:flex; gap:10px; align-items:center; padding:8px 0; border-top:1px solid #e6e6ef;",
                                div { style: if done { "flex:1; min-width:0; opacity:.55;" } else { "flex:1; min-width:0;" },
                                    div { style: "font-size:13px; font-weight:600; color:{theme::TEXT};", IconText { text: title, size: 14 } }
                                    div { style: "font-size:13px; color:{theme::TEXT}; overflow-wrap:anywhere;", IconText { text: body, size: 14 } }
                                }
                                if done {
                                    span { style: "font-size:12px; color:{theme::POSITIVE}; font-weight:600; flex-shrink:0;", IconText { text: t.t("✓ Done").to_string(), size: 14, gap: 4 } }
                                } else {
                                    button {
                                        style: "border:none; background:{theme::PRIMARY}; color:#fff; padding:6px 12px; font-size:12px; font-weight:600; cursor:pointer; flex-shrink:0;",
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
                            div { style: "display:flex; gap:10px; align-items:center; padding:8px 0; border-top:1px solid #e6e6ef;",
                                div { style: "flex:1; min-width:0;",
                                    div { style: "font-size:13px; font-weight:600; color:{theme::NEGATIVE};", IconText { text: title, size: 14 } }
                                    div { style: "font-size:13px; color:{theme::TEXT}; overflow-wrap:anywhere;", IconText { text: body, size: 14 } }
                                }
                                button {
                                    style: "border:none; background:{theme::POSITIVE}; color:#fff; padding:6px 12px; font-size:12px; font-weight:600; cursor:pointer; flex-shrink:0;",
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
                        style: "background:#fff; border:1px solid {theme::BORDER}; border-left:3px solid {theme::NEGATIVE}; padding:12px 14px;",
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

                // ── Top categories ──
                div {
                    style: "display:flex; flex-direction:column; gap:14px;",
                    SectionTitle { title: t.t("Expense").to_string() }
                    div {
                        style: "background:#fff; border:1px solid {theme::BORDER};",
                        if cat_items.is_empty() {
                            div { style: "text-align:center; color:{theme::TEXT_MUTED}; font-size:14px; padding:12px 14px;", {t.t("No expenses this month")} }
                        }
                        for (i, item) in cat_items.iter().enumerate() {
                            div {
                                style: if i == 0 { "padding:8px 14px; cursor:pointer;" } else { "padding:8px 14px; border-top:1px solid #e6e6ef; cursor:pointer;" },
                                onclick: {
                                    let category_id = item.category_id.clone();
                                    move |_| drill.set(Some(TypeDrill::category(category_id.clone(), year, month)))
                                },
                                CategoryRow { item: item.clone() }
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
            style: "display:flex; align-items:center; gap:10px;",
            IconCircle { icon: item.icon.clone(), fallback: IconName::Package, color, size: 30 }
            div { style: "flex:1; min-width:0;",
                div { style: "display:flex; justify-content:space-between; gap:8px; align-items:baseline;",
                    span { style: "font-size:13px; color:{theme::TEXT};", {t.tr(&item.category_name)} }
                    span { style: "font-size:13px; color:{theme::signed_color(-item.amount)}; white-space:nowrap;", "{fmt_vnd(item.amount)}" }
                }
                div { style: "display:flex; align-items:center; gap:8px; margin-top:2px;",
                    span { style: "font-size:11px; color:{theme::TEXT_MUTED}; min-width:36px;", "{pct:.0}%" }
                    div { style: "flex:1; height:3px; background:#eeeef4; overflow:hidden;",
                        div { style: "height:100%; width:{pct}%; background:{bar_color};" }
                    }
                }
            }
        }
    }
}
