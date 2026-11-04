use dioxus::prelude::*;
use crate::i18n::use_i18n;

use crate::app::AppState;
use crate::models::{Transaction, Wallet, WalletType};
use crate::repository::transaction_repo::TxnFilter;
use crate::repository::{CategoryRepo, CreditCardRepo, HoldingRepo, TransactionRepo, WalletRepo};
use crate::services::CreditCardService;
use crate::views::ui::{ScreenHeader, TxnItem, UnderlineTabs};
use crate::views::{fmt_currency, fmt_date, fmt_vnd, theme};
use crate::models::{CardSummary, CreditCard};
use chrono::{Local, NaiveDate};
use rust_decimal::Decimal;
use std::collections::HashMap;
use super::{holdings::HoldingsView, WalletView};

#[component]
pub fn WalletDetail(view: Signal<WalletView>, wallet: Wallet) -> Element {
    let t = use_i18n();
    let state    = use_context::<AppState>();
    let txns     = use_signal(|| vec![]);
    let mut sub_view = use_signal(|| DetailTab::History);

    use_effect({
        let state     = state.clone();
        let wallet_id = wallet.id.clone();
        let mut txns  = txns.clone();
        move || {
            state.with_conn(|conn| {
                if let Ok(list) = TransactionRepo::list(conn, &TxnFilter {
                    wallet_id: Some(wallet_id.clone()),
                    limit: Some(50),
                    ..Default::default()
                }) {
                    txns.set(list);
                }
            });
        }
    });

    let balances = wallet.display_balances();
    let is_investment = wallet.wallet_type == WalletType::Investment;

    rsx! {
        div {
            style: "display:flex; flex-direction:column; height:100%;",

            // ── Header ──
            ScreenHeader {
                title: wallet.name.clone(),
                on_back: Some(EventHandler::new(move |_| { view.set(WalletView::List); })),
                action: rsx! {
                    button {
                        style: "{theme::BTN_SECONDARY} padding:8px 16px; font-size:14px; margin-right:8px;",
                        onclick: {
                            let w = wallet.clone();
                            move |_| { view.set(WalletView::Edit(w.clone())); }
                        },
                        {t.t("Edit")}
                    }
                },
            }

            // ── Balance ──
            div {
                style: "padding:16px 16px 0;",
                div {
                    style: "background:{theme::PRIMARY}; border-radius:0; padding:16px; color:#fff;",
                    div {
                        style: "font-size:13px; opacity:.8; margin-bottom:6px;",
                        "{t.t(\"Balance\")} · {t.code(&wallet.wallet_type.to_string())}"
                        if let Some(ref b) = wallet.broker { " · {b}" }
                    }
                    for (i, b) in balances.iter().enumerate() {
                        div {
                            style: if i == 0 { "font-size:22px; font-weight:700;" } else { "font-size:16px; font-weight:600;" },
                            "{fmt_currency(b.balance, &b.currency)}"
                        }
                    }
                }
            }

            if !wallet.is_active {
                div {
                    style: "padding:8px 16px 0; font-size:12px; color:{theme::TEXT_MUTED};",
                    {t.t("This wallet is disabled")}
                }
            }

            if wallet.wallet_type == WalletType::CreditCard {
                CardStatement { wallet_id: wallet.id.clone(), readonly: !wallet.is_active }
            }

            // ── Tabs (investment wallet shows Holdings tab) ──
            if is_investment {
                div {
                    style: "margin-top:8px;",
                    UnderlineTabs {
                        tabs: vec![(t.t("History").to_string(), None), (t.t("Holdings").to_string(), None)],
                        active: if *sub_view.read() == DetailTab::History { 0 } else { 1 },
                        on_select: move |i: usize| {
                            sub_view.set(if i == 0 { DetailTab::History } else { DetailTab::Holdings });
                        },
                    }
                }
            }

            // ── Content ──
            div {
                style: "flex:1; overflow-y:auto; padding:16px;",
                match sub_view.read().clone() {
                    DetailTab::History  => rsx! { TxnHistory { wallet_id: wallet.id.clone(), txns: txns.read().clone() } },
                    DetailTab::Holdings => rsx! { HoldingsView { wallet_id: wallet.id.clone(), readonly: !wallet.is_active } },
                }
            }
        }
    }
}

// ─── Sub-tabs ─────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum DetailTab { History, Holdings }

// ─── Transaction history ─────────────────────────────────────────────────────

#[component]
fn TxnHistory(wallet_id: String, txns: Vec<Transaction>) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let wallets: HashMap<String, String> = state
        .with_conn(|conn| WalletRepo::names(conn).unwrap_or_default());
    let holdings: HashMap<String, String> = state
        .with_conn(|conn| HoldingRepo::list_all(conn).unwrap_or_default())
        .into_iter().map(|h| (h.id, h.symbol)).collect();
    let categories: HashMap<String, (String, String, String)> = state
        .with_conn(|conn| CategoryRepo::list(conn).unwrap_or_default())
        .into_iter()
        .map(|c| (c.id, (
            c.icon.unwrap_or_default(),
            t.tr(&c.name),
            c.color.unwrap_or_else(|| theme::TEXT_MUTED.to_string()),
        )))
        .collect();
    rsx! {
        div {
            style: "{theme::CARD} display:flex; flex-direction:column;",
            if txns.is_empty() {
                div {
                    style: "text-align:center; padding:32px 0; color:{theme::TEXT_MUTED}; font-size:14px;",
                    {t.t("No transactions for this wallet")}
                }
            }
            for (i, txn) in txns.iter().enumerate() {
                div {
                    key: "{txn.id}",
                    style: if i + 1 == txns.len() { "" } else { theme::ROW_DIVIDER },
                    TxnItem {
                        category: txn.category_id.as_deref().and_then(|id| categories.get(id).cloned()),
                        from_wallet: wallets.get(&txn.wallet_id).cloned(),
                        to_wallet: txn.to_wallet_id.as_deref().and_then(|id| wallets.get(id).cloned()),
                        holding: txn.holding_id.as_deref().and_then(|id| holdings.get(id).cloned()),
                        perspective: Some(wallet_id.clone()),
                        show_date: true,
                        txn: txn.clone(),
                    }
                }
            }
        }
    }
}

