use dioxus::prelude::*;
use crate::i18n::use_i18n;
use crate::app::AppState;
use crate::models::Family;
use crate::repository::FamilyRepo;
use crate::views::theme;
use crate::views::ui::{Field, ScreenHeader};
use super::MemberView;

#[component]
pub fn FamilyForm(view: Signal<MemberView>, editing: Option<Family>) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let is_editing = editing.is_some();
    let title = if is_editing { t.t("Edit Family") } else { t.t("Add Family") };

    let mut name = use_signal(|| editing.as_ref().map(|f| f.name.clone()).unwrap_or_default());
    let mut common_address = use_signal(|| editing.as_ref().and_then(|f| f.common_address.clone()).unwrap_or_default());
    let mut note = use_signal(|| editing.as_ref().and_then(|f| f.note.clone()).unwrap_or_default());

    let state1 = state.clone();
    let state2 = state.clone();

    rsx! {
        ScreenHeader { title: title, on_back: move |_| { view.set(MemberView::List); } }
        div {
            style: "padding:16px; display:flex; flex-direction:column; gap:16px;",

            Field { label: t.t("Family Name").to_string(),
                input {
                    value: "{name.read()}",
                    style: "{theme::INPUT}",
                    oninput: move |e| name.set(e.value()),
                }
            }

            Field { label: t.t("Common Address").to_string(),
                textarea {
                    value: "{common_address.read()}",
                    style: "{theme::INPUT} min-height:80px; resize:vertical; font-family:inherit;",
                    oninput: move |e| common_address.set(e.value()),
                }
            }

            Field { label: t.t("Note").to_string(),
                textarea {
                    value: "{note.read()}",
                    style: "{theme::INPUT} min-height:80px; resize:vertical; font-family:inherit;",
                    oninput: move |e| note.set(e.value()),
                }
            }

            // Save button
            button {
                style: "{theme::BTN_PRIMARY} margin-top:8px;",
                onclick: move |_| {
                    let name_val = name.read().trim().to_string();
                    if name_val.is_empty() {
                        return;
                    }

                    state1.with_conn(|conn| {
                        if let Some(family) = &editing {
                            let mut updated = family.clone();
                            updated.name = name_val;
                            updated.common_address = if common_address.read().is_empty() { None } else { Some(common_address.read().clone()) };
                            updated.note = if note.read().is_empty() { None } else { Some(note.read().clone()) };
                            let _ = FamilyRepo::update(conn, &updated);
                        } else {
                            let new_family = crate::models::family::NewFamily {
                                name: name_val,
                                common_address: if common_address.read().is_empty() { None } else { Some(common_address.read().clone()) },
                                note: if note.read().is_empty() { None } else { Some(note.read().clone()) },
                            };
                            let _ = FamilyRepo::create(conn, &new_family);
                        }
                    });
                    view.set(MemberView::List);
                },
                {t.t("Save")}
            }

            // Delete button (only for editing)
            if is_editing {
                button {
                    style: "{theme::BTN_DANGER}",
                    onclick: {
                        let family = editing.clone().unwrap();
                        move |_| {
                            state2.with_conn(|conn| {
                                let _ = FamilyRepo::delete(conn, &family.id);
                            });
                            view.set(MemberView::List);
                        }
                    },
                    {t.t("Delete Family")}
                }
            }
        }
    }
}
