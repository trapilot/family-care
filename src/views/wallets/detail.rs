use dioxus::prelude::*;
use crate::i18n::use_i18n;

use crate::app::AppState;
use crate::models::{Transaction, Wallet, WalletType};
use crate::repository::transaction_repo::TxnFilter;
use crate::repository::TransactionRepo;
use crate::views::{fmt_currency, ts_to_date_str};
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
            div {
                style: "padding:16px; background:#fff; border-bottom:1px solid #f3f4f6;",
                div {
                    style: "display:flex; align-items:center; gap:12px; margin-bottom:12px;",
                    button {
                        style: "border:none; background:transparent; font-size:22px; cursor:pointer; padding:0;",
                        onclick: move |_| { view.set(WalletView::List); },
                        "←"
                    }
                    div {
                        style: "flex:1;",
                        h1 { style: "margin:0; font-size:18px; font-weight:700; color:#1f2937;", "{wallet.name}" }
                        span {
                            style: "font-size:12px; color:#9ca3af;",
                            "{t.code(&wallet.wallet_type.to_string())}"
                            if let Some(ref b) = wallet.broker { " · {b}" }
                        }
                    }
                    button {
                        style: "border:none; background:#f3f4f6; border-radius:8px; padding:8px 12px; font-size:13px; cursor:pointer; color:#374151;",
                        onclick: {
                            let w = wallet.clone();
                            move |_| { view.set(WalletView::Edit(w.clone())); }
                        },
                        {t.t("Edit")}
                    }
                }

                // Balance
                div {
                    style: "background:linear-gradient(135deg, #6366f1, #8b5cf6); border-radius:12px; padding:16px; color:#fff;",
                    div { style: "font-size:11px; opacity:.8; text-transform:uppercase; letter-spacing:.5px; margin-bottom:4px;", {t.t("Balance")} }
                    for (i, b) in balances.iter().enumerate() {
                        div {
                            style: if i == 0 { "font-size:26px; font-weight:800;" } else { "font-size:18px; font-weight:700;" },
                            "{fmt_currency(b.balance, &b.currency)}"
                        }
                    }
                }
            }

            // ── Tabs (investment wallet shows Holdings tab) ──
            if is_investment {
                div {
                    style: "display:flex; background:#fff; border-bottom:1px solid #f3f4f6;",
                    for (tab, label) in [(DetailTab::History, t.t("History")), (DetailTab::Holdings, t.t("Holdings"))] {
                        {
                            let is_active = *sub_view.read() == tab;
                            let tab_clone = tab.clone();
                            rsx! {
                                button {
                                    style: if is_active {
                                        "flex:1; padding:12px; border:none; border-bottom:2px solid #6366f1; background:#fff; font-size:14px; font-weight:600; color:#6366f1; cursor:pointer;"
                                    } else {
                                        "flex:1; padding:12px; border:none; border-bottom:2px solid transparent; background:#fff; font-size:14px; color:#9ca3af; cursor:pointer;"
                                    },
                                    onclick: move |_| { sub_view.set(tab_clone.clone()); },
                                    "{label}"
                                }
                            }
                        }
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
            style: "display:flex; flex-direction:column; gap:8px;",
            if txns.is_empty() {
                div {
                    style: "text-align:center; padding:40px 0; color:#9ca3af; font-size:14px;",
                    {t.t("No transactions for this wallet")}
                }
            }
            for txn in txns.iter() {
                TxnHistoryRow { txn: txn.clone() }
            }
        }
    }
}

#[component]
fn TxnHistoryRow(txn: Transaction) -> Element {
    let t = use_i18n();
    use crate::models::TransactionType;
    let (icon, color) = match txn.txn_type {
        TransactionType::Income     => ("↑", "#10b981"),
        TransactionType::Expense    => ("↓", "#ef4444"),
        TransactionType::Transfer   => ("⇄", "#6366f1"),
        TransactionType::InvestBuy  => ("📈", "#f59e0b"),
        TransactionType::InvestSell => ("📉", "#8b5cf6"),
        TransactionType::OpeningBalance => ("🏁", "#0ea5e9"),
    };
    let sign = match txn.txn_type {
        TransactionType::Income | TransactionType::InvestSell | TransactionType::OpeningBalance => "+",
        _ => "-",
    };
    let type_str = t.code(&txn.txn_type.to_string());
    let amount_str = fmt_currency(txn.amount, &txn.currency);
    let date_str   = ts_to_date_str(txn.txn_date);
    let label      = txn.note.as_deref().unwrap_or(&type_str);

    rsx! {
        div {
            style: "background:#fff; border-radius:10px; padding:12px; box-shadow:0 1px 2px rgba(0,0,0,.05); display:flex; align-items:center; gap:10px;",
            span {
                style: "width:32px; height:32px; border-radius:50%; background:#f3f4f6; display:flex; align-items:center; justify-content:center; font-size:15px; flex-shrink:0;",
                "{icon}"
            }
            div { style: "flex:1; min-width:0;",
                div { style: "font-size:13px; font-weight:500; color:#374151; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{label}" }
                div { style: "font-size:11px; color:#9ca3af;", "{date_str}" }
            }
            span { style: "font-size:14px; font-weight:700; color:{color}; flex-shrink:0;", "{sign}{amount_str}" }
        }
    }
}
