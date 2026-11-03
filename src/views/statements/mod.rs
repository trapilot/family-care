use dioxus::prelude::*;
use futures_util::StreamExt;
use rust_decimal::Decimal;

use crate::error::AppError;
use crate::i18n::use_i18n;
use crate::models::{CounterpartyAccount, Statement, StatementTxn};
use crate::services::statement::{ocr, store, Bank};
use crate::services::StatementService;
use crate::views::photo_picker::{use_photo_picker, PhotoInput};
use crate::views::icons::{emoji_icon, Icon, IconName};
use crate::views::ui::{IconText, ScreenHeader, UnderlineTabs};
use crate::views::{fmt_vnd, now_ts, theme};

#[derive(Clone, PartialEq)]
enum StatementsTab { Accounts, Transactions, Photos }

/// Message of an error without its "Parse error:" style prefix, so it can be translated
fn err_msg(e: &AppError) -> String {
    match e {
        AppError::Parse(m) | AppError::Validation(m) => m.clone(),
        other => other.to_string(),
    }
}

/// Read one photo, parse it and save it to the statements file
async fn import_photo(bank: Bank, jpeg: Vec<u8>) -> Result<(), AppError> {
    let page = ocr::recognize_async(jpeg).await?;
    let parsed = bank.parse(&page)?;
    store::add(Statement {
        id: uuid::Uuid::new_v4().to_string(),
        imported_at: now_ts(),
        account_no: parsed.account_no,
        account_name: parsed.account_name,
        period_from: parsed.period_from,
        period_to: parsed.period_to,
        page: parsed.page,
        opening_balance: parsed.opening_balance,
        transactions: parsed.transactions,
        warnings: parsed.warnings,
    })
}

