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

#[derive(Clone, PartialEq)]
enum SettingsTab { Categories, Currencies, Reminders, Backup }

#[component]
pub fn Settings() -> Element {
    let t = use_i18n();
    let mut tab = use_signal(|| SettingsTab::Categories);

    rsx! {
        div {
            style: "display:flex; flex-direction:column; height:100%;",

            // ── Header ──
            div {
                style: "padding:16px 16px 0; background:#fff;",
                div {
                    style: "display:flex; align-items:center; justify-content:space-between; gap:12px; margin:0 0 12px;",
                    h1 { style: "margin:0; font-size:20px; font-weight:700; color:#1f2937;", {t.t("Settings")} }
                    LanguageSelect {}
                }
                div {
                    style: "display:flex; border-bottom:1px solid #f3f4f6;",
                    for (tab_kind, label) in [(SettingsTab::Categories, t.t("Categories")), (SettingsTab::Currencies, t.t("Money types")), (SettingsTab::Reminders, t.t("Reminders")), (SettingsTab::Backup, t.t("Backup"))] {
                        {
                            let is_active = *tab.read() == tab_kind;
                            let t_clone = tab_kind.clone();
                            rsx! {
                                button {
                                    style: if is_active {
                                        "flex:1; padding:10px 0; border:none; border-bottom:2px solid #6366f1; background:#fff; font-size:14px; font-weight:600; color:#6366f1; cursor:pointer;"
                                    } else {
                                        "flex:1; padding:10px 0; border:none; border-bottom:2px solid transparent; background:#fff; font-size:14px; color:#9ca3af; cursor:pointer;"
                                    },
                                    onclick: move |_| { tab.set(t_clone.clone()); },
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }

            div {
                style: "flex:1; overflow-y:auto; padding:16px;",
                match tab.read().clone() {
                    SettingsTab::Categories => rsx! { CategoriesSettings {} },
                    SettingsTab::Currencies => rsx! { CurrenciesSettings {} },
                    SettingsTab::Reminders  => rsx! { RemindersSettings {} },
                    SettingsTab::Backup     => rsx! { BackupSettings {} },
                }
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
            style: "display:flex; align-items:center; gap:6px; font-size:13px; color:#6b7280;",
            "🌐"
            select {
                style: "border:1px solid #e5e7eb; border-radius:8px; padding:6px 8px; font-size:14px; background:#fff; color:#1f2937;",
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
