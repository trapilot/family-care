use dioxus::prelude::*;
use rust_decimal::Decimal;
use std::time::Duration;

use crate::app::AppState;
use crate::i18n::use_i18n;
use crate::models::{Currency, ExchangeRate};
use crate::repository::{CurrencyRepo, RateRepo};
use crate::services::RateService;
use crate::views::icons::{Icon, IconName};
use crate::views::ui::IconText;
use crate::views::{currency_name, fmt_vnd, theme, AmountInput};

/// How often the background task checks for stale rates (a fetch only happens when stale)
const CHECK_EVERY: Duration = Duration::from_secs(15 * 60);

/// Latest exchange rates, shared by every screen
#[derive(Clone, Copy, PartialEq)]
pub struct Rates(pub Signal<Vec<ExchangeRate>>);

fn load_rates(state: &AppState) -> Vec<ExchangeRate> {
    state.with_conn(|conn| RateRepo::list(conn).unwrap_or_default())
}

/// Provide `Rates` to the tree and keep them fresh while the app is open.
pub fn use_rates_provider() -> Rates {
    let state = use_context::<AppState>();
    let mut rates = use_signal({
        let state = state.clone();
        move || load_rates(&state)
    });

    use_future(move || {
        let state = state.clone();
        async move {
            loop {
                if RateService::refresh(state.db.clone(), false).await {
                    rates.set(load_rates(&state));
                }
                tokio::time::sleep(CHECK_EVERY).await;
            }
        }
    });

    use_context_provider(|| Rates(rates))
}

// ─── ConvertedTotal ───────────────────────────────────────────────────────────

/// "≈ total in VND" card under the per-currency totals, with the rates used.
/// Each rate can be typed in by hand; "Update rates" fetches the latest online.
#[component]
pub fn ConvertedTotal(vnd: Decimal, usd: Decimal, gold: Decimal) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let Rates(mut rates) = use_context::<Rates>();
    let mut refreshing = use_signal(|| false);
    // Result of the last "Update rates" tap: Some(true) updated, Some(false) failed
    let mut refresh_result = use_signal(|| None::<bool>);

    let (total, missing) = RateService::total_vnd(vnd, usd, gold, &rates.read());
    let money_types = state.with_conn(|conn| CurrencyRepo::list(conn).unwrap_or_default());
    let active = state.with_conn(|conn| CurrencyRepo::list_active(conn).unwrap_or_default());
    let is_shown = |c: &Currency| active.iter().any(|i| &i.code == c);
    let has_rates = is_shown(&Currency::USD) || is_shown(&Currency::GOLD);
    let missing_names: Vec<String> = missing.iter().map(|c| currency_name(&t, &money_types, c)).collect();
    let usd_name = currency_name(&t, &money_types, &Currency::USD);
    let gold_unit = crate::views::gold_unit();

    let on_refresh = move |_| {
        if refreshing() { return; }
        refreshing.set(true);
        refresh_result.set(None);
        let state = state.clone();
        spawn(async move {
            let updated = RateService::refresh(state.db.clone(), true).await;
            if updated {
                rates.set(load_rates(&state));
            }
            refresh_result.set(Some(updated));
            refreshing.set(false);
        });
    };

    rsx! {
        div {
            style: "{theme::CARD} display:flex; flex-direction:column; gap:10px;",
            div {
                div { style: "font-size:13px; font-weight:600; color:{theme::TEXT_MUTED};", {t.t("Total (converted)")} }
                div { style: "font-size:16px; font-weight:600; color:{theme::TEXT}; margin-top:2px; overflow-wrap:anywhere;", "≈ {fmt_vnd(total)}" }
                if !missing_names.is_empty() {
                    div { style: "font-size:12px; color:#f59e0b; margin-top:2px;",
                        {t.t("Not included (no rate yet): {list}").replace("{list}", &missing_names.join(", "))}
                    }
                }
            }

            if is_shown(&Currency::USD) {
                RateRow { currency: Currency::USD, unit: usd_name }
            }
            if is_shown(&Currency::GOLD) {
                RateRow { currency: Currency::GOLD, unit: gold_unit }
            }

            // Only VND shown: there is no rate to update
            if has_rates {
                button {
                    style: "{theme::BTN_SECONDARY} padding:8px 14px; font-size:13px; display:flex; align-items:center; justify-content:center; gap:6px;",
                    disabled: refreshing(),
                    onclick: on_refresh,
                    if refreshing() {
                        Icon { name: IconName::RefreshCw, size: 16 }
                        {t.t("Updating…")}
                    } else {
                        IconText { text: t.t("↻ Update rates").to_string() }
                    }
                }
                match refresh_result() {
                    Some(true)  => rsx! { div { style: "font-size:12px; color:{theme::POSITIVE};", {t.t("Rates updated")} } },
                    Some(false) => rsx! { div { style: "font-size:12px; color:{theme::DANGER};", {t.t("Couldn't update — check your internet connection")} } },
                    None        => rsx! {},
                }
            }
        }
    }
}

// ─── RateRow ──────────────────────────────────────────────────────────────────

