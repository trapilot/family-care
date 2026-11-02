use dioxus::prelude::*;
use crate::i18n::use_i18n;

use crate::app::AppState;
use crate::models::{member::MemberRole, Family, Member};
use crate::repository::{FamilyRepo, MemberRepo};
use crate::views::icons::{Icon, IconName};
use crate::views::theme;
use crate::views::ui::{HeaderAction, IconCircle, InitialsAvatar, ScreenHeader, SectionTitle};
use super::MemberView;

#[component]
pub fn MemberList(view: Signal<MemberView>) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let families_with_members = use_signal(|| vec![]);
    let unassigned_members = use_signal(|| vec![]);

    let mut families_effect = families_with_members.clone();
    let mut unassigned_effect = unassigned_members.clone();
    let state_effect = state.clone();

    use_effect(move || {
        state_effect.with_conn(|conn| {
            if let Ok(families) = FamilyRepo::list_with_members(conn) {
                families_effect.set(families);
            }
            if let Ok(unassigned) = MemberRepo::list_unassigned(conn) {
                unassigned_effect.set(unassigned);
            }
        });
    });

    rsx! {
        ScreenHeader {
            title: t.t("Families & Members"),
            action: rsx! {
                HeaderAction {
                    icon: IconName::Plus,
                    label: t.t("Add Member"),
                    onclick: move |_| { view.set(MemberView::Add); },
                }
            },
        }
        div {
            style: "padding:16px; display:flex; flex-direction:column; gap:16px;",

            div {
                style: "display:flex; gap:10px; flex-wrap:wrap;",
                button {
                    style: "{theme::BTN_SECONDARY} display:flex; align-items:center; gap:8px; padding:10px 16px; font-size:14px; border-radius:999px;",
                    onclick: move |_| { view.set(MemberView::AddFamily); },
                    Icon { name: IconName::Plus, size: 18, color: theme::PRIMARY.to_string() }
                    {t.t("Add Family")}
                }
                button {
                    style: "{theme::BTN_SECONDARY} display:flex; align-items:center; gap:8px; padding:10px 16px; font-size:14px; border-radius:999px;",
                    onclick: move |_| { view.set(MemberView::IdCardsPdf); },
                    Icon { name: IconName::IdCard, size: 18, color: theme::PRIMARY.to_string() }
                    {t.t("ID Cards → PDF")}
                }
            }

            if families_with_members.read().is_empty() && unassigned_members.read().is_empty() {
                EmptyState {}
            }

            // Families
            for (family, members) in families_with_members.read().iter() {
                FamilyCard { family: family.clone(), members: members.clone(), view }
            }

            // Unassigned members
            if !unassigned_members.read().is_empty() {
                SectionTitle { title: t.t("Unassigned Members").to_string() }
                div {
                    style: "{theme::CARD} padding:0 16px;",
                    for m in unassigned_members.read().iter() {
                        ContactRow { member: m.clone(), view }
                    }
                }
            }
        }
    }
}

// ─── FamilyCard ───────────────────────────────────────────────────────────────

#[component]
fn FamilyCard(family: Family, members: Vec<Member>, view: Signal<MemberView>) -> Element {
    let common_address_preview = family.common_address.as_deref()
        .map(|a| if a.chars().count() > 50 { format!("{}…", a.chars().take(50).collect::<String>()) } else { a.to_string() })
        .unwrap_or_else(|| "—".into());

    rsx! {
        div {
            style: "{theme::CARD} padding:0 16px;",

            div {
                style: "display:flex; align-items:center; gap:12px; padding:14px 0; cursor:pointer; {theme::ROW_DIVIDER}",
                onclick: {
                    let family = family.clone();
                    move |_| { view.set(MemberView::FamilyDetail(family.clone())); }
                },
                IconCircle { icon: None, fallback: IconName::House, color: theme::PRIMARY.to_string(), size: 44 }
                div { style: "flex:1; min-width:0;",
                    div { style: "font-size:15px; font-weight:600; color:{theme::TEXT};", "{family.name}" }
                    if family.common_address.is_some() {
                        div {
                            style: "font-size:13px; color:{theme::TEXT_MUTED}; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                            "{common_address_preview}"
                        }
                    }
                }
                Icon { name: IconName::ChevronRight, size: 20, color: theme::TEXT_MUTED.to_string() }
            }

            for m in members.iter() {
                MiniContactRow { member: m.clone(), view }
            }
        }
    }
}

// ─── MiniContactRow ───────────────────────────────────────────────────────────────

#[component]
fn MiniContactRow(member: Member, view: Signal<MemberView>) -> Element {
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:12px; cursor:pointer; padding:10px 0 10px 12px; {theme::ROW_DIVIDER}",
            onclick: {
                let m = member.clone();
                move |_| { view.set(MemberView::Detail(m.clone())); }
            },
            InitialsAvatar { name: member.full_name.clone(), seed: member.id.clone(), size: 36 }
            div { style: "flex:1; min-width:0;",
                span { style: "font-size:14px; color:{theme::TEXT};", "{member.full_name}" }
            }
            Icon { name: IconName::ChevronRight, size: 18, color: theme::TEXT_MUTED.to_string() }
        }
    }
}

// ─── ContactRow ───────────────────────────────────────────────────────────────

#[component]
fn ContactRow(member: Member, view: Signal<MemberView>) -> Element {
    let t = use_i18n();
    let phone_preview   = member.phone.as_deref().unwrap_or("—");
    let address_preview = member.address.as_deref()
        .map(|a| if a.chars().count() > 35 { format!("{}…", a.chars().take(35).collect::<String>()) } else { a.to_string() })
        .unwrap_or_else(|| "—".into());
    let is_owner = member.role == MemberRole::Owner;

    rsx! {
        div {
            style: "display:flex; align-items:center; gap:12px; padding:12px 0; cursor:pointer; {theme::ROW_DIVIDER}",
            onclick: {
                let m = member.clone();
                move |_| { view.set(MemberView::Detail(m.clone())); }
            },

            InitialsAvatar { name: member.full_name.clone(), seed: member.id.clone(), size: 48 }

            div { style: "flex:1; min-width:0;",
                div {
                    style: "display:flex; align-items:center; gap:6px; margin-bottom:3px;",
                    span { style: "font-size:15px; color:{theme::TEXT};", "{member.full_name}" }
                    if is_owner {
                        span {
                            style: "font-size:10px; font-weight:700; background:#fef3c7; color:#d97706; border-radius:999px; padding:2px 8px;",
                            {t.t("OWNER")}
                        }
                    }
                }
                div { style: "font-size:13px; color:{theme::TEXT_MUTED};", "{phone_preview}" }
                div {
                    style: "font-size:13px; color:{theme::TEXT_MUTED}; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                    "{address_preview}"
                }
            }

            Icon { name: IconName::ChevronRight, size: 20, color: theme::TEXT_MUTED.to_string() }
        }
    }
}

#[component]
fn EmptyState() -> Element {
    let t = use_i18n();
    rsx! {
        div {
            style: "text-align:center; padding:56px 0; color:{theme::TEXT_MUTED}; display:flex; flex-direction:column; align-items:center;",
            Icon { name: IconName::Users, size: 40, color: theme::TEXT_MUTED.to_string() }
            p { style: "font-size:14px; font-weight:600; margin:12px 0 4px; color:{theme::TEXT};", {t.t("No families yet")} }
            p { style: "font-size:13px; margin:0;", {t.t("Add a family and members to organize contacts!")} }
        }
    }
}
