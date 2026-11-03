use dioxus::prelude::*;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::i18n::{use_i18n, I18n, Lang};
use crate::views::{
    icons::{Icon, IconName},
    theme,
    dashboard::Dashboard,
    reports::Reports,
    members::Members,
    settings::Settings,
    statements::Statements,
    transactions::Transactions,
    wallets::Wallets,
    pin::{PinSetup, PinUnlock},
};

/// Bundled fonts + their @font-face rules; a folder asset keeps file names, so fonts.css can use relative URLs
const FONTS: Asset = asset!("/assets/fonts", AssetOptions::folder());

// ─── AppState ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
}

impl AppState {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    pub fn with_conn<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&Connection) -> T,
    {
        let conn = self.db.lock().expect("DB lock poisoned");
        f(&conn)
    }
}

/// Month shown by Overview, Transactions and Reports, shared between them
#[derive(Clone, Copy)]
pub struct SelectedMonth(pub Signal<(i32, u32)>);

// ─── Tabs ─────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug)]
pub enum Tab {
    Dashboard,
    Transactions,
    Wallets,
    Members,
    Reports,
    Statements,
    Settings,
}

// ─── Root App ─────────────────────────────────────────────────────────────────

#[component]
pub fn App() -> Element {
    let db = crate::db::get();
    let state = AppState::new(db);
    provide_context(state.clone());

    // UI language: loaded once from settings, switched from the Settings screen
    let lang = use_signal({
        let state = state.clone();
        move || {
            state.with_conn(|conn| crate::repository::SettingsRepo::get_language(conn).ok().flatten())
                .map(|code| Lang::from_code(&code))
                .unwrap_or(Lang::Vi)
        }
    });
    let t = use_context_provider(|| I18n::new(lang));

    let mut refresh_trigger = use_signal(|| 0);
    let state_reset = state.clone();

    let is_pin_set = use_resource(move || {
        let state = state.clone();
        let _ = refresh_trigger();
        async move {
            state.with_conn(|conn| {
                crate::repository::SettingsRepo::is_pin_set(conn).unwrap_or(false)
            })
        }
    });

    let mut is_unlocked = use_signal(|| false);

    let screen = match is_pin_set.cloned() {
        None => rsx! { div { style: "padding: 40px;", {t.t("Loading...")} } },
        Some(false) => {
            rsx! {
                PinSetup {
                    on_set: move |_| {
                        is_unlocked.set(true);
                        refresh_trigger += 1;
                    }
                }
            }
        },
        Some(true) => {
            if *is_unlocked.read() {
                rsx! { MainApp {} }
            } else {
                rsx! {
                    PinUnlock {
                        on_unlock: move |_| is_unlocked.set(true),
                        // Fresh database: keep the current language, then show PIN setup
                        on_reset: {
                            let state = state_reset.clone();
                            move |_| {
                                let _ = state.with_conn(|conn| crate::repository::SettingsRepo::set_language(conn, lang().code()));
                                refresh_trigger += 1;
                            }
                        },
                    }
                }
            }
        }
    };

    rsx! {
        document::Stylesheet { href: "{FONTS}/fonts.css" }
        {screen}
    }
}

#[component]
fn MainApp() -> Element {
    let active_tab = use_signal(|| Tab::Dashboard);
    let selected_month = use_signal(crate::views::current_month);
    use_context_provider(|| SelectedMonth(selected_month));
    crate::views::rates::use_rates_provider();

    // Once per launch: ask to show notifications and re-schedule the next 14 days
    let state = use_context::<AppState>();
    use_hook(move || {
        crate::notify::request_permission();
        let _ = state.with_conn(crate::services::ReminderService::resync);
    });

    rsx! {
        div {
            // Pinned to the screen edges: content scrolls, tab bar stays at the bottom
            style: "position:fixed; top:0; right:0; bottom:0; left:0; display:flex; flex-direction:column; padding-top:env(safe-area-inset-top); padding-left:env(safe-area-inset-left); padding-right:env(safe-area-inset-right); font-family:inherit; background:{theme::BG};",
            div {
                style: "position:relative; flex:1 1 auto; min-height:0; overflow-x:hidden; overflow-y:auto; overscroll-behavior:contain; -webkit-overflow-scrolling:touch;",
                match *active_tab.read() {
                    Tab::Dashboard    => rsx! { Dashboard {} },
                    Tab::Transactions => rsx! { Transactions {} },
                    Tab::Wallets      => rsx! { Wallets {} },
                    Tab::Members      => rsx! { Members {} },
                    Tab::Reports      => rsx! { Reports {} },
                    Tab::Statements   => rsx! { Statements {} },
                    Tab::Settings     => rsx! { Settings {} },
                }
            }
            BottomTabBar { active_tab }
        }
    }
}

// ─── BottomTabBar ─────────────────────────────────────────────────────────────

#[component]
fn BottomTabBar(active_tab: Signal<Tab>) -> Element {
    let t = use_i18n();
    let tabs: &[(Tab, IconName, &str)] = &[
        (Tab::Dashboard,    IconName::Home,     "Overview"),
        (Tab::Transactions, IconName::Receipt,  "Txns"),
        (Tab::Wallets,      IconName::Wallet,   "Wallets"),
        (Tab::Members,      IconName::Users,    "Family"),
        (Tab::Reports,      IconName::BarChart, "Reports"),
        (Tab::Statements,   IconName::FileText, "Statements"),
        (Tab::Settings,     IconName::Settings, "Settings"),
    ];

    rsx! {
        div {
            style: "display:flex; flex-shrink:0; background:{theme::HEADER_BG}; padding:6px 0 calc(8px + env(safe-area-inset-bottom));",
            for (tab, icon, label) in tabs.iter().cloned() {
                {
                    let is_active = *active_tab.read() == tab;
                    let tab_clone = tab.clone();
                    let pill_bg = if is_active { theme::PRIMARY_SOFT } else { "transparent" };
                    rsx! {
                        button {
                            // min-width:0 + overflow:hidden keep all 7 tabs inside the screen width
                            style: "flex:1 1 0; min-width:0; overflow:hidden; border:none; background:transparent; cursor:pointer; display:flex; flex-direction:column; align-items:center; gap:2px; padding:4px 0;",
                            onclick: move |_| { active_tab.set(tab_clone.clone()); },
                            span {
                                style: "width:44px; height:28px; border-radius:0; background:{pill_bg}; display:flex; align-items:center; justify-content:center;",
                                Icon { name: icon, size: 22, color: theme::TEXT.to_string() }
                            }
                            span {
                                style: if is_active { "max-width:100%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:10px; font-weight:700; color:#1f2133;" }
                                       else { "max-width:100%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:10px; font-weight:500; color:#8a8d9f;" },
                                {t.t(label)}
                            }
                        }
                    }
                }
            }
        }
    }
}
