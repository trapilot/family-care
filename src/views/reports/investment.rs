use dioxus::prelude::*;
use crate::i18n::use_i18n;
use rust_decimal::Decimal;
use std::collections::HashMap;

use crate::app::AppState;
use crate::repository::RateRepo;
use crate::services::{report_service::InvestmentPnlItem, QuoteService, ReportService};
use crate::views::icons::{Icon, IconName};
use crate::views::{fmt_currency, fmt_number, theme};
use crate::models::Currency;
use super::expense::EmptyState;

#[component]
pub fn InvestmentReport() -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let mut items = use_signal(|| vec![]);
    // Stored exchange rates (currency -> VND), used only for the VND overview totals
    let mut rates = use_signal(HashMap::<Currency, Decimal>::new);
    // Live prices (holding id -> price per unit in the holding's money type): kept only in this component, never saved
    let mut live = use_signal(HashMap::<String, Decimal>::new);
    let mut fetching = use_signal(|| false);
    let mut fetch_note = use_signal(|| None::<(usize, usize)>);

    // ── Load data (runs once on mount) ──
    let effect_state = state.clone();
    use_effect(move || {
        effect_state.with_conn(|conn| {
            if let Ok(list) = ReportService::investment_pnl(conn) {
                items.set(list);
            }
            if let Ok(list) = RateRepo::list(conn) {
                rates.set(list.into_iter().map(|r| (r.currency, r.rate_vnd)).collect());
            }
        });
    });

    let data = items.read();

    // ── Aggregations (in VND; a holding whose money type has no stored rate is left out) ──
    let rate_map = rates.read();
    let live_prices = live.read();
    let mut total_cost = Decimal::ZERO;
    let mut total_mv = Decimal::ZERO;
    let mut total_unrealized = Decimal::ZERO;
    let mut total_realized = Decimal::ZERO;
    let mut left_out = false;
    for i in data.iter() {
        let rate = match i.currency {
            Currency::VND => Some(Decimal::ONE),
            ref c => rate_map.get(c).copied(),
        };
        let Some(rate) = rate else {
            left_out = true;
            continue;
        };
        let (_, mv, unrealized, _) = with_price(i, live_prices.get(&i.holding_id).copied());
        total_cost += i.cost_basis * rate;
        total_mv += mv.unwrap_or(Decimal::ZERO) * rate;
        total_unrealized += unrealized.unwrap_or(Decimal::ZERO) * rate;
        total_realized += i.realized_pnl * rate;
    }

    let total_holdings = data.len();
    let on_fetch = move |_| {
        if fetching() { return; }
        let list: Vec<_> = items
            .read()
            .iter()
            .map(|i: &InvestmentPnlItem| (i.holding_id.clone(), i.symbol.clone(), i.currency.clone()))
            .collect();
        let total = list.len();
        let usd = state
            .with_conn(|c| RateRepo::get(c, &Currency::USD))
            .ok()
            .flatten()
            .map(|r| r.rate_vnd);
        fetching.set(true);
        spawn(async move {
            let got = QuoteService::fetch_prices(list, usd).await;
            fetch_note.set(Some((got.len(), total)));
            live.set(got);
            fetching.set(false);
        });
    };

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
                class: "money-box",
                style: "background:{theme::PRIMARY}; border-radius:0; padding:16px; color:#fff;",

                div {
                    style: "display:flex; justify-content:space-between; align-items:center; margin-bottom:12px;",
                    div {
                        style: "font-size:13px; opacity:.8;",
                        {t.t("Investment Overview")}
                    }
                    if total_holdings > 0 {
                        button {
                            style: if fetching() {
                                "width:32px; height:32px; background:transparent; border:none; padding:0; display:flex; align-items:center; justify-content:center; opacity:.5;"
                            } else {
                                "width:32px; height:32px; background:transparent; border:none; padding:0; display:flex; align-items:center; justify-content:center;"
                            },
                            aria_label: t.t("Get prices"),
                            disabled: fetching(),
                            onclick: on_fetch,
                            Icon { name: IconName::RefreshCw, size: 18, color: "#fff" }
                        }
                    }
                }

                if fetching() {
                    div { style: "font-size:11px; opacity:.8; margin-bottom:8px;", {t.t("Getting prices…")} }
                } else if let Some((got, total)) = fetch_note() {
                    div {
                        style: "font-size:11px; opacity:.8; margin-bottom:8px;",
                        {
                            let mut note = format!("{}: {got}/{total}", t.t("Live prices"));
                            if got < total {
                                note.push_str(&format!(" · {}", t.t("some codes have no price")));
                            }
                            note
                        }
                    }
                }

                if left_out {
                    div {
                        style: "font-size:11px; opacity:.8; margin-bottom:8px;",
                        {t.t("Some holdings left out: no exchange rate")}
                    }
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
                                key: "{item.holding_id}",
                                item: item.clone(),
                                live_price: live_prices.get(&item.holding_id).copied()
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
fn InvestmentCard(item: InvestmentPnlItem, live_price: Option<Decimal>) -> Element {
    let t = use_i18n();
    let (price, mv, unrealized, pct) = with_price(&item, live_price);
    let unr = unrealized.unwrap_or(Decimal::ZERO);
    let unr_color  = if unr >= Decimal::ZERO { theme::POSITIVE } else { theme::NEGATIVE };
    let real_color = if item.realized_pnl >= Decimal::ZERO { theme::POSITIVE } else { theme::NEGATIVE };

    let cur = item.currency.clone();
    let money = |v: Decimal| fmt_currency(v, &cur);
    // Per-unit prices keep their decimals unless the money type is VND
    let unit = |v: Decimal| if cur == Currency::VND { fmt_number(v) } else { fmt_currency(v, &cur) };

    let unr_sign  = if unr >= Decimal::ZERO { "+" } else { "" };
    let real_sign = if item.realized_pnl >= Decimal::ZERO { "+" } else { "" };

    let pct_str = pct
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
                        "{unr_sign}{money(unr)}"
                    }
                    div { style: "font-size:12px; color:{unr_color};", "{pct_str}" }
                }
            }

            // Stats grid
            div {
                style: "display:grid; grid-template-columns:repeat(3,1fr); gap:6px;",
                PnlCell { label: t.t("Quantity"),    value: fmt_number(item.quantity),           color: theme::TEXT }
                PnlCell { label: t.t("Avg Cost"),    value: unit(item.avg_buy_price),       color: theme::TEXT }
                PnlCell {
                    label: if live_price.is_some() { t.t("Live Price") } else { t.t("Last Price") },
                    value: price.map(unit).unwrap_or("—".into()),
                    color: if live_price.is_some() { theme::PRIMARY } else { theme::TEXT }
                }
                PnlCell { label: t.t("Cost Basis"),  value: money(item.cost_basis),  color: theme::TEXT_MUTED }
                PnlCell { label: t.t("Mkt Value"),   value: mv.map(money).unwrap_or("—".into()), color: theme::TEXT_MUTED }
                PnlCell { label: t.t("Realized P&L"), value: format!("{real_sign}{}", money(item.realized_pnl)), color: real_color }
            }
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Effective numbers for a holding: (price, market value, unrealized P&L, unrealized %).
/// A live price wins over the stored last price.
fn with_price(
    item: &InvestmentPnlItem,
    price: Option<Decimal>,
) -> (Option<Decimal>, Option<Decimal>, Option<Decimal>, Option<Decimal>) {
    let price = price.or(item.last_price);
    let mv = price.map(|p| p * item.quantity);
    let unrealized = price.map(|p| (p - item.avg_buy_price) * item.quantity);
    let pct = unrealized.and_then(|u| {
        if item.cost_basis > Decimal::ZERO {
            Some(u / item.cost_basis * Decimal::from(100))
        } else {
            None
        }
    });
    (price, mv, unrealized, pct)
}

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
            style: "background:{theme::BG}; border-radius:0; padding:6px 8px;",
            div { style: "font-size:10px; color:{theme::TEXT_MUTED}; margin-bottom:1px;", "{label}" }
            div { style: "font-size:12px; font-weight:600; color:{color};", "{value}" }
        }
    }
}
