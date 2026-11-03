use dioxus::prelude::*;
use crate::i18n::use_i18n;
use crate::app::AppState;
use crate::repository::SettingsRepo;
use crate::views::icons::{Icon, IconName};
use crate::views::theme;
use crate::views::ui::IconText;

pub mod verify;
pub use verify::PinVerifyModal;

#[component]
pub fn PinSetup(on_set: EventHandler) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let mut pin = use_signal(|| String::new());
    let mut confirm_pin = use_signal(|| String::new());
    let mut error = use_signal(|| None::<String>);

    let state_clone = state.clone();
    let mut on_submit = move || {
        let p = pin.read().clone();
        let cp = confirm_pin.read().clone();
        
        if p.len() < 4 {
            error.set(Some(t.t("PIN must be at least 4 digits!").into()));
            return;
        }
        
        if p != cp {
            error.set(Some(t.t("PINs do not match!").into()));
            return;
        }
        
        let result = state_clone.with_conn(|conn| SettingsRepo::set_pin(conn, &p));
        
        match result {
            Ok(_) => {
                on_set.call(());
            },
            Err(e) => {
                error.set(Some(format!("{}: {}", t.t("Failed to set PIN"), e)));
            }
        }
    };

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:20px; padding:calc(40px + env(safe-area-inset-top)) 20px calc(40px + env(safe-area-inset-bottom)); align-items:center; position:fixed; top:0; right:0; bottom:0; left:0; overflow-y:auto; justify-content:center; font-family:inherit; background:{theme::BG};",
            div { style: "width:72px; height:72px; border-radius:50%; background:{theme::PRIMARY_SOFT}; display:flex; align-items:center; justify-content:center;", Icon { name: IconName::Lock, size: 36, color: theme::PRIMARY.to_string() } }
            h1 { style: "margin:0; font-size:21px; font-weight:600; color:{theme::TEXT};", {t.t("Set your PIN")} },
            p { style: "margin:0; font-size:14px; color:{theme::TEXT_MUTED}; text-align:center;", {t.t("You will need this PIN to unlock the app and access sensitive features")} },
            
            if let Some(err) = error.read().clone() {
                div {
                    style: "width:100%; max-width:300px; box-sizing:border-box; background:{theme::NEGATIVE_SOFT}; border-radius:0; padding:12px 14px; color:{theme::DANGER}; font-weight:600; text-align:center; font-size:14px;",
                    "{err}"
                }
            }
            
            form {
                style: "display:flex; flex-direction:column; gap:16px; width:100%; max-width:300px;",
                onsubmit: move |e| {
                    e.prevent_default();
                    on_submit();
                },
                div { style: "display:flex; flex-direction:column; gap:8px;",
                    label { style: "{theme::LABEL} margin:0;", {t.t("Enter PIN")} },
                    input {
                        r#type: "password",
                        inputmode: "numeric",
                        enterkeyhint: "done",
                        autocomplete: "off",
                        maxlength: 6,
                        style: "{theme::INPUT} text-align:center; letter-spacing:4px;",
                        value: "{pin.read().clone()}",
                        oninput: move |e| {
                            let v = e.value().chars().filter(|c| c.is_ascii_digit()).collect();
                            pin.set(v);
                            error.set(None);
                        }
                    }
                }
                
                div { style: "display:flex; flex-direction:column; gap:8px;",
                    label { style: "{theme::LABEL} margin:0;", {t.t("Confirm PIN")} },
                    input {
                        r#type: "password",
                        inputmode: "numeric",
                        enterkeyhint: "done",
                        autocomplete: "off",
                        maxlength: 6,
                        style: "{theme::INPUT} text-align:center; letter-spacing:4px;",
                        value: "{confirm_pin.read().clone()}",
                        oninput: move |e| {
                            let v = e.value().chars().filter(|c| c.is_ascii_digit()).collect();
                            confirm_pin.set(v);
                            error.set(None);
                        }
                    }
                }
                
                button {
                    style: "{theme::BTN_PRIMARY} margin-top:16px; width:100%;",
                    r#type: "submit",
                    {t.t("Continue")}
                }
            }
        }
    }
}

