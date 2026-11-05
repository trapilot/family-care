use dioxus::prelude::*;
use crate::i18n::use_i18n;
use rust_decimal::Decimal;

use crate::app::{AppState, SelectedMonth};
use crate::views::icons::{Icon, IconName};
use crate::views::theme;
use crate::views::ui::{IconCircle, IconText};
use crate::services::{report_service::ExpenseByCategoryItem, ReportService};
use crate::models::TransactionType;
use crate::views::fmt_vnd;
use super::type_transactions::{use_open_drill, TypeDrill};

#[component]
pub fn ExpenseReport() -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let SelectedMonth(selected) = use_context::<SelectedMonth>();

    let mut drill = use_open_drill();
    let items = use_signal(|| vec![]);
    let total = use_signal(|| Decimal::ZERO);

    // Reload when the shared month changes
    use_effect({
        let state = state.clone();
        let (mut items, mut total) = (items, total);
        move || {
            let (y, m) = selected();
            state.with_conn(|conn| {
                if let Ok(list) = ReportService::expense_by_category(conn, y, m) {
                    let sum: Decimal = list.iter().map(|i| i.amount).sum();
                    total.set(sum);
                    items.set(list);
                }
            });
        }
    });

    let (year, month) = selected();

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:10px;",

            // ── Total ──
            div {
                style: "background:{theme::SUMMARY_BG}; color:{theme::TEXT}; padding:14px 16px; display:flex; align-items:center; cursor:pointer;",
                onclick: move |_| drill.set(Some(TypeDrill::month(TransactionType::Expense, year, month))),
                div { style: "flex:1;",
                    div { style: "font-size:13px; color:{theme::MUTED_ON_PLATINUM}; margin-bottom:4px;", {t.t("Total Expense")} }
                    div { style: "font-size:21px; font-weight:600; color:{theme::signed_on_platinum(-*total.read())};", "{fmt_vnd(*total.read())}" }
                }
                Icon { name: IconName::ChevronRight, size: 22, color: theme::MUTED_ON_PLATINUM.to_string() }
            }

            // ── Category breakdown ──
            if items.read().is_empty() {
                EmptyState { msg: t.t("No expenses this month") }
            }

            for item in items.read().iter() {
                CategoryBar {
                    item: item.clone(),
                    onclick: {
                        let category_id = item.category_id.clone();
                        move |_| drill.set(Some(TypeDrill::category(category_id.clone(), year, month)))
                    },
                }
            }
        }
    }
}

// ─── CategoryBar ─────────────────────────────────────────────────────────────

#[component]
fn CategoryBar(item: ExpenseByCategoryItem, onclick: EventHandler<MouseEvent>) -> Element {
    let t = use_i18n();
    use rust_decimal::prelude::ToPrimitive;

    let pct = item.share_pct.to_f64().unwrap_or(0.0).max(0.0).min(100.0);
    let color = item.color.clone().unwrap_or_else(|| theme::PRIMARY.to_string());
    let bar_color = if item.is_over_budget { theme::NEGATIVE.to_string() } else { color.chars().take(7).collect() };

    rsx! {
        div {
            style: "display:flex; align-items:center; gap:10px; cursor:pointer;",
            onclick: move |e| onclick.call(e),
            IconCircle { icon: item.icon.clone(), fallback: IconName::Package, color: color.clone(), size: 30 }

            div { style: "flex:1; min-width:0;",
                div { style: "display:flex; justify-content:space-between; align-items:baseline; gap:8px;",
                    span { style: "font-size:13px; color:{theme::TEXT};", {t.tr(&item.category_name)} }
                    span { style: "font-size:13px; color:{theme::signed_color(-item.amount)}; white-space:nowrap;", "{fmt_vnd(item.amount)}" }
                }
                div { style: "display:flex; align-items:center; gap:8px; margin-top:2px;",
                    span { style: "font-size:11px; color:{theme::TEXT_MUTED}; min-width:36px;", "{pct:.0}%" }
                    div { style: "flex:1; height:3px; background:#eeeef4; border-radius:3px; overflow:hidden;",
                        div { style: "height:100%; width:{pct}%; background:{bar_color}; border-radius:3px;" }
                    }
                }
                if item.is_over_budget {
                    div { style: "font-size:11px; color:{theme::NEGATIVE}; font-weight:600; margin-top:2px;", IconText { text: t.t("⚠️ Over budget").to_string(), size: 14, gap: 4 } }
                } else if let Some(budget) = item.budget_amount {
                    div { style: "font-size:11px; color:{theme::TEXT_MUTED}; margin-top:2px;", "Budget: {fmt_vnd(budget)}" }
                }
                // Budget bar (if set)
                if let Some(budget) = item.budget_amount {
                    if budget > Decimal::ZERO {
                        {
                            let budget_pct = (item.amount / budget * Decimal::from(100))
                                .to_f64().unwrap_or(0.0).min(100.0);
                            let fill = if item.is_over_budget { theme::NEGATIVE } else { theme::POSITIVE };
                            rsx! {
                                div { style: "margin-top:2px;",
                                    div { style: "font-size:11px; color:{theme::TEXT_MUTED}; margin-bottom:2px;",
                                        {t.t("Budget usage: {pct}%").replace("{pct}", &format!("{budget_pct:.0}"))}
                                    }
                                    div {
                                        style: "height:3px; background:#eeeef4; border-radius:2px; overflow:hidden;",
                                        div { style: "height:100%; width:{budget_pct}%; background:{fill}; border-radius:2px;" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

#[component]
pub fn EmptyState(msg: &'static str) -> Element {
    rsx! {
        div {
            style: "text-align:center; padding:40px 0; color:#8a8d9f;",
            div { style: "display:flex; justify-content:center;",
                Icon { name: IconName::Inbox, size: 40, color: theme::TEXT_MUTED.to_string() }
            }
            p { style: "font-size:14px; margin:8px 0 0;", "{msg}" }
        }
    }
}
