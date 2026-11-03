use dioxus::prelude::*;
use crate::i18n::use_i18n;
use rust_decimal::Decimal;

use crate::app::{AppState, SelectedMonth};
use crate::models::{Currency, Transaction, TransactionType};
use crate::repository::transaction_repo::{month_bounds, TxnFilter};
use crate::repository::{CategoryRepo, TransactionRepo, TxnImageRepo};
use crate::services::TransactionService;
use crate::views::icons::IconName;
use crate::views::ui::{AppHeader, Fab, HeaderAction, TxnItem};
use crate::views::{fmt_vnd, theme, ts_to_date_str};
use std::collections::HashMap;
use super::TxnView;

#[component]
pub fn TransactionList(view: Signal<TxnView>) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let SelectedMonth(mut selected) = use_context::<SelectedMonth>();

    let txns        = use_signal(|| vec![]);
    // Types shown; toggled by the chips (at least one stays selected)
    let mut filter_types = use_signal(|| vec![
        TransactionType::Income,
        TransactionType::Expense,
        TransactionType::Transfer,
    ]);
    let mut search_note = use_signal(|| String::new());
    let mut show_search = use_signal(|| false);
    let mut show_confirm_delete = use_signal(|| None::<String>); // holds txn id
    // Photos per transaction (camera badge); refreshed whenever the list mounts
    let photo_counts = use_signal({
        let state = state.clone();
        move || state.with_conn(|conn| TxnImageRepo::counts(conn).unwrap_or_default())
    });
    // category id → (icon, name, color) for the rows
    let categories = use_signal({
        let state = state.clone();
        move || -> HashMap<String, (String, String, String)> {
            state.with_conn(|conn| CategoryRepo::list(conn).unwrap_or_default())
                .into_iter()
                .map(|c| (c.id, (
                    c.icon.unwrap_or_default(),
                    c.name,
                    c.color.unwrap_or_else(|| theme::TEXT_MUTED.to_string()),
                )))
                .collect()
        }
    });

    // ── Load (selected month + type chips) ──
    let load = {
        let state = state.clone();
        move || {
            // Local copies keep the closure `Fn` (signals are Copy handles)
            let mut txns = txns;
            let types = filter_types.read().clone();
            let (year, month) = selected();
            let (from, to) = month_bounds(year, month);
            state.with_conn(|conn| {
                if let Ok(list) = TransactionRepo::list(conn, &TxnFilter {
                    txn_types: types,
                    date_from: Some(from),
                    date_to: Some(to),
                    limit: Some(500),
                    ..Default::default()
                }) {
                    txns.set(list);
                }
            });
        }
    };
    use_effect({
        let load = load.clone();
        move || load()
    });

    let (year, month) = selected();

    // Search filter in memory
    let query = search_note.read().to_lowercase();
    let visible: Vec<Transaction> = txns.read().iter()
        .filter(|t| {
            if query.is_empty() { return true; }
            t.note.as_deref().unwrap_or("").to_lowercase().contains(&query)
        })
        .cloned()
        .collect();

    // Group by day (list is already newest first): (date, net of VND txns, rows)
    let mut groups: Vec<(String, Decimal, Vec<Transaction>)> = vec![];
    for txn in visible {
        let day = ts_to_date_str(txn.txn_date);
        if groups.last().map(|g| g.0 != day).unwrap_or(true) {
            groups.push((day.clone(), Decimal::ZERO, vec![]));
        }
        let group = groups.last_mut().unwrap();
        if txn.currency == Currency::VND {
            match txn.txn_type {
                TransactionType::Income  => group.1 += txn.amount,
                TransactionType::Expense => group.1 -= txn.amount,
                _ => {}
            }
        }
        group.2.push(txn);
    }

    rsx! {
        div {
            style: "display:flex; flex-direction:column; min-height:100%;",

            AppHeader {
                title: t.t("Transactions").to_string(),
                month: Some((year, month)),
                on_month: Some(EventHandler::new(move |ym| selected.set(ym))),
                action: rsx! {
                    HeaderAction {
                        icon: IconName::Search,
                        label: t.t("Search").to_string(),
                        onclick: move |_| show_search.set(!show_search()),
                    }
                },
            }

            div {
                style: "padding:16px; display:flex; flex-direction:column; gap:12px; flex:1;",

                // ── Search ──
                if show_search() {
                    input {
                        style: "border:1px solid {theme::BORDER}; border-radius:0; padding:10px 12px; font-size:14px; outline:none; background:#fff;",
                        r#type: "text",
                        placeholder: t.t("Search by note…"),
                        value: "{search_note.read()}",
                        oninput: move |e| {
                            search_note.set(e.value());
                        },
                    }
                }

                // ── Type Filter (multi-select) ──
                div {
                    style: "display:flex; gap:6px; overflow-x:auto; padding-bottom:4px;",
                    for (tt, label) in [
                        (TransactionType::Income,    t.t("Income")),
                        (TransactionType::Expense,   t.t("Expense")),
                        (TransactionType::Transfer,  t.t("Transfer")),
                        (TransactionType::InvestBuy, t.t("Buy")),
                        (TransactionType::InvestSell,t.t("Sell")),
                        (TransactionType::OpeningBalance, t.t("Opening Balance")),
                    ] {
                        {
                            let tt_clone = tt.clone();
                            rsx! {
                                FilterChip {
                                    label,
                                    active: filter_types.read().contains(&tt),
                                    onclick: move |_| {
                                        // The load effect reads the chips, so it re-runs on change
                                        let mut types = filter_types.write();
                                        if let Some(pos) = types.iter().position(|x| *x == tt_clone) {
                                            // Keep at least one type selected
                                            if types.len() > 1 { types.remove(pos); }
                                        } else {
                                            types.push(tt_clone.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ── List, one card per day ──
                if groups.is_empty() {
                    div {
                        style: "text-align:center; padding:40px 0; color:{theme::TEXT_MUTED}; font-size:14px;",
                        {t.t("No transactions found")}
                    }
                }

                for (day, net, rows) in groups.iter() {
                    div {
                        div {
                            style: "display:flex; justify-content:space-between; align-items:baseline; padding:0 4px 6px; font-size:13px; font-weight:600; color:{theme::TEXT_MUTED};",
                            span { "{day}" }
                            if !net.is_zero() {
                                span {
                                    style: if *net > Decimal::ZERO { "color:#17a589;" } else { "color:#e5527f;" },
                                    if *net > Decimal::ZERO { "+{fmt_vnd(*net)}" } else { "−{fmt_vnd(net.abs())}" }
                                }
                            }
                        }
                        div {
                            style: "{theme::CARD} padding-top:4px; padding-bottom:4px;",
                            for (i, txn) in rows.iter().enumerate() {
                                div {
                                    style: if i > 0 { "border-top:1px solid #e6e6ef;" } else { "" },
                                    TxnItem {
                                        photos: photo_counts.read().get(&txn.id).copied().unwrap_or(0),
                                        category: txn.category_id.as_deref()
                                            .and_then(|id| categories.read().get(id).cloned())
                                            .map(|(icon, name, color)| (icon, t.tr(&name), color)),
                                        txn: txn.clone(),
                                        on_edit: {
                                            let txn = txn.clone();
                                            EventHandler::new(move |_| { view.set(TxnView::Edit(txn.clone())); })
                                        },
                                        on_delete: {
                                            let id = txn.id.clone();
                                            EventHandler::new(move |_| { show_confirm_delete.set(Some(id.clone())); })
                                        },
                                    }
                                }
                            }
                        }
                    }
                }

                Fab { onclick: move |_| { view.set(TxnView::Add); } }
            }

            // ── Delete confirm modal ──
            if let Some(txn_id) = show_confirm_delete.read().clone() {
                div {
                    style: "position:fixed; inset:0; background:rgba(0,0,0,.4); display:flex; align-items:center; justify-content:center; z-index:100;",
                    div {
                        style: "background:#fff; border-radius:0; padding:24px; margin:16px; max-width:320px; width:100%;",
                        h3 { style: "margin:0 0 8px; font-size:15px;", {t.t("Delete transaction?")} }
                        p  { style: "margin:0 0 20px; font-size:14px; color:{theme::TEXT_MUTED};", {t.t("This will reverse all balance changes.")} }
                        div {
                            style: "display:flex; gap:10px;",
                            button {
                                style: "flex:1; padding:10px; border:1px solid {theme::BORDER}; border-radius:0; background:#fff; cursor:pointer; font-size:14px;",
                                onclick: move |_| { show_confirm_delete.set(None); },
                                {t.t("Cancel")}
                            }
                            button {
                                style: "flex:1; padding:10px; border:none; border-radius:0; background:{theme::NEGATIVE}; color:#fff; cursor:pointer; font-size:14px; font-weight:600;",
                                onclick: {
                                    let state = state.clone();
                                    let txn_id = txn_id.clone();
                                    let load = load.clone();
                                    move |_| {
                                        state.with_conn(|conn| {
                                            let _ = TransactionService::delete(conn, &txn_id);
                                        });
                                        show_confirm_delete.set(None);
                                        load();
                                    }
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

// ─── FilterChip ───────────────────────────────────────────────────────────────

#[component]
fn FilterChip(label: &'static str, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            style: if active {
                "border:1px solid #e2e5f7; border-radius:999px; padding:6px 14px; font-size:12px; font-weight:600; cursor:pointer; white-space:nowrap; background:#e2e5f7; color:#3f51b5;"
            } else {
                "border:1px solid #e6e6ef; border-radius:999px; padding:6px 14px; font-size:12px; cursor:pointer; white-space:nowrap; background:transparent; color:#8a8d9f;"
            },
            onclick: move |e| { onclick.call(e); },
            "{label}"
        }
    }
}
