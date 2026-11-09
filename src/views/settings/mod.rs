pub mod backup;
pub mod categories;
pub mod currencies;
pub mod defaults;
pub mod reminders;

use dioxus::prelude::*;
use crate::i18n::use_i18n;
use categories::CategoriesSettings;
use currencies::CurrenciesSettings;
use defaults::DefaultsSettings;
use backup::BackupSettings;
use reminders::RemindersSettings;
use crate::app::AppState;
use crate::i18n::Lang;
use crate::repository::SettingsRepo;
use crate::views::icons::{Icon, IconName};
use crate::views::theme;
use crate::views::ui::{IconText, ScreenHeader, UnderlineTabs};

#[derive(Clone, PartialEq)]
enum SettingsTab { Defaults, Categories, Currencies, Reminders, Backup }

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
                    (t.t("Defaults").to_string(), None),
                    (t.t("Categories").to_string(), None),
                    (t.t("Money types").to_string(), None),
                    (t.t("Reminders").to_string(), None),
                    (t.t("Backup").to_string(), None),
                ],
                active: match *tab.read() {
                    SettingsTab::Defaults => 0,
                    SettingsTab::Categories => 1,
                    SettingsTab::Currencies => 2,
                    SettingsTab::Reminders => 3,
                    SettingsTab::Backup => 4,
                },
                on_select: move |i: usize| {
                    tab.set(match i {
                        0 => SettingsTab::Defaults,
                        1 => SettingsTab::Categories,
                        2 => SettingsTab::Currencies,
                        3 => SettingsTab::Reminders,
                        _ => SettingsTab::Backup,
                    });
                },
            }

            div {
                style: "flex:1; overflow-y:auto; padding:16px;",
                match tab.read().clone() {
                    SettingsTab::Defaults   => rsx! { DefaultsSettings {} },
                    SettingsTab::Categories => rsx! { CategoriesSettings {} },
                    SettingsTab::Currencies => rsx! { CurrenciesSettings {} },
                    SettingsTab::Reminders  => rsx! { RemindersSettings {} },
                    SettingsTab::Backup     => rsx! { BackupSettings {} },
                }
                FeedbackCard {}
                DonateCard {}
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

// ─── DonateCard ───────────────────────────────────────────────────────────────

/// Owner's VietQR image, embedded at compile time so it works offline.
const DONATE_QR: &[u8] = include_bytes!("../../../assets/donate_qr.jpg");
const DONATE_BANK: &str = "Techcombank";
const DONATE_ACCOUNT_NAME: &str = "DINH TRONG NAM";
const DONATE_ACCOUNT_NUMBER: &str = "19030152672011";
const DONATE_ACCOUNT_DISPLAY: &str = "1903 0152 6720 11";

/// "Support the app" card: collapsible VietQR plus copyable bank details.
#[component]
fn DonateCard() -> Element {
    let t = use_i18n();
    let mut open = use_signal(|| false);
    let mut copied = use_signal(|| false);
    let qr_url = use_hook(|| crate::models::id_card::jpeg_data_url(DONATE_QR));

    rsx! {
        div {
            style: "{theme::CARD} margin-top:16px; display:flex; flex-direction:column; gap:10px; border-left:3px solid #f5b301;",
            div {
                style: "display:flex; align-items:center; gap:8px; font-weight:700; font-size:16px; color:{theme::TEXT};",
                Icon { name: IconName::Heart, size: 20, color: "#e5527f".to_string() }
                "{t.t(\"Support TaGi\")}"
            }
            div {
                style: "font-size:14px; color:{theme::TEXT_MUTED};",
                "{t.t(\"TaGi is free and has no ads. If it helps your family, you can buy the author a coffee — scan the QR with any banking app.\")}"
            }
            button {
                style: "{theme::BTN_SECONDARY} width:100%;",
                onclick: move |_| {
                    let v = open();
                    open.set(!v);
                    copied.set(false);
                },
                if open() {
                    "{t.t(\"Hide QR\")}"
                } else {
                    IconText { text: t.t("☕ Show QR to donate").to_string() }
                }
            }
            if open() {
                div {
                    style: "background:#fff; padding:12px; border:1px solid {theme::BORDER};",
                    img {
                        src: "{qr_url}",
                        alt: "VietQR",
                        style: "display:block; width:100%; max-width:240px; margin:4px auto 0; background:#fff;",
                    }
                }
                div {
                    style: "display:flex; flex-direction:column; gap:6px; font-size:13px;",
                    div {
                        style: "display:flex; justify-content:space-between; gap:8px;",
                        span { style: "color:{theme::TEXT_MUTED};", {t.t("Bank")} }
                        span { style: "color:{theme::TEXT};", "{DONATE_BANK}" }
                    }
                    div {
                        style: "display:flex; justify-content:space-between; gap:8px;",
                        span { style: "color:{theme::TEXT_MUTED};", {t.t("Account name")} }
                        span { style: "color:{theme::TEXT};", "{DONATE_ACCOUNT_NAME}" }
                    }
                    div {
                        style: "display:flex; justify-content:space-between; gap:8px;",
                        span { style: "color:{theme::TEXT_MUTED};", {t.t("Account number")} }
                        span { style: "color:{theme::TEXT}; font-weight:600; user-select:text;", "{DONATE_ACCOUNT_DISPLAY}" }
                    }
                    button {
                        style: "border:none; background:none; padding:4px 0; align-self:flex-end; color:{theme::PRIMARY}; font-size:13px; font-weight:600; cursor:pointer;",
                        onclick: move |_| {
                            spawn(async move {
                                if crate::views::clipboard::copy_to_clipboard(DONATE_ACCOUNT_NUMBER).await {
                                    copied.set(true);
                                }
                            });
                        },
                        if copied() { "{t.t(\"Copied\")}" } else { "{t.t(\"Copy account number\")}" }
                    }
                }
                div {
                    style: "font-size:12px; color:{theme::TEXT_MUTED};",
                    "{t.t(\"VietQR · Napas 247 — works with every Vietnamese banking app\")}"
                }
            }
        }
    }
}