/// One rate with its source/time and an inline editor for typing it in manually.
#[component]
fn RateRow(currency: Currency, unit: String) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let Rates(mut rates) = use_context::<Rates>();
    let mut editing = use_signal(|| false);
    let mut input = use_signal(String::new);
    let mut error = use_signal(|| None::<&'static str>);

    let rate = rates.read().iter().find(|r| r.currency == currency).cloned();
    let has_rate = rate.is_some();
    let value = rate.as_ref().map(|r| fmt_vnd(r.rate_vnd)).unwrap_or_else(|| "—".into());
    let meta = rate.as_ref().map(|r| {
        let source = if r.source == "manual" { t.t("Manual") } else { "Yahoo" };
        format!("{source} · {}", fmt_time(r.fetched_at))
    });

    let start_edit = {
        let rate = rate.clone();
        move |_| {
            // Prefill with the current whole-VND value
            input.set(rate.as_ref().map(|r| r.rate_vnd.round().to_string()).unwrap_or_default());
            error.set(None);
            editing.set(true);
        }
    };

    let currency_save = currency.clone();
    let state_clear = state.clone();
    let save = move |_| {
        // Empty = no rate (left out of the converted total)
        let result = if input.read().trim().is_empty() {
            state.with_conn(|conn| RateService::clear(conn, currency_save.clone()))
        } else {
            let rate_vnd = match input.read().parse::<Decimal>() {
                Ok(v) if v > Decimal::ZERO => v,
                _ => { error.set(Some(t.t("Enter a rate greater than 0, or leave it empty"))); return; }
            };
            // Saved like a fetched rate; the next online update replaces it when stale
            state.with_conn(|conn| RateService::save_manual(conn, currency_save.clone(), rate_vnd))
        };
        match result {
            Ok(_) => {
                rates.set(load_rates(&state));
                editing.set(false);
            }
            Err(_) => error.set(Some(t.t("Save failed"))),
        }
    };

    rsx! {
        if editing() {
            div { style: "display:flex; flex-direction:column; gap:6px;",
                div { style: "font-size:13px; color:{theme::TEXT_MUTED};", "1 {unit} = ? ₫" }
                div { style: "display:flex; gap:6px; align-items:center;",
                    AmountInput {
                        style: "flex:1; min-width:0; box-sizing:border-box; border:1px solid #3f51b5; border-radius:0; padding:10px 12px; font-size:15px; outline:none; color:#1f2133;",
                        value: input(),
                        placeholder: t.t("Empty = no rate").to_string(),
                        oninput: move |v| input.set(v),
                    }
                    button {
                        style: "{theme::BTN_PRIMARY} padding:10px 14px; font-size:13px;",
                        onclick: save,
                        {t.t("Save")}
                    }
                    button {
                        style: "{theme::BTN_SECONDARY} padding:10px 12px; font-size:13px; display:flex; align-items:center;",
                        onclick: move |_| editing.set(false),
                        Icon { name: IconName::X, size: 16 }
                    }
                }
                if let Some(err) = error() {
                    div { style: "font-size:12px; color:{theme::DANGER};", "{err}" }
                }
                if has_rate {
                    button {
                        style: "align-self:flex-start; border:none; background:transparent; color:{theme::DANGER}; font-size:12px; cursor:pointer; padding:2px 0;",
                        onclick: {
                            let state = state_clear.clone();
                            let currency = currency.clone();
                            move |_| match state.with_conn(|conn| RateService::clear(conn, currency.clone())) {
                                Ok(_)  => { rates.set(load_rates(&state)); editing.set(false); }
                                Err(_) => error.set(Some(t.t("Save failed"))),
                            }
                        },
                        {t.t("Clear (no rate)")}
                    }
                }
            }
        } else {
            div { style: "display:flex; align-items:center; gap:8px;",
                div { style: "flex:1; min-width:0;",
                    div { style: "font-size:14px; font-weight:600; color:{theme::TEXT}; overflow-wrap:anywhere;", "1 {unit} = {value}" }
                    div { style: "font-size:12px; color:{theme::TEXT_MUTED};",
                        match meta {
                            Some(meta) => rsx! { "{meta}" },
                            None       => {
                                let hint = t.t("No rate yet — update online or tap ✏️ to enter it").replace("✏️", "✏");
                                rsx! {
                                    for (i, part) in hint.split('✏').enumerate() {
                                        if i > 0 {
                                            span { style: "display:inline-flex; vertical-align:middle;",
                                                Icon { name: IconName::Pencil, size: 13 }
                                            }
                                        }
                                        "{part}"
                                    }
                                }
                            }
                        }
                    }
                }
                button {
                    style: "border:none; background:transparent; cursor:pointer; padding:6px; flex-shrink:0; display:flex; align-items:center;",
                    aria_label: t.t("Edit"),
                    onclick: start_edit,
                    Icon { name: IconName::Pencil, size: 18, color: theme::TEXT_MUTED.to_string() }
                }
            }
        }
    }
}

fn fmt_time(ts: i64) -> String {
    use chrono::{Local, TimeZone};
    Local.timestamp_opt(ts, 0).single()
        .map(|dt| dt.format("%H:%M %d/%m").to_string())
        .unwrap_or_default()
}
