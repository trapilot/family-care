use dioxus::prelude::*;
use crate::i18n::use_i18n;
use rust_decimal::Decimal;
use std::str::FromStr;

use crate::app::AppState;
use crate::models::{
    Currency, CurrencyInfo, IncomeType, NewReminder, NewTransaction, RepeatUnit, Transaction, TransactionType,
};
use crate::repository::{CategoryRepo, CurrencyRepo, HoldingRepo, ReminderRepo, TxnImageRepo, WalletRepo};
use crate::services::{ReminderService, TransactionService};
use crate::views::{currency_name, date_str_add, date_str_to_ts, fmt_currency, now_ts, repeat_unit_label, AmountInput};
use crate::models::id_card::jpeg_data_url;
use crate::views::photo_picker::{use_photo_picker, PhotoInput};
use super::TxnView;

#[component]
pub fn TransactionForm(view: Signal<TxnView>, editing: Option<Transaction>) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();

    // ── Reference data ──
    let wallets    = use_signal(|| vec![]);
    let categories = use_signal(|| vec![]);
    let holdings   = use_signal(|| vec![]);
    // Every money type (active ones fill the pickers; names also label a hidden type being edited)
    let money_types = use_signal(Vec::<CurrencyInfo>::new);

    // ── Form fields ──
    let mut txn_type    = use_signal(|| editing.as_ref().map(|t| t.txn_type.clone()).unwrap_or(TransactionType::Expense));
    let mut wallet_id   = use_signal(|| editing.as_ref().map(|t| t.wallet_id.clone()).unwrap_or_default());
    let mut amount_str  = use_signal(|| editing.as_ref().map(|t| format!("{}", t.amount)).unwrap_or_default());
    let mut currency    = use_signal(|| editing.as_ref().map(|t| t.currency.clone()).unwrap_or(Currency::VND));
    let mut note        = use_signal(|| editing.as_ref().and_then(|t| t.note.clone()).unwrap_or_default());

    // date stored as "YYYY-MM-DD" for <input type=date>
    let mut txn_date = use_signal(|| {
        editing.as_ref().map(|t| {
            use chrono::{Local, TimeZone};
            Local.timestamp_opt(t.txn_date, 0)
                .single()
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default()
        }).unwrap_or_else(|| {
            use chrono::{Local, Datelike};
            let now = Local::now();
            format!("{}-{:02}-{:02}", now.year(), now.month(), now.day())
        })
    });

    // Income
    let mut income_type = use_signal(|| editing.as_ref().and_then(|t| t.income_type.clone()).unwrap_or(IncomeType::Salary));

    // Expense
    let mut category_id = use_signal(|| editing.as_ref().and_then(|t| t.category_id.clone()).unwrap_or_default());

    // Transfer
    let mut to_wallet_id  = use_signal(|| editing.as_ref().and_then(|t| t.to_wallet_id.clone()).unwrap_or_default());
    let mut to_amount_str = use_signal(|| editing.as_ref().and_then(|t| t.to_amount.map(|a| format!("{a}"))).unwrap_or_default());
    let mut to_currency   = use_signal(|| editing.as_ref().and_then(|t| t.to_currency.clone()).unwrap_or(Currency::VND));

    // Invest
    let mut holding_id = use_signal(|| editing.as_ref().and_then(|t| t.holding_id.clone()).unwrap_or_default());
    let mut qty_str    = use_signal(|| editing.as_ref().and_then(|t| t.asset_quantity.map(|q| format!("{q}"))).unwrap_or_default());
    let mut price_str  = use_signal(|| editing.as_ref().and_then(|t| t.asset_price.map(|p| format!("{p}"))).unwrap_or_default());

    // Recurring reminder (new Income / Transfer only): notify, never auto-add
    let mut remind       = use_signal(|| false);
    let mut remind_start = use_signal(String::new);
    let mut remind_every = use_signal(|| "1".to_string());
    let mut remind_unit  = use_signal(|| RepeatUnit::Month);

    let error_msg = use_signal(|| String::new());

    // Photos (receipts…): loaded when editing, saved with the transaction
    let mut photos = use_signal({
        let state = state.clone();
        let editing = editing.clone();
        move || editing.as_ref()
            .map(|old| state.with_conn(|conn| TxnImageRepo::list(conn, &old.id).unwrap_or_default()))
            .unwrap_or_default()
    });
    let mut photo_error = use_signal(|| false);
    let mut preview     = use_signal(|| None::<usize>);
    use_photo_picker(move |_, jpeg| match jpeg {
        Some(jpeg) => { photos.write().push(jpeg); photo_error.set(false); }
        None       => photo_error.set(true),
    });

    // ── Load reference data ──
    use_effect({
        let state = state.clone();
        let mut wallets    = wallets.clone();
        let mut categories = categories.clone();
        let mut holdings   = holdings.clone();
        let mut money_types = money_types;
        move || {
            state.with_conn(|conn| {
                if let Ok(list) = CurrencyRepo::list(conn)        { money_types.set(list); }
                if let Ok(ws) = WalletRepo::list_active(conn)    { wallets.set(ws); }
                if let Ok(cs) = CategoryRepo::list(conn)          { categories.set(cs); }
                if let Ok(hs) = HoldingRepo::list_all_active(conn) { holdings.set(hs); }
            });
        }
    });

    // ── Submit ──
    let submit = {
        let state      = state.clone();
        let txn_type   = txn_type.clone();
        let wallet_id  = wallet_id.clone();
        let amount_str = amount_str.clone();
        let currency   = currency.clone();
        let note       = note.clone();
        let txn_date   = txn_date.clone();
        let income_type = income_type.clone();
        let category_id = category_id.clone();
        let to_wallet_id  = to_wallet_id.clone();
        let to_amount_str = to_amount_str.clone();
        let to_currency   = to_currency.clone();
        let holding_id = holding_id.clone();
        let qty_str    = qty_str.clone();
        let price_str  = price_str.clone();
        let mut error_msg = error_msg.clone();
        let editing = editing.clone();

        move |_: MouseEvent| {
            let amount = match Decimal::from_str(&amount_str.read()) {
                Ok(a) => a,
                Err(_) => { error_msg.set(t.t("Invalid amount").into()); return; }
            };
            let date_ts = match date_str_to_ts(&txn_date.read()) {
                Some(ts) => ts,
                None => now_ts(),
            };
            let note_opt = {
                let n = note.read();
                if n.is_empty() { None } else { Some(n.clone()) }
            };

            let mut new_txn = match txn_type.read().clone() {
                TransactionType::Income => {
                    NewTransaction::income(
                        &wallet_id.read(),
                        amount,
                        currency.read().clone(),
                        income_type.read().clone(),
                        date_ts,
                    )
                }
                TransactionType::Expense => {
                    NewTransaction::expense(
                        &wallet_id.read(),
                        amount,
                        currency.read().clone(),
                        &category_id.read(),
                        date_ts,
                    )
                }
                TransactionType::Transfer => {
                    let to_amount = match Decimal::from_str(&to_amount_str.read()) {
                        Ok(a) => a,
                        Err(_) => { error_msg.set(t.t("Invalid to_amount").into()); return; }
                    };
                    NewTransaction::transfer(
                        &wallet_id.read(),
                        amount,
                        currency.read().clone(),
                        &to_wallet_id.read(),
                        to_amount,
                        to_currency.read().clone(),
                        date_ts,
                    )
                }
                TransactionType::InvestBuy => {
                    let qty   = match Decimal::from_str(&qty_str.read())   { Ok(q) => q, Err(_) => { error_msg.set(t.t("Invalid quantity").into()); return; } };
                    let price = match Decimal::from_str(&price_str.read()) { Ok(p) => p, Err(_) => { error_msg.set(t.t("Invalid price").into()); return; } };
                    NewTransaction::invest_buy(&wallet_id.read(), &holding_id.read(), qty, price, currency.read().clone(), date_ts)
                }
                TransactionType::InvestSell => {
                    let qty   = match Decimal::from_str(&qty_str.read())   { Ok(q) => q, Err(_) => { error_msg.set(t.t("Invalid quantity").into()); return; } };
                    let price = match Decimal::from_str(&price_str.read()) { Ok(p) => p, Err(_) => { error_msg.set(t.t("Invalid price").into()); return; } };
                    NewTransaction::invest_sell(&wallet_id.read(), &holding_id.read(), qty, price, currency.read().clone(), date_ts)
                }
                TransactionType::OpeningBalance => {
                    NewTransaction::opening_balance(&wallet_id.read(), amount, currency.read().clone(), date_ts)
                }
            };

            new_txn.note = note_opt.clone();

            let kind = txn_type.read().clone();
            let reminder = if editing.is_none() && remind() && matches!(kind, TransactionType::Income | TransactionType::Transfer) {
                let Some(every) = remind_every.read().parse::<u32>().ok().filter(|n| *n > 0) else {
                    error_msg.set(t.t("Repeat every must be at least 1").into());
                    return;
                };
                let Some(start_date) = date_str_to_ts(&remind_start.read()) else {
                    error_msg.set(t.t("Choose the first reminder date").into());
                    return;
                };
                Some(NewReminder {
                    txn_type:     kind.clone(),
                    wallet_id:    wallet_id.read().clone(),
                    to_wallet_id: (kind == TransactionType::Transfer).then(|| to_wallet_id.read().clone()),
                    amount,
                    currency:     currency.read().clone(),
                    note:         note_opt,
                    start_date,
                    repeat_unit:  remind_unit(),
                    repeat_every: every,
                })
            } else {
                None
            };

            let result = state.with_conn(|conn| -> crate::error::Result<()> {
                // If editing: delete old then insert new (simple approach)
                // (its photos are deleted with it and re-saved below from `photos`)
                if let Some(ref old) = editing {
                    let _ = TransactionService::delete(conn, &old.id);
                }
                let saved = TransactionService::execute(conn, &new_txn)?;
                for jpeg in photos.read().iter() {
                    TxnImageRepo::add(conn, &saved.id, jpeg)?;
                }
                if let Some(ref reminder) = reminder {
                    ReminderRepo::create(conn, reminder)?;
                    ReminderService::resync(conn)?;
                }
                Ok(())
            });

            match result {
                Ok(_)  => { view.set(TxnView::List); }
                Err(e) => { error_msg.set(e.to_string()); }
            }
        }
    };

    let title = if editing.is_some() { t.t("Edit Transaction") } else { t.t("New Transaction") };

    rsx! {
        div {
            style: "padding:16px; display:flex; flex-direction:column; gap:14px;",

            // ── Header ──
            div {
                style: "display:flex; align-items:center; gap:12px;",
                button {
                    style: "border:none; background:transparent; font-size:22px; cursor:pointer; padding:0;",
                    onclick: move |_| { view.set(TxnView::List); },
                    "←"
                }
                h1 { style: "margin:0; font-size:18px; font-weight:700; color:#1f2937;", "{title}" }
            }

            // ── Transaction type selector ──
            div {
                style: "display:flex; gap:6px; flex-wrap:wrap;",
                for (tt, label) in [
                    (TransactionType::Expense,   t.t("Expense")),
                    (TransactionType::Income,    t.t("Income")),
                    (TransactionType::Transfer,  t.t("Transfer")),
                    (TransactionType::InvestBuy, t.t("Buy")),
                    (TransactionType::InvestSell,t.t("Sell")),
                ] {
                    {
                        let tt_clone = tt.clone();
                        let is_active = *txn_type.read() == tt;
                        rsx! {
                            button {
                                style: if is_active {
                                    "padding:6px 14px; border:none; border-radius:20px; background:#6366f1; color:#fff; font-size:12px; font-weight:600; cursor:pointer;"
                                } else {
                                    "padding:6px 14px; border:1px solid #e5e7eb; border-radius:20px; background:#fff; color:#6b7280; font-size:12px; cursor:pointer;"
                                },
                                onclick: move |_| {
                                    txn_type.set(tt_clone.clone());
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }

            // ── Wallet ──
            FieldLabel { label: t.t("From Wallet") }
            SelectField {
                value: wallet_id.read().clone(),
                onchange: move |v: String| { wallet_id.set(v); },
                placeholder: t.t("Select wallet"),
                options: wallets.read().iter().map(|w| (w.id.clone(), w.name.clone())).collect()
            }

            // ── Category (expense) — above the amount ──
            if *txn_type.read() == TransactionType::Expense {
                FieldLabel { label: t.t("Category") }
                SelectField {
                    value: category_id.read().clone(),
                    onchange: move |v| { category_id.set(v); },
                    placeholder: t.t("Select category"),
                    options: categories.read().iter().map(|c| (c.id.clone(), format!("{} {}", c.icon.as_deref().unwrap_or(""), t.tr(&c.name)))).collect()
                }
            }

            // ── Type-specific fields ──
            match txn_type.read().clone() {
                TransactionType::Income => rsx! {
                    FieldLabel { label: t.t("Income Type") }
                    SelectField {
                        value: income_type.read().to_string(),
                        onchange: move |v: String| {
                            if let Ok(it) = IncomeType::from_str(&v) {
                                income_type.set(it);
                            }
                        },
                        placeholder: t.t("Select type"),
                        options: vec![
                            ("salary".into(),      t.t("Salary").into()),
                            ("bonus".into(),       t.t("Bonus").into()),
                            ("side_income".into(), t.t("Side Income").into()),
                            ("rental".into(),      t.t("Rental").into()),
                            ("investment".into(),  t.t("Investment").into()),
                            ("other".into(),       t.t("Other").into()),
                        ]
                    }
                },

                // Category is shown above the amount
                TransactionType::Expense => rsx! {},

                TransactionType::Transfer => rsx! {
                    FieldLabel { label: t.t("To Wallet") }
                    SelectField {
                        value: to_wallet_id.read().clone(),
                        onchange: move |v: String| { to_wallet_id.set(v); },
                        placeholder: t.t("Select destination wallet"),
                        options: wallets.read().iter().map(|w| (w.id.clone(), w.name.clone())).collect()
                    }
                    FieldLabel { label: t.t("To Amount (leave same for same-currency)") }
                    div {
                        style: "display:flex; gap:8px;",
                        AmountInput {
                            value: to_amount_str.read().clone(),
                            placeholder: t.t("Received amount"),
                            oninput: move |v| to_amount_str.set(v),
                            style: AMOUNT_WITH_CURRENCY_STYLE.to_string(),
                        }
                        CurrencySelect { value: to_currency(), options: currency_options(&t, &money_types.read(), &to_currency()), onchange: move |c| to_currency.set(c) }
                    }
                },

                TransactionType::OpeningBalance => rsx! {},

                TransactionType::InvestBuy | TransactionType::InvestSell => rsx! {
                    FieldLabel { label: t.t("Holding") }
                    SelectField {
                        value: holding_id.read().clone(),
                        onchange: move |v| { holding_id.set(v); },
                        placeholder: t.t("Select symbol"),
                        options: holdings.read().iter().map(|h| (h.id.clone(), format!("{} ({})", h.symbol, h.asset_type))).collect()
                    }
                    FieldLabel { label: t.t("Quantity") }
                    AmountInput {
                        value: qty_str.read().clone(),
                        oninput: move |v| qty_str.set(v),
                    }
                    FieldLabel { label: t.t("Price per unit") }
                    AmountInput {
                        value: price_str.read().clone(),
                        oninput: move |v| price_str.set(v),
                    }
                },
            }

            // ── Date ──
            FieldLabel { label: t.t("Date") }
            input {
                style: "border:1px solid #e5e7eb; border-radius:8px; padding:10px 12px; font-size:14px; outline:none; background:#fff;",
                r#type: "date",
                value: "{txn_date.read()}",
                oninput: move |e| { txn_date.set(e.value()); },
            }

            // ── Amount + currency ──
            FieldLabel { label: t.t("Amount") }
            div {
                style: "display:flex; gap:8px;",
                AmountInput {
                    value: amount_str.read().clone(),
                    oninput: move |v| amount_str.set(v),
                    style: AMOUNT_WITH_CURRENCY_STYLE.to_string(),
                }
                CurrencySelect {
                    value: currency(),
                    options: currency_options(&t, &money_types.read(), &currency()),
                    // The received money type follows by default; it can still be changed
                    onchange: move |c: Currency| { to_currency.set(c.clone()); currency.set(c); },
                }
            }
            // Available balance of the chosen wallet in the chosen money type
            if matches!(*txn_type.read(), TransactionType::Expense | TransactionType::Transfer | TransactionType::InvestBuy) {
                if let Some(w) = wallets.read().iter().find(|w| w.id == *wallet_id.read()) {
                    div {
                        style: "font-size:12px; color:#6b7280;",
                        "{t.t(\"Available\")}: {fmt_currency(w.balance(&currency()), &currency())}"
                    }
                }
            }

            // ── Note ──
            FieldLabel { label: t.t("Note (optional)") }
            input {
                style: "border:1px solid #e5e7eb; border-radius:8px; padding:10px 12px; font-size:14px; outline:none; background:#fff;",
                r#type: "text",
                placeholder: t.t("Add a note…"),
                value: "{note.read()}",
                oninput: move |e| { note.set(e.value()); },
            }

            // ── Photos ──
            FieldLabel { label: t.t("Photos (optional)") }
            div { style: "display:flex; gap:8px;",
                label {
                    style: "flex:1; padding:10px; border:1px dashed #c7d2fe; border-radius:8px; background:#fff; color:#4f46e5; font-size:14px; font-weight:600; text-align:center; cursor:pointer;",
                    {t.t("🖼 Upload")}
                    PhotoInput { key_name: "txn", multiple: true }
                }
                label {
                    style: "flex:1; padding:10px; border:1px dashed #c7d2fe; border-radius:8px; background:#fff; color:#4f46e5; font-size:14px; font-weight:600; text-align:center; cursor:pointer;",
                    {t.t("📷 Camera")}
                    PhotoInput { key_name: "txn", camera: true }
                }
            }
            if photo_error() {
                div { style: "font-size:12px; color:#ef4444;", {t.t("❌ Could not read that photo")} }
            }
            if !photos.read().is_empty() {
                div { style: "display:flex; flex-wrap:wrap; gap:8px;",
                    for (i, src) in photos.read().iter().map(|j| jpeg_data_url(j)).enumerate() {
                        div { key: "{i}", style: "position:relative; width:76px; height:76px;",
                            img {
                                src,
                                style: "width:100%; height:100%; object-fit:cover; border-radius:8px; border:1px solid #e5e7eb; cursor:pointer;",
                                onclick: move |_| preview.set(Some(i)),
                            }
                            button {
                                style: "position:absolute; top:-6px; right:-6px; width:22px; height:22px; border-radius:50%; border:none; background:#ef4444; color:#fff; font-size:12px; line-height:22px; padding:0; cursor:pointer;",
                                onclick: move |_| { photos.write().remove(i); },
                                "✕"
                            }
                        }
                    }
                }
            }
            // Full-size view; tap anywhere to close
            if let Some(src) = preview().and_then(|i| photos.read().get(i).map(|j| jpeg_data_url(j))) {
                div {
                    style: "position:fixed; inset:0; background:rgba(0,0,0,.85); display:flex; align-items:center; justify-content:center; z-index:100; padding:16px;",
                    onclick: move |_| preview.set(None),
                    img { src, style: "max-width:100%; max-height:100%; object-fit:contain; border-radius:8px;" }
                }
            }

            // ── Recurring reminder (new Income / Transfer) ──
            if editing.is_none() && matches!(*txn_type.read(), TransactionType::Income | TransactionType::Transfer) {
                div {
                    style: "background:#fff; border:1px solid #e5e7eb; border-radius:10px; padding:12px; display:flex; flex-direction:column; gap:10px;",
                    label {
                        style: "display:flex; align-items:center; gap:10px; font-size:14px; font-weight:600; color:#374151;",
                        input {
                            r#type: "checkbox",
                            style: "width:18px; height:18px;",
                            checked: remind(),
                            onchange: move |e| {
                                let on = e.checked();
                                remind.set(on);
                                // Default: the next occurrence after this transaction
                                if on && remind_start.read().is_empty() {
                                    remind_start.set(date_str_add(&txn_date.read(), remind_unit(), 1));
                                }
                            },
                        }
                        {t.t("🔔 Remind me when this repeats")}
                    }
                    if remind() {
                        FieldLabel { label: t.t("First reminder date") }
                        input {
                            style: "border:1px solid #e5e7eb; border-radius:8px; padding:10px 12px; font-size:14px; outline:none; background:#fff;",
                            r#type: "date",
                            value: "{remind_start.read()}",
                            oninput: move |e| { remind_start.set(e.value()); },
                        }
                        FieldLabel { label: t.t("Repeat every") }
                        div { style: "display:flex; gap:8px;",
                            input {
                                style: "width:72px; border:1px solid #e5e7eb; border-radius:8px; padding:10px 12px; font-size:16px; outline:none; background:#fff;",
                                r#type: "text",
                                inputmode: "numeric",
                                value: "{remind_every.read()}",
                                oninput: move |e| { remind_every.set(e.value().chars().filter(|c| c.is_ascii_digit()).take(3).collect()); },
                            }
                            select {
                                style: "flex:1; border:1px solid #e5e7eb; border-radius:8px; padding:10px 12px; font-size:14px; outline:none; background:#fff;",
                                onchange: move |e| {
                                    if let Ok(unit) = e.value().parse::<RepeatUnit>() { remind_unit.set(unit); }
                                },
                                for unit in RepeatUnit::ALL {
                                    option { value: "{unit}", selected: unit == remind_unit(), {t.t(repeat_unit_label(unit))} }
                                }
                            }
                        }
                        p { style: "margin:0; font-size:12px; color:#6b7280; line-height:1.4;",
                            {t.t("On each due day you'll get notifications from the first notification time, then every few hours (Settings → Reminders). Tap \"Done for today\" to stop them. Nothing is added automatically.")}
                        }
                    }
                }
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
                if editing.is_some() { {t.t("Update Transaction")} } else { {t.t("Add Transaction")} }
            }
        }
    }
}

// ─── Shared small components ─────────────────────────────────────────────────

#[component]
fn FieldLabel(label: &'static str) -> Element {
    rsx! {
        label {
            style: "font-size:12px; font-weight:600; color:#374151; text-transform:uppercase; letter-spacing:.5px;",
            "{label}"
        }
    }
}

/// Amount input sharing its row with a `CurrencySelect`
const AMOUNT_WITH_CURRENCY_STYLE: &str = "flex:1; min-width:0; border:1px solid #e5e7eb; border-radius:8px; padding:10px 12px; font-size:16px; outline:none; background:#fff;";

/// Active money types with their display names; `selected` is appended when it is hidden,
/// so an old transaction in a hidden type still shows its type
fn currency_options(t: &crate::i18n::I18n, list: &[CurrencyInfo], selected: &Currency) -> Vec<(Currency, String)> {
    let mut options: Vec<(Currency, String)> = list.iter()
        .filter(|i| i.is_active)
        .map(|i| (i.code.clone(), currency_name(t, list, &i.code)))
        .collect();
    if !options.iter().any(|(c, _)| c == selected) {
        options.push((selected.clone(), currency_name(t, list, selected)));
    }
    options
}

/// Compact money-type dropdown shown right of an amount
#[component]
fn CurrencySelect(value: Currency, options: Vec<(Currency, String)>, onchange: EventHandler<Currency>) -> Element {
    rsx! {
        select {
            style: "flex-shrink:0; width:120px; border:1px solid #e5e7eb; border-radius:8px; padding:10px 8px; font-size:14px; font-weight:600; outline:none; background:#fff; color:#374151;",
            onchange: move |e| {
                if let Ok(c) = e.value().parse::<Currency>() { onchange.call(c); }
            },
            for (cur, label) in options.iter() {
                option { value: "{cur}", selected: *cur == value, "{label}" }
            }
        }
    }
}

#[component]
fn SelectField(
    value: String,
    onchange: EventHandler<String>,
    placeholder: &'static str,
    options: Vec<(String, String)>,
) -> Element {
    rsx! {
        select {
            style: "border:1px solid #e5e7eb; border-radius:8px; padding:10px 12px; font-size:14px; outline:none; background:#fff; width:100%; appearance:none;",
            value: "{value}",
            onchange: move |e| { onchange.call(e.value()); },
            option { value: "", disabled: true, selected: value.is_empty(), "{placeholder}" }
            for (val, label) in options.iter() {
                option { value: "{val}", selected: *val == value, "{label}" }
            }
        }
    }
}