#[component]
pub fn Statements() -> Element {
    let t = use_i18n();
    let mut tab = use_signal(|| StatementsTab::Accounts);
    let mut bank = use_signal(Bank::default);
    let mut selected = use_signal(Vec::<String>::new);
    let mut version = use_signal(|| 0u32);
    let mut errors = use_signal(Vec::<(usize, String)>::new);
    let mut total = use_signal(|| 0usize);
    let mut done = use_signal(|| 0usize);

    // Photos are read one after another so the UI never freezes
    let importer = use_coroutine(move |mut rx: UnboundedReceiver<(Bank, Vec<u8>)>| async move {
        while let Some((bank, jpeg)) = rx.next().await {
            let n = done() + 1;
            match import_photo(bank, jpeg).await {
                Ok(()) => version += 1,
                Err(e) => errors.write().push((n, err_msg(&e))),
            }
            done += 1;
            if done() >= total() {
                total.set(0);
                done.set(0);
            }
        }
    });

    use_photo_picker(move |_, jpeg| match jpeg {
        Some(jpeg) => {
            total += 1;
            importer.send((bank(), jpeg));
        }
        None => {
            let n = total() + 1;
            errors.write().push((n, "Could not read the photo".to_string()));
        }
    });

    let data = use_memo(move || {
        version();
        store::load().map(|f| f.statements).map_err(|e| err_msg(&e))
    });
    let (statements, load_error) = match &*data.read() {
        Ok(s) => (s.clone(), None),
        Err(e) => (Vec::new(), Some(e.clone())),
    };
    let photo_count = statements.len();
    let selected_count = selected.read().len();

    let tab_labels = [
        (StatementsTab::Accounts, t.t("Accounts").to_string()),
        (
            StatementsTab::Transactions,
            if selected_count > 0 {
                t.t("Transactions ({n} selected)").replace("{n}", &selected_count.to_string())
            } else {
                t.t("Transactions").to_string()
            },
        ),
        (StatementsTab::Photos, t.t("Photos ({n})").replace("{n}", &photo_count.to_string())),
    ];
    let status = (total() > done()).then(|| {
        t.t("Reading photo {done}/{total}…")
            .replace("{done}", &(done() + 1).to_string())
            .replace("{total}", &total().to_string())
    });
    let photo_btn = "width:40px; height:40px; border-radius:50%; display:flex; align-items:center; justify-content:center; cursor:pointer; color:#1f2133;";
    let tab_defs = [StatementsTab::Accounts, StatementsTab::Transactions, StatementsTab::Photos];
    let tab_items: Vec<(String, Option<IconName>)> = tab_labels.iter().map(|(_, l)| (l.clone(), None)).collect();
    let active_tab = tab_defs.iter().position(|k| *k == *tab.read()).unwrap_or(0);
    let gallery_label = emoji_icon(t.t("📷 Add photos")).map_or("", |(_, rest)| rest).to_string();

    let camera_label = emoji_icon(t.t("📷 Camera")).map_or("", |(_, rest)| rest).to_string();

    rsx! {
        div {
            style: "display:flex; flex-direction:column; height:100%; background:{theme::BG};",

            // ── Header ──
            ScreenHeader {
                title: t.t("Statements"),
                action: rsx! {
                    label {
                        style: "{photo_btn}",
                        aria_label: "{gallery_label}",
                        Icon { name: IconName::Image, size: 26, color: theme::TEXT.to_string() }
                        PhotoInput { key_name: "statement", multiple: true, max_size: 3000 }
                    }
                    label {
                        style: "{photo_btn}",
                        aria_label: "{camera_label}",
                        Icon { name: IconName::Camera, size: 26, color: theme::TEXT.to_string() }
                        PhotoInput { key_name: "statement", camera: true, max_size: 3000 }
                    }
                },
            }
            div {
                style: "padding:12px 16px; background:{theme::HEADER_BG};",
                label {
                    style: "display:flex; align-items:center; gap:10px; font-size:13px; font-weight:600; color:{theme::TEXT_MUTED};",
                    {t.t("Bank")}
                    select {
                        style: "{theme::INPUT} flex:1; width:auto; padding:8px 12px; font-size:14px;",
                        aria_label: t.t("Bank"),
                        value: bank().code(),
                        onchange: move |e| bank.set(Bank::from_code(&e.value())),
                        for b in Bank::ALL {
                            option { value: b.code(), selected: b == bank(), "{b.name()}" }
                        }
                    }
                }
            }
            UnderlineTabs {
                tabs: tab_items,
                active: active_tab,
                on_select: move |i: usize| { tab.set(tab_defs[i].clone()); },
            }

            div {
                class: "scroll-col",
                style: "flex:1; overflow-y:auto; padding:16px; display:flex; flex-direction:column; gap:12px;",

                if let Some(s) = status {
                    div { style: "padding:12px 14px; border-radius:12px; background:{theme::PRIMARY_SOFT}; color:{theme::PRIMARY}; font-size:13px; display:flex; align-items:center; gap:8px;",
                        Icon { name: IconName::Clock, size: 16 }
                        "{s}"
                    }
                }
                if !errors.read().is_empty() {
                    div {
                        style: "padding:12px 14px; border-radius:12px; background:{theme::NEGATIVE_SOFT}; color:{theme::DANGER}; font-size:13px; display:flex; gap:8px; align-items:flex-start;",
                        div { style: "flex:1;",
                            for (n, msg) in errors.read().iter() {
                                div {
                                    {t.t("Photo {n}: {error}").replace("{n}", &n.to_string()).replace("{error}", &t.tr(msg))}
                                }
                            }
                        }
                        button {
                            style: "border:none; background:transparent; color:{theme::DANGER}; cursor:pointer; padding:0; display:flex;",
                            aria_label: t.t("Dismiss"),
                            onclick: move |_| errors.write().clear(),
                            Icon { name: IconName::X, size: 16, color: theme::DANGER.to_string() }
                        }
                    }
                }
                if let Some(e) = load_error {
                    div { style: "color:{theme::DANGER}; font-size:13px;", "{t.tr(&e)}" }
                }

                match tab.read().clone() {
                    StatementsTab::Accounts     => rsx! { AccountsTab { statements: statements.clone(), selected } },
                    StatementsTab::Transactions => rsx! { TransactionsTab { statements: statements.clone(), selected } },
                    StatementsTab::Photos       => rsx! { PhotosTab {
                        statements: statements.clone(),
                        version,
                        busy: total() > done(),
                        on_cleared: move |_| {
                            selected.write().clear();
                            errors.write().clear();
                        },
                    } },
                }
            }
        }
    }
}

