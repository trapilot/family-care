use dioxus::prelude::*;
use crate::i18n::use_i18n;
use rust_decimal::Decimal;
use std::str::FromStr;

use crate::app::AppState;
use crate::models::{Category, NewCategory};
use crate::repository::CategoryRepo;
use crate::views::AmountInput;
use crate::views::icons::{Icon, IconName};
use crate::views::theme;
use crate::views::ui::{IconCircle, IconPicker, ScreenHeader};

#[derive(Clone, PartialEq)]
enum CatMode { List, Add, Edit(Category) }

#[component]
pub fn CategoriesSettings() -> Element {
    let state = use_context::<AppState>();
    let mode  = use_signal(|| CatMode::List);
    let cats  = use_signal(|| vec![]);

    let load = {
        let state = state.clone();
        let mut cats = cats.clone();
        move || {
            state.with_conn(|conn| {
                if let Ok(list) = CategoryRepo::list(conn) { cats.set(list); }
            });
        }
    };

    use_effect({
        let mut load = load.clone();
        move || {
            load();
        }
    });

    let current_mode = mode.read().clone();
    match current_mode {
        CatMode::List => rsx! { CatList { cats: cats.read().clone(), mode, on_reload: load } },
        CatMode::Add  => rsx! { CatForm { mode, editing: None, on_reload: load } },
        CatMode::Edit(cat) => rsx! { CatForm { mode, editing: Some(cat), on_reload: load } },
    }
}

// ─── List ─────────────────────────────────────────────────────────────────────

