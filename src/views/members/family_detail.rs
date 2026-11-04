use dioxus::prelude::*;
use crate::i18n::use_i18n;
use crate::app::AppState;
use crate::models::{Family, Member};
use crate::repository::MemberRepo;
use crate::views::icons::{Icon, IconName};
use crate::views::theme;
use crate::views::ui::{HeaderAction, InitialsAvatar, ScreenHeader, SectionTitle};
use super::MemberView;

#[component]
pub fn FamilyDetail(view: Signal<MemberView>, family: Family) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let members = use_signal(|| vec![]);

    let mut members_effect = members.clone();
    let family_id = family.id.clone();
    let state_effect = state.clone();

    use_effect(move || {
        state_effect.with_conn(|conn| {
            if let Ok(list) = MemberRepo::list_by_family(conn, &family_id) {
                members_effect.set(list);
            }
        });
    });

    let common_address_clone = family.common_address.clone();

    let family_for_edit = family.clone();

    rsx! {
        ScreenHeader {
            title: family.name.clone(),
            on_back: move |_| { view.set(MemberView::List); },
            action: rsx! {
                HeaderAction {
                    icon: IconName::Pencil,
                    label: t.t("Edit"),
                    onclick: move |_| { view.set(MemberView::EditFamily(family_for_edit.clone())); },
                }
            },
        }
        div {
            style: "padding:16px; display:flex; flex-direction:column; gap:16px;",

            // Common address
            if let Some(addr) = common_address_clone {
                div {
                    style: "{theme::CARD}",
                    div { style: "display:flex; align-items:center; justify-content:space-between; gap:8px;",
                        div { style: "display:flex; flex-direction:column; gap:4px; min-width:0;",
                            div { style: "{theme::LABEL} margin:0;", {t.t("Common Address")} }
                            div { style: "font-size:14px; color:{theme::TEXT};", "{addr}" }
                        }
                        button {
                            style: "{theme::BTN_SECONDARY} padding:8px 14px; font-size:13px; border-radius:999px; flex-shrink:0;",
                            onclick: {
                                let addr = addr.clone();
                                move |_| {
                                    let addr = addr.clone();
                                    spawn(async move { crate::views::clipboard::copy_to_clipboard(&addr).await; });
                                }
                            },
                            {t.t("Copy")}
                        }
                    }
                }
            }

            // Note
            if let Some(note) = &family.note {
                div {
                    style: "{theme::CARD}",
                    div { style: "{theme::LABEL}", {t.t("Note")} }
                    div { style: "font-size:14px; color:{theme::TEXT};", "{note}" }
                }
            }

            // Members section
            if !members.read().is_empty() {
                SectionTitle { title: t.t("Members").to_string() }
                div {
                    style: "{theme::CARD} padding:0 16px;",
                    for m in members.read().iter() {
                        MemberRow { member: m.clone(), view }
                    }
                }
            }
        }
    }
}

#[component]
fn MemberRow(member: Member, view: Signal<MemberView>) -> Element {
    let t = use_i18n();
    let address = member.address.clone();
    let copy_address = move |_| {
        if let Some(addr) = address.clone() {
            spawn(async move { crate::views::clipboard::copy_to_clipboard(&addr).await; });
        }
    };

    rsx! {
        div {
            style: "display:flex; align-items:center; gap:12px; padding:12px 0; cursor:pointer; {theme::ROW_DIVIDER}",
            onclick: {
                let m = member.clone();
                move |_| { view.set(MemberView::Detail(m.clone())); }
            },
            InitialsAvatar { name: member.full_name.clone(), seed: member.id.clone(), size: 44 }
            div { style: "flex:1; min-width:0;",
                div {
                    style: "display:flex; align-items:center; gap:6px;",
                    span { style: "font-size:15px; color:{theme::TEXT};", "{member.full_name}" }
                    if member.role == crate::models::member::MemberRole::Owner {
                        span {
                            style: "font-size:10px; font-weight:700; background:#fef3c7; color:#d97706; border-radius:999px; padding:2px 8px;",
                            {t.t("OWNER")}
                        }
                    }
                }
                if let Some(phone) = &member.phone {
                    div { style: "font-size:13px; color:{theme::TEXT_MUTED};", "{phone}" }
                }
            }
            if member.address.is_some() {
                button {
                    style: "background:{theme::PRIMARY_SOFT}; color:{theme::PRIMARY}; border:none; border-radius:999px; padding:6px 12px; font-size:12px; font-weight:600; cursor:pointer; flex-shrink:0;",
                    onclick: copy_address,
                    {t.t("Copy Address")}
                }
            }
            Icon { name: IconName::ChevronRight, size: 20, color: theme::TEXT_MUTED.to_string() }
        }
    }
}
