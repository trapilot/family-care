use dioxus::prelude::*;
use crate::i18n::use_i18n;
use rust_decimal::Decimal;
use std::str::FromStr;

use crate::app::AppState;
use crate::models::{Currency, NewTransaction, NewWallet, Wallet, WalletType};
use crate::repository::{CurrencyRepo, WalletRepo};
use crate::services::TransactionService;
use super::WalletView;
use crate::views::{currency_name, now_ts, AmountInput};

#[component]
pub fn WalletForm(view: Signal<WalletView>, editing: Option<Wallet>) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();

    let mut name        = use_signal(|| editing.as_ref().map(|w| w.name.clone()).unwrap_or_default());
    let mut wallet_type = use_signal(|| editing.as_ref().map(|w| w.wallet_type.clone()).unwrap_or(WalletType::Bank));
    // Opening balance rows: the active money types (sort order) plus any hidden type the
    // wallet already has an opening record for
    let (money_types, opening) = {
        let editing = editing.clone();
        state.with_conn(|conn| {
            let all = CurrencyRepo::list(conn).unwrap_or_default();
            let opening = editing.as_ref()
                .and_then(|w| TransactionService::find_opening_balances(conn, &w.id).ok())
                .unwrap_or_default();
            (all, opening)
        })
    };
    let rows: Vec<Currency> = money_types.iter()
        .filter(|i| i.is_active || opening.iter().any(|o| o.currency == i.code))
        .map(|i| i.code.clone())
        .collect();
    // Opening balance per money type: the wallet's "Opening Balance" transaction (editable later, 0 allowed)
    let mut balances = use_signal({
        let rows = rows.clone();
        move || rows.iter().map(|c| {
            let amount = opening.iter().find(|o| &o.currency == c)
                .map(|o| o.amount.normalize().to_string())
                .unwrap_or_else(|| "0".into());
            (c.clone(), amount)
        }).collect::<Vec<(Currency, String)>>()
    });
    let mut broker      = use_signal(|| editing.as_ref().and_then(|w| w.broker.clone()).unwrap_or_default());
    let mut icon        = use_signal(|| editing.as_ref().and_then(|w| w.icon.clone()).unwrap_or_default());
    let error_msg   = use_signal(|| String::new());
    // Editing only: whether the wallet is unused (see `WalletRepo::can_delete`), and the confirm dialog
    let can_delete = use_signal({
        let state = state.clone();
        let editing = editing.clone();
        move || editing.as_ref().is_some_and(|w| state.with_conn(|conn| WalletRepo::can_delete(conn, &w.id).unwrap_or(false)))
    });
    let mut confirm_delete = use_signal(|| false);

    let title = if editing.is_some() { t.t("Edit Wallet") } else { t.t("New Wallet") };

    let submit = {
        let state = state.clone();
        let name = name.clone();
        let wallet_type = wallet_type.clone();
        let balances = balances.clone();
        let broker = broker.clone();
        let icon = icon.clone();
        let mut error_msg = error_msg.clone();
        let editing = editing.clone();

        move |_: MouseEvent| {
            let name_val = name.read().trim().to_uppercase();
            if name_val.is_empty() {
                error_msg.set(t.t("Name is required").into());
                return;
            }
            // Parse every row first so nothing is written when one is invalid (empty = 0)
            let mut parsed: Vec<(Currency, Decimal)> = vec![];
            for (cur, raw) in balances.read().iter() {
                let amount = match raw.as_str() {
                    "" => Decimal::ZERO,
                    s  => match Decimal::from_str(s) {
                        Ok(b) => b,
                        Err(_) => { error_msg.set(t.t("Invalid balance").into()); return; }
                    },
                };
                parsed.push((cur.clone(), amount));
            }
            let broker_opt = {
                let b = broker.read();
                if b.is_empty() { None } else { Some(b.clone()) }
            };
            let icon_opt = {
                let i = icon.read();
                if i.is_empty() { None } else { Some(i.clone()) }
            };

            let result = state.with_conn(|conn| -> crate::error::Result<()> {
                if let Some(ref old) = editing {
                    let mut updated = old.clone();
                    updated.name        = name_val.clone();
                    updated.wallet_type = wallet_type.read().clone();
                    updated.broker      = broker_opt.clone();
                    updated.icon        = icon_opt.clone();
                    WalletRepo::update(conn, &updated)?;
                    for (cur, amount) in &parsed {
                        TransactionService::set_opening_balance(conn, &updated.id, *amount, cur.clone(), t.t("Opening Balance"))?;
                    }
                    Ok(())
                } else {
                    // The balance only ever moves through transactions, so the opening amounts
                    // are recorded as "Opening Balance" transactions on a wallet created empty.
                    let wallet = WalletRepo::create(conn, &NewWallet {
                        name: name_val.clone(),
                        wallet_type: wallet_type.read().clone(),
                        broker: broker_opt,
                        icon: icon_opt,
                        color: None,
                        sort_order: 0,
                    })?;
                    // One record per non-zero money type; if all are zero, a single 0 record
                    // so the wallet keeps a dated opening balance that can be edited later
                    let mut openings: Vec<(Currency, Decimal)> = parsed.iter().filter(|(_, a)| !a.is_zero()).cloned().collect();
                    if openings.is_empty() {
                        if let Some((cur, _)) = parsed.first() {
                            openings.push((cur.clone(), Decimal::ZERO));
                        }
                    }
                    for (cur, amount) in openings {
                        let mut opening = NewTransaction::opening_balance(&wallet.id, amount, cur, now_ts());
                        opening.note = Some(t.t("Opening Balance").to_string());
                        TransactionService::execute(conn, &opening)?;
                    }
                    Ok(())
                }
            });

            match result {
                Ok(_)  => { view.set(WalletView::List); }
                Err(e) => { error_msg.set(e.to_string()); }
            }
        }
    };

    rsx! {
        div {
            style: "padding:16px; display:flex; flex-direction:column; gap:14px;",

            // ── Header ──
            div {
                style: "display:flex; align-items:center; gap:12px;",
                button {
                    style: "border:none; background:transparent; font-size:22px; cursor:pointer; padding:0;",
                    onclick: move |_| { view.set(WalletView::List); },
                    "←"
                }
                h1 { style: "margin:0; font-size:18px; font-weight:700; color:#1f2937;", "{title}" }
            }

            // ── Type (first option = default for a new wallet) ──
            FieldLabel { label: t.t("Type") }
            select {
                style: "border:1px solid #e5e7eb; border-radius:8px; padding:10px 12px; font-size:16px; outline:none; background:#fff; width:100%; box-sizing:border-box;",
                oninput: move |e| {
                    if let Ok(wt) = e.value().parse::<WalletType>() { wallet_type.set(wt); }
                },
                for (wt, label, icon_str) in [
                    (WalletType::Bank,       t.t("Bank"),       "🏦"),
                    (WalletType::Cash,       t.t("Cash"),       "💵"),
                    (WalletType::EWallet,    t.t("E-Wallet"),   "📱"),
                    (WalletType::Investment, t.t("Investment"), "📈"),
                    (WalletType::Loan,       t.t("Loan"),       "🤝"),
                ] {
                    option {
                        value: "{wt}",
                        selected: *wallet_type.read() == wt,
                        "{icon_str} {label}"
                    }
                }
            }

            // ── Name ──
            FieldLabel { label: t.t("Name") }
            input {
                style: "border:1px solid #e5e7eb; border-radius:8px; padding:10px 12px; font-size:16px; outline:none; background:#fff;",
                r#type: "text",
                // Wallet names are UPPERCASE: caps keyboard on phones, and converted as you type
                autocapitalize: "characters",
                placeholder: t.t("e.g. HSBC"),
                value: "{name.read()}",
                oninput: move |e| { name.set(e.value().to_uppercase()); },
            }

            // ── Opening balance, one row per money type (new and edit) ──
            FieldLabel { label: t.t("Opening Balance") }
            for (cur, amount) in balances.read().clone() {
                {
                    let cur_for_input = cur.clone();
                    let label = currency_name(&t, &money_types, &cur);
                    rsx! {
                        div {
                            style: "display:flex; flex-direction:column; gap:4px;",
                            span { style: "font-size:12px; color:#6b7280;", "{label}" }
                            AmountInput {
                                value: amount,
                                oninput: move |v: String| {
                                    for row in balances.write().iter_mut() {
                                        if row.0 == cur_for_input { row.1 = v.clone(); }
                                    }
                                },
                            }
                        }
                    }
                }
            }

            // ── Broker (investment only) ──
            if *wallet_type.read() == WalletType::Investment {
                FieldLabel { label: t.t("Broker") }
                input {
                    style: "border:1px solid #e5e7eb; border-radius:8px; padding:10px 12px; font-size:14px; outline:none; background:#fff;",
                    r#type: "text",
                    placeholder: t.t("e.g. HSBC, VPS, Binance, OKX"),
                    value: "{broker.read()}",
                    oninput: move |e| { broker.set(e.value()); },
                }
            }

            // ── Icon ──
            FieldLabel { label: t.t("Icon (emoji, optional)") }
            input {
                style: "border:1px solid #e5e7eb; border-radius:8px; padding:10px 12px; font-size:22px; outline:none; background:#fff; width:60px;",
                r#type: "text",
                placeholder: "🏦",
                value: "{icon.read()}",
                oninput: move |e| {
                    icon.set(e.value());
                },
            }

            // ── Error ──
            if !error_msg.read().is_empty() {
                div {
                    style: "background:#fef2f2; border-radius:8px; padding:10px 12px; font-size:13px; color:#ef4444;",
                    "{error_msg.read()}"
                }
            }

            // ── Submit ──
            button {
                style: "padding:14px; background:#6366f1; color:#fff; border:none; border-radius:12px; font-size:16px; font-weight:700; cursor:pointer; margin-top:4px;",
                onclick: submit,
                if editing.is_some() { {t.t("Update Wallet")} } else { {t.t("Create Wallet")} }
            }

            // ── Delete (unused wallets only) ──
            if let Some(wallet) = editing.clone() {
                if can_delete() {
                    button {
                        style: "padding:12px; background:transparent; color:#ef4444; border:1px solid #ef4444; border-radius:12px; font-size:15px; font-weight:600; cursor:pointer;",
                        onclick: move |_| confirm_delete.set(true),
                        {t.t("🗑 Delete wallet")}
                    }
                } else {
                    div { style: "font-size:12px; color:#9ca3af; text-align:center;",
                        {t.t("This wallet has transactions, so it can't be deleted.")}
                    }
                }

                if confirm_delete() {
                    div {
                        style: "position:fixed; inset:0; background:rgba(0,0,0,.45); display:flex; align-items:center; justify-content:center; z-index:100; padding:20px;",
                        div {
                            style: "background:#fff; border-radius:16px; padding:22px; max-width:320px; width:100%;",
                            h3 { style: "margin:0 0 8px; font-size:17px; font-weight:700;", {t.t("Delete wallet?")} }
                            p { style: "margin:0 0 18px; font-size:14px; color:#6b7280; line-height:1.5;",
                                {t.t("\"{name}\" will be permanently deleted.").replace("{name}", &wallet.name)}
                            }
                            div { style: "display:flex; gap:10px;",
                                button {
                                    style: "flex:1; padding:11px; border:1px solid #e5e7eb; border-radius:8px; background:#fff; font-size:14px; cursor:pointer;",
                                    onclick: move |_| confirm_delete.set(false),
                                    {t.t("Cancel")}
                                }
                                button {
                                    style: "flex:1; padding:11px; border:none; border-radius:8px; background:#ef4444; color:#fff; font-size:14px; font-weight:700; cursor:pointer;",
                                    onclick: {
                                        let state = state.clone();
                                        let mut error_msg = error_msg.clone();
                                        let id = wallet.id.clone();
                                        move |_| {
                                            confirm_delete.set(false);
                                            match state.with_conn(|conn| WalletRepo::delete(conn, &id)) {
                                                Ok(())  => view.set(WalletView::List),
                                                Err(e)  => error_msg.set(e.to_string()),
                                            }
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
}

#[component]
fn FieldLabel(label: &'static str) -> Element {
    rsx! {
        label {
            style: "font-size:12px; font-weight:600; color:#374151; text-transform:uppercase; letter-spacing:.5px;",
            "{label}"
        }
    }
}