#[component]
/// `on_reset`: the app was wiped from "Forgot PIN?" — show PIN setup again
pub fn PinUnlock(on_unlock: EventHandler, on_reset: EventHandler) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let mut pin = use_signal(|| String::new());
    let mut error = use_signal(|| None::<String>);

    let mut on_submit = move || {
        let p = pin.read().clone();
        
        let result = state.with_conn(|conn| SettingsRepo::verify_pin(conn, &p));
        
        match result {
            Ok(true) => {
                on_unlock.call(());
            },
            Ok(false) => {
                error.set(Some(t.t("Incorrect PIN!").into()));
                pin.set(String::new());
            },
            Err(e) => {
                error.set(Some(format!("{}: {}", t.t("Error"), e)));
            }
        }
    };

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:20px; padding:calc(40px + env(safe-area-inset-top)) 20px calc(40px + env(safe-area-inset-bottom)); align-items:center; position:fixed; top:0; right:0; bottom:0; left:0; overflow-y:auto; justify-content:center; font-family:inherit; background:{theme::BG};",
            div { style: "width:72px; height:72px; border-radius:50%; background:{theme::PRIMARY_SOFT}; display:flex; align-items:center; justify-content:center;", Icon { name: IconName::Lock, size: 36, color: theme::PRIMARY.to_string() } }
            h1 { style: "margin:0; font-size:21px; font-weight:600; color:{theme::TEXT};", {t.t("Enter your PIN")} },
            
            if let Some(err) = error.read().clone() {
                div {
                    style: "width:100%; max-width:300px; box-sizing:border-box; background:{theme::NEGATIVE_SOFT}; border-radius:0; padding:12px 14px; color:{theme::DANGER}; font-weight:600; text-align:center; font-size:14px;",
                    "{err}"
                }
            }
            
            form {
                style: "display:flex; flex-direction:column; gap:16px; width:100%; max-width:300px;",
                onsubmit: move |e| {
                    e.prevent_default();
                    on_submit();
                },
                div { style: "display:flex; flex-direction:column; gap:8px;",
                    input {
                        r#type: "password",
                        inputmode: "numeric",
                        enterkeyhint: "done",
                        autocomplete: "off",
                        maxlength: 6,
                        style: "{theme::INPUT} text-align:center; letter-spacing:4px;",
                        value: "{pin.read().clone()}",
                        oninput: move |e| {
                            let v = e.value().chars().filter(|c| c.is_ascii_digit()).collect();
                            pin.set(v);
                            error.set(None);
                        }
                    }
                }
                
                button {
                    style: "{theme::BTN_PRIMARY} margin-top:16px; width:100%;",
                    r#type: "submit",
                    {t.t("Unlock")}
                }
            }

            ForgotPin { on_reset }
        }
    }
}

/// Word to type before the reset button works
const RESET_WORD: &str = "RESET";

/// "Forgot PIN?" → wipe the database and start over (asks to type RESET first)
#[component]
fn ForgotPin(on_reset: EventHandler) -> Element {
    let t = use_i18n();
    let mut open  = use_signal(|| false);
    let mut typed = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let confirmed = typed.read().trim().eq_ignore_ascii_case(RESET_WORD);

    rsx! {
        button {
            style: "border:none; background:transparent; color:{theme::TEXT_MUTED}; font-size:14px; text-decoration:underline; cursor:pointer; padding:4px;",
            onclick: move |_| { typed.set(String::new()); error.set(None); open.set(true); },
            {t.t("Forgot PIN?")}
        }

        if open() {
            div {
                style: "{theme::OVERLAY}",
                div {
                    style: "{theme::DIALOG} display:flex; flex-direction:column; gap:12px;",
                    h3 { style: "margin:0; font-size:16px; font-weight:600; color:{theme::DANGER};", IconText { text: t.t("⚠️ Reset the app?").to_string(), size: 18 } }
                    p { style: "margin:0; font-size:14px; color:{theme::TEXT}; line-height:1.5;",
                        {t.t("This permanently deletes ALL data on this device: wallets, transactions, members, ID photos, reminders and bills. You will then create a new PIN.")}
                    }
                    p { style: "margin:0; font-size:13px; color:{theme::TEXT_MUTED}; line-height:1.5;",
                        {t.t("Backup files are kept — after setting the new PIN you can import one from Settings → Backup.")}
                    }
                    label { style: "{theme::LABEL} margin:0;",
                        {t.t("Type {word} to confirm").replace("{word}", RESET_WORD)}
                    }
                    input {
                        r#type: "text",
                        autocomplete: "off",
                        autocapitalize: "characters",
                        style: "{theme::INPUT} text-align:center; letter-spacing:2px;",
                        value: "{typed}",
                        oninput: move |e| typed.set(e.value()),
                    }
                    if let Some(err) = error() {
                        div { style: "font-size:13px; color:{theme::DANGER};", "{err}" }
                    }
                    div { style: "display:flex; gap:10px;",
                        button {
                            style: "{theme::BTN_SECONDARY} flex:1;",
                            onclick: move |_| open.set(false),
                            {t.t("Cancel")}
                        }
                        button {
                            style: if confirmed {
                                "{theme::BTN_PRIMARY} flex:1; background:#c0262d;"
                            } else {
                                "{theme::BTN_PRIMARY} flex:1; background:#e5a3a7; cursor:default;"
                            },
                            disabled: !confirmed,
                            onclick: move |_| match crate::db::reset() {
                                Ok(())  => { open.set(false); on_reset.call(()); }
                                Err(e)  => error.set(Some(format!("{}: {e}", t.t("Error")))),
                            },
                            {t.t("Delete everything")}
                        }
                    }
                }
            }
        }
    }
}
