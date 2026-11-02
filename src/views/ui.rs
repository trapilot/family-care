use dioxus::prelude::*;

use crate::i18n::use_i18n;
use crate::models::{Transaction, TransactionType};
use crate::views::icons::{emoji_icon, icon_for_emoji, Icon, IconName};
use crate::views::{fmt_currency, month_label, theme};

/// `#RRGGBB` from a DB color that may carry an alpha suffix (`#RRGGBBAA`)
fn hex6(color: &str) -> String {
    color.chars().take(7).collect()
}

/// Shared body of `IconCircle` / `IconTile`
fn icon_box(icon: Option<String>, fallback: IconName, color: &str, size: u32, filled: bool, radius: &str) -> Element {
    let color = hex6(color);
    let (bg, fg) = if filled { (color.clone(), "#ffffff".to_string()) } else { (format!("{color}26"), color) };
    let inner = (size as f64 * 0.5).round() as u32;
    let emoji = icon.as_deref().filter(|e| !e.is_empty() && icon_for_emoji(e).is_none()).map(str::to_string);
    let name = icon.as_deref().and_then(icon_for_emoji).unwrap_or(fallback);
    rsx! {
        span {
            style: "display:inline-flex; align-items:center; justify-content:center; flex-shrink:0; width:{size}px; height:{size}px; border-radius:{radius}; background:{bg}; color:{fg};",
            if let Some(emoji) = emoji {
                span { style: "font-size:{inner}px; line-height:1;", "{emoji}" }
            } else {
                Icon { name, size: inner, color: fg.clone() }
            }
        }
    }
}

/// Round pastel icon badge
#[component]
pub fn IconCircle(
    icon: Option<String>,
    fallback: IconName,
    color: String,
    #[props(default = 48)] size: u32,
    #[props(default = false)] filled: bool,
) -> Element {
    icon_box(icon, fallback, &color, size, filled, "50%")
}

/// A label that may start with an emoji: the emoji is drawn as a line icon, the rest as text
#[component]
pub fn IconText(
    text: String,
    #[props(default = 16)] size: u32,
    #[props(default)] color: Option<String>,
    #[props(default = 6)] gap: u32,
) -> Element {
    match emoji_icon(&text) {
        Some((name, rest)) => {
            let rest = rest.to_string();
            let color = color.unwrap_or_else(|| "currentColor".to_string());
            rsx! {
                span {
                    style: "display:inline-flex; align-items:center; gap:{gap}px;",
                    Icon { name, size, color }
                    "{rest}"
                }
            }
        }
        None => rsx! { "{text}" },
    }
}