// ─── Accounts ─────────────────────────────────────────────────────────────────

#[component]
fn AccountsTab(statements: Vec<Statement>, mut selected: Signal<Vec<String>>) -> Element {
    let t = use_i18n();
    let mut search = use_signal(String::new);
    let all = StatementService::counterparties(&statements);

    if all.is_empty() {
        return rsx! {
            div { style: "text-align:center; color:{theme::TEXT_MUTED}; padding:40px 0; font-size:14px;", {t.t("Add statement photos to see accounts")} }
        };
    }

    let q = search.read().trim().to_lowercase();
    let shown: Vec<CounterpartyAccount> = all
        .into_iter()
        .filter(|a| {
            q.is_empty()
                || [&a.name, &a.account_no, &a.bank]
                    .iter()
                    .any(|f| f.as_deref().is_some_and(|v| v.to_lowercase().contains(&q)))
        })
        .collect();
    let shown_keys: Vec<String> = shown.iter().map(|a| a.key.clone()).collect();

    rsx! {
        input {
            style: "{theme::INPUT} margin-bottom:10px;",
            r#type: "text",
            placeholder: t.t("Search name, account or bank…"),
            value: "{search}",
            oninput: move |e| search.set(e.value()),
        }
        div { style: "display:flex; gap:8px; margin-bottom:10px;",
            button {
                style: "{theme::CHIP} flex:1;",
                onclick: move |_| {
                    let mut sel = selected.write();
                    for k in &shown_keys {
                        if !sel.contains(k) { sel.push(k.clone()); }
                    }
                },
                {t.t("Select all shown")}
            }
            button {
                style: "{theme::CHIP} flex:1;",
                onclick: move |_| selected.write().clear(),
                {t.t("Clear selection")}
            }
        }
        div {
            style: "background:#fff; border-radius:16px; overflow:hidden;",
            for a in shown {
                {
                    let is_sel = selected.read().contains(&a.key);
                    let key = a.key.clone();
                    let name = a.name.clone().unwrap_or_else(|| t.t("Unknown name").to_string());
                    let acc = a.account_no.clone().unwrap_or_else(|| t.t("No account number").to_string());
                    let sub = match &a.bank { Some(b) => format!("{acc} · {b}"), None => acc };
                    rsx! {
                        div {
                            key: "{a.key}",
                            style: "display:flex; align-items:center; gap:10px; padding:12px 16px; {theme::ROW_DIVIDER} cursor:pointer;",
                            onclick: move |_| {
                                let mut sel = selected.write();
                                if let Some(i) = sel.iter().position(|k| *k == key) { sel.remove(i); } else { sel.push(key.clone()); }
                            },
                            // Checkbox drawn with the theme instead of ☑️ / ⬜ emoji
                            span {
                                style: if is_sel { "width:22px; height:22px; border-radius:7px; flex-shrink:0; display:flex; align-items:center; justify-content:center; background:#3f51b5; border:2px solid #3f51b5;" }
                                       else { "width:22px; height:22px; border-radius:7px; flex-shrink:0; display:flex; align-items:center; justify-content:center; background:#fff; border:2px solid #c9cbe0;" },
                                if is_sel { Icon { name: IconName::Check, size: 14, color: "#ffffff".to_string() } }
                            }
                            div { style: "flex:1; min-width:0;",
                                div { style: "font-size:14px; font-weight:600; color:{theme::TEXT}; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{name}" }
                                div { style: "font-size:12px; color:{theme::TEXT_MUTED}; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{sub}" }
                            }
                            div { style: "text-align:right; font-size:12px; flex-shrink:0;",
                                if a.sent > Decimal::ZERO {
                                    div { style: "color:{theme::NEGATIVE}; font-weight:600; display:inline-flex; align-items:center; gap:2px; justify-content:flex-end; width:100%;",
                                        Icon { name: IconName::ArrowUp, size: 12 }
                                        "{fmt_vnd(a.sent)}"
                                    }
                                }
                                if a.received > Decimal::ZERO {
                                    div { style: "color:{theme::POSITIVE}; font-weight:600; display:inline-flex; align-items:center; gap:2px; justify-content:flex-end; width:100%;",
                                        Icon { name: IconName::ArrowDown, size: 12 }
                                        "{fmt_vnd(a.received)}"
                                    }
                                }
                                div { style: "color:{theme::TEXT_MUTED};",
                                    {t.t("{n} txns").replace("{n}", &a.count.to_string())}
                                    if let Some(d) = a.last_date { " · {crate::views::fmt_date(d)}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── Transactions ─────────────────────────────────────────────────────────────

#[component]
fn TransactionsTab(statements: Vec<Statement>, selected: Signal<Vec<String>>) -> Element {
    let mut search = use_signal(String::new);
    let t = use_i18n();
    let keys = selected.read().clone();
    let query = search.read().trim().to_string();
    let show_hint = keys.is_empty() && query.is_empty();
    let txns = if show_hint { Vec::new() } else { StatementService::transactions_for(&statements, &keys, &query) };
    let sent: Decimal = txns.iter().map(|(_, x)| x.debit).sum();
    let received: Decimal = txns.iter().map(|(_, x)| x.credit).sum();
    let net = received - sent;
    let net_color = if net < Decimal::ZERO { theme::NEGATIVE } else { theme::POSITIVE };

    rsx! {
        input {
            style: "{theme::INPUT} margin-bottom:10px;",
            r#type: "text",
            placeholder: t.t("Search transfer content…"),
            value: "{search}",
            oninput: move |e| search.set(e.value()),
        }
        if show_hint {
            div { style: "text-align:center; color:{theme::TEXT_MUTED}; padding:40px 0; font-size:14px;", {t.t("Select accounts in the Accounts tab")} }
        } else if txns.is_empty() {
            div { style: "text-align:center; color:{theme::TEXT_MUTED}; padding:40px 0; font-size:14px;", {t.t("No matching transactions")} }
        } else {
            div {
                style: "display:flex; gap:8px; {theme::CARD} margin-bottom:12px;",
                div { style: "flex:1;",
                    div { style: "font-size:12px; color:{theme::TEXT_MUTED};", {t.t("Sent")} }
                    div { style: "font-size:14px; font-weight:700; color:{theme::NEGATIVE};", "{fmt_vnd(sent)}" }
                }
                div { style: "flex:1;",
                    div { style: "font-size:12px; color:{theme::TEXT_MUTED};", {t.t("Received")} }
                    div { style: "font-size:14px; font-weight:700; color:{theme::POSITIVE};", "{fmt_vnd(received)}" }
                }
                div { style: "flex:1;",
                    div { style: "font-size:12px; color:{theme::TEXT_MUTED};", {t.t("Net")} }
                    div { style: "font-size:14px; font-weight:700; color:{net_color};", "{fmt_vnd(net)}" }
                }
            }
            div {
                style: "background:#fff; border-radius:16px; overflow:hidden;",
                for (i, (own, txn)) in txns.into_iter().enumerate() {
                    TxnRow { key: "{i}", own, txn }
                }
            }
        }
    }
}

#[component]
fn TxnRow(own: Option<String>, txn: StatementTxn) -> Element {
    let t = use_i18n();
    let signed = txn.signed_amount();
    let (sign, color) = if signed < Decimal::ZERO { ("−", theme::NEGATIVE) } else { ("+", theme::POSITIVE) };
    let date = txn.date.map(crate::views::fmt_date).unwrap_or_else(|| "—".into());
    let who = [txn.counterparty_name.clone(), txn.counterparty_account.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let who = if who.is_empty() { t.t("Unknown name").to_string() } else { who };

    rsx! {
        div {
            style: "display:flex; gap:10px; padding:12px 16px; {theme::ROW_DIVIDER}",
            div { style: "flex:1; min-width:0;",
                div { style: "font-size:12px; color:{theme::TEXT_MUTED};", "{date}" }
                div { style: "font-size:14px; font-weight:600; color:{theme::TEXT}; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{who}" }
                if !txn.details.is_empty() {
                    div {
                        style: "font-size:12px; color:{theme::TEXT_MUTED}; overflow:hidden; display:-webkit-box; -webkit-line-clamp:2; -webkit-box-orient:vertical;",
                        "{txn.details}"
                    }
                }
                if txn.needs_review {
                    span { style: "display:inline-block; margin-top:4px; padding:2px 6px; border-radius:6px; background:#fef3c7; color:#b45309; font-size:11px; font-weight:600; display:inline-flex; align-items:center; gap:4px;",
                        Icon { name: IconName::AlertTriangle, size: 12 }
                        {t.t("Check")}
                    }
                }
            }
            div { style: "text-align:right; flex-shrink:0;",
                div { style: "font-size:14px; font-weight:700; color:{color};", "{sign}{fmt_vnd(signed.abs())}" }
                if let Some(own) = own {
                    div { style: "font-size:11px; color:{theme::TEXT_MUTED};", "{own}" }
                }
            }
        }
    }
}

// ─── Photos ───────────────────────────────────────────────────────────────────

#[component]
fn PhotosTab(statements: Vec<Statement>, mut version: Signal<u32>, busy: bool, on_cleared: EventHandler<()>) -> Element {
    let t = use_i18n();
    let mut confirm = use_signal(|| None::<String>);
    let mut delete_error = use_signal(|| None::<String>);
    let mut confirm_all = use_signal(|| false);

    if statements.is_empty() {
        return rsx! {
            div { style: "text-align:center; color:{theme::TEXT_MUTED}; padding:40px 0; font-size:14px;", {t.t("Add statement photos to see accounts")} }
        };
    }

    let clear_style = format!(
        "{} width:100%; margin-bottom:12px;{}",
        theme::BTN_DANGER,
        if busy { " opacity:.5;" } else { "" }
    );

    rsx! {
        button {
            style: "{clear_style}",
            disabled: busy,
            onclick: move |_| confirm_all.set(true),
            IconText { text: t.t("🗑 Clear all statement data").to_string() }
        }
        if let Some(e) = delete_error() {
            div { style: "color:{theme::DANGER}; font-size:13px;", "{t.tr(&e)}" }
        }
        for s in statements.iter() {
            StatementCard { key: "{s.id}", statement: s.clone(), on_delete: move |id: String| confirm.set(Some(id)) }
        }

        // ── Clear-all confirm ──
        if confirm_all() {
            div {
                style: "{theme::OVERLAY}",
                div {
                    style: "{theme::DIALOG}",
                    h3 { style: "margin:0 0 8px; font-size:16px; font-weight:600; color:{theme::TEXT};", {t.t("Clear all statement data?")} }
                    p  { style: "margin:0 0 20px; font-size:14px; color:{theme::TEXT_MUTED};", {t.t("All imported statements and their transactions will be deleted. This cannot be undone.")} }
                    div { style: "display:flex; gap:10px;",
                        button {
                            style: "{theme::BTN_SECONDARY} flex:1;",
                            onclick: move |_| { confirm_all.set(false); },
                            {t.t("Cancel")}
                        }
                        button {
                            style: "{theme::BTN_DANGER} flex:1;",
                            onclick: move |_| {
                                if !busy {
                                    match store::clear() {
                                        Ok(()) => { delete_error.set(None); version += 1; on_cleared.call(()); }
                                        Err(e) => delete_error.set(Some(err_msg(&e))),
                                    }
                                }
                                confirm_all.set(false);
                            },
                            {t.t("Clear all")}
                        }
                    }
                }
            }
        }

        // ── Delete confirm ──
        if let Some(id) = confirm.read().clone() {
            div {
                style: "{theme::OVERLAY}",
                div {
                    style: "{theme::DIALOG}",
                    h3 { style: "margin:0 0 8px; font-size:16px; font-weight:600; color:{theme::TEXT};", {t.t("Delete this statement photo?")} }
                    p  { style: "margin:0 0 20px; font-size:14px; color:{theme::TEXT_MUTED};", {t.t("Its transactions will be removed from the lists.")} }
                    div { style: "display:flex; gap:10px;",
                        button {
                            style: "{theme::BTN_SECONDARY} flex:1;",
                            onclick: move |_| { confirm.set(None); },
                            {t.t("Cancel")}
                        }
                        button {
                            style: "{theme::BTN_DANGER} flex:1;",
                            onclick: move |_| {
                                match store::delete(&id) {
                                    Ok(()) => { delete_error.set(None); version += 1; }
                                    Err(e) => delete_error.set(Some(err_msg(&e))),
                                }
                                confirm.set(None);
                            },
                            {t.t("Delete")}
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StatementCard(statement: Statement, on_delete: EventHandler<String>) -> Element {
    let t = use_i18n();
    let s = &statement;
    let fmt = |d: chrono::NaiveDate| crate::views::fmt_date(d);
    let period = match (s.period_from, s.period_to) {
        (Some(a), Some(b)) => Some(format!("{} – {}", fmt(a), fmt(b))),
        _ => None,
    };
    let review = s.transactions.iter().filter(|x| x.needs_review).count();
    let id = s.id.clone();
    let imported = crate::views::ts_to_date_str(s.imported_at);

    rsx! {
        div {
            style: "{theme::CARD} margin-bottom:10px; display:flex; gap:10px;",
            div { style: "flex:1; min-width:0; font-size:13px; color:{theme::TEXT_MUTED};",
                div { style: "font-size:14px; font-weight:600; color:{theme::TEXT};", "{s.account_no.clone().unwrap_or_else(|| \"—\".into())}" }
                if let Some(n) = &s.account_name { div { "{n}" } }
                if let Some(p) = period { div { "{p}" } }
                if let Some(p) = &s.page { div { {t.t("Page {p}").replace("{p}", p)} } }
                if let Some(b) = s.opening_balance {
                    div { {t.t("Opening balance {amount}").replace("{amount}", &fmt_vnd(b))} }
                }
                div { {t.t("{n} transactions").replace("{n}", &s.transactions.len().to_string())} }
                if review > 0 {
                    div { style: "color:#b45309;", IconText { text: t.t("⚠️ {n} rows need checking").replace("{n}", &review.to_string()), size: 14, gap: 4 } }
                }
                for w in s.warnings.iter() {
                    div { style: "color:#b45309;", "{t.tr(w)}" }
                }
                div { style: "font-size:11px; color:{theme::TEXT_MUTED}; margin-top:4px;", {t.t("Imported {date}").replace("{date}", &imported)} }
            }
            button {
                style: "align-self:flex-start; border:none; background:transparent; color:{theme::DANGER}; font-size:13px; cursor:pointer;",
                onclick: move |_| on_delete.call(id.clone()),
                {t.t("Delete")}
            }
        }
    }
}
