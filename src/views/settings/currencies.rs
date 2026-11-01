use dioxus::prelude::*;
use crate::i18n::use_i18n;

use crate::app::AppState;
use crate::error::AppError;
use crate::models::{Currency, CurrencyInfo};
use crate::repository::CurrencyRepo;

// ─── Money types ──────────────────────────────────────────────────────────────

/// The fixed VND / USD / GOLD money types: reorder, show / hide and rename them.
/// Types can't be added or deleted.
#[component]
pub fn CurrenciesSettings() -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let list = use_signal(Vec::<CurrencyInfo>::new);
    // The row being edited, with its draft name / unit
    let mut editing = use_signal(|| None::<Currency>);
    let mut name    = use_signal(String::new);
    let mut unit    = use_signal(String::new);
    // Last repo error and the row it belongs to
    let mut error   = use_signal(|| None::<(Currency, String)>);

    let reload = {
        let state = state.clone();
        let mut list = list;
        move || {
            state.with_conn(|conn| {
                if let Ok(l) = CurrencyRepo::list(conn) { list.set(l); }
            });
        }
    };

    use_effect({
        let mut reload = reload.clone();
        move || reload()
    });

    // Apply a repo change to one row, then reload; a refused change is shown under that row
    let apply = {
        let state = state.clone();
        let reload = reload.clone();
        move |code: Currency, change: &dyn Fn(&rusqlite::Connection) -> crate::error::Result<()>| -> bool {
            let result = state.with_conn(|conn| change(conn));
            let mut reload = reload.clone();
            match result {
                Ok(()) => { error.set(None); reload(); true }
                Err(e) => {
                    let msg = match e {
                        AppError::Validation(m) => t.tr(&m),
                        other                   => other.to_string(),
                    };
                    error.set(Some((code, msg)));
                    false
                }
            }
        }
    };

    let items = list.read().clone();
    let last = items.len().saturating_sub(1);

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:14px;",

            // ── Header ──
            div {
                h2 { style: "margin:0; font-size:16px; font-weight:700; color:#1f2937;", {t.t("Money types")} }
                p { style: "margin:4px 0 0; font-size:12px; color:#6b7280; line-height:1.4;",
                    {t.t("Money types every wallet can hold. Hidden types are left out of pickers.")}
                }
            }

            div {
                style: "background:#fff; border-radius:12px; overflow:hidden; box-shadow:0 1px 3px rgba(0,0,0,.06);",
                for (i, info) in items.iter().enumerate() {
                    {
                        let code = info.code.clone();
                        let is_editing = editing.read().as_ref() == Some(&code);
                        let order: Vec<Currency> = items.iter().map(|c| c.code.clone()).collect();
                        let row_error = error.read().as_ref().filter(|(c, _)| *c == code).map(|(_, m)| m.clone());
                        let display_name = t.tr(&info.name);
                        let status = if info.is_active { t.t("Shown") } else { t.t("Hidden") };
                        rsx! {
                            div {
                                key: "{code}",
                                style: "padding:12px 14px; border-bottom:1px solid #f3f4f6; display:flex; flex-direction:column; gap:8px;",

                                if is_editing {
                                    // ── Inline edit ──
                                    div { style: "display:flex; gap:6px; align-items:center;",
                                        span {
                                            style: "font-size:11px; font-weight:700; color:#4f46e5; background:#eef2ff; border-radius:6px; padding:3px 7px; flex-shrink:0;",
                                            "{code}"
                                        }
                                        input {
                                            style: "flex:1; min-width:0; border:1px solid #6366f1; border-radius:8px; padding:8px 10px; font-size:14px; outline:none;",
                                            r#type: "text",
                                            placeholder: t.t("Name"),
                                            value: "{name.read()}",
                                            oninput: move |e| name.set(e.value()),
                                        }
                                        input {
                                            style: "width:64px; border:1px solid #6366f1; border-radius:8px; padding:8px 10px; font-size:14px; outline:none;",
                                            r#type: "text",
                                            placeholder: t.t("Unit"),
                                            value: "{unit.read()}",
                                            oninput: move |e| unit.set(e.value()),
                                        }
                                    }
                                    div { style: "display:flex; gap:6px; justify-content:flex-end;",
                                        button {
                                            style: "padding:7px 14px; background:#6366f1; color:#fff; border:none; border-radius:8px; font-size:13px; font-weight:600; cursor:pointer;",
                                            onclick: {
                                                let mut apply = apply.clone();
                                                let info = info.clone();
                                                let code = code.clone();
                                                move |_| {
                                                    let mut updated = info.clone();
                                                    updated.name = name.read().clone();
                                                    updated.unit = unit.read().clone();
                                                    if apply(code.clone(), &|conn| CurrencyRepo::update(conn, &updated)) {
                                                        editing.set(None);
                                                    }
                                                }
                                            },
                                            {t.t("Save")}
                                        }
                                        button {
                                            style: "padding:7px 10px; background:#f3f4f6; color:#6b7280; border:none; border-radius:8px; font-size:13px; cursor:pointer;",
                                            onclick: move |_| { editing.set(None); error.set(None); },
                                            "✕"
                                        }
                                    }
                                } else {
                                    // ── Row ──
                                    div { style: "display:flex; gap:10px; align-items:center;",
                                        span {
                                            style: "font-size:11px; font-weight:700; color:#4f46e5; background:#eef2ff; border-radius:6px; padding:3px 7px; flex-shrink:0;",
                                            "{code}"
                                        }
                                        div { style: if info.is_active { "flex:1; min-width:0;" } else { "flex:1; min-width:0; opacity:.55;" },
                                            div { style: "font-size:14px; font-weight:600; color:#1f2937; overflow-wrap:anywhere;", "{display_name}" }
                                            div { style: "font-size:12px; color:#9ca3af;", "{t.t(\"Unit\")}: {info.unit} · {status}" }
                                        }
                                        button {
                                            style: "border:none; background:transparent; font-size:16px; padding:4px; cursor:pointer;",
                                            disabled: i == 0,
                                            onclick: {
                                                let mut apply = apply.clone();
                                                let code = code.clone();
                                                let mut order = order.clone();
                                                move |_| {
                                                    order.swap(i, i - 1);
                                                    apply(code.clone(), &|conn| CurrencyRepo::reorder(conn, &order));
                                                }
                                            },
                                            "↑"
                                        }
                                        button {
                                            style: "border:none; background:transparent; font-size:16px; padding:4px; cursor:pointer;",
                                            disabled: i == last,
                                            onclick: {
                                                let mut apply = apply.clone();
                                                let code = code.clone();
                                                let mut order = order.clone();
                                                move |_| {
                                                    order.swap(i, i + 1);
                                                    apply(code.clone(), &|conn| CurrencyRepo::reorder(conn, &order));
                                                }
                                            },
                                            "↓"
                                        }
                                        button {
                                            style: if info.is_active {
                                                "padding:5px 10px; border:none; border-radius:14px; background:#d1fae5; color:#047857; font-size:12px; font-weight:600; cursor:pointer;"
                                            } else {
                                                "padding:5px 10px; border:none; border-radius:14px; background:#f3f4f6; color:#6b7280; font-size:12px; font-weight:600; cursor:pointer;"
                                            },
                                            onclick: {
                                                let mut apply = apply.clone();
                                                let info = info.clone();
                                                let code = code.clone();
                                                move |_| {
                                                    let mut toggled = info.clone();
                                                    toggled.is_active = !toggled.is_active;
                                                    apply(code.clone(), &|conn| CurrencyRepo::update(conn, &toggled));
                                                }
                                            },
                                            "{status}"
                                        }
                                        button {
                                            style: "border:none; background:transparent; font-size:16px; cursor:pointer; padding:4px;",
                                            onclick: {
                                                let info = info.clone();
                                                move |_| {
                                                    name.set(info.name.clone());
                                                    unit.set(info.unit.clone());
                                                    error.set(None);
                                                    editing.set(Some(info.code.clone()));
                                                }
                                            },
                                            "✏️"
                                        }
                                    }
                                }

                                if let Some(msg) = row_error {
                                    div { style: "font-size:11px; color:#ef4444;", "{msg}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
