pub mod backup;
pub mod categories;
pub mod currencies;
pub mod reminders;

use dioxus::prelude::*;
use crate::i18n::use_i18n;
use categories::CategoriesSettings;
use currencies::CurrenciesSettings;
use backup::BackupSettings;
use reminders::RemindersSettings;
use crate::app::AppState;
use crate::i18n::Lang;
use crate::repository::SettingsRepo;
use crate::views::icons::{Icon, IconName};
use crate::views::theme;
use crate::views::ui::{ScreenHeader, UnderlineTabs};

#[derive(Clone, PartialEq)]
enum SettingsTab { Categories, Currencies, Reminders, Backup }

#[component]
pub fn Settings() -> Element {
    let t = use_i18n();
    let mut tab = use_signal(|| SettingsTab::Categories);

    rsx! {
        div {
            style: "display:flex; flex-direction:column; height:100%;",

            ScreenHeader { title: t.t("Settings").to_string(), action: rsx! { LanguageSelect {} } }
            UnderlineTabs {
                tabs: vec![
                    (t.t("Categories").to_string(), None),
                    (t.t("Money types").to_string(), None),
                    (t.t("Reminders").to_string(), None),
                    (t.t("Backup").to_string(), None),
                ],
                active: match *tab.read() {
                    SettingsTab::Categories => 0,
                    SettingsTab::Currencies => 1,
                    SettingsTab::Reminders => 2,
                    SettingsTab::Backup => 3,
                },
                on_select: move |i: usize| {
                    tab.set(match i {
                        0 => SettingsTab::Categories,
                        1 => SettingsTab::Currencies,
                        2 => SettingsTab::Reminders,
                        _ => SettingsTab::Backup,
                    });
                },
            }

            div {
                style: "flex:1; overflow-y:auto; padding:16px;",
                match tab.read().clone() {
                    SettingsTab::Categories => rsx! { CategoriesSettings {} },
                    SettingsTab::Currencies => rsx! { CurrenciesSettings {} },
                    SettingsTab::Reminders  => rsx! { RemindersSettings {} },
                    SettingsTab::Backup     => rsx! { BackupSettings {} },
                }
                FeedbackCard {}
            }
        }
    }
}

// ─── LanguageSelect ───────────────────────────────────────────────────────────

/// Switches the UI language for the whole app and remembers it in the settings table.
#[component]
fn LanguageSelect() -> Element {
    let mut t = use_i18n();
    let state = use_context::<AppState>();
    let current = t.lang();

    rsx! {
        label {
            style: "display:flex; align-items:center; gap:6px; padding-right:8px; color:{theme::TEXT_MUTED};",
            Icon { name: IconName::Globe, size: 20, color: theme::TEXT_MUTED.to_string() }
            select {
                style: "{theme::INPUT} width:auto; padding:6px 10px; font-size:14px;",
                aria_label: t.t("Language"),
                value: current.code(),
                onchange: move |e| {
                    let lang = Lang::from_code(&e.value());
                    // Set first: notification amounts (e.g. the gold unit) follow the language
                    t.set_lang(lang);
                    let _ = state.with_conn(|conn| {
                        SettingsRepo::set_language(conn, lang.code())?;
                        // Notification text follows the language
                        crate::services::ReminderService::resync(conn)
                    });
                },
                for lang in Lang::ALL {
                    option { value: lang.code(), selected: lang == current, "{lang.native_name()}" }
                }
            }
        }
    }
}

// ─── FeedbackCard ─────────────────────────────────────────────────────────────

/// Suggestions & feedback: shows the contact email address.
#[component]
fn FeedbackCard() -> Element {
    let t = use_i18n();
    let email = crate::views::FEEDBACK_EMAIL;

    rsx! {
        div {
            style: "{theme::CARD} margin-top:24px; display:flex; flex-direction:column; gap:10px;",
            div {
                style: "display:flex; align-items:center; gap:8px; font-weight:700; font-size:16px; color:{theme::TEXT};",
                Icon { name: IconName::Mail, size: 20, color: theme::PRIMARY.to_string() }
                "{t.t(\"Suggestions & feedback\")}"
            }
            div {
                style: "font-size:14px; color:{theme::TEXT_MUTED};",
                "{t.t(\"Have an idea or found a bug? Email us — every suggestion is welcome.\")}"
            }
            div {
                style: "font-size:14px; color:{theme::TEXT};",
                span { style: "color:{theme::TEXT_MUTED};", {t.t("Contact:")} " " }
                span { style: "font-weight:600; user-select:text;", "{email}" }
            }
        }
    }
}
