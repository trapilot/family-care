use dioxus::prelude::*;
use crate::i18n::use_i18n;
use rust_decimal::Decimal;

use crate::app::AppState;
use crate::services::{report_service::InvestmentPnlItem, ReportService};
use crate::views::{fmt_currency, theme};
use crate::models::Currency;
use super::expense::EmptyState;

#[component]
pub fn InvestmentReport() -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let mut items = use_signal(|| vec![]);

    // ── Load data (runs once on mount) ──
    use_effect(move || {
        state.with_conn(|conn| {
            if let Ok(list) = ReportService::investment_pnl(conn) {
                items.set(list);
            }
        });
    });

    let data = items.read();

    // ── Aggregations ──
    let total_cost: Decimal = data.iter().map(|i| i.cost_basis).sum();

    let total_mv: Decimal = data
        .iter()
        .filter_map(|i| i.market_value)
        .fold(Decimal::ZERO, |acc, v| acc + v);

    let total_unrealized: Decimal = data
        .iter()
        .filter_map(|i| i.unrealized_pnl)
        .fold(Decimal::ZERO, |acc, v| acc + v);

    let total_realized: Decimal = data
        .iter()
        .map(|i| i.realized_pnl)
        .sum();

    // ── Precomputed formatted values ──
    let unrealized_value = format!(
        "{}{}",
        if total_unrealized >= Decimal::ZERO { "+" } else { "" },
        fmt_currency(total_unrealized, &Currency::VND)
    );

    let realized_value = format!(
        "{}{}",
        if total_realized >= Decimal::ZERO { "+" } else { "" },
        fmt_currency(total_realized, &Currency::VND)
    );

    let unrealized_color = if total_unrealized >= Decimal::ZERO {
        "#b8f0e2"
    } else {
        "#ffc2d3"
    };

    let realized_color = if total_realized >= Decimal::ZERO {
        "#ffffff"
    } else {
        "#ffc2d3"
    };

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:16px;",

            // ── Overview ──
            div {
                style: "background:{theme::PRIMARY}; border-radius:16px; padding:16px; color:#fff;",

                div {
                    style: "font-size:13px; opacity:.8; margin-bottom:12px;",
                    {t.t("Investment Overview")}
                }

                div {
                    style: "display:grid; grid-template-columns:1fr 1fr; gap:10px;",

                    InvStat {
                        label: t.t("Cost Basis"),
                        value: fmt_currency(total_cost, &Currency::VND),
                        color: "#ffffff"
                    }

                    InvStat {
                        label: t.t("Market Value"),
                        value: fmt_currency(total_mv, &Currency::VND),
                        color: "#b8f0e2"
                    }

                    InvStat {
                        label: t.t("Unrealized P&L"),
                        value: unrealized_value,
                        color: unrealized_color
                    }

                    InvStat {
                        label: t.t("Realized P&L"),
                        value: realized_value,
                        color: realized_color
                    }
                }
            }

            // ── Body ──
            {
                if data.is_empty() {
                    rsx! {
                        EmptyState {
                            msg: t.t("No investment holdings yet")
                        }
                    }
                } else {
                    rsx! {
                        for item in data.iter() {
                            InvestmentCard {
                                item: item.clone()
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── InvestmentCard ───────────────────────────────────────────────────────────

#[component]
fn InvestmentCard(item: InvestmentPnlItem) -> Element {
    let t = use_i18n();
    let unr = item.unrealized_pnl.unwrap_or(Decimal::ZERO);
    let unr_color  = if unr >= Decimal::ZERO { theme::POSITIVE } else { theme::NEGATIVE };
    let real_color = if item.realized_pnl >= Decimal::ZERO { theme::POSITIVE } else { theme::NEGATIVE };

    let unr_sign  = if unr >= Decimal::ZERO { "+" } else { "" };
    let real_sign = if item.realized_pnl >= Decimal::ZERO { "+" } else { "" };

    let pct_str = item.unrealized_pnl_pct
        .map(|p| {
            let sign = if p >= Decimal::ZERO { "+" } else { "" };
            format!("{sign}{p:.2}%")
        })
        .unwrap_or("—".into());

    rsx! {
        div {
            style: "{theme::CARD}",

            // Symbol + name
            div {
                style: "display:flex; align-items:center; justify-content:space-between; margin-bottom:10px;",
                div {
                    div { style: "font-size:15px; font-weight:600; color:{theme::TEXT};", "{item.symbol}" }
                    if let Some(ref name) = item.name {
                        div { style: "font-size:12px; color:{theme::TEXT_MUTED};", "{name}" }
                    }
                }
                div { style: "text-align:right;",
                    div { style: "font-size:15px; font-weight:600; color:{unr_color};",
                        "{unr_sign}{fmt_currency(unr, &Currency::VND)}"
                    }
                    div { style: "font-size:12px; color:{unr_color};", "{pct_str}" }
                }
            }

            // Stats grid
            div {
                style: "display:grid; grid-template-columns:repeat(3,1fr); gap:6px;",
                PnlCell { label: t.t("Quantity"),    value: format!("{:.4}", item.quantity),           color: theme::TEXT }
                PnlCell { label: t.t("Avg Cost"),    value: format!("{:.0}", item.avg_buy_price),       color: theme::TEXT }
                PnlCell { label: t.t("Last Price"),  value: item.last_price.map(|p| format!("{p:.0}")).unwrap_or("—".into()), color: theme::TEXT }
                PnlCell { label: t.t("Cost Basis"),  value: fmt_currency(item.cost_basis, &Currency::VND),  color: theme::TEXT_MUTED }
                PnlCell { label: t.t("Mkt Value"),   value: item.market_value.map(|v| fmt_currency(v, &Currency::VND)).unwrap_or("—".into()), color: theme::TEXT_MUTED }
                PnlCell { label: t.t("Realized P&L"), value: format!("{real_sign}{}", fmt_currency(item.realized_pnl, &Currency::VND)), color: real_color }
            }
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

#[component]
fn InvStat(label: &'static str, value: String, color: &'static str) -> Element {
    rsx! {
        div {
            div {
                style: "font-size:10px; opacity:.6; margin-bottom:2px;",
                "{label}"
            }

            div {
                style: "font-size:14px; font-weight:600; color:{color};",
                "{value}"
            }
        }
    }
}

#[component]
fn PnlCell(label: &'static str, value: String, color: &'static str) -> Element {
    rsx! {
        div {
            style: "background:{theme::BG}; border-radius:10px; padding:6px 8px;",
            div { style: "font-size:10px; color:{theme::TEXT_MUTED}; margin-bottom:1px;", "{label}" }
            div { style: "font-size:12px; font-weight:600; color:{color};", "{value}" }
        }
    }
}