#[component]
fn CatList(
    cats: Vec<Category>,
    mode: Signal<CatMode>,
    on_reload: EventHandler<()>,
) -> Element {
    let t = use_i18n();
    let state   = use_context::<AppState>();
    let mut confirm = use_signal(|| None::<String>);

    let user_cats:   Vec<&Category> = cats.iter().filter(|c| !c.is_system).collect();
    let system_cats: Vec<&Category> = cats.iter().filter(|c|  c.is_system).collect();

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:14px;",

            // ── Header ──
            div {
                style: "display:flex; justify-content:space-between; align-items:center;",
                h2 { style: "{theme::SECTION_TITLE}", {t.t("Categories")} }
                button {
                    style: "background:{theme::PRIMARY}; color:#fff; border:none; border-radius:999px; padding:8px 16px; font-size:14px; font-weight:600; cursor:pointer; display:flex; align-items:center; gap:6px;",
                    onclick: move |_| { mode.set(CatMode::Add); },
                    {t.t("+ Add")}
                }
            }

            // ── User categories ──
            if !user_cats.is_empty() {
                div {
                    style: "background:#fff; border-radius:0; overflow:hidden;",
                    SectionLabel { text: t.t("Custom") }
                    for cat in user_cats.iter() {
                        CatRow {
                            cat: (*cat).clone(),
                            on_edit: {
                                let c = (*cat).clone();
                                move |_| { mode.set(CatMode::Edit(c.clone())); }
                            },
                            on_delete: {
                                let id = cat.id.clone();
                                move |_| { confirm.set(Some(id.clone())); }
                            }
                        }
                    }
                }
            }

            // ── System categories (read-only) ──
            if !system_cats.is_empty() {
                div {
                    style: "background:#fff; border-radius:0; overflow:hidden;",
                    SectionLabel { text: t.t("System (read-only)") }
                    for cat in system_cats.iter() {
                        CatRow {
                            cat: (*cat).clone(),
                            on_edit: move |_| {},
                            on_delete: move |_| {}
                        }
                    }
                }
            }

            // ── Delete confirm ──
            if let Some(cat_id) = confirm.read().clone() {
                div {
                    style: "{theme::OVERLAY}",
                    div {
                        style: "{theme::DIALOG}",
                        h3 { style: "margin:0 0 8px; font-size:16px; font-weight:600; color:{theme::TEXT};", {t.t("Delete category?")} }
                        p  { style: "margin:0 0 20px; font-size:14px; color:{theme::TEXT_MUTED};", {t.t("Categories in use by transactions cannot be deleted.")} }
                        div { style: "display:flex; gap:10px;",
                            button {
                                style: "{theme::BTN_SECONDARY} flex:1;",
                                onclick: move |_| { confirm.set(None); },
                                {t.t("Cancel")}
                            }
                            button {
                                style: "{theme::BTN_DANGER} flex:1;",
                                onclick: {
                                    let state = state.clone();
                                    let cat_id = cat_id.clone();
                                    let on_reload = on_reload.clone();
                                    move |_| {
                                        state.with_conn(|conn| {
                                            let _ = CategoryRepo::delete(conn, &cat_id);
                                        });
                                        confirm.set(None);
                                        on_reload.call(());
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

// ─── Form ─────────────────────────────────────────────────────────────────────

#[component]
fn CatForm(
    mode: Signal<CatMode>,
    editing: Option<Category>,
    on_reload: EventHandler<()>,
) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();

    let mut name          = use_signal(|| editing.as_ref().map(|c| c.name.clone()).unwrap_or_default());
    let mut icon          = use_signal(|| editing.as_ref().and_then(|c| c.icon.clone()).unwrap_or_default());
    let mut color         = use_signal(|| editing.as_ref().and_then(|c| c.color.clone()).unwrap_or_else(|| "#3f51b5".into()));
    let mut budget_str    = use_signal(|| editing.as_ref().and_then(|c| c.budget_amount.map(|b| format!("{b}"))).unwrap_or_default());
    let error_msg     = use_signal(|| String::new());

    let title = if editing.is_some() { t.t("Edit Category") } else { t.t("New Category") };

    let preset_icons = ["🍜","🚗","🛍️","💊","🎮","📚","🏠","💆","✈️","📦","☕","🎵","🐶","💡","🏋️"];

    let submit = {
        let state = state.clone();
        let name = name.clone();
        let icon = icon.clone();
        let color = color.clone();
        let budget_str = budget_str.clone();
        let mut error_msg = error_msg.clone();
        let editing = editing.clone();
        let on_reload = on_reload.clone();

        move |_: MouseEvent| {
            let name_val = name.read().trim().to_string();
            if name_val.is_empty() { error_msg.set(t.t("Name required").into()); return; }

            let budget = if budget_str.read().is_empty() {
                None
            } else {
                match Decimal::from_str(&budget_str.read()) {
                    Ok(b) => Some(b),
                    Err(_) => { error_msg.set(t.t("Invalid budget amount").into()); return; }
                }
            };

            let icon_opt  = { let i = icon.read();  if i.is_empty() { None } else { Some(i.clone()) } };
            let color_opt = { let c = color.read(); if c.is_empty() { None } else { Some(c.clone()) } };

            let result = state.with_conn(|conn| {
                if let Some(ref old) = editing {
                    let mut updated = old.clone();
                    updated.name          = name_val.clone();
                    updated.icon          = icon_opt;
                    updated.color         = color_opt;
                    updated.budget_amount = budget;
                    CategoryRepo::update(conn, &updated)
                } else {
                    CategoryRepo::create(conn, &NewCategory {
                        name: name_val,
                        icon: icon_opt,
                        color: color_opt,
                        budget_amount: budget,
                        parent_id: None,
                        sort_order: 0,
                    }).map(|_| ())
                }
            });

            match result {
                Ok(_)  => { mode.set(CatMode::List); on_reload.call(()); }
                Err(e) => { error_msg.set(e.to_string()); }
            }
        }
    };

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:14px;",

            div { style: "margin:-16px -16px 0;",
                ScreenHeader {
                    title: title.to_string(),
                    on_back: move |_| { mode.set(CatMode::List); },
                }
            }

            FieldLabel { label: t.t("Name") }
            input {
                style: "{theme::INPUT}",
                r#type: "text",
                placeholder: t.t("e.g. Food & Drinks"),
                value: "{name.read()}",
                oninput: move |e| { name.set(e.value()); },
            }

            FieldLabel { label: t.t("Icon") }
            IconPicker {
                value: icon.read().clone(),
                options: preset_icons.to_vec(),
                color: color.read().clone(),
                on_select: move |e: String| { icon.set(e); },
            }

            FieldLabel { label: t.t("Color") }
            div {
                style: "display:flex; flex-wrap:wrap; gap:8px;",
                for hex in ["#3f51b5","#17a589","#e5527f","#f59e0b","#8b5cf6","#ec4899","#06b6d4","#84cc16","#f97316","#8a8d9f"] {
                    {
                        let h = hex.to_string();
                        let is_active = *color.read() == hex;
                        let style = format!(
                            "width:32px;height:32px;border-radius:50%;background:{};border:{};cursor:pointer;",
                            hex,
                            if is_active {
                                "3px solid #1f2133"
                            } else {
                                "2px solid transparent"
                            }
                        );
                        rsx! {
                            button {
                                style: "{style}",
                                onclick: move |_| { color.set(h.clone()); },
                            }
                        }
                    }
                }
            }

            FieldLabel { label: t.t("Monthly Budget (optional)") }
            AmountInput {
                value: budget_str.read().clone(),
                placeholder: t.t("Leave empty for unlimited"),
                oninput: move |v| budget_str.set(v),
            }

            if !error_msg.read().is_empty() {
                div {
                    style: "background:{theme::NEGATIVE_SOFT}; border-radius:0; padding:10px 12px; font-size:13px; color:{theme::DANGER};",
                    "{error_msg.read()}"
                }
            }

            button {
                style: "{theme::BTN_PRIMARY}",
                onclick: submit,
                if editing.is_some() { {t.t("Update")} } else { {t.t("Create Category")} }
            }
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

#[component]
fn CatRow(
    cat: Category,
    on_edit:   EventHandler<MouseEvent>,
    on_delete: EventHandler<MouseEvent>,
) -> Element {
    let t = use_i18n();
    let icon  = cat.icon.as_deref().unwrap_or("📦");
    let color = cat.color.as_deref().unwrap_or("#8a8d9f");
    let budget_label = cat.budget_amount
        .map(|b| format!("{}: {b:.0} ₫", t.t("Budget")))
        .unwrap_or_else(|| t.t("No budget").into());

    rsx! {
        div {
            style: "display:flex; align-items:center; gap:12px; padding:12px 16px; {theme::ROW_DIVIDER}",
            IconCircle {
                icon: Some(icon.to_string()),
                fallback: IconName::Tag,
                color: color.to_string(),
                size: 40,
            }
            div { style: "flex:1; min-width:0;",
                div { style: "font-size:15px; font-weight:600; color:{theme::TEXT};", {t.tr(&cat.name)} }
                div { style: "font-size:13px; color:{theme::TEXT_MUTED};", "{budget_label}" }
            }
            if !cat.is_system {
                button {
                    style: "border:none; background:transparent; cursor:pointer; padding:6px; display:flex;",
                    onclick: move |e| { on_edit.call(e); },
                    Icon { name: IconName::Pencil, size: 20, color: theme::TEXT_MUTED.to_string() }
                }
                button {
                    style: "border:none; background:transparent; cursor:pointer; padding:6px; display:flex;",
                    onclick: move |e| { on_delete.call(e); },
                    Icon { name: IconName::Trash, size: 20, color: theme::DANGER.to_string() }
                }
            }
        }
    }
}

#[component]
fn SectionLabel(text: &'static str) -> Element {
    rsx! {
        div {
            style: "padding:10px 16px 6px; font-size:14px; font-weight:600; color:{theme::TEXT};",
            "{text}"
        }
    }
}

#[component]
fn FieldLabel(label: &'static str) -> Element {
    rsx! {
        label {
            style: "{theme::LABEL}",
            "{label}"
        }
    }
}
