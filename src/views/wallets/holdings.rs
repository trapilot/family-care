use dioxus::prelude::*;
use crate::i18n::use_i18n;
use rust_decimal::Decimal;

use crate::app::AppState;
use crate::models::holding::AssetType;
use crate::services::{holding_service::HoldingWithPnl, HoldingService};
use crate::views::icons::{Icon, IconName};
use crate::views::ui::IconTile;
use crate::views::{fmt_currency, theme, AmountInput};
use crate::models::Currency;

#[component]
pub fn HoldingsView(wallet_id: String) -> Element {
    let t = use_i18n();
    let state    = use_context::<AppState>();
    let holdings = use_signal(|| vec![]);
    let mut edit_price_id  = use_signal(|| None::<String>);
    let mut price_input    = use_signal(|| String::new());

    let load = {
        let state     = state.clone();
        let wallet_id = wallet_id.clone();
        let mut holdings = holdings.clone();
        move || {
            state.with_conn(|conn| {
                if let Ok(list) = HoldingService::portfolio_summary(conn, &wallet_id) {
                    holdings.set(list);
                }
            });
        }
    };

    use_effect({
        let mut load = load.clone();
        move || {
            load();
        }
    });

    let total_cost: Decimal = holdings.read().iter().map(|h| h.cost_basis).sum();
    let total_mv: Option<Decimal> = {
        let mvs: Vec<Decimal> = holdings.read().iter().filter_map(|h| h.market_value).collect();
        if mvs.len() == holdings.read().len() && !mvs.is_empty() {
            Some(mvs.iter().sum())
        } else {
            None
        }
    };
    let total_pnl: Option<Decimal> = total_mv.map(|mv| mv - total_cost);

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:16px;",

            // ── Portfolio summary ──
            div {
                style: "background:{theme::PRIMARY}; border-radius:0; padding:16px; color:#fff;",
                div { style: "font-size:13px; opacity:.8; margin-bottom:8px;", {t.t("Portfolio")} }
                div {
                    style: "display:grid; grid-template-columns:1fr 1fr; gap:10px;",
                    PortfolioStat {
                        label: t.t("Cost Basis"),
                        value: fmt_currency(total_cost, &Currency::VND),
                        color: "#ffffff"
                    }
                    PortfolioStat {
                        label: t.t("Market Value"),
                        value: total_mv.map(|v| fmt_currency(v, &Currency::VND)).unwrap_or("—".into()),
                        color: "#b8f0e2"
                    }
                    PortfolioStat {
                        label: t.t("Unrealized P&L"),
                        value: total_pnl.map(|p| {
                            let sign = if p >= Decimal::ZERO { "+" } else { "" };
                            format!("{sign}{}", fmt_currency(p, &Currency::VND))
                        }).unwrap_or("—".into()),
                        color: total_pnl.map(|p| if p >= Decimal::ZERO { "#b8f0e2" } else { "#ffc2d3" }).unwrap_or("#ffffff")
                    }
                    PortfolioStat {
                        label: t.t("Holdings"),
                        value: holdings.read().len().to_string(),
                        color: "#ffffff"
                    }
                }
            }

            // ── Holdings list ──
            if holdings.read().is_empty() {
                div {
                    style: "display:flex; flex-direction:column; align-items:center; padding:32px 0; color:{theme::TEXT_MUTED}; font-size:14px;",
                    Icon { name: IconName::TrendingUp, size: 40, color: theme::TEXT_MUTED.to_string() }
                    p { style: "margin:8px 0 0;", {t.t("No holdings yet. Add a Buy transaction.")} }
                }
            }

            for item in holdings.read().iter() {
                HoldingCard {
                    item: item.clone(),
                    is_editing_price: *edit_price_id.read() == Some(item.holding.id.clone()),
                    price_input: price_input.read().clone(),
                    on_edit_price: {
                        let id    = item.holding.id.clone();
                        let price = item.holding.last_price.map(|p| format!("{p}")).unwrap_or_default();
                        move |_| {
                            edit_price_id.set(Some(id.clone()));
                            price_input.set(price.clone());
                        }
                    },
                    on_price_input: move |v| { price_input.set(v); },
                    on_price_save: {
                        let state = state.clone();
                        let id    = item.holding.id.clone();
                        let mut load  = load.clone();
                        move |_: MouseEvent| {
                            let price_str = price_input.read().clone();
                            if let Some(price) = price_str.trim().parse::<Decimal>().ok().filter(|p| *p > Decimal::ZERO) {
                                state.with_conn(|conn| {
                                    let _ = HoldingService::update_last_price(conn, &id, price);
                                });
                                edit_price_id.set(None);
                                load();
                            }
                        }
                    },
                    on_price_cancel: move |_| { edit_price_id.set(None); }
                }
            }
        }
    }
}

// ─── HoldingCard ─────────────────────────────────────────────────────────────

