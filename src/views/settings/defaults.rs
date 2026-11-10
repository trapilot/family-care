use dioxus::prelude::*;

use crate::app::AppState;
use crate::i18n::use_i18n;
use crate::models::Wallet;
use crate::repository::{SettingsRepo, WalletRepo};
use crate::views::theme;

/// Wallet pre-selected when adding a new transaction.
#[component]
pub fn DefaultsSettings() -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();

    let mut wallets    = use_signal(Vec::<Wallet>::new);
    let mut wallet_id   = use_signal(String::new);

    use_hook({
        let state = state.clone();
        move || {
            state.with_conn(|conn| {
                let ws = WalletRepo::list_active(conn).unwrap_or_default();
                // A stored id that is no longer offered shows as "First wallet" (nothing is written)
                if let Ok(Some(id)) = SettingsRepo::get_default_wallet_id(conn) {
                    if ws.iter().any(|w| w.id == id) { wallet_id.set(id); }
                }
                wallets.set(ws);
            });
        }
    });

    rsx! {
        div {
            style: "{theme::CARD} display:flex; flex-direction:column; gap:10px;",
            div { style: "font-weight:700; font-size:16px; color:{theme::TEXT};", {t.t("New transaction defaults")} }
            div {
                style: "font-size:13px; color:{theme::TEXT_MUTED};",
                {t.t("Filled in automatically when you add a transaction.")}
            }

            label { style: "{theme::LABEL}", {t.t("Default wallet")} }
            select {
                style: "{theme::INPUT} appearance:none;",
                value: "{wallet_id.read()}",
                onchange: {
                    let state = state.clone();
                    move |e: FormEvent| {
                        let v = e.value();
                        let _ = state.with_conn(|conn| {
                            SettingsRepo::set_default_wallet_id(conn, (!v.is_empty()).then_some(v.as_str()))
                        });
                        wallet_id.set(v);
                    }
                },
                option { value: "", selected: wallet_id.read().is_empty(), {t.t("First wallet")} }
                for w in wallets.read().iter() {
                    option { key: "{w.id}", value: "{w.id}", selected: *wallet_id.read() == w.id, "{w.name}" }
                }
            }
        }
    }
}
