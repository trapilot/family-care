use dioxus::prelude::*;
use crate::i18n::use_i18n;

use crate::app::AppState;
use crate::models::{Currency, CurrencyInfo, Wallet, WalletType};
use crate::repository::{CreditCardRepo, CurrencyRepo, WalletRepo};
use crate::services::CreditCardService;
use crate::models::CardSummary;
use crate::views::fmt_date;
use std::collections::HashMap;
use crate::views::icons::{Icon, IconName};
use crate::views::ui::{HeaderAction, ScreenHeader, IconTile};
use crate::views::{currency_name, fmt_currency, fmt_vnd, shown_totals, theme};
use crate::views::rates::ConvertedTotal;
use rust_decimal::Decimal;
use chrono::NaiveDate;
use super::WalletView;

#[component]
pub fn WalletList(view: Signal<WalletView>) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let wallets = use_signal(|| vec![]);
    let money_types = use_signal(Vec::<CurrencyInfo>::new);
    // Credit card statement summaries by wallet id, computed once per load
    let card_summaries = use_signal(HashMap::<String, CardSummary>::new);

    use_effect({
        let state = state.clone();
        let mut wallets = wallets.clone();
        let mut money_types = money_types;
        let mut card_summaries = card_summaries;
        move || {
            state.with_conn(|conn| {
                let today = chrono::Local::now().date_naive();
                let summaries: HashMap<String, CardSummary> = CreditCardRepo::list(conn).unwrap_or_default().iter()
                    .filter_map(|c| CreditCardService::summary(conn, c, today).ok().map(|s| (c.wallet_id.clone(), s)))
                    .collect();
                card_summaries.set(summaries);
                if let Ok(ws) = WalletRepo::list_all(conn) { wallets.set(ws); }
                if let Ok(list) = CurrencyRepo::list(conn) { money_types.set(list); }
            });
        }
    });

    // Groups list active wallets only; disabled ones are listed apart. Totals and the credit
    // summary still count every wallet (a disabled wallet is kept for tracking).
    let (active, disabled): (Vec<Wallet>, Vec<Wallet>) = wallets.read().iter().cloned().partition(|w| w.is_active);

    // Group wallets by type, in display order (empty groups are skipped)
    let groups: Vec<(&'static str, Vec<Wallet>)> = [
        (WalletType::Cash,       "Cash"),
        (WalletType::Bank,       "Bank"),
        (WalletType::EWallet,    "E-Wallet"),
        (WalletType::CreditCard, "Credit Card"),
        (WalletType::Investment, "Investment"),
        (WalletType::Loan,       "Loan"),
    ]
    .into_iter()
    .map(|(wallet_type, title)| {
        let ws: Vec<Wallet> = active.iter()
            .filter(|w| w.wallet_type == wallet_type)
            .cloned().collect();
        (title, ws)
    })
    .filter(|(_, ws)| !ws.is_empty())
    .collect();

    // Totals exclude investment wallets (their value lives in holdings) and credit cards (borrowed money, not owned)
    let regular: Vec<Wallet> = wallets.read().iter()
        .filter(|w| !matches!(w.wallet_type, WalletType::Investment | WalletType::CreditCard))
        .cloned().collect();

    // Net worth per currency
    let total_of = |c: Currency| -> rust_decimal::Decimal { regular.iter().map(|w| w.balance(&c)).sum() };
    let (total_vnd, total_usd, total_gold) = (total_of(Currency::VND), total_of(Currency::USD), total_of(Currency::GOLD));
    let rows = shown_totals(&money_types.read(), total_vnd, total_usd, total_gold);

    // Credit card summary (cards are excluded from the totals above)
    let credit: Vec<Wallet> = wallets.read().iter()
        .filter(|w| w.wallet_type == WalletType::CreditCard)
        .cloned().collect();
    let owed: Decimal = credit.iter().map(|w| (-w.balance(&Currency::VND)).max(Decimal::ZERO)).sum();
    let sums: Vec<CardSummary> = credit.iter()
        .filter_map(|w| card_summaries.read().get(&w.id).cloned())
        .collect();
    let due_now: Decimal = sums.iter().filter(|s| !s.is_paid).map(|s| s.remaining_due).sum();
    let next_due = sums.iter()
        .filter(|s| !s.is_paid && s.remaining_due > Decimal::ZERO)
        .map(|s| s.due_date).min();
    let overdue = next_due.is_some_and(|d| d < chrono::Local::now().date_naive());
    let has_limit = sums.iter().any(|s| s.available.is_some());
    let limit: Option<Decimal> = has_limit.then(|| sums.iter()
        .filter_map(|s| s.available.map(|a| a + s.owed_total)).sum());
    let available: Option<Decimal> = has_limit.then(|| sums.iter().filter_map(|s| s.available).sum());

    rsx! {
        div {
            style: "display:flex; flex-direction:column;",

            ScreenHeader {
                title: t.t("Wallets").to_string(),
                action: rsx! {
                    HeaderAction {
                        icon: IconName::Plus,
                        label: t.t("Add").to_string(),
                        onclick: move |_| { view.set(WalletView::Add); },
                    }
                },
            }

            div {
                style: "padding:16px; display:flex; flex-direction:column; gap:16px;",

                // ── Totals per money type ──
                div {
                    style: "background:{theme::SUMMARY_BG}; color:{theme::TEXT}; padding:14px 16px; display:flex; flex-direction:column; gap:8px;",
                    for (cur, total) in rows {
                        div {
                            style: "display:flex; justify-content:space-between; align-items:baseline; gap:8px;",
                            span { style: "font-size:13px; color:{theme::MUTED_ON_PLATINUM};", "{currency_name(&t, &money_types.read(), &cur)}" }
                            span {
                                style: "font-size:18px; font-weight:700; color:{theme::signed_on_platinum(total)}; overflow-wrap:anywhere;",
                                "{fmt_currency(total, &cur)}"
                            }
                        }
                    }
                }
                ConvertedTotal { vnd: total_vnd, usd: total_usd, gold: total_gold }
                if !credit.is_empty() {
                    CreditSummary { owed, due_now, next_due, overdue, limit, available }
                }

                // ── Wallets grouped by type: Cash > Bank > E-Wallet > Investment > Loan ──
                for (title, group) in groups.iter() {
                    {
                        let vnd_sum: Decimal = group.iter().map(|w| w.balance(&Currency::VND)).sum();
                        let has_other = group.iter().any(|w| w.balances.iter().any(|b| b.currency != Currency::VND && !b.balance.is_zero()));
                        let trailing = if vnd_sum.is_zero() && has_other { None } else { Some(fmt_vnd(vnd_sum)) };
                        let trailing_color = theme::signed_color(vnd_sum);
                        rsx! {
                            div {
                                style: "{theme::CARD} padding-top:4px; padding-bottom:4px;",
                                div {
                                    style: "display:flex; justify-content:space-between; align-items:baseline; gap:8px; padding:10px 0; border-bottom:1px solid #e6e6ef;",
                                    span { style: "font-size:15px; font-weight:700; color:{theme::TEXT};", "{t.t(title)}" }
                                    if let Some(trailing) = trailing {
                                        span { style: "font-size:15px; font-weight:600; color:{trailing_color};", "{trailing}" }
                                    }
                                }
                                for (i, wallet) in group.iter().enumerate() {
                                    div {
                                        key: "{wallet.id}",
                                        style: if i > 0 { "border-top:1px solid #e6e6ef; padding:6px 0;" } else { "padding:6px 0;" },
                                        WalletRow {
                                            wallet: wallet.clone(),
                                            card: card_summaries.read().get(&wallet.id).cloned(),
                                            on_tap: {
                                                let w = wallet.clone();
                                                move |_| { view.set(WalletView::Detail(w.clone())); }
                                            },
                                            on_edit: {
                                                let w = wallet.clone();
                                                move |_| { view.set(WalletView::Edit(w.clone())); }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if !disabled.is_empty() {
                    div {
                        style: "{theme::CARD} padding-top:4px; padding-bottom:4px;",
                        div {
                            style: "padding:10px 0; border-bottom:1px solid #e6e6ef;",
                            span { style: "font-size:15px; font-weight:700; color:{theme::TEXT};", "{t.t(\"Disabled\")}" }
                        }
                        for (i, wallet) in disabled.iter().enumerate() {
                            div {
                                key: "{wallet.id}",
                                style: if i > 0 { "opacity:.5; border-top:1px solid #e6e6ef; padding:6px 0;" } else { "opacity:.5; padding:6px 0;" },
                                WalletRow {
                                    wallet: wallet.clone(),
                                    card: None,
                                    on_tap: {
                                        let w = wallet.clone();
                                        move |_| { view.set(WalletView::Detail(w.clone())); }
                                    },
                                    on_edit: {
                                        let w = wallet.clone();
                                        move |_| { view.set(WalletView::Edit(w.clone())); }
                                    }
                                }
                            }
                        }
                    }
                }

                if wallets.read().is_empty() {
                    div {
                        style: "text-align:center; padding:40px 0; color:{theme::TEXT_MUTED}; display:flex; flex-direction:column; align-items:center; gap:8px;",
                        Icon { name: IconName::Wallet, size: 40, color: theme::TEXT_MUTED.to_string() }
                        p { style: "font-size:14px; margin:0;", {t.t("No wallets yet. Add one!")} }
                    }
                }
            }
        }
    }
}

// ─── Sub-components ───────────────────────────────────────────────────────────

#[component]
fn CreditSummary(
    owed: Decimal,
    due_now: Decimal,
    next_due: Option<NaiveDate>,
    overdue: bool,
    limit: Option<Decimal>,
    available: Option<Decimal>,
) -> Element {
    let t = use_i18n();
    let row = "display:flex; justify-content:space-between; align-items:baseline; gap:8px;";
    let owed_color = theme::signed_color(-owed);

    rsx! {
        div {
            style: "{theme::CARD} display:flex; flex-direction:column; gap:8px; border-left:3px solid #e11d48;",
            div {
                style: "display:flex; align-items:center; gap:8px;",
                Icon { name: IconName::CreditCard, size: 18, color: "#e11d48".to_string() }
                span { style: "font-size:13px; font-weight:600; color:{theme::TEXT};", {t.t("Credit cards")} }
            }
            div {
                style: "{row}",
                span { style: "font-size:13px; color:{theme::TEXT_MUTED};", {t.t("Total owed")} }
                span { style: "font-size:14px; font-weight:600; color:{owed_color}; overflow-wrap:anywhere;", "{fmt_vnd(owed)}" }
            }
            if due_now > Decimal::ZERO {
                div {
                    style: "{row}",
                    span { style: "font-size:13px; color:{theme::TEXT_MUTED};", {t.t("To pay this statement")} }
                    span { style: "font-size:14px; font-weight:600; color:{theme::signed_color(-due_now)}; overflow-wrap:anywhere;", "{fmt_vnd(due_now)}" }
                }
                if let Some(d) = next_due {
                    {
                        let color = if overdue { theme::DANGER } else { theme::TEXT_MUTED };
                        rsx! {
                            div {
                                style: "font-size:12px; color:{color};",
                                "{t.t(\"Due\")} {fmt_date(d)}"
                                if overdue { " · {t.t(\"overdue\")}" }
                            }
                        }
                    }
                }
            }
            if let Some(limit) = limit {
                div {
                    style: "{row}",
                    span { style: "font-size:13px; color:{theme::TEXT_MUTED};", {t.t("Credit limit")} }
                    span { style: "font-size:14px; font-weight:600; color:{theme::TEXT}; overflow-wrap:anywhere;", "{fmt_vnd(limit)}" }
                }
            }
            if let Some(avail) = available {
                div {
                    style: "{row}",
                    span { style: "font-size:13px; color:{theme::TEXT_MUTED};", {t.t("Available")} }
                    {
                        let color = theme::signed_color(avail);
                        rsx! { span { style: "font-size:14px; font-weight:600; color:{color}; overflow-wrap:anywhere;", "{fmt_vnd(avail)}" } }
                    }
                }
            }
        }
    }
}

#[component]
fn WalletRow(
    wallet: Wallet,
    card: Option<CardSummary>,
    on_tap:  EventHandler<MouseEvent>,
    on_edit: EventHandler<MouseEvent>,
) -> Element {
    let (type_icon, type_color) = match wallet.wallet_type {
        WalletType::Cash       => (IconName::Banknote, "#17a589"),
        WalletType::Bank       => (IconName::Landmark, "#3f51b5"),
        WalletType::Loan       => (IconName::Handshake, "#bb3baa"),
        WalletType::EWallet    => (IconName::Smartphone, "#f59e0b"),
        WalletType::CreditCard => (IconName::CreditCard, "#e11d48"),
        WalletType::Investment => (IconName::TrendingUp, "#8b5cf6"),
    };
    let t = use_i18n();
    let balances = wallet.display_balances();
    let several = balances.len() > 1;

    rsx! {
        div {
            style: "display:flex; align-items:center; gap:10px; cursor:pointer;",
            onclick: move |e| { on_tap.call(e); },

            IconTile { icon: wallet.icon.clone(), fallback: type_icon, color: type_color.to_string(), size: 30, filled: true }

            div { style: "flex:1; min-width:0;",
                div { style: "font-size:13px; color:{theme::TEXT}; overflow-wrap:anywhere;", "{wallet.name}" }
                if let Some(s) = card.as_ref().filter(|s| !s.is_paid && s.remaining_due > Decimal::ZERO) {
                    {
                        let overdue = chrono::Local::now().date_naive() > s.due_date;
                        let color = if overdue { theme::DANGER } else { theme::TEXT_MUTED };
                        rsx! {
                            div { style: "font-size:11px; color:{color};", "{t.t(\"Due\")} {fmt_date(s.due_date)}" }
                        }
                    }
                }
                for b in balances {
                    div {
                        style: {
                            let size = if several { 12 } else { 13 };
                            let color = theme::signed_color(b.balance);
                            format!("font-size:{size}px; color:{color}; overflow-wrap:anywhere;")
                        },
                        "{fmt_currency(b.balance, &b.currency)}"
                    }
                }
                // The type is already the group header, so only the broker is shown here
                if let Some(ref broker) = wallet.broker {
                    div { style: "font-size:11px; color:{theme::TEXT_MUTED}; margin-top:1px;", "{broker}" }
                }
            }

            button {
                style: "border:none; background:transparent; cursor:pointer; padding:4px; flex-shrink:0; display:flex;",
                onclick: move |e| { e.stop_propagation(); on_edit.call(e); },
                Icon { name: IconName::Pencil, size: 16, color: theme::TEXT_MUTED.to_string() }
            }
        }
    }
}