#[component]
fn HoldingCard(
    item: HoldingWithPnl,
    is_editing_price: bool,
    price_input: String,
    on_edit_price:   EventHandler<MouseEvent>,
    on_price_input:  EventHandler<String>,
    on_price_save:   EventHandler<MouseEvent>,
    on_price_cancel: EventHandler<MouseEvent>,
) -> Element {
    let t = use_i18n();
    let h = &item.holding;
    let asset_color = match h.asset_type {
        AssetType::Stock  => theme::PRIMARY,
        AssetType::Crypto => "#f59e0b",
    };
    let asset_icon = match h.asset_type {
        AssetType::Stock  => IconName::BarChart,
        AssetType::Crypto => IconName::Coins,
    };

    let pnl_color = item.unrealized_pnl
        .map(|p| if p >= Decimal::ZERO { theme::POSITIVE } else { theme::NEGATIVE })
        .unwrap_or(theme::TEXT_MUTED);

    let pnl_str = item.unrealized_pnl
        .map(|p| {
            let sign = if p >= Decimal::ZERO { "+" } else { "" };
            format!("{sign}{p:.0}")
        })
        .unwrap_or("—".into());

    let pnl_pct_str = item.unrealized_pnl_pct()
        .map(|p| format!("{p:.2}%"))
        .unwrap_or("—".into());

    rsx! {
        div {
            style: "{theme::CARD}",

            // ── Symbol row ──
            div {
                style: "display:flex; align-items:center; gap:10px; margin-bottom:10px;",
                IconTile { icon: None, fallback: asset_icon, color: asset_color.to_string(), size: 40, filled: false }
                div { style: "flex:1;",
                    div { style: "font-size:15px; font-weight:600; color:{theme::TEXT};", "{h.symbol}" }
                    if let Some(ref name) = h.name {
                        div { style: "font-size:12px; color:{theme::TEXT_MUTED};", "{name}" }
                    }
                }
                div { style: "text-align:right;",
                    div { style: "font-size:14px; font-weight:700; color:{pnl_color};", "{pnl_str}" }
                    div { style: "font-size:12px; color:{pnl_color};", "{pnl_pct_str}" }
                }
            }

            // ── Stats grid ──
            div {
                style: "display:grid; grid-template-columns:repeat(3,1fr); gap:8px; margin-bottom:10px;",
                StatCell { label: t.t("Qty"),       value: format!("{:.4}", h.quantity) }
                StatCell { label: t.t("Avg Cost"),  value: format!("{:.0}", h.avg_buy_price) }
                StatCell { label: t.t("Last Price"), value: h.last_price.map(|p| format!("{p:.0}")).unwrap_or("—".into()) }
                StatCell { label: t.t("Cost Basis"), value: format!("{:.0}", item.cost_basis) }
                StatCell { label: t.t("Mkt Value"),  value: item.market_value.map(|v| format!("{v:.0}")).unwrap_or("—".into()) }
                StatCell { label: t.t("Type"),       value: t.code(&h.asset_type.to_string()) }
            }

            // ── Price update ──
            if is_editing_price {
                div {
                    style: "display:flex; gap:8px; align-items:center;",
                    AmountInput {
                        style: "flex:1; min-width:0; box-sizing:border-box; border:1px solid #3f51b5; border-radius:0; padding:10px 12px; font-size:14px; outline:none; color:#1f2133;",
                        placeholder: t.t("New price"),
                        value: price_input.clone(),
                        oninput: move |v| on_price_input.call(v),
                    }
                    button {
                        style: "{theme::BTN_PRIMARY} padding:10px 14px; font-size:13px;",
                        onclick: move |e| { on_price_save.call(e); },
                        {t.t("Save")}
                    }
                    button {
                        style: "{theme::BTN_SECONDARY} padding:10px 12px; font-size:13px; display:flex; align-items:center;",
                        onclick: move |e| { on_price_cancel.call(e); },
                        Icon { name: IconName::X, size: 16 }
                    }
                }
            } else {
                button {
                    style: "width:100%; padding:10px; border:1px dashed #e6e6ef; border-radius:0; background:transparent; color:#8a8d9f; font-size:13px; cursor:pointer;",
                    onclick: move |e| { on_edit_price.call(e); },
                    {t.t("Update last price")}
                }
            }
        }
    }
}

#[component]
fn PortfolioStat(label: &'static str, value: String, color: &'static str) -> Element {
    rsx! {
        div {
            div { style: "font-size:10px; opacity:.7; margin-bottom:2px;", "{label}" }
            div { style: "font-size:14px; font-weight:600; color:{color};", "{value}" }
        }
    }
}

#[component]
fn StatCell(label: &'static str, value: String) -> Element {
    rsx! {
        div {
            style: "background:{theme::BG}; border-radius:0; padding:6px 8px;",
            div { style: "font-size:10px; color:{theme::TEXT_MUTED}; margin-bottom:1px;", "{label}" }
            div { style: "font-size:12px; font-weight:600; color:{theme::TEXT};", "{value}" }
        }
    }
}
