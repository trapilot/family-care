use dioxus::prelude::*;
use rust_decimal::Decimal;
use std::collections::HashMap;

use crate::app::AppState;
use crate::i18n::use_i18n;
use crate::models::{Transaction, TransactionType};
use crate::repository::transaction_repo::{month_bounds, TxnFilter};
use crate::repository::{CategoryRepo, TransactionRepo, WalletRepo};
use crate::views::icons::IconName;
use crate::views::ui::{txn_type_look, IconCircle, ScreenHeader};
use crate::views::{fmt_currency, fmt_vnd, month_label, theme, ts_to_date_str};
use super::expense::EmptyState;

/// Category look: (translated name, icon, color)
type CategoryLook = (String, Option<String>, Option<String>);

/// Transactions behind a report number: one type (optionally one category) over a range of whole months
#[derive(Clone, PartialEq, Debug)]
pub struct TypeDrill {
    pub txn_type: TransactionType,
    /// (year, month), inclusive
    pub from: (i32, u32),
    pub to:   (i32, u32),
    /// Only this category (Expense report rows)
    pub category_id: Option<String>,
}

impl TypeDrill {
    pub fn month(txn_type: TransactionType, year: i32, month: u32) -> Self {
        Self { txn_type, from: (year, month), to: (year, month), category_id: None }
    }

    pub fn category(category_id: String, year: i32, month: u32) -> Self {
        Self { category_id: Some(category_id), ..Self::month(TransactionType::Expense, year, month) }
    }
}

/// Open the drill-down from any report (provided by `Reports`)
pub fn use_open_drill() -> Signal<Option<TypeDrill>> {
    use_context::<Signal<Option<TypeDrill>>>()
}

