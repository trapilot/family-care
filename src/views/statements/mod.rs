use dioxus::prelude::*;
use futures_util::StreamExt;
use rust_decimal::Decimal;

use crate::error::AppError;
use crate::i18n::use_i18n;
use crate::models::{CounterpartyAccount, Statement, StatementTxn};
use crate::services::statement::{ocr, parser, store};
use crate::services::StatementService;
use crate::views::photo_picker::{use_photo_picker, PhotoInput};
use crate::views::{fmt_vnd, now_ts};

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
async fn import_photo(jpeg: Vec<u8>) -> Result<(), AppError> {
    let page = ocr::recognize_async(jpeg).await?;
    let parsed = parser::parse(&page)?;
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
    let mut selected = use_signal(Vec::<String>::new);
    let mut version = use_signal(|| 0u32);
    let mut errors = use_signal(Vec::<(usize, String)>::new);
    let mut total = use_signal(|| 0usize);
    let mut done = use_signal(|| 0usize);

    // Photos are read one after another so the UI never freezes
    let importer = use_coroutine(move |mut rx: UnboundedReceiver<Vec<u8>>| async move {
        while let Some(jpeg) = rx.next().await {
            let n = done() + 1;
            match import_photo(jpeg).await {
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
            importer.send(jpeg);
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
    let photo_btn = "padding:8px 12px; border-radius:8px; background:#6366f1; color:#fff; font-size:13px; font-weight:600; cursor:pointer; white-space:nowrap;";

    rsx! {
        div {
            style: "display:flex; flex-direction:column; height:100%;",

            // ── Header ──
            div {
                style: "padding:16px 16px 0; background:#fff;",
                div {
                    style: "display:flex; align-items:center; justify-content:space-between; gap:12px; margin:0 0 12px;",
                    h1 { style: "margin:0; font-size:20px; font-weight:700; color:#1f2937;", {t.t("Statements")} }
                    div { style: "display:flex; gap:8px;",
                        label {
                            style: "{photo_btn}",
                            {t.t("📷 Add photos")}
                            PhotoInput { key_name: "statement", multiple: true, max_size: 3000 }
                        }
                        label {
                            style: "{photo_btn}",
                            "📸"
                            PhotoInput { key_name: "statement", camera: true, max_size: 3000 }
                        }
                    }
                }
                div {
                    style: "display:flex; border-bottom:1px solid #f3f4f6;",
                    for (tab_kind, label) in tab_labels {
                        {
                            let is_active = *tab.read() == tab_kind;
                            let target = tab_kind.clone();
                            rsx! {
                                button {
                                    style: if is_active {
                                        "flex:1; padding:10px 0; border:none; border-bottom:2px solid #6366f1; background:#fff; font-size:13px; font-weight:600; color:#6366f1; cursor:pointer;"
                                    } else {
                                        "flex:1; padding:10px 0; border:none; border-bottom:2px solid transparent; background:#fff; font-size:13px; color:#9ca3af; cursor:pointer;"
                                    },
                                    onclick: move |_| { tab.set(target.clone()); },
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }

            div {
                style: "flex:1; overflow-y:auto; padding:16px;",

                if let Some(s) = status {
                    div { style: "margin-bottom:12px; padding:10px 12px; border-radius:8px; background:#eef2ff; color:#4f46e5; font-size:13px;", "⏳ {s}" }
                }
                if !errors.read().is_empty() {
                    div {
                        style: "margin-bottom:12px; padding:10px 12px; border-radius:8px; background:#fef2f2; color:#b91c1c; font-size:13px; display:flex; gap:8px; align-items:flex-start;",
                        div { style: "flex:1;",
                            for (n, msg) in errors.read().iter() {
                                div {
                                    {t.t("Photo {n}: {error}").replace("{n}", &n.to_string()).replace("{error}", &t.tr(msg))}
                                }
                            }
                        }
                        button {
                            style: "border:none; background:transparent; color:#b91c1c; font-size:16px; cursor:pointer; line-height:1;",
                            aria_label: t.t("Dismiss"),
                            onclick: move |_| errors.write().clear(),
                            "✕"
                        }
                    }
                }
                if let Some(e) = load_error {
                    div { style: "margin-bottom:12px; color:#b91c1c; font-size:13px;", "{t.tr(&e)}" }
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
            div { style: "text-align:center; color:#9ca3af; padding:40px 0; font-size:14px;", {t.t("Add statement photos to see accounts")} }
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
            style: "width:100%; box-sizing:border-box; border:1px solid #e5e7eb; border-radius:8px; padding:10px 12px; font-size:16px; outline:none; background:#fff; margin-bottom:10px;",
            r#type: "text",
            placeholder: t.t("Search name, account or bank…"),
            value: "{search}",
            oninput: move |e| search.set(e.value()),
        }
        div { style: "display:flex; gap:8px; margin-bottom:10px;",
            button {
                style: "flex:1; padding:8px; border:1px solid #e5e7eb; border-radius:8px; background:#fff; font-size:13px; cursor:pointer;",
                onclick: move |_| {
                    let mut sel = selected.write();
                    for k in &shown_keys {
                        if !sel.contains(k) { sel.push(k.clone()); }
                    }
                },
                {t.t("Select all shown")}
            }
            button {
                style: "flex:1; padding:8px; border:1px solid #e5e7eb; border-radius:8px; background:#fff; font-size:13px; cursor:pointer;",
                onclick: move |_| selected.write().clear(),
                {t.t("Clear selection")}
            }
        }
        div {
            style: "background:#fff; border-radius:12px; overflow:hidden; box-shadow:0 1px 3px rgba(0,0,0,.06);",
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
                            style: "display:flex; align-items:center; gap:10px; padding:12px; border-bottom:1px solid #f3f4f6; cursor:pointer;",
                            onclick: move |_| {
                                let mut sel = selected.write();
                                if let Some(i) = sel.iter().position(|k| *k == key) { sel.remove(i); } else { sel.push(key.clone()); }
                            },
                            span { style: "font-size:20px; flex-shrink:0;", if is_sel { "☑️" } else { "⬜" } }
                            div { style: "flex:1; min-width:0;",
                                div { style: "font-size:14px; font-weight:700; color:#1f2937; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{name}" }
                                div { style: "font-size:12px; color:#9ca3af; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{sub}" }
                            }
                            div { style: "text-align:right; font-size:12px; flex-shrink:0;",
                                if a.sent > Decimal::ZERO {
                                    div { style: "color:#ef4444; font-weight:600;", "↑ {fmt_vnd(a.sent)}" }
                                }
                                if a.received > Decimal::ZERO {
                                    div { style: "color:#10b981; font-weight:600;", "↓ {fmt_vnd(a.received)}" }
                                }
                                div { style: "color:#9ca3af;",
                                    {t.t("{n} txns").replace("{n}", &a.count.to_string())}
                                    if let Some(d) = a.last_date { " · {d.format(\"%d/%m/%Y\")}" }
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
    let net_color = if net < Decimal::ZERO { "#ef4444" } else { "#10b981" };

    rsx! {
        input {
            style: "width:100%; box-sizing:border-box; border:1px solid #e5e7eb; border-radius:8px; padding:10px 12px; font-size:16px; outline:none; background:#fff; margin-bottom:10px;",
            r#type: "text",
            placeholder: t.t("Search transfer content…"),
            value: "{search}",
            oninput: move |e| search.set(e.value()),
        }
        if show_hint {
            div { style: "text-align:center; color:#9ca3af; padding:40px 0; font-size:14px;", {t.t("Select accounts in the Accounts tab")} }
        } else if txns.is_empty() {
            div { style: "text-align:center; color:#9ca3af; padding:40px 0; font-size:14px;", {t.t("No matching transactions")} }
        } else {
            div {
                style: "display:flex; gap:8px; background:#fff; border-radius:12px; padding:12px; margin-bottom:12px; box-shadow:0 1px 3px rgba(0,0,0,.06);",
                div { style: "flex:1;",
                    div { style: "font-size:12px; color:#9ca3af;", {t.t("Sent")} }
                    div { style: "font-size:14px; font-weight:700; color:#ef4444;", "{fmt_vnd(sent)}" }
                }
                div { style: "flex:1;",
                    div { style: "font-size:12px; color:#9ca3af;", {t.t("Received")} }
                    div { style: "font-size:14px; font-weight:700; color:#10b981;", "{fmt_vnd(received)}" }
                }
                div { style: "flex:1;",
                    div { style: "font-size:12px; color:#9ca3af;", {t.t("Net")} }
                    div { style: "font-size:14px; font-weight:700; color:{net_color};", "{fmt_vnd(net)}" }
                }
            }
            div {
                style: "background:#fff; border-radius:12px; overflow:hidden; box-shadow:0 1px 3px rgba(0,0,0,.06);",
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
    let (sign, color) = if signed < Decimal::ZERO { ("−", "#ef4444") } else { ("+", "#10b981") };
    let date = txn.date.map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_else(|| "—".into());
    let who = [txn.counterparty_name.clone(), txn.counterparty_account.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let who = if who.is_empty() { t.t("Unknown name").to_string() } else { who };

    rsx! {
        div {
            style: "display:flex; gap:10px; padding:12px; border-bottom:1px solid #f3f4f6;",
            div { style: "flex:1; min-width:0;",
                div { style: "font-size:12px; color:#6b7280;", "{date}" }
                div { style: "font-size:14px; font-weight:600; color:#1f2937; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{who}" }
                if !txn.details.is_empty() {
                    div {
                        style: "font-size:12px; color:#6b7280; overflow:hidden; display:-webkit-box; -webkit-line-clamp:2; -webkit-box-orient:vertical;",
                        "{txn.details}"
                    }
                }
                if txn.needs_review {
                    span { style: "display:inline-block; margin-top:4px; padding:2px 6px; border-radius:6px; background:#fef3c7; color:#b45309; font-size:11px; font-weight:600;", "⚠️ {t.t(\"Check\")}" }
                }
            }
            div { style: "text-align:right; flex-shrink:0;",
                div { style: "font-size:14px; font-weight:700; color:{color};", "{sign}{fmt_vnd(signed.abs())}" }
                if let Some(own) = own {
                    div { style: "font-size:11px; color:#9ca3af;", "{own}" }
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
            div { style: "text-align:center; color:#9ca3af; padding:40px 0; font-size:14px;", {t.t("Add statement photos to see accounts")} }
        };
    }

    let clear_style = format!(
        "width:100%; padding:10px; margin-bottom:12px; border:1px solid #fecaca; border-radius:8px; background:#fff; color:#ef4444; font-size:14px; font-weight:600; cursor:pointer;{}",
        if busy { " opacity:.5;" } else { "" }
    );

    rsx! {
        button {
            style: "{clear_style}",
            disabled: busy,
            onclick: move |_| confirm_all.set(true),
            {t.t("🗑 Clear all statement data")}
        }
        if let Some(e) = delete_error() {
            div { style: "margin-bottom:12px; color:#b91c1c; font-size:13px;", "{t.tr(&e)}" }
        }
        for s in statements.iter() {
            StatementCard { key: "{s.id}", statement: s.clone(), on_delete: move |id: String| confirm.set(Some(id)) }
        }

        // ── Clear-all confirm ──
        if confirm_all() {
            div {
                style: "position:fixed; inset:0; background:rgba(0,0,0,.4); display:flex; align-items:center; justify-content:center; z-index:100;",
                div {
                    style: "background:#fff; border-radius:16px; padding:24px; margin:16px; max-width:320px; width:100%;",
                    h3 { style: "margin:0 0 8px; font-size:17px;", {t.t("Clear all statement data?")} }
                    p  { style: "margin:0 0 20px; font-size:14px; color:#6b7280;", {t.t("All imported statements and their transactions will be deleted. This cannot be undone.")} }
                    div { style: "display:flex; gap:10px;",
                        button {
                            style: "flex:1; padding:10px; border:1px solid #e5e7eb; border-radius:8px; background:#fff; cursor:pointer; font-size:14px;",
                            onclick: move |_| { confirm_all.set(false); },
                            {t.t("Cancel")}
                        }
                        button {
                            style: "flex:1; padding:10px; border:none; border-radius:8px; background:#ef4444; color:#fff; cursor:pointer; font-size:14px; font-weight:600;",
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
                style: "position:fixed; inset:0; background:rgba(0,0,0,.4); display:flex; align-items:center; justify-content:center; z-index:100;",
                div {
                    style: "background:#fff; border-radius:16px; padding:24px; margin:16px; max-width:320px; width:100%;",
                    h3 { style: "margin:0 0 8px; font-size:17px;", {t.t("Delete this statement photo?")} }
                    p  { style: "margin:0 0 20px; font-size:14px; color:#6b7280;", {t.t("Its transactions will be removed from the lists.")} }
                    div { style: "display:flex; gap:10px;",
                        button {
                            style: "flex:1; padding:10px; border:1px solid #e5e7eb; border-radius:8px; background:#fff; cursor:pointer; font-size:14px;",
                            onclick: move |_| { confirm.set(None); },
                            {t.t("Cancel")}
                        }
                        button {
                            style: "flex:1; padding:10px; border:none; border-radius:8px; background:#ef4444; color:#fff; cursor:pointer; font-size:14px; font-weight:600;",
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
    let fmt = |d: chrono::NaiveDate| d.format("%d/%m/%Y").to_string();
    let period = match (s.period_from, s.period_to) {
        (Some(a), Some(b)) => Some(format!("{} – {}", fmt(a), fmt(b))),
        _ => None,
    };
    let review = s.transactions.iter().filter(|x| x.needs_review).count();
    let id = s.id.clone();
    let imported = crate::views::ts_to_date_str(s.imported_at);

    rsx! {
        div {
            style: "background:#fff; border-radius:12px; padding:12px; margin-bottom:10px; box-shadow:0 1px 3px rgba(0,0,0,.06); display:flex; gap:10px;",
            div { style: "flex:1; min-width:0; font-size:13px; color:#6b7280;",
                div { style: "font-size:14px; font-weight:700; color:#1f2937;", "{s.account_no.clone().unwrap_or_else(|| \"—\".into())}" }
                if let Some(n) = &s.account_name { div { "{n}" } }
                if let Some(p) = period { div { "{p}" } }
                if let Some(p) = &s.page { div { {t.t("Page {p}").replace("{p}", p)} } }
                if let Some(b) = s.opening_balance {
                    div { {t.t("Opening balance {amount}").replace("{amount}", &fmt_vnd(b))} }
                }
                div { {t.t("{n} transactions").replace("{n}", &s.transactions.len().to_string())} }
                if review > 0 {
                    div { style: "color:#b45309;", {t.t("⚠️ {n} rows need checking").replace("{n}", &review.to_string())} }
                }
                for w in s.warnings.iter() {
                    div { style: "color:#b45309;", "{t.tr(w)}" }
                }
                div { style: "font-size:11px; color:#9ca3af; margin-top:4px;", {t.t("Imported {date}").replace("{date}", &imported)} }
            }
            button {
                style: "align-self:flex-start; border:none; background:transparent; color:#ef4444; font-size:13px; cursor:pointer;",
                onclick: move |_| on_delete.call(id.clone()),
                {t.t("Delete")}
            }
        }
    }
}