// ─── Credit card statement ───────────────────────────────────────────────────

fn load_card(state: &AppState, wallet_id: &str, today: NaiveDate) -> Option<(CreditCard, CardSummary)> {
    state.with_conn(|conn| {
        let card = CreditCardRepo::get(conn, wallet_id).ok().flatten()?;
        let summary = CreditCardService::summary(conn, &card, today).ok()?;
        Some((card, summary))
    })
}

#[component]
fn CardStatement(wallet_id: String, #[props(default = false)] readonly: bool) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let today = Local::now().date_naive();
    let mut data = use_signal({
        let (state, id) = (state.clone(), wallet_id.clone());
        move || load_card(&state, &id, today)
    });

    let Some((card, s)) = data.read().clone() else {
        return rsx! {
            div {
                style: "padding:16px 16px 0;",
                div { style: "{theme::CARD} font-size:13px; color:{theme::TEXT_MUTED};",
                    {t.t("Set the statement and due days in Edit to track payments.")}
                }
            }
        };
    };

    let overdue = today > s.due_date && !s.is_paid && s.remaining_due > Decimal::ZERO;
    let remaining_color = if s.remaining_due > Decimal::ZERO && !s.is_paid { theme::DANGER } else { theme::TEXT };
    let due_text = if overdue { format!("{} · {}", fmt_date(s.due_date), t.t("overdue")) } else { fmt_date(s.due_date) };
    let due_color = if overdue { theme::DANGER } else { theme::TEXT };
    let available_negative = s.available.is_some_and(|a| a < Decimal::ZERO);

    let row = |label: &str, value: String, color: &str, bold: bool| {
        let weight = if bold { 700 } else { 500 };
        rsx! {
            div {
                style: "display:flex; justify-content:space-between; align-items:baseline; gap:12px; font-size:14px;",
                span { style: "color:{theme::TEXT_MUTED};", "{label}" }
                span { style: "color:{color}; font-weight:{weight}; text-align:right; overflow-wrap:anywhere;", "{value}" }
            }
        }
    };

    rsx! {
        div {
            style: "padding:16px 16px 0;",
            div {
                style: "{theme::CARD} display:flex; flex-direction:column; gap:8px;",
                div { style: "font-size:15px; font-weight:700; color:{theme::TEXT};", {t.t("Statement")} }
                {row(t.t("Statement date"), fmt_date(s.statement_date), theme::TEXT, false)}
                {row(t.t("Statement balance"), fmt_vnd(s.statement_balance), theme::TEXT, false)}
                {row(t.t("Paid since statement"), fmt_vnd(s.paid_since), theme::TEXT, false)}
                {row(t.t("Remaining to pay"), fmt_vnd(s.remaining_due), remaining_color, true)}
                {row(t.t("Due date"), due_text, due_color, false)}
                {row(t.t("New charges this cycle"), fmt_vnd(s.new_charges), theme::TEXT, false)}
                if let Some(limit) = card.credit_limit {
                    {row(t.t("Credit limit"), fmt_vnd(limit), theme::TEXT, false)}
                }
                if let Some(available) = s.available {
                    {row(t.t("Available"), fmt_vnd(available), if available_negative { theme::DANGER } else { theme::TEXT }, false)}
                }
                if s.is_paid {
                    div {
                        style: "display:flex; align-items:center; justify-content:space-between; margin-top:4px;",
                        span { style: "color:{theme::POSITIVE}; font-weight:600; font-size:14px;", "✓ {t.t(\"Paid\")}" }
                        if !readonly { button {
                            style: "border:none; background:transparent; color:{theme::TEXT_MUTED}; font-size:13px; cursor:pointer; padding:4px 8px;",
                            onclick: {
                                let (state, id) = (state.clone(), wallet_id.clone());
                                move |_| {
                                    let _ = state.with_conn(|conn| CreditCardService::unmark_paid(conn, &id));
                                    data.set(load_card(&state, &id, today));
                                }
                            },
                            {t.t("Undo")}
                        } }
                    }
                } else if s.statement_balance > Decimal::ZERO && !readonly {
                    button {
                        style: "{theme::BTN_PRIMARY} margin-top:4px;",
                        onclick: {
                            let (state, id) = (state.clone(), wallet_id.clone());
                            move |_| {
                                let _ = state.with_conn(|conn| CreditCardService::mark_paid(conn, &id, today));
                                data.set(load_card(&state, &id, today));
                            }
                        },
                        {t.t("Mark as paid")}
                    }
                }
            }
        }
    }
}
