use dioxus::prelude::*;
use crate::i18n::use_i18n;
use rust_decimal::Decimal;

use crate::app::AppState;
use crate::models::{CurrencyInfo, Transaction};
use crate::repository::{CurrencyRepo, TransactionRepo};
use crate::services::{ReminderService, ReportService};
use crate::views::rates::ConvertedTotal;
use crate::views::{currency_color, currency_name, current_month, fmt_currency, fmt_vnd, month_label, shown_totals, ts_to_date_str};

// ─── Dashboard ────────────────────────────────────────────────────────────────

#[component]
pub fn Dashboard() -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let (year, month) = current_month();

    // ── Data signals ──
    let net_worth = use_signal(|| crate::services::report_service::NetWorth {
        vnd: Decimal::ZERO,
        usd: Decimal::ZERO,
        gold: Decimal::ZERO,
    });
    let summary   = use_signal(|| crate::services::report_service::MonthlySummary {
        year, month,
        income: Decimal::ZERO, expense: Decimal::ZERO,
        savings: Decimal::ZERO, savings_rate: Decimal::ZERO,
        net_transfers: Decimal::ZERO,
    });
    let money_types = use_signal(Vec::<CurrencyInfo>::new);
    let alerts    = use_signal(|| vec![]);
    let recent    = use_signal(|| vec![]);
    // Wallet names by id, for the opening-balance rows of Recent
    let wallet_names = use_signal(std::collections::HashMap::<String, String>::new);
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

    // ── Load on mount ──
    use_effect({
        let state = state.clone();
        let mut net_worth = net_worth.clone();
        let mut summary   = summary.clone();
        let mut money_types = money_types;
        let mut alerts    = alerts.clone();
        let mut recent    = recent.clone();
        let mut wallet_names = wallet_names;
        let mut load_due = load_due.clone();
        move || {
            load_due(t.lang());
            state.with_conn(|conn| {
                if let Ok(list) = CurrencyRepo::list(conn) {
                    money_types.set(list);
                }
                if let Ok(nw) = ReportService::net_worth(conn) {
                    net_worth.set(nw);
                }
                if let Ok(s) = ReportService::monthly_summary(conn, year, month) {
                    summary.set(s);
                }
                if let Ok(a) = ReportService::overspending_alerts(conn, year, month) {
                    alerts.set(a);
                }
                if let Ok(txns) = TransactionRepo::list(conn, &crate::repository::transaction_repo::TxnFilter {
                    limit: Some(5),
                    skip_zero_opening: true,
                    ..Default::default()
                }) {
                    recent.set(txns);
                }
                if let Ok(ws) = crate::repository::WalletRepo::list_active(conn) {
                    wallet_names.set(ws.into_iter().map(|w| (w.id, w.name)).collect());
                }
            });
        }
    });

    let s = summary.read();
    let worth_cards = shown_totals(&money_types.read(), net_worth.read().vnd, net_worth.read().usd, net_worth.read().gold);

    rsx! {
        div {
            style: "padding:16px; display:flex; flex-direction:column; gap:16px;",

            // ── Header ──
            div {
                style: "display:flex; justify-content:space-between; align-items:center;",
                h1 { style: "margin:0; font-size:20px; font-weight:700; color:#1f2937;", "Family Finance" }
                span { style: "font-size:13px; color:#6b7280;", "{month_label(year, month)}" }
            }

            // ── Net Worth Cards ──
            div {
                style: "display:grid; grid-template-columns:repeat({worth_cards.len()},1fr); gap:10px;",
                for (cur, total) in worth_cards.iter() {
                    NetWorthCard {
                        label: currency_name(&t, &money_types.read(), cur),
                        value: fmt_currency(*total, cur),
                        color: currency_color(cur),
                    }
                }
            }
            ConvertedTotal { vnd: net_worth.read().vnd, usd: net_worth.read().usd, gold: net_worth.read().gold }

            // ── Reminders due today ──
            if !due_today.read().is_empty() || !unpaid_bills.read().is_empty() {
                div {
                    style: "background:#eef2ff; border:1px solid #c7d2fe; border-radius:12px; padding:14px 16px; display:flex; flex-direction:column; gap:8px;",
                    h2 { style: "margin:0; font-size:15px; font-weight:600; color:#4338ca;", {t.t("🔔 Due today")} }
                    for (id, title, body, done) in due_today.read().iter().cloned() {
                        div { style: "display:flex; gap:10px; align-items:center;",
                            div { style: if done { "flex:1; min-width:0; opacity:.55;" } else { "flex:1; min-width:0;" },
                                div { style: "font-size:13px; font-weight:600; color:#1f2937;", "{title}" }
                                div { style: "font-size:13px; color:#374151; overflow-wrap:anywhere;", "{body}" }
                            }
                            if done {
                                span { style: "font-size:12px; color:#10b981; font-weight:600; flex-shrink:0;", {t.t("✓ Done")} }
                            } else {
                                button {
                                    style: "border:none; background:#6366f1; color:#fff; border-radius:8px; padding:6px 10px; font-size:12px; font-weight:600; cursor:pointer; flex-shrink:0;",
                                    onclick: {
                                        let state = state.clone();
                                        let mut load_due = load_due.clone();
                                        move |_| {
                                            let today = chrono::Local::now().date_naive();
                                            let _ = state.with_conn(|conn| ReminderService::mark_done(conn, &id, today));
                                            load_due(t.lang());
                                        }
                                    },
                                    {t.t("✓ Done for today")}
                                }
                            }
                        }
                    }
                    for (id, month, title, body) in unpaid_bills.read().iter().cloned() {
                        div { style: "display:flex; gap:10px; align-items:center;",
                            div { style: "flex:1; min-width:0;",
                                div { style: "font-size:13px; font-weight:600; color:#b91c1c;", "{title}" }
                                div { style: "font-size:13px; color:#374151; overflow-wrap:anywhere;", "{body}" }
                            }
                            button {
                                style: "border:none; background:#10b981; color:#fff; border-radius:8px; padding:6px 10px; font-size:12px; font-weight:600; cursor:pointer; flex-shrink:0;",
                                onclick: {
                                    let state = state.clone();
                                    let mut load_due = load_due.clone();
                                    move |_| {
                                        let _ = state.with_conn(|conn| ReminderService::mark_bill_paid(conn, &id, &month));
                                        load_due(t.lang());
                                    }
                                },
                                {t.t("✓ Paid")}
                            }
                        }
                    }
                }
            }

            // ── Monthly Summary ──
            div {
                style: "background:#fff; border-radius:12px; padding:16px; box-shadow:0 1px 3px rgba(0,0,0,.08);",
                h2 { style: "margin:0 0 12px; font-size:15px; font-weight:600; color:#374151;", {t.t("This Month")} }
                div {
                    style: "display:grid; grid-template-columns:1fr 1fr; gap:10px;",
                    SummaryRow { label: t.t("Income"),       value: fmt_vnd(s.income),       color: "#10b981" }
                    SummaryRow { label: t.t("Expense"),      value: fmt_vnd(s.expense),      color: "#ef4444" }
                    SummaryRow { label: t.t("Savings"),      value: fmt_vnd(s.savings),      color: "#6366f1" }
                    SummaryRow { label: t.t("Savings Rate"), value: format!("{:.1}%", s.savings_rate), color: "#f59e0b" }
                }
            }

            // ── Overspending Alerts ──
            if !alerts.read().is_empty() {
                div {
                    style: "background:#fff; border-radius:12px; padding:16px; box-shadow:0 1px 3px rgba(0,0,0,.08);",
                    h2 { style: "margin:0 0 10px; font-size:15px; font-weight:600; color:#ef4444;", {t.t("⚠️ Overspending")} }
                    for alert in alerts.read().iter() {
                        div {
                            style: "display:flex; justify-content:space-between; align-items:center; padding:6px 0; border-bottom:1px solid #f3f4f6;",
                            span { style: "font-size:13px; color:#374151;", {t.tr(&alert.category_name)} }
                            div {
                                style: "text-align:right;",
                                div { style: "font-size:12px; color:#ef4444; font-weight:600;", "+{fmt_vnd(alert.overage)}" }
                                div { style: "font-size:11px; color:#9ca3af;", "{fmt_vnd(alert.spent)} / {fmt_vnd(alert.budget)}" }
                            }
                        }
                    }
                }
            }

            // ── Recent Transactions ──
            div {
                style: "background:#fff; border-radius:12px; padding:16px; box-shadow:0 1px 3px rgba(0,0,0,.08);",
                h2 { style: "margin:0 0 10px; font-size:15px; font-weight:600; color:#374151;", {t.t("Recent")} }
                if recent.read().is_empty() {
                    p { style: "font-size:13px; color:#9ca3af; text-align:center; padding:16px 0;", {t.t("No transactions yet")} }
                }
                for txn in recent.read().iter() {
                    RecentTxnRow {
                        txn: txn.clone(),
                        // An opening balance says which wallet it started
                        wallet_name: (txn.txn_type == crate::models::TransactionType::OpeningBalance)
                            .then(|| wallet_names.read().get(&txn.wallet_id).cloned())
                            .flatten(),
                    }
                }
            }
        }
    }
}

