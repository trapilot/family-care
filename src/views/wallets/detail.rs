use dioxus::prelude::*;
use crate::i18n::use_i18n;

use crate::app::AppState;
use crate::models::{Transaction, Wallet, WalletType};
use crate::repository::transaction_repo::TxnFilter;
use crate::repository::TransactionRepo;
use crate::views::ui::{txn_type_look, IconCircle, ScreenHeader, UnderlineTabs};
use crate::views::{fmt_currency, theme, ts_to_date_str};
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
                    style: "background:{theme::PRIMARY}; border-radius:16px; padding:16px; color:#fff;",
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
                    DetailTab::History  => rsx! { TxnHistory { txns: txns.read().clone() } },
                    DetailTab::Holdings => rsx! { HoldingsView { wallet_id: wallet.id.clone() } },
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
fn TxnHistory(txns: Vec<Transaction>) -> Element {
    let t = use_i18n();
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
                TxnHistoryRow { key: "{txn.id}", txn: txn.clone(), last: i + 1 == txns.len() }
            }
        }
    }
}

#[component]
fn TxnHistoryRow(txn: Transaction, last: bool) -> Element {
    let t = use_i18n();
    use crate::models::TransactionType;
    let (icon, color) = txn_type_look(&txn.txn_type);
    let amount_color = match txn.txn_type {
        TransactionType::Income | TransactionType::InvestSell | TransactionType::OpeningBalance => theme::POSITIVE,
        _ => theme::NEGATIVE,
    };
    let sign = match txn.txn_type {
        TransactionType::Income | TransactionType::InvestSell | TransactionType::OpeningBalance => "+",
        _ => "-",
    };
    let type_str = t.code(&txn.txn_type.to_string());
    let amount_str = fmt_currency(txn.amount, &txn.currency);
    let date_str   = ts_to_date_str(txn.txn_date);
    let label      = txn.note.as_deref().unwrap_or(&type_str);
    let divider    = if last { "" } else { theme::ROW_DIVIDER };

    rsx! {
        div {
            style: "display:flex; align-items:center; gap:12px; padding:10px 0; {divider}",
            IconCircle { icon: None, fallback: icon, color: color.to_string(), size: 40 }
            div { style: "flex:1; min-width:0;",
                div { style: "font-size:14px; color:{theme::TEXT}; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{label}" }
                div { style: "font-size:12px; color:{theme::TEXT_MUTED};", "{date_str}" }
            }
            span { style: "font-size:14px; font-weight:600; color:{amount_color}; flex-shrink:0;", "{sign}{amount_str}" }
        }
    }
}
