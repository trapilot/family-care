use dioxus::prelude::*;
use crate::i18n::use_i18n;

use crate::app::AppState;
use crate::models::{member::MemberRole, Member};
use crate::repository::MemberRepo;
use crate::views::ts_to_date_str;
use crate::views::icons::{Icon, IconName};
use crate::views::theme;
use crate::views::ui::{HeaderAction, IconText, InitialsAvatar, ScreenHeader};
use super::id_cards::IdPhotoPreview;
use super::MemberView;

#[component]
pub fn MemberDetail(view: Signal<MemberView>, member: Member) -> Element {
    let t = use_i18n();
    let state          = use_context::<AppState>();
    let mut confirm_delete = use_signal(|| false);
    let mut copied_field   = use_signal(|| None::<&'static str>);

    // Helper: copy text to clipboard via JS eval in WebView
    let copy = move |text: String, field: &'static str| {
        let js = format!("navigator.clipboard.writeText('{text}').catch(()=>{{}})");
        let _ = dioxus::document::eval(&js);
        copied_field.set(Some(field));
        // Clear "Copied!" badge after 1.5s
        spawn(async move {
            // tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            copied_field.set(None);
        });
    };
    
    let member_editable = member.clone();

    rsx! {
        div {
            style: "display:flex; flex-direction:column; height:100%;",

            // ── Top bar ──
            ScreenHeader {
                title: member.full_name.clone(),
                on_back: move |_| { view.set(MemberView::List); },
                action: rsx! {
                    HeaderAction {
                        icon: IconName::Pencil,
                        label: t.t("Edit"),
                        onclick: move |_| { view.set(MemberView::Edit(member_editable.clone())); },
                    }
                    HeaderAction {
                        icon: IconName::Trash,
                        label: t.t("Delete"),
                        onclick: move |_| { confirm_delete.set(true); },
                    }
                },
            }

            div {
                style: "flex:1; overflow-y:auto;",

                // ── Hero ──
                div {
                    style: "padding:24px 20px 8px; display:flex; flex-direction:column; align-items:center; gap:10px;",
                    InitialsAvatar { name: member.full_name.clone(), seed: member.id.clone(), size: 88 }
                    div { style: "font-size:19px; font-weight:600; color:{theme::TEXT}; text-align:center;", "{member.full_name}" }
                    div {
                        style: "display:flex; gap:8px; align-items:center;",
                        if member.role == MemberRole::Owner {
                            span {
                                style: "font-size:12px; font-weight:700; background:#fef3c7; color:#d97706; border-radius:999px; padding:4px 10px; display:inline-flex; align-items:center; gap:4px;",
                                IconText { text: t.t("👑 OWNER").to_string(), size: 14, gap: 4 }
                            }
                        } else {
                            span {
                                style: "font-size:12px; font-weight:700; background:{theme::PRIMARY_SOFT}; color:{theme::PRIMARY}; border-radius:999px; padding:4px 10px; display:inline-flex; align-items:center; gap:4px;",
                                IconText { text: t.t("👤 MEMBER").to_string(), size: 14, gap: 4 }
                            }
                        }
                        if let Some(ref gender) = member.gender {
                            span {
                                style: "font-size:12px; background:{theme::BG}; color:{theme::TEXT_MUTED}; border:1px solid {theme::BORDER}; border-radius:999px; padding:4px 10px;",
                                {t.code(&gender.to_string())}
                            }
                        }
                    }
                }

                // ── Contact fields ──
                div {
                    style: "padding:16px; display:flex; flex-direction:column; gap:16px;",

                    // Phone
                    if let Some(ref phone) = member.phone {
                        {
                            let phone = phone.clone();
                            let mut copy = copy.clone();
                            rsx! {
                                CopyField {
                                    icon:  IconName::Smartphone,
                                    label: t.t("Phone"),
                                    value: phone.clone(),
                                    copied: *copied_field.read() == Some("phone"),
                                    on_copy: move |_| { copy(phone.clone(), "phone"); }
                                }
                            }
                        }
                    }

                    // Address
                    if let Some(ref address) = member.address {
                        {
                            let address = address.clone();
                            let mut copy = copy.clone();
                            rsx! {
                                CopyField {
                                    icon:  IconName::House,
                                    label: t.t("Address"),
                                    value: address.clone(),
                                    copied: *copied_field.read() == Some("address"),
                                    on_copy: move |_| { copy(address.clone(), "address"); }
                                }
                            }
                        }
                    }

                    // ID Number
                    if let Some(ref id_number) = member.id_number {
                        {
                            let id_number = id_number.clone();
                            let mut copy = copy.clone();
                            rsx! {
                                CopyField {
                                    icon:  IconName::IdCard,
                                    label: t.t("ID Number (CCCD/CMND)"),
                                    value: id_number.clone(),
                                    copied: *copied_field.read() == Some("id_number"),
                                    on_copy: move |_| { copy(id_number.clone(), "id_number"); }
                                }
                            }
                        }
                    }

                    // ID Issue info
                    if member.id_issue_date.is_some() || member.id_issue_place.is_some() {
                        div {
                            style: "{theme::CARD}",
                            div { style: "font-size:14px; font-weight:600; color:{theme::TEXT}; margin-bottom:8px;", {t.t("ID Issue")} }
                            if let Some(ts) = member.id_issue_date {
                                InfoRow { label: t.t("Issue Date"),  value: ts_to_date_str(ts) }
                            }
                            if let Some(ref place) = member.id_issue_place {
                                InfoRow { label: t.t("Issue Place"), value: place.clone() }
                            }
                        }
                    }

                    IdPhotoPreview { member_id: member.id.clone() }

                    // Personal info
                    div {
                        style: "{theme::CARD}",
                        div { style: "font-size:14px; font-weight:600; color:{theme::TEXT}; margin-bottom:8px;", {t.t("Personal Info")} }
                        if let Some(bd) = member.birth_date {
                            InfoRow { label: t.t("Date of Birth"), value: ts_to_date_str(bd) }
                        }
                        if let Some(ref gender) = member.gender {
                            InfoRow { label: t.t("Gender"), value: t.code(&gender.to_string()) }
                        }
                        InfoRow { label: t.t("Role"), value: t.code(&member.role.to_string()) }
                    }

                    // Note
                    if let Some(ref note) = member.note {
                        div {
                            style: "{theme::CARD}",
                            div { style: "font-size:14px; font-weight:600; color:{theme::TEXT}; margin-bottom:6px;", {t.t("Note")} }
                            div { style: "font-size:14px; color:{theme::TEXT}; line-height:1.5;", "{note}" }
                        }
                    }
                }
            }

            // ── Delete confirm modal ──
            if *confirm_delete.read() {
                div {
                    style: "{theme::OVERLAY}",
                    div {
                        style: "{theme::DIALOG}",
                        div { style: "margin:0 0 8px; font-size:16px; font-weight:600; color:{theme::TEXT};", {t.t("Remove member?")} }
                        p  { style: "margin:0 0 20px; font-size:14px; color:{theme::TEXT_MUTED}; line-height:1.5;",
                            {t.t("\"{name}\" will be permanently removed from your family directory.").replace("{name}", &member.full_name)}
                        }
                        div { style: "display:flex; gap:10px;",
                            button {
                                style: "{theme::BTN_SECONDARY} flex:1;",
                                onclick: move |_| { confirm_delete.set(false); },
                                {t.t("Cancel")}
                            }
                            button {
                                style: "{theme::BTN_DANGER} flex:1;",
                                onclick: {
                                    let state = state.clone();
                                    let mid   = member.id.clone();
                                    move |_| {
                                        state.with_conn(|conn| { let _ = MemberRepo::delete(conn, &mid); });
                                        view.set(MemberView::List);
                                    }
                                },
                                {t.t("Remove")}
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── CopyField ────────────────────────────────────────────────────────────────
// Tap the copy button → text goes to clipboard, badge flashes "Copied!"

#[component]
fn CopyField(
    icon:    IconName,
    label:   &'static str,
    value:   String,
    copied:  bool,
    on_copy: EventHandler<MouseEvent>,
) -> Element {
    let t = use_i18n();
    let button_style = if copied { 
        "background:#17a589; color:#fff;" 
    } else { 
        "background:#e2e5f7; color:#3f51b5;" 
    };

    rsx! {
        div {
            style: "{theme::CARD} display:flex; align-items:flex-start; gap:12px;",

            span { style: "flex-shrink:0; margin-top:1px;", Icon { name: icon, size: 22, color: theme::PRIMARY.to_string() } }

            div { style: "flex:1; min-width:0;",
                div { style: "{theme::LABEL} margin:0 0 3px;", "{label}" }
                div { style: "font-size:14px; font-weight:500; color:{theme::TEXT}; word-break:break-word;", "{value}" }
            }

            button {
                style: format!(
                    "flex-shrink:0; border:none; border-radius:999px; padding:7px 14px; font-size:12px; font-weight:600; cursor:pointer; transition:all .15s; {}", 
                    button_style
                ),
                onclick: move |e| { on_copy.call(e); },
                if copied { IconText { text: t.t("✓ Copied").to_string(), size: 14, gap: 4 } } else { {t.t("Copy")} }
            }
        }
    }
}

// ─── InfoRow ─────────────────────────────────────────────────────────────────

#[component]
fn InfoRow(label: &'static str, value: String) -> Element {
    rsx! {
        div {
            style: "display:flex; justify-content:space-between; align-items:baseline; padding:8px 0; border-bottom:1px solid {theme::BORDER};",
            span { style: "font-size:14px; color:{theme::TEXT_MUTED}; flex-shrink:0; margin-right:12px;", "{label}" }
            span { style: "font-size:14px; font-weight:600; color:{theme::TEXT}; text-align:right; word-break:break-all;", "{value}" }
        }
    }
}
