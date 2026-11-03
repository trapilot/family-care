use dioxus::prelude::*;
use crate::i18n::use_i18n;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::str::FromStr;

use crate::app::AppState;
use crate::models::{
    Currency, CurrencyInfo, IncomeType, NewReminder, NewTransaction, RepeatUnit, Transaction, TransactionType,
};
use crate::repository::{CategoryRepo, CurrencyRepo, HoldingRepo, ReminderRepo, TxnImageRepo, WalletRepo};
use crate::services::statement::ocr;
use crate::services::{receipt, ReminderService, TransactionService};
use crate::views::icons::{Icon, IconName};
use crate::views::ui::{txn_type_look, DateInput, IconCircle, IconText, ScreenHeader};
use crate::views::{currency_name, date_str_add, date_str_to_ts, fmt_currency, fmt_vnd, repeat_unit_label, theme, AmountInput};
use crate::models::id_card::jpeg_data_url;
use crate::views::photo_picker::{use_photo_picker, PhotoInput};
use super::TxnView;

#[component]
pub fn TransactionForm(view: Signal<TxnView>, editing: Option<Transaction>) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();

    // ── Reference data ──
    let wallets    = use_signal(|| vec![]);
    // Credit limit per credit-card wallet (None = card without a limit)
    let card_limits = use_signal(std::collections::HashMap::<String, Option<Decimal>>::new);
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

    // Numbers found by the Scan button, per photo hash. They are offered as tappable chips; a photo
    // with exactly one number fills the amount; removing photos never changes the amount.
    let mut detected    = use_signal(Vec::<(u64, Vec<Decimal>)>::new);
    let mut scanning    = use_signal(|| 0usize);

    use_photo_picker(move |key, jpeg| match jpeg {
        Some(jpeg) => {
            photos.write().push(jpeg.clone());
            photo_error.set(false);
            // Only the Scan button reads the amount; Upload / Camera in Photos just attach
            if key != "scan" { return; }
            let hash = photo_hash(&jpeg);
            scanning.set(scanning() + 1);
            spawn(async move {
                // Any failure (incl. unsupported device) is silent: the user just types the amount
                let found = match ocr::recognize_async(jpeg).await {
                    Ok(page) => receipt::candidate_amounts(&page),
                    Err(_) => Vec::new(),
                };
                // Exactly one number: nothing to choose, use it
                if let [only] = found.as_slice() {
                    amount_str.set(only.normalize().to_string());
                }
                detected.write().push((hash, found));
                scanning.set(scanning().saturating_sub(1));
            });
        }
        None => photo_error.set(true),
    });

    // Numbers found in the photos still attached, biggest first
    let candidates = use_memo(move || {
        let hashes: Vec<u64> = photos.read().iter().map(|j| photo_hash(j)).collect();
        let mut all: Vec<Decimal> = detected.read().iter()
            .filter(|(h, _)| hashes.contains(h))
            .flat_map(|(_, v)| v.iter().copied())
            .collect();
        all.sort_by(|a, b| b.cmp(a));
        all.dedup();
        all
    });
    let scanned_attached = use_memo(move || {
        let hashes: Vec<u64> = photos.read().iter().map(|j| photo_hash(j)).collect();
        detected.read().iter().any(|(h, _)| hashes.contains(h))
    });

    // ── Load reference data ──
    use_effect({
        let state = state.clone();
        let mut wallets    = wallets.clone();
        let mut card_limits = card_limits;
        let mut categories = categories.clone();
        let mut holdings   = holdings.clone();
        let mut money_types = money_types;
        move || {
            state.with_conn(|conn| {
                if let Ok(list) = CurrencyRepo::list(conn)        { money_types.set(list); }
                if let Ok(ws) = WalletRepo::list_active(conn)    { wallets.set(ws); }
                if let Ok(cards) = crate::repository::CreditCardRepo::list(conn) {
                    card_limits.set(cards.into_iter().map(|c| (c.wallet_id, c.credit_limit)).collect());
                }
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
            let kind = txn_type.read().clone();
            let positive = |s: &str| Decimal::from_str(s.trim()).ok();

            if wallet_id.read().is_empty() {
                error_msg.set(t.t("Choose a wallet").into());
                return;
            }
            let amount = match Decimal::from_str(amount_str.read().trim()) {
                Ok(a) => a,
                Err(_) => { error_msg.set(t.t("Invalid amount").into()); return; }
            };
            // Only invest trades derive their cost from qty x price; an opening balance may be 0
            let is_invest = matches!(kind, TransactionType::InvestBuy | TransactionType::InvestSell);
            let zero_ok = kind == TransactionType::OpeningBalance;
            if !is_invest && (amount < Decimal::ZERO || (amount.is_zero() && !zero_ok)) {
                error_msg.set(t.t("Amount must be greater than 0").into());
                return;
            }
            if kind == TransactionType::Expense && category_id.read().is_empty() {
                error_msg.set(t.t("Choose a category").into());
                return;
            }
            let mut to_amount = Decimal::ZERO;
            if kind == TransactionType::Transfer {
                if to_wallet_id.read().is_empty() {
                    error_msg.set(t.t("Choose the receiving wallet").into());
                    return;
                }
                if *to_wallet_id.read() == *wallet_id.read() && *to_currency.read() == *currency.read() {
                    error_msg.set(t.t("Choose a different wallet to transfer to").into());
                    return;
                }
                match positive(&to_amount_str.read()).filter(|a| *a > Decimal::ZERO) {
                    Some(a) => to_amount = a,
                    None => { error_msg.set(t.t("Invalid received amount").into()); return; }
                }
            }
            let (mut qty, mut price) = (Decimal::ZERO, Decimal::ZERO);
            if is_invest {
                if holding_id.read().is_empty() {
                    error_msg.set(t.t("Choose a holding").into());
                    return;
                }
                match (positive(&qty_str.read()), positive(&price_str.read())) {
                    (Some(q), Some(p)) if q > Decimal::ZERO && p > Decimal::ZERO => { qty = q; price = p; }
                    _ => { error_msg.set(t.t("Quantity and price must be greater than 0").into()); return; }
                }
            }
            let Some(date_ts) = date_str_to_ts(&txn_date.read()) else {
                error_msg.set(t.t("Choose a date").into());
                return;
            };
            let note_opt = {
                let n = note.read();
                if n.is_empty() { None } else { Some(n.clone()) }
            };

            let mut new_txn = match kind.clone() {
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
                    NewTransaction::invest_buy(&wallet_id.read(), &holding_id.read(), qty, price, currency.read().clone(), date_ts)
                }
                TransactionType::InvestSell => {
                    NewTransaction::invest_sell(&wallet_id.read(), &holding_id.read(), qty, price, currency.read().clone(), date_ts)
                }
                TransactionType::OpeningBalance => {
                    NewTransaction::opening_balance(&wallet_id.read(), amount, currency.read().clone(), date_ts)
                }
            };

            new_txn.note = note_opt.clone();

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
                // Delete-old + insert-new + photos commit or roll back together
                TransactionService::execute_with_photos(
                    conn,
                    editing.as_ref().map(|old| old.id.as_str()),
                    &new_txn,
                    &photos.read(),
                )?;
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
            style: "display:flex; flex-direction:column; min-height:100%;",
            ScreenHeader {
                title: title.to_string(),
                on_back: Some(EventHandler::new(move |_| { view.set(TxnView::List); })),
            }
        div {
            style: "padding:16px; display:flex; flex-direction:column; gap:16px;",

            // ── Transaction type selector ──
            div {
                style: "display:flex; gap:8px; overflow-x:auto;",
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
                        let (icon, _) = txn_type_look(&tt);
                        rsx! {
                            button {
                                style: if is_active {
                                    "{theme::CHIP_ACTIVE} display:inline-flex; align-items:center; gap:6px;"
                                } else {
                                    "{theme::CHIP} display:inline-flex; align-items:center; gap:6px;"
                                },
                                onclick: move |_| {
                                    txn_type.set(tt_clone.clone());
                                },
                                Icon { name: icon, size: 16 }
                                "{label}"
                            }
                        }
                    }
                }
            }

            // ── Wallet ──
            FieldLabel { label: t.t("From Wallet") }
            div { style: "display:flex; gap:8px; align-items:stretch;",
                div { style: "flex:1; min-width:0;",
                    SelectField {
                        value: wallet_id.read().clone(),
                        onchange: move |v: String| { wallet_id.set(v); },
                        placeholder: t.t("Select wallet"),
                        options: wallets.read().iter().map(|w| (w.id.clone(), w.name.clone())).collect()
                    }
                }
                // Camera-only scan (front camera): the photo is attached and its amount is detected
                label {
                    style: "width:44px; flex-shrink:0; display:flex; align-items:center; justify-content:center; background:{theme::PRIMARY_SOFT}; cursor:pointer;",
                    aria_label: t.t("Scan"),
                    Icon { name: IconName::Camera, size: 20, color: theme::PRIMARY.to_string() }
                    PhotoInput { key_name: "scan", camera: true, front: true }
                }
            }

            // ── Category (expense) — above the amount ──
            if *txn_type.read() == TransactionType::Expense {
                FieldLabel { label: t.t("Category") }
                div {
                    style: "display:grid; grid-template-columns:repeat(4, 1fr); gap:12px 8px;",
                    for c in categories.read().iter() {
                        {
                            let cid = c.id.clone();
                            let selected = *category_id.read() == c.id;
                            // Always set box-shadow: dropping the declaration leaves the old ring on Android
                            let ring = if selected { "0 0 0 2px #3f51b5" } else { "none" };
                            let color = c.color.clone().unwrap_or_else(|| theme::TEXT_MUTED.to_string());
                            let name = t.tr(&c.name).to_string();
                            let name_color = if selected { theme::PRIMARY } else { theme::TEXT };
                            rsx! {
                                button {
                                    key: "{c.id}",
                                    style: "display:flex; flex-direction:column; align-items:center; gap:6px; border:none; background:transparent; padding:0; cursor:pointer; min-width:0;",
                                    onclick: move |_| { category_id.set(cid.clone()); },
                                    span {
                                        style: "display:inline-flex; border-radius:50%; box-shadow:{ring};",
                                        IconCircle { icon: c.icon.clone(), fallback: IconName::Tag, color, size: 48 }
                                    }
                                    span {
                                        style: "font-size:12px; color:{name_color}; max-width:100%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                                        "{name}"
                                    }
                                }
                            }
                        }
                    }
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
            DateInput { value: txn_date.read().clone(), oninput: move |v: String| txn_date.set(v) }

            // ── Amount + currency ──
            FieldLabel { label: t.t("Amount") }
            div {
                style: "display:flex; gap:8px;",
                AmountInput {
                    value: amount_str.read().clone(),
                    oninput: move |v| { amount_str.set(v); },
                    style: AMOUNT_WITH_CURRENCY_STYLE.to_string(),
                }
                CurrencySelect {
                    value: currency(),
                    options: currency_options(&t, &money_types.read(), &currency()),
                    // The received money type follows by default; it can still be changed
                    onchange: move |c: Currency| { to_currency.set(c.clone()); currency.set(c); },
                }
            }
            // Scan status
            if scanning() > 0 {
                div {
                    style: "display:flex; align-items:center; gap:6px; font-size:12px; color:{theme::TEXT_MUTED};",
                    Icon { name: IconName::Clock, size: 14 }
                    {t.t("Reading amount from photo…")}
                }
            }
            if !candidates().is_empty() {
                div {
                    style: "display:flex; flex-wrap:wrap; gap:6px; align-items:center;",
                    span {
                        style: "font-size:12px; color:{theme::TEXT_MUTED};",
                        {t.t("Pick the amount:")}
                    }
                    for a in candidates() {
                        {
                            let selected = Decimal::from_str(&amount_str.read()).ok() == Some(a);
                            let (bg, fg) = if selected { (theme::PRIMARY, "#fff") } else { (theme::PRIMARY_SOFT, theme::PRIMARY) };
                            let label = fmt_vnd(Decimal::from(a.to_i64().unwrap_or(0)));
                            rsx! {
                                button {
                                    key: "{a}",
                                    style: "border:none; border-radius:999px; background:{bg}; color:{fg}; font-size:12px; font-weight:600; padding:4px 10px; cursor:pointer;",
                                    onclick: move |_| amount_str.set(a.normalize().to_string()),
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            } else if scanning() == 0 && scanned_attached() {
                div {
                    style: "font-size:12px; color:{theme::TEXT_MUTED};",
                    {t.t("No numbers found in the photo")}
                }
            }
            // Available balance of the chosen wallet in the chosen money type
            if matches!(*txn_type.read(), TransactionType::Expense | TransactionType::Transfer | TransactionType::InvestBuy) {
                if let Some(w) = wallets.read().iter().find(|w| w.id == *wallet_id.read()) {
                    div {
                        style: "font-size:12px; color:{theme::TEXT_MUTED};",
                        {
                            let cur = currency();
                            let balance = w.balance(&cur);
                            if w.wallet_type == crate::models::WalletType::CreditCard {
                                // A card's balance is what is owed (negative); what can be spent is the remaining credit limit
                                let owed = (-balance).max(Decimal::ZERO);
                                match card_limits.read().get(&w.id).copied().flatten() {
                                    Some(limit) if cur == Currency::VND => format!("{} {}: {}", t.t("Available"), t.t("(credit limit)"), fmt_currency(limit - owed, &cur)),
                                    _ => format!("{}: {}", t.t("Owed"), fmt_currency(owed, &cur)),
                                }
                            } else {
                                format!("{}: {}", t.t("Available"), fmt_currency(balance, &cur))
                            }
                        }
                    }
                }
            }

            // ── Note ──
            FieldLabel { label: t.t("Note (optional)") }
            input {
                style: "{theme::INPUT}",
                r#type: "text",
                placeholder: t.t("Add a note…"),
                value: "{note.read()}",
                oninput: move |e| { note.set(e.value()); },
            }

            // ── Photos ──
            FieldLabel { label: t.t("Photos (optional)") }
            div { style: "display:flex; gap:8px;",
                label {
                    style: "flex:1; padding:12px; border:1px dashed #3f51b5; border-radius:0; background:#e2e5f7; color:#3f51b5; font-size:14px; font-weight:600; text-align:center; cursor:pointer; display:flex; align-items:center; justify-content:center;",
                    IconText { text: t.t("🖼 Upload").to_string() }
                    PhotoInput { key_name: "txn", multiple: true }
                }
                label {
                    style: "flex:1; padding:12px; border:1px dashed #3f51b5; border-radius:0; background:#e2e5f7; color:#3f51b5; font-size:14px; font-weight:600; text-align:center; cursor:pointer; display:flex; align-items:center; justify-content:center;",
                    IconText { text: t.t("📷 Camera").to_string() }
                    PhotoInput { key_name: "txn", camera: true }
                }
            }
            if photo_error() {
                div { style: "font-size:12px; color:{theme::DANGER};", IconText { text: t.t("❌ Could not read that photo").to_string(), size: 14, gap: 4 } }
            }
            if !photos.read().is_empty() {
                div { style: "display:flex; flex-wrap:wrap; gap:8px;",
                    for (i, src) in photos.read().iter().map(|j| jpeg_data_url(j)).enumerate() {
                        div { key: "{i}", style: "position:relative; width:76px; height:76px;",
                            img {
                                src,
                                style: "width:100%; height:100%; object-fit:cover; border-radius:0; border:1px solid #e6e6ef; cursor:pointer;",
                                onclick: move |_| preview.set(Some(i)),
                            }
                            button {
                                style: "position:absolute; top:-6px; right:-6px; width:22px; height:22px; border-radius:50%; border:none; background:#c0262d; color:#fff; padding:0; cursor:pointer; display:flex; align-items:center; justify-content:center;",
                                onclick: move |_| { photos.write().remove(i); },
                                Icon { name: IconName::X, size: 12, color: "#ffffff".to_string() }
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
                    style: "{theme::CARD} display:flex; flex-direction:column; gap:12px;",
                    label {
                        style: "display:flex; align-items:center; gap:10px; font-size:14px; font-weight:600; color:{theme::TEXT};",
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
                        IconText { text: t.t("🔔 Remind me when this repeats").to_string() }
                    }
                    if remind() {
                        FieldLabel { label: t.t("First reminder date") }
                        DateInput { value: remind_start.read().clone(), oninput: move |v: String| remind_start.set(v) }
                        FieldLabel { label: t.t("Repeat every") }
                        div { style: "display:flex; gap:8px;",
                            input {
                                style: "{theme::INPUT} width:72px;",
                                r#type: "text",
                                inputmode: "numeric",
                                value: "{remind_every.read()}",
                                oninput: move |e| { remind_every.set(e.value().chars().filter(|c| c.is_ascii_digit()).take(3).collect()); },
                            }
                            select {
                                style: "{theme::INPUT} flex:1; width:auto;",
                                onchange: move |e| {
                                    if let Ok(unit) = e.value().parse::<RepeatUnit>() { remind_unit.set(unit); }
                                },
                                for unit in RepeatUnit::ALL {
                                    option { value: "{unit}", selected: unit == remind_unit(), {t.t(repeat_unit_label(unit))} }
                                }
                            }
                        }
                        p { style: "margin:0; font-size:12px; color:{theme::TEXT_MUTED}; line-height:1.4;",
                            {t.t("On each due day you'll get notifications from the first notification time, then every few hours (Settings → Reminders). Tap \"Done for today\" to stop them. Nothing is added automatically.")}
                        }
                    }
                }
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
                if editing.is_some() { {t.t("Update Transaction")} } else { {t.t("Add Transaction")} }
            }
        }
        }
    }
}

// ─── Shared small components ─────────────────────────────────────────────────

/// Hash of a photo's bytes, used to tie a detected amount to the photo it came from
fn photo_hash(jpeg: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    jpeg.hash(&mut h);
    h.finish()
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

/// Amount input sharing its row with a `CurrencySelect`
const AMOUNT_WITH_CURRENCY_STYLE: &str = "flex:1; min-width:0; box-sizing:border-box; border:1px solid #e6e6ef; border-radius:0; padding:12px 14px; font-size:15px; outline:none; background:#fff; color:#1f2133;";

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
            style: "flex-shrink:0; width:120px; box-sizing:border-box; border:1px solid #e6e6ef; border-radius:0; padding:12px 8px; font-size:14px; font-weight:600; outline:none; background:#fff; color:#1f2133;",
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
            style: "{theme::INPUT} appearance:none;",
            value: "{value}",
            onchange: move |e| { onchange.call(e.value()); },
            option { value: "", disabled: true, selected: value.is_empty(), "{placeholder}" }
            for (val, label) in options.iter() {
                option { value: "{val}", selected: *val == value, "{label}" }
            }
        }
    }
}
