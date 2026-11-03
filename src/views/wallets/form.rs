use dioxus::prelude::*;
use crate::i18n::use_i18n;
use rust_decimal::Decimal;
use std::str::FromStr;

use crate::app::AppState;
use crate::models::{Currency, NewTransaction, NewWallet, Wallet, WalletType};
use crate::repository::{CreditCardRepo, CurrencyRepo, WalletRepo};
use crate::services::{ReminderService, TransactionService};
use super::WalletView;
use crate::views::icons::{icon_for_emoji, Icon, IconName};
use crate::views::ui::{IconPicker, IconText, ScreenHeader};
use crate::views::{currency_name, now_ts, theme, AmountInput};

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
    // Credit card settings: statement day, payment due day and optional limit (empty for a new card)
    let existing_card = editing.as_ref().and_then(|w| state.with_conn(|conn| CreditCardRepo::get(conn, &w.id).ok().flatten()));
    let mut statement_day = use_signal(|| existing_card.as_ref().map(|c| c.statement_day.to_string()).unwrap_or_default());
    let mut due_day       = use_signal(|| existing_card.as_ref().map(|c| c.due_day.to_string()).unwrap_or_default());
    let mut credit_limit  = use_signal(|| existing_card.as_ref().and_then(|c| c.credit_limit).map(|l| l.normalize().to_string()).unwrap_or_default());
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
        let statement_day = statement_day.clone();
        let due_day = due_day.clone();
        let credit_limit = credit_limit.clone();
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
                        Ok(b) if b < Decimal::ZERO => { error_msg.set(t.t("Invalid balance").into()); return; }
                        Ok(b) => b,
                        Err(_) => { error_msg.set(t.t("Invalid balance").into()); return; }
                    },
                };
                parsed.push((cur.clone(), amount));
            }
            // Credit card settings, validated before anything is written
            let is_card = *wallet_type.read() == WalletType::CreditCard;
            let mut card_settings: Option<(u32, u32, Option<Decimal>)> = None;
            if is_card {
                let day = |raw: &str| raw.parse::<u32>().ok().filter(|d| (1..=31).contains(d));
                let (Some(statement), Some(due)) = (day(&statement_day.read()), day(&due_day.read())) else {
                    error_msg.set(t.t("Enter statement and due days (1–31)").into());
                    return;
                };
                let limit = match credit_limit.read().as_str() {
                    "" => None,
                    s  => match Decimal::from_str(s) {
                        Ok(l) if l >= Decimal::ZERO => Some(l),
                        _ => { error_msg.set(t.t("Invalid credit limit").into()); return; }
                    },
                };
                card_settings = Some((statement, due, limit));
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
                let wallet_id = if let Some(ref old) = editing {
                    let mut updated = old.clone();
                    updated.name        = name_val.clone();
                    updated.wallet_type = wallet_type.read().clone();
                    updated.broker      = broker_opt.clone();
                    updated.icon        = icon_opt.clone();
                    WalletRepo::update(conn, &updated)?;
                    for (cur, amount) in &parsed {
                        TransactionService::set_opening_balance(conn, &updated.id, *amount, cur.clone(), t.t("Opening Balance"))?;
                    }
                    updated.id
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
                    wallet.id
                };
                match card_settings {
                    Some((statement, due, limit)) => CreditCardRepo::upsert(conn, &wallet_id, statement, due, limit)?,
                    None if editing.is_some() => CreditCardRepo::delete(conn, &wallet_id)?,
                    None => {}
                }
                ReminderService::resync(conn)
            });

            match result {
                Ok(_)  => { view.set(WalletView::List); }
                Err(e) => { error_msg.set(e.to_string()); }
            }
        }
    };

    let (default_emoji, type_color) = match *wallet_type.read() {
        WalletType::Cash       => ("💵", "#17a589"),
        WalletType::Bank       => ("🏦", "#3f51b5"),
        WalletType::Loan       => ("🤝", "#bb3baa"),
        WalletType::EWallet    => ("📱", "#f59e0b"),
        WalletType::CreditCard => ("💳", "#e11d48"),
        WalletType::Investment => ("📈", "#8b5cf6"),
    };
    let icon_options: Vec<&'static str> = std::iter::once(default_emoji)
        .chain(["🏦", "💵", "📱", "💳", "📈", "🤝", "🐷", "💰"].into_iter().filter(|e| *e != default_emoji))
        .collect();

    rsx! {
        div {
            style: "display:flex; flex-direction:column; min-height:100%;",
            ScreenHeader {
                title: title.to_string(),
                on_back: Some(EventHandler::new(move |_| { view.set(WalletView::List); })),
            }
        div {
            style: "padding:16px; display:flex; flex-direction:column; gap:16px;",

            // ── Type (first option = default for a new wallet) ──
            FieldLabel { label: t.t("Type") }
            div {
                style: "display:flex; flex-wrap:wrap; gap:8px;",
                for (wt, label, icon_str) in [
                    (WalletType::Bank,       t.t("Bank"),       "🏦"),
                    (WalletType::Cash,       t.t("Cash"),       "💵"),
                    (WalletType::EWallet,    t.t("E-Wallet"),   "📱"),
                    (WalletType::CreditCard, t.t("Credit Card"), "💳"),
                    (WalletType::Investment, t.t("Investment"), "📈"),
                    (WalletType::Loan,       t.t("Loan"),       "🤝"),
                ] {
                    button {
                        key: "{wt}",
                        style: if *wallet_type.read() == wt {
                            "{theme::CHIP_ACTIVE} display:inline-flex; align-items:center; gap:6px;"
                        } else {
                            "{theme::CHIP} display:inline-flex; align-items:center; gap:6px;"
                        },
                        onclick: {
                            let wt = wt.clone();
                            move |_| wallet_type.set(wt.clone())
                        },
                        Icon { name: icon_for_emoji(icon_str).unwrap_or(IconName::Package), size: 16 }
                        "{label}"
                    }
                }
            }

            // ── Name ──
            FieldLabel { label: t.t("Name") }
            input {
                style: "{theme::INPUT}",
                r#type: "text",
                // Wallet names are UPPERCASE: caps keyboard on phones, and converted as you type
                autocapitalize: "characters",
                placeholder: t.t("e.g. HSBC"),
                value: "{name.read()}",
                oninput: move |e| { name.set(e.value().to_uppercase()); },
            }

            // ── Credit card: statement day, due day, limit ──
            if *wallet_type.read() == WalletType::CreditCard {
                div { style: "display:flex; gap:8px;",
                    div { style: "flex:1; display:flex; flex-direction:column; gap:6px;",
                        FieldLabel { label: t.t("Statement day") }
                        input {
                            style: "{theme::INPUT}",
                            r#type: "text",
                            inputmode: "numeric",
                            value: "{statement_day.read()}",
                            oninput: move |e| { statement_day.set(e.value().chars().filter(|c| c.is_ascii_digit()).take(2).collect()); },
                        }
                    }
                    div { style: "flex:1; display:flex; flex-direction:column; gap:6px;",
                        FieldLabel { label: t.t("Payment due day") }
                        input {
                            style: "{theme::INPUT}",
                            r#type: "text",
                            inputmode: "numeric",
                            value: "{due_day.read()}",
                            oninput: move |e| { due_day.set(e.value().chars().filter(|c| c.is_ascii_digit()).take(2).collect()); },
                        }
                    }
                }
                div { style: "font-size:12px; color:{theme::TEXT_MUTED}; margin-top:-10px;",
                    {t.t("Day of month (1–31). Short months use their last day.")}
                }
                FieldLabel { label: t.t("Credit limit (optional)") }
                AmountInput {
                    value: credit_limit.read().clone(),
                    oninput: move |v: String| credit_limit.set(v),
                }
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
                            span { style: "font-size:13px; color:{theme::TEXT_MUTED};", "{label}" }
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
                    style: "{theme::INPUT}",
                    r#type: "text",
                    placeholder: t.t("e.g. HSBC, VPS, Binance, OKX"),
                    value: "{broker.read()}",
                    oninput: move |e| { broker.set(e.value()); },
                }
            }

            // ── Icon ──
            FieldLabel { label: t.t("Icon") }
            IconPicker {
                value: if icon.read().is_empty() { default_emoji.to_string() } else { icon.read().clone() },
                options: icon_options.clone(),
                color: type_color.to_string(),
                on_select: move |e: String| {
                    // The type's own icon is the default: store nothing for it
                    if e == default_emoji { icon.set(String::new()); } else { icon.set(e); }
                },
            }

            // ── Error ──
            if !error_msg.read().is_empty() {
                div {
                    style: "background:{theme::NEGATIVE_SOFT}; border-radius:0; padding:10px 12px; font-size:13px; color:{theme::DANGER};",
                    "{error_msg.read()}"
                }
            }

            // ── Submit ──
            button {
                style: "{theme::BTN_PRIMARY} width:100%; margin-top:4px;",
                onclick: submit,
                if editing.is_some() { {t.t("Update Wallet")} } else { {t.t("Create Wallet")} }
            }

            // ── Delete (unused wallets only) ──
            if let Some(wallet) = editing.clone() {
                if can_delete() {
                    button {
                        style: "{theme::BTN_DANGER} width:100%;",
                        onclick: move |_| confirm_delete.set(true),
                        IconText { text: t.t("🗑 Delete wallet").to_string() }
                    }
                } else {
                    div { style: "font-size:12px; color:{theme::TEXT_MUTED}; text-align:center;",
                        {t.t("This wallet has transactions, so it can't be deleted.")}
                    }
                }

                if confirm_delete() {
                    div {
                        style: "{theme::OVERLAY}",
                        div {
                            style: "{theme::DIALOG}",
                            h3 { style: "margin:0 0 8px; font-size:16px; font-weight:600; color:{theme::TEXT};", {t.t("Delete wallet?")} }
                            p { style: "margin:0 0 18px; font-size:14px; color:{theme::TEXT_MUTED}; line-height:1.5;",
                                {t.t("\"{name}\" will be permanently deleted.").replace("{name}", &wallet.name)}
                            }
                            div { style: "display:flex; gap:10px;",
                                button {
                                    style: "{theme::BTN_SECONDARY} flex:1;",
                                    onclick: move |_| confirm_delete.set(false),
                                    {t.t("Cancel")}
                                }
                                button {
                                    style: "{theme::BTN_DANGER} flex:1;",
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
}

#[component]
fn FieldLabel(label: &'static str) -> Element {
    rsx! {
        label {
            style: "{theme::LABEL} margin:0 0 -10px;",
            "{label}"
        }
    }
}