#[component]
pub fn TypeTransactions(drill: TypeDrill, on_back: EventHandler<()>) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();

    // Same bounds as the report sums (`sum_by_type_in_month`), so the total matches
    let (txns, wallets, categories) = state.with_conn(|conn| {
        let filter = TxnFilter {
            txn_type:    Some(drill.txn_type.clone()),
            category_id: drill.category_id.clone(),
            date_from: Some(month_bounds(drill.from.0, drill.from.1).0),
            date_to:   Some(month_bounds(drill.to.0, drill.to.1).1),
            ..Default::default()
        };
        let wallets: HashMap<String, String> = WalletRepo::list_active(conn).unwrap_or_default()
            .into_iter().map(|w| (w.id, w.name)).collect();
        let categories: HashMap<String, CategoryLook> = CategoryRepo::list(conn).unwrap_or_default()
            .into_iter().map(|c| (c.id, (t.tr(&c.name), c.icon, c.color))).collect();
        (TransactionRepo::list(conn, &filter).unwrap_or_default(), wallets, categories)
    });
    let total: Decimal = txns.iter().map(|tx| tx.amount).sum();
    let count = txns.len();

    // A category drill is titled by the category name, otherwise by the type
    let type_label = drill.category_id.as_deref()
        .and_then(|c| categories.get(c))
        .map(|(name, _, _)| name.clone())
        .unwrap_or_else(|| t.code(&drill.txn_type.to_string()));
    let period = if drill.from == drill.to {
        month_label(drill.from.0, drill.from.1)
    } else {
        format!("{} – {}", month_label(drill.from.0, drill.from.1), month_label(drill.to.0, drill.to.1))
    };
    let color = type_color(&drill.txn_type);

    rsx! {
        div {
            style: "display:flex; flex-direction:column; height:100%;",

            ScreenHeader {
                title: type_label.clone(),
                on_back: Some(EventHandler::new(move |_| on_back.call(()))),
            }

            div {
                class: "scroll-col",
                style: "flex:1; overflow-y:auto; padding:16px; display:flex; flex-direction:column; gap:16px;",

                // ── Total ──
                div {
                    style: "{theme::CARD} display:flex; justify-content:space-between; align-items:center; gap:12px;",
                    div {
                        div { style: "font-size:14px; font-weight:600; color:{theme::TEXT};", "{period}" }
                        div { style: "font-size:13px; color:{theme::TEXT_MUTED};",
                            {t.t("{n} transactions").replace("{n}", &txns.len().to_string())}
                        }
                    }
                    span { style: "font-size:19px; font-weight:600; color:{color};", "{fmt_vnd(total)}" }
                }

                if txns.is_empty() {
                    EmptyState { msg: t.t("No transactions in this period") }
                }
                if !txns.is_empty() {
                    div {
                        style: "{theme::CARD} display:flex; flex-direction:column;",
                        for (i, txn) in txns.iter().cloned().enumerate() {
                            TxnLine {
                                key: "{txn.id}",
                                detail: detail_line(&txn, &wallets, &categories, drill.category_id.is_some()),
                                category: if txn.txn_type == TransactionType::Expense {
                                    txn.category_id.as_deref().and_then(|c| categories.get(c))
                                        .map(|(_, icon, color)| (icon.clone(), color.clone()))
                                } else { None },
                                last: i + 1 == count,
                                txn,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Read-only row: what it was (category / wallets), note, date, amount
#[component]
fn TxnLine(txn: Transaction, detail: String, category: Option<(Option<String>, Option<String>)>, last: bool) -> Element {
    let note = txn.note.clone().unwrap_or_default();
    let color = type_color(&txn.txn_type);
    let (icon, icon_color) = txn_type_look(&txn.txn_type);
    let sign = match txn.txn_type {
        TransactionType::Income  => "+",
        TransactionType::Expense => "−",
        _ => "",
    };
    let divider = if last { "" } else { theme::ROW_DIVIDER };
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:12px; padding:10px 0; {divider}",
            if let Some((cat_icon, cat_color)) = category {
                IconCircle {
                    icon: cat_icon,
                    fallback: IconName::Tag,
                    color: cat_color.unwrap_or_else(|| theme::TEXT_MUTED.to_string()),
                    size: 40,
                }
            } else {
                IconCircle { icon: None, fallback: icon, color: icon_color.to_string(), size: 40 }
            }
            div { style: "flex:1; min-width:0;",
                div { style: "font-size:14px; color:{theme::TEXT}; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{detail}" }
                if !note.is_empty() {
                    div { style: "font-size:13px; color:{theme::TEXT_MUTED}; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{note}" }
                }
                div { style: "font-size:12px; color:{theme::TEXT_MUTED};", "{ts_to_date_str(txn.txn_date)}" }
            }
            div { style: "font-size:14px; font-weight:600; color:{color}; flex-shrink:0;",
                "{sign}{fmt_currency(txn.amount, &txn.currency)}"
            }
        }
    }
}

/// `in_category`: the list is already one category (it's in the title), so rows show only the wallet
fn detail_line(txn: &Transaction, wallets: &HashMap<String, String>, categories: &HashMap<String, CategoryLook>, in_category: bool) -> String {
    let wallet = |id: &str| wallets.get(id).cloned().unwrap_or_default();
    match txn.txn_type {
        TransactionType::Expense if in_category => wallet(&txn.wallet_id),
        TransactionType::Expense => {
            let category = txn.category_id.as_deref().and_then(|c| categories.get(c)).map(|(name, _, _)| name.clone()).unwrap_or_default();
            format!("{category} · {}", wallet(&txn.wallet_id))
        }
        TransactionType::Transfer => {
            format!("{} → {}", wallet(&txn.wallet_id), txn.to_wallet_id.as_deref().map(wallet).unwrap_or_default())
        }
        _ => wallet(&txn.wallet_id),
    }
}

fn type_color(txn_type: &TransactionType) -> &'static str {
    match txn_type {
        TransactionType::Income  => theme::POSITIVE,
        TransactionType::Expense => theme::NEGATIVE,
        _                        => theme::PRIMARY,
    }
}