/// Grid of selectable line icons; options are emoji strings shown through `icon_for_emoji`
#[component]
pub fn IconPicker(
    value: String,
    options: Vec<&'static str>,
    color: String,
    on_select: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            style: "display:grid; grid-template-columns:repeat(auto-fill, minmax(48px, 1fr)); gap:8px;",
            for opt in options {
                {
                    let selected = icon_for_emoji(&value) == icon_for_emoji(opt) && icon_for_emoji(opt).is_some() || value == opt;
                    let shadow = if selected { "box-shadow:0 0 0 2px #3f51b5;" } else { "" };
                    rsx! {
                        button {
                            key: "{opt}",
                            style: "border:none; background:transparent; padding:2px; cursor:pointer; display:flex; align-items:center; justify-content:center; border-radius:0;",
                            onclick: move |_| on_select.call(opt.to_string()),
                            span {
                                style: "display:inline-flex; border-radius:50%; {shadow}",
                                IconCircle { icon: Some(opt.to_string()), fallback: IconName::Package, color: color.clone(), size: 44 }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Initial of a person's given name (last word of a Vietnamese full name), uppercased
fn initial(name: &str) -> Option<String> {
    let word = name.split_whitespace().last()?;
    word.chars().next().map(|c| c.to_uppercase().collect())
}

/// Pastel circle with the initial of a person's given name
#[component]
pub fn InitialsAvatar(
    name: String,
    /// Stable seed for the color, e.g. the member id
    seed: String,
    #[props(default = 44)] size: u32,
) -> Element {
    const PALETTE: [&str; 8] =
        ["#3f51b5", "#e5527f", "#17a589", "#f59e0b", "#8b5cf6", "#0ea5e9", "#ef6c00", "#6d4c41"];
    let idx = seed.bytes().map(|b| b as usize).sum::<usize>() % PALETTE.len();
    let color = PALETTE[idx];
    let font = (size as f64 * 0.42).round() as u32;
    let inner = (size as f64 * 0.5).round() as u32;
    rsx! {
        span {
            style: "display:inline-flex; align-items:center; justify-content:center; flex-shrink:0; width:{size}px; height:{size}px; border-radius:50%; background:{color}26; color:{color}; font-size:{font}px; font-weight:600; line-height:1;",
            if let Some(letter) = initial(&name) {
                "{letter}"
            } else {
                Icon { name: IconName::User, size: inner, color: color.to_string() }
            }
        }
    }
}

/// Rounded-square icon badge (wallet rows)
#[component]
pub fn IconTile(
    icon: Option<String>,
    fallback: IconName,
    color: String,
    #[props(default = 48)] size: u32,
    #[props(default = true)] filled: bool,
) -> Element {
    icon_box(icon, fallback, &color, size, filled, "12px")
}

#[component]
pub fn MonthSwitcher(year: i32, month: u32, on_change: EventHandler<(i32, u32)>) -> Element {
    let prev = if month == 1 { (year - 1, 12) } else { (year, month - 1) };
    let next = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    rsx! {
        div {
            style: "display:flex; align-items:center; justify-content:center; gap:12px;",
            button {
                style: "border:none; background:transparent; cursor:pointer; padding:0; width:32px; height:32px; display:flex; align-items:center; justify-content:center;",
                onclick: move |_| on_change.call(prev),
                Icon { name: IconName::ChevronsLeft, size: 28, color: theme::MONTH_FG.to_string() }
            }
            span {
                style: "display:inline-flex; align-items:center; gap:8px; background:{theme::MONTH_BG}; color:{theme::MONTH_FG}; border-radius:999px; padding:8px 20px; font-weight:700; font-size:15px; text-transform:uppercase;",
                Icon { name: IconName::Calendar, size: 18 }
                "{month_label(year, month)}"
            }
            button {
                style: "border:none; background:transparent; cursor:pointer; padding:0; width:32px; height:32px; display:flex; align-items:center; justify-content:center;",
                onclick: move |_| on_change.call(next),
                Icon { name: IconName::ChevronsRight, size: 28, color: theme::TEXT.to_string() }
            }
        }
    }
}

/// Top band: month switcher (or the screen title) centered, with a right-hand action
#[component]
pub fn AppHeader(
    title: String,
    #[props(default)] month: Option<(i32, u32)>,
    #[props(default)] on_month: Option<EventHandler<(i32, u32)>>,
    #[props(default)] action: Option<Element>,
) -> Element {
    rsx! {
        div {
            style: "background:{theme::HEADER_BG}; padding:10px 8px; min-height:56px; box-sizing:border-box; color:{theme::TEXT}; display:flex; align-items:center;",
            div { style: "width:40px; flex-shrink:0;" }
            div {
                style: "flex:1; min-width:0; display:flex; justify-content:center; text-align:center;",
                if let (Some((year, month)), Some(on_month)) = (month, on_month) {
                    MonthSwitcher { year, month, on_change: move |ym| on_month.call(ym) }
                } else {
                    div { style: "font-size:17px; font-weight:600; color:{theme::TEXT}; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{title}" }
                }
            }
            div { style: "width:40px; flex-shrink:0; display:flex; justify-content:flex-end;",
                if let Some(action) = action { {action} }
            }
        }
    }
}

/// Round 40px header button with a line icon
#[component]
pub fn HeaderAction(icon: IconName, label: String, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            style: "width:40px; height:40px; border:none; background:transparent; border-radius:0; cursor:pointer; display:flex; align-items:center; justify-content:center; color:{theme::TEXT}; padding:0;",
            aria_label: "{label}",
            onclick: move |e| onclick.call(e),
            Icon { name: icon, size: 26, color: theme::TEXT.to_string() }
        }
    }
}

#[component]
pub fn SectionTitle(
    title: String,
    #[props(default)] trailing: Option<String>,
    #[props(default = theme::POSITIVE)] trailing_color: &'static str,
) -> Element {
    rsx! {
        div {
            style: "display:flex; justify-content:space-between; align-items:baseline; gap:8px;",
            h2 { style: "{theme::SECTION_TITLE}", "{title}" }
            if let Some(trailing) = trailing {
                span { style: "font-size:16px; font-weight:600; color:{trailing_color};", "{trailing}" }
            }
        }
    }
}

/// Floating "+" button, sticky at the bottom-right of the scrolling content
#[component]
pub fn Fab(onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            style: "position:sticky; bottom:16px; margin-left:auto; width:56px; height:56px; border:none; border-radius:0; background:{theme::PRIMARY_SOFT}; box-shadow:0 4px 12px rgba(63,81,181,.25); cursor:pointer; display:flex; align-items:center; justify-content:center; padding:0; flex-shrink:0;",
            aria_label: "Add",
            onclick: move |e| onclick.call(e),
            Icon { name: IconName::Plus, size: 28, color: theme::PRIMARY.to_string() }
        }
    }
}

/// Icon and color of a transaction type (used when it has no category)
pub fn txn_type_look(txn_type: &TransactionType) -> (IconName, &'static str) {
    match txn_type {
        TransactionType::Income         => (IconName::ArrowUp, theme::POSITIVE),
        TransactionType::Expense        => (IconName::ArrowDown, theme::NEGATIVE),
        TransactionType::Transfer       => (IconName::ArrowLeftRight, theme::PRIMARY),
        TransactionType::InvestBuy      => (IconName::TrendingUp, "#f59e0b"),
        TransactionType::InvestSell     => (IconName::TrendingDown, "#8b5cf6"),
        TransactionType::OpeningBalance => (IconName::Flag, "#0ea5e9"),
    }
}

/// One transaction row: icon, title, note, amount. Edit / delete buttons show only when handlers are given.
#[component]
pub fn TxnItem(
    txn: Transaction,
    /// (emoji, translated name, color) of the category
    #[props(default)] category: Option<(String, String, String)>,
    /// Wallet name, appended to the title of opening balances
    #[props(default)] wallet_name: Option<String>,
    #[props(default = 0)] photos: i64,
    #[props(default)] on_edit: Option<EventHandler<MouseEvent>>,
    #[props(default)] on_delete: Option<EventHandler<MouseEvent>>,
) -> Element {
    let t = use_i18n();
    let (type_icon, type_color) = txn_type_look(&txn.txn_type);
    let type_label = t.code(&txn.txn_type.to_string());
    let (title, icon, color) = match category {
        Some((emoji, name, color)) => (name, Some(emoji), color),
        None => {
            let title = match wallet_name {
                Some(w) => format!("{type_label} · {w}"),
                None    => type_label,
            };
            (title, None, type_color.to_string())
        }
    };
    let amount_color = match txn.txn_type {
        TransactionType::Income | TransactionType::InvestSell => theme::POSITIVE,
        TransactionType::Expense => theme::NEGATIVE,
        _ => theme::TEXT,
    };
    let sign = match txn.txn_type {
        TransactionType::Income  => "+",
        TransactionType::Expense => "−",
        _ => "",
    };
    let amount = format!("{sign}{}", fmt_currency(txn.amount, &txn.currency));
    let note = txn.note.clone().unwrap_or_default();
    let has_actions = on_edit.is_some() || on_delete.is_some();

    rsx! {
        div {
            style: "display:flex; align-items:center; gap:12px; padding:10px 0;",
            IconCircle { icon, fallback: type_icon, color, size: 44 }
            div { style: "flex:1; min-width:0;",
                div { style: "font-size:15px; color:{theme::TEXT}; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{title}" }
                if !note.is_empty() {
                    div { style: "font-size:13px; color:{theme::TEXT_MUTED}; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{note}" }
                }
                if photos > 0 {
                    div { style: "display:flex; align-items:center; gap:4px; font-size:12px; color:{theme::TEXT_MUTED};",
                        Icon { name: IconName::Camera, size: 12 }
                        "{photos}"
                    }
                }
            }
            div { style: "display:flex; flex-direction:column; align-items:flex-end; flex-shrink:0;",
                div { style: "font-size:15px; font-weight:600; color:{amount_color}; white-space:nowrap;", "{amount}" }
                if has_actions {
                    div { style: "display:flex;",
                        if let Some(on_edit) = on_edit {
                            button {
                                style: "width:32px; height:32px; border:none; background:transparent; cursor:pointer; padding:0; display:flex; align-items:center; justify-content:center;",
                                onclick: move |e| on_edit.call(e),
                                Icon { name: IconName::Pencil, size: 18, color: theme::TEXT_MUTED.to_string() }
                            }
                        }
                        if let Some(on_delete) = on_delete {
                            button {
                                style: "width:32px; height:32px; border:none; background:transparent; cursor:pointer; padding:0; display:flex; align-items:center; justify-content:center;",
                                onclick: move |e| on_delete.call(e),
                                Icon { name: IconName::Trash, size: 18, color: theme::TEXT_MUTED.to_string() }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Top band of a screen or sub-screen: optional back arrow, title and a right-hand action
#[component]
pub fn ScreenHeader(
    title: String,
    #[props(default)] on_back: Option<EventHandler<MouseEvent>>,
    #[props(default)] action: Option<Element>,
) -> Element {
    let t = use_i18n();
    rsx! {
        div {
            style: "background:{theme::HEADER_BG}; padding:12px 8px; min-height:56px; box-sizing:border-box; display:flex; align-items:center; gap:4px; color:{theme::TEXT};",
            if let Some(on_back) = on_back {
                button {
                    style: "width:40px; height:40px; border:none; background:transparent; border-radius:50%; cursor:pointer; display:flex; align-items:center; justify-content:center; padding:0; flex-shrink:0;",
                    aria_label: t.t("Back"),
                    onclick: move |e| on_back.call(e),
                    Icon { name: IconName::ArrowLeft, size: 24, color: theme::TEXT.to_string() }
                }
            } else {
                div { style: "width:8px; flex-shrink:0;" }
            }
            div {
                style: "flex:1; min-width:0; font-size:17px; font-weight:600; color:{theme::TEXT}; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                "{title}"
            }
            if let Some(action) = action { {action} }
        }
    }
}

/// Horizontally scrollable tab row with an underline on the active tab
#[component]
pub fn UnderlineTabs(
    tabs: Vec<(String, Option<IconName>)>,
    active: usize,
    on_select: EventHandler<usize>,
) -> Element {
    rsx! {
        div {
            style: "display:flex; background:{theme::HEADER_BG}; padding:0 8px; overflow-x:auto;",
            for (i, (label, icon)) in tabs.into_iter().enumerate() {
                button {
                    key: "{i}",
                    style: if i == active {
                        "flex:1 0 auto; display:flex; align-items:center; justify-content:center; gap:6px; padding:10px 12px 8px; border:none; border-bottom:3px solid #3f51b5; border-radius:0; background:transparent; font-size:14px; font-weight:600; color:#3f51b5; cursor:pointer; white-space:nowrap;"
                    } else {
                        "flex:1 0 auto; display:flex; align-items:center; justify-content:center; gap:6px; padding:10px 12px 8px; border:none; border-bottom:3px solid transparent; border-radius:0; background:transparent; font-size:14px; color:#1f2133; cursor:pointer; white-space:nowrap;"
                    },
                    onclick: move |_| on_select.call(i),
                    if let Some(icon) = icon { Icon { name: icon, size: 18 } }
                    "{label}"
                }
            }
        }
    }
}

/// Form field: small muted label above its control
#[component]
pub fn Field(label: String, children: Element) -> Element {
    rsx! {
        div { style: "display:flex; flex-direction:column;",
            div { style: "{theme::LABEL}", "{label}" }
            {children}
        }
    }
}

/// Modal: dimmed overlay with a centered white card
#[component]
pub fn Dialog(children: Element) -> Element {
    rsx! {
        div { style: "{theme::OVERLAY}",
            div { style: "{theme::DIALOG}", {children} }
        }
    }
}

#[cfg(test)]
mod initials_tests {
    use super::initial;

    #[test]
    fn initial_uses_last_word() {
        assert_eq!(initial("Nguyễn Văn Nam").as_deref(), Some("N"));
        assert_eq!(initial("Trần thị đào").as_deref(), Some("Đ"));
        assert_eq!(initial("  "), None);
        assert_eq!(initial("An").as_deref(), Some("A"));
    }
}