// ─── Sub-components ───────────────────────────────────────────────────────────

#[component]
fn NetWorthCard(label: String, value: String, color: &'static str) -> Element {
    rsx! {
        div {
            style: "background:#fff; border-radius:12px; padding:12px; box-shadow:0 1px 3px rgba(0,0,0,.08); border-top:3px solid {color};",
            div { style: "font-size:10px; font-weight:600; color:#9ca3af; text-transform:uppercase; margin-bottom:4px;", "{label}" }
            div { style: "font-size:13px; font-weight:700; color:#1f2937; word-break:break-all;", "{value}" }
        }
    }
}

#[component]
fn SummaryRow(label: &'static str, value: String, color: &'static str) -> Element {
    rsx! {
        div {
            style: "background:#f9fafb; border-radius:8px; padding:10px;",
            div { style: "font-size:11px; color:#6b7280; margin-bottom:2px;", "{label}" }
            div { style: "font-size:14px; font-weight:700; color:{color};", "{value}" }
        }
    }
}

#[component]
fn RecentTxnRow(txn: Transaction, wallet_name: Option<String>) -> Element {
    let t = use_i18n();
    let (icon, color) = match txn.txn_type {
        crate::models::TransactionType::Income    => ("↑", "#10b981"),
        crate::models::TransactionType::Expense   => ("↓", "#ef4444"),
        crate::models::TransactionType::Transfer  => ("⇄", "#6366f1"),
        crate::models::TransactionType::InvestBuy  => ("📈", "#f59e0b"),
        crate::models::TransactionType::InvestSell => ("📉", "#8b5cf6"),
        crate::models::TransactionType::OpeningBalance => ("🏁", "#0ea5e9"),
    };
    let amount_str = fmt_currency(txn.amount, &txn.currency);
    let date_str   = ts_to_date_str(txn.txn_date);
    let title = match wallet_name {
        Some(name) => format!("{} · {name}", t.code(&txn.txn_type.to_string())),
        None       => t.code(&txn.txn_type.to_string()),
    };

    rsx! {
        div {
            style: "display:flex; justify-content:space-between; align-items:center; padding:8px 0; border-bottom:1px solid #f3f4f6;",
            div {
                style: "display:flex; align-items:center; gap:10px;",
                span {
                    style: "width:32px; height:32px; border-radius:50%; background:#f3f4f6; display:flex; align-items:center; justify-content:center; font-size:16px;",
                    "{icon}"
                }
                div {
                    div { style: "font-size:13px; font-weight:500; color:#374151;", "{title}" }
                    div { style: "font-size:11px; color:#9ca3af;", "{date_str}" }
                }
            }
            span { style: "font-size:13px; font-weight:600; color:{color};", "{amount_str}" }
        }
    }
}
