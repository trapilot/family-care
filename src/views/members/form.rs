use dioxus::prelude::*;
use crate::i18n::use_i18n;

use crate::app::AppState;
use crate::models::{Family, Member, NewMember};
use crate::models::member::{Gender, MemberRole};
use crate::repository::{FamilyRepo, IdCardRepo, MemberRepo};
use crate::views::date_str_to_ts;
use super::id_cards::{save_photos, IdPhotoFields};
use crate::views::theme;
use crate::views::icons::{Icon, IconName};
use crate::views::ui::{DateInput, IconText, InitialsAvatar, ScreenHeader};
use super::MemberView;

#[component]
pub fn MemberForm(view: Signal<MemberView>, editing: Option<Member>) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let families = use_signal(|| vec![]);

    let editing_id = editing.as_ref().map(|m| m.id.clone()).unwrap_or_else(|| "new".to_string());
    let mut full_name      = use_signal(|| editing.as_ref().map(|m| m.full_name.clone()).unwrap_or_default());
    let mut family_id      = use_signal(|| editing.as_ref().and_then(|m| m.family_id.clone()));
    let mut role           = use_signal(|| editing.as_ref().map(|m| m.role.clone()).unwrap_or(MemberRole::Member));
    let mut gender         = use_signal(|| editing.as_ref().and_then(|m| m.gender.clone()));
    let avatar_emoji       = use_signal(|| editing.as_ref().and_then(|m| m.avatar_emoji.clone()).unwrap_or_default());
    let mut phone          = use_signal(|| editing.as_ref().and_then(|m| m.phone.clone()).unwrap_or_default());
    let mut address        = use_signal(|| editing.as_ref().and_then(|m| m.address.clone()).unwrap_or_default());
    let mut birth_date     = use_signal(|| date_from_ts(editing.as_ref().and_then(|m| m.birth_date)));
    let mut id_number      = use_signal(|| editing.as_ref().and_then(|m| m.id_number.clone()).unwrap_or_default());
    let mut id_issue_date  = use_signal(|| date_from_ts(editing.as_ref().and_then(|m| m.id_issue_date)));
    let mut id_issue_place = use_signal(|| editing.as_ref().and_then(|m| m.id_issue_place.clone()).unwrap_or_default());
    let mut note           = use_signal(|| editing.as_ref().and_then(|m| m.note.clone()).unwrap_or_default());
    let mut show_id        = use_signal(|| false);
    let id_front           = use_signal(|| load_photo(&state, editing.as_ref(), crate::models::IdSide::Front));
    let id_back            = use_signal(|| load_photo(&state, editing.as_ref(), crate::models::IdSide::Back));
    let error_msg          = use_signal(|| String::new());

    let title = if editing.is_some() { t.t("Edit Member") } else { t.t("New Member") };

    // Load families on mount
    let mut families_effect = families.clone();
    let state_effect = state.clone();
    use_effect(move || {
        state_effect.with_conn(|conn| {
            if let Ok(list) = FamilyRepo::list(conn) {
                families_effect.set(list);
            }
        });
    });

    let submit = {
        let state = state.clone();
        let editing = editing.clone();
        let mut error_msg = error_msg.clone();
        let family_id_clone = family_id.clone();

        move |_: MouseEvent| {
            let name = full_name.read().trim().to_string();
            if name.is_empty() { error_msg.set(t.t("Full name is required").into()); return; }

            let opt = |s: &str| if s.trim().is_empty() { None } else { Some(s.trim().to_string()) };

            let input = NewMember {
                family_id:      family_id_clone.read().clone(),
                full_name:      name,
                birth_date:     date_str_to_ts(&birth_date.read()),
                gender:         gender.read().clone(),
                phone:          opt(&phone.read()),
                id_number:      opt(&id_number.read()),
                id_issue_date:  date_str_to_ts(&id_issue_date.read()),
                id_issue_place: opt(&id_issue_place.read()),
                address:        opt(&address.read()),
                role:           role.read().clone(),
                avatar_emoji:   opt(&avatar_emoji.read()),
                note:           opt(&note.read()),
            };

            let result = state.with_conn(|conn| {
                if let Some(ref old) = editing {
                    let mut updated = old.clone();
                    updated.family_id      = input.family_id.clone();
                    updated.full_name      = input.full_name.clone();
                    updated.birth_date     = input.birth_date;
                    updated.gender         = input.gender.clone();
                    updated.phone          = input.phone.clone();
                    updated.id_number      = input.id_number.clone();
                    updated.id_issue_date  = input.id_issue_date;
                    updated.id_issue_place = input.id_issue_place.clone();
                    updated.address        = input.address.clone();
                    updated.role           = input.role.clone();
                    updated.avatar_emoji   = input.avatar_emoji.clone();
                    updated.note           = input.note.clone();
                    MemberRepo::update(conn, &updated)?;
                    save_photos(conn, &updated.id, id_front.read().as_deref(), id_back.read().as_deref())
                } else {
                    let created = MemberRepo::create(conn, &input)?;
                    save_photos(conn, &created.id, id_front.read().as_deref(), id_back.read().as_deref())
                }
            });

            match result {
                Ok(_)  => { view.set(MemberView::List); }
                Err(e) => { error_msg.set(e.to_string()); }
            }
        }
    };

    rsx! {
        ScreenHeader { title: title, on_back: move |_| { view.set(MemberView::List); } }
        div {
            style: "padding:16px; display:flex; flex-direction:column; gap:16px; padding-bottom:40px;",

            // ── Avatar preview (initials, follows the name input) ──
            div {
                style: "display:flex; justify-content:center;",
                InitialsAvatar {
                    name: full_name.read().clone(),
                    seed: editing_id.clone(),
                    size: 64,
                }
            }

            // ── Full name ──
            Label { text: t.t("Full Name *") }
            TextInput { value: full_name.read().clone(), placeholder: "", on_input: move |v| { full_name.set(v); } }

            // ── Family ──
            Label { text: t.t("Family") }
            div {
                style: "display:flex; flex-direction:column; gap:6px;",
                select {
                    style: "{theme::INPUT}",
                    oninput: move |e| {
                        let val = e.value();
                        if val.is_empty() {
                            family_id.set(None);
                        } else {
                            family_id.set(Some(val));
                        }
                    },
                    option {
                        value: "",
                        selected: family_id.read().is_none(),
                        {t.t("Unassigned")}
                    }
                    for family in families.read().iter() {
                        option {
                            value: "{family.id}",
                            selected: family_id.read().as_deref() == Some(&family.id),
                            "{family.name}"
                        }
                    }
                }
            }

            // ── Role ──
            Label { text: t.t("Role") }
            div {
                style: "display:flex; gap:8px;",
                for (r, label, icon) in [(MemberRole::Owner,t.t("Owner"),IconName::Crown),(MemberRole::Member,t.t("Member"),IconName::User)] {
                    {
                        let rc = r.clone();
                        let is_sel = *role.read() == r;
                        rsx! {
                            button {
                                style: if is_sel {
                                    "flex:1; padding:10px 14px; border:1px solid #e2e5f7; border-radius:999px; background:#e2e5f7; color:#3f51b5; font-size:14px; font-weight:600; cursor:pointer; display:flex; align-items:center; justify-content:center; gap:6px;"
                                } else {
                                    "flex:1; padding:10px 14px; border:1px solid #e6e6ef; border-radius:999px; background:transparent; color:#8a8d9f; font-size:14px; cursor:pointer; display:flex; align-items:center; justify-content:center; gap:6px;"
                                },
                                onclick: move |_| { role.set(rc.clone()); },
                                Icon { name: icon, size: 16 }
                                "{label}"
                            }
                        }
                    }
                }
            }

            // ── Gender ──
            Label { text: t.t("Gender") }
            div {
                style: "display:flex; gap:8px;",
                for (g, label) in [(Some(Gender::Male),t.t("Male")),(Some(Gender::Female),t.t("Female")),(None,t.t("Other"))] {
                    {
                        let gc = g.clone();
                        let is_sel = *gender.read() == g;
                        rsx! {
                            button {
                                style: if is_sel {
                                    "flex:1; padding:10px 14px; border:1px solid #e2e5f7; border-radius:999px; background:#e2e5f7; color:#3f51b5; font-size:14px; font-weight:600; cursor:pointer;"
                                } else {
                                    "flex:1; padding:10px 14px; border:1px solid #e6e6ef; border-radius:999px; background:transparent; color:#8a8d9f; font-size:14px; cursor:pointer;"
                                },
                                onclick: move |_| { gender.set(gc.clone()); },
                                "{label}"
                            }
                        }
                    }
                }
            }

            // ── Phone ──
            Label { text: t.t("Phone") }
            TextInput { value: phone.read().clone(), placeholder: "", on_input: move |v| { phone.set(v); } }

            // ── Address (the main use-case: copy for Grab/Uber) ──
            Label { text: t.t("Address") }
            div {
                style: "position:relative;",
                textarea {
                    style: "{theme::INPUT} resize:none; height:72px; font-family:inherit;",
                    placeholder: "",
                    value: "{address.read()}",
                    oninput: move |e| { address.set(e.value()); },
                }
            }
            div { style: "font-size:12px; color:{theme::TEXT_MUTED}; margin-top:-8px;",
                IconText { text: t.t("💡 This is the address you will copy when ordering a ride to this person's house.").to_string(), size: 14, color: theme::TEXT_MUTED.to_string() }
            }

            // ── Date of birth ──
            Label { text: t.t("Date of Birth") }
            DateInput { value: birth_date.read().clone(), oninput: move |v: String| birth_date.set(v) }

            // ── Identity (collapsible) ──
            button {
                style: "text-align:left; border:none; background:transparent; padding:4px 0; font-size:14px; font-weight:600; color:{theme::PRIMARY}; cursor:pointer; display:flex; align-items:center; gap:6px;",
                onclick: move |_| show_id.toggle(),
                if *show_id.read() {
                    Icon { name: IconName::ChevronDown, size: 16 }
                } else {
                    Icon { name: IconName::ChevronRight, size: 16 }
                }
                {t.t("▼ Identity Document (CCCD/CMND)").trim_start_matches(['▼', '▶']).trim().to_string()}
            }

            if *show_id.read() {
                div {
                    style: "background:{theme::BG}; border-radius:16px; padding:14px; display:flex; flex-direction:column; gap:12px;",
                    Label { text: t.t("ID Number") }
                    TextInput { value: id_number.read().clone(), placeholder: "", on_input: move |v| { id_number.set(v); } }
                    Label { text: t.t("Issue Date") }
                    DateInput { value: id_issue_date.read().clone(), oninput: move |v: String| id_issue_date.set(v) }
                    Label { text: t.t("Issue Place") }
                    TextInput { value: id_issue_place.read().clone(), placeholder: "", on_input: move |v| { id_issue_place.set(v); } }
                    Label { text: t.t("ID Card Photos") }
                    IdPhotoFields { front: id_front, back: id_back }
                }
            }

            // ── Note ──
            Label { text: t.t("Note (optional)") }
            TextInput { value: note.read().clone(), placeholder: "", on_input: move |v| { note.set(v); } }

            // ── Error ──
            if !error_msg.read().is_empty() {
                div {
                    style: "background:{theme::NEGATIVE_SOFT}; border-radius:12px; padding:10px 12px; font-size:13px; color:{theme::DANGER};",
                    "{error_msg.read()}"
                }
            }

            button {
                style: "{theme::BTN_PRIMARY}",
                onclick: submit,
                if editing.is_some() { {t.t("Update Member")} } else { {t.t("Add Member")} }
            }
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn load_photo(state: &AppState, editing: Option<&Member>, side: crate::models::IdSide) -> Option<Vec<u8>> {
    let member = editing?;
    state.with_conn(|conn| IdCardRepo::get(conn, &member.id, side).ok().flatten())
}

fn date_from_ts(ts: Option<i64>) -> String {
    ts.and_then(|t| {
        use chrono::{Local, TimeZone};
        Local.timestamp_opt(t, 0).single()
            .map(|d| d.format("%Y-%m-%d").to_string())
    }).unwrap_or_default()
}

#[component]
fn Label(text: &'static str) -> Element {
    rsx! {
        label {
            style: "{theme::LABEL} margin:4px 0 -8px;",
            "{text}"
        }
    }
}

#[component]
fn TextInput(value: String, placeholder: &'static str, on_input: EventHandler<String>) -> Element {
    rsx! {
        input {
            style: "{theme::INPUT}",
            r#type: "text",
            placeholder,
            value: "{value}",
            oninput: move |e| { on_input.call(e.value()); },
        }
    }
}
