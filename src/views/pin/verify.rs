use dioxus::prelude::*;
use crate::i18n::use_i18n;
use crate::app::AppState;
use crate::repository::SettingsRepo;
use crate::views::theme;

#[component]
pub fn PinVerifyModal(
    is_visible: ReadSignal<bool>,
    on_cancel: EventHandler,
    on_verify: EventHandler,
) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let mut pin = use_signal(|| String::new());
    let mut error = use_signal(|| None::<String>);

    let mut on_submit = move || {
        let p = pin.read().clone();

        let result = state.with_conn(|conn| SettingsRepo::verify_pin(conn, &p));

        match result {
            Ok(true) => {
                pin.set(String::new());
                error.set(None);
                on_verify.call(());
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
        if *is_visible.read() {
            div {
                style: "{theme::OVERLAY} z-index:200;",
                div {
                    style: "{theme::DIALOG}",
                    h3 { style: "margin:0 0 8px; font-size:16px; font-weight:600; color:{theme::TEXT}; text-align:center;", {t.t("Enter PIN")} },
                    p { style: "margin:0 0 20px; font-size:14px; color:{theme::TEXT_MUTED}; text-align:center;", {t.t("Please enter your PIN to continue.")} },

                    if let Some(err) = error.read().clone() {
                        div { style: "width:100%; box-sizing:border-box; background:{theme::NEGATIVE_SOFT}; border-radius:12px; padding:10px; color:{theme::DANGER}; text-align:center; font-size:14px; margin-bottom:16px;", "{err}" }
                    }

                    form {
                        onsubmit: move |e| {
                            e.prevent_default();
                            on_submit();
                        },
                        input {
                            r#type: "password",
                            inputmode: "numeric",
                            enterkeyhint: "done",
                            autocomplete: "off",
                            maxlength: 6,
                            style: "{theme::INPUT} text-align:center; letter-spacing:4px; margin-bottom:16px;",
                            value: "{pin.read().clone()}",
                            oninput: move |e| {
                                let v = e.value().chars().filter(|c| c.is_ascii_digit()).collect();
                                pin.set(v);
                                error.set(None);
                            }
                        }

                        div { style: "display:flex; gap:10px;",
                            button {
                                style: "{theme::BTN_SECONDARY} flex:1;",
                                r#type: "button",
                                onclick: move |_| {
                                    pin.set(String::new());
                                    error.set(None);
                                    on_cancel.call(());
                                },
                                {t.t("Cancel")}
                            }
                            button {
                                style: "{theme::BTN_PRIMARY} flex:1;",
                                r#type: "submit",
                                {t.t("Verify")}
                            }
                        }
                    }
                }
            }
        }
    }
}
