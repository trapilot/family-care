use base64::Engine;
use dioxus::prelude::*;
use std::collections::HashMap;

use crate::app::AppState;
use crate::i18n::use_i18n;
use crate::models::{id_card::jpeg_data_url, IdSide, Member};
use crate::repository::{FamilyRepo, IdCardRepo, MemberRepo};
use crate::services::id_card_pdf::{self, CardPair};
use crate::views::photo_picker::{use_photo_picker, PhotoInput};
use crate::views::icons::{Icon, IconName};
use crate::views::theme;
use crate::views::ui::{IconText, InitialsAvatar, ScreenHeader};
use super::MemberView;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

// ─── Photo slots (member form) ────────────────────────────────────────────────

/// Front + back photo slots. Edits stay in the signals until the form is saved.
#[component]
pub fn IdPhotoFields(front: Signal<Option<Vec<u8>>>, back: Signal<Option<Vec<u8>>>) -> Element {
    let t = use_i18n();
    let mut error = use_signal(|| false);

    use_photo_picker(move |key, jpeg| {
        let mut target = if key == "back" { back } else { front };
        match jpeg {
            Some(jpeg) => { target.set(Some(jpeg)); error.set(false); }
            None       => error.set(true),
        }
    });

    rsx! {
        div {
            style: "display:flex; gap:10px;",
            PhotoSlot { side: IdSide::Front, photo: front }
            PhotoSlot { side: IdSide::Back,  photo: back }
        }
        if error() {
            div {
                style: "font-size:12px; color:{theme::DANGER};",
                IconText { text: t.t("❌ Could not read that photo").to_string(), size: 12, gap: 4 }
            }
        }
    }
}

#[component]
fn PhotoSlot(side: IdSide, photo: Signal<Option<Vec<u8>>>) -> Element {
    let t = use_i18n();
    let label = side_label(side);
    let src = photo.read().as_deref().map(jpeg_data_url);

    rsx! {
        div {
            style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:6px;",
            div { style: "font-size:13px; font-weight:600; color:{theme::TEXT_MUTED};", {t.t(label)} }
            label {
                style: "aspect-ratio:1.585; border:1px dashed {theme::PRIMARY}; border-radius:12px; background:{theme::PRIMARY_SOFT}; display:flex; align-items:center; justify-content:center; overflow:hidden; cursor:pointer; font-size:13px; color:{theme::PRIMARY}; text-align:center;",
                if let Some(src) = src {
                    img { src, style: "width:100%; height:100%; object-fit:contain;" }
                } else {
                    IconText { text: t.t("📷 Add photo").to_string() }
                }
                PhotoInput { key_name: side.to_string() }
            }
            if photo.read().is_some() {
                button {
                    style: "border:none; background:transparent; color:{theme::DANGER}; font-size:12px; cursor:pointer; padding:0;",
                    onclick: move |_| photo.set(None),
                    {t.t("Remove")}
                }
            }
        }
    }
}

/// Write the form's photos for a member: present → saved, cleared → deleted
pub fn save_photos(
    conn: &rusqlite::Connection,
    member_id: &str,
    front: Option<&[u8]>,
    back: Option<&[u8]>,
) -> crate::error::Result<()> {
    for (side, jpeg) in [(IdSide::Front, front), (IdSide::Back, back)] {
        match jpeg {
            Some(jpeg) => IdCardRepo::set(conn, member_id, side, jpeg)?,
            None       => IdCardRepo::remove(conn, member_id, side)?,
        }
    }
    Ok(())
}

fn side_label(side: IdSide) -> &'static str {
    match side {
        IdSide::Front => "Front side",
        IdSide::Back  => "Back side",
    }
}

// ─── Photo preview (member detail) ────────────────────────────────────────────

#[component]
pub fn IdPhotoPreview(member_id: String) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let photos: Vec<(IdSide, String)> = state.with_conn(|conn| {
        IdSide::ALL.iter()
            .filter_map(|&side| IdCardRepo::get(conn, &member_id, side).ok().flatten().map(|j| (side, jpeg_data_url(&j))))
            .collect()
    });

    if photos.is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            style: "{theme::CARD}",
            div { style: "font-size:14px; font-weight:600; color:{theme::TEXT}; margin-bottom:8px;", {t.t("ID Card Photos")} }
            div {
                style: "display:flex; gap:10px;",
                for (side, src) in photos {
                    div {
                        style: "flex:1; min-width:0;",
                        img { src, style: "width:100%; aspect-ratio:1.585; object-fit:contain; border-radius:8px; background:{theme::BG};" }
                        div { style: "font-size:12px; color:{theme::TEXT_MUTED}; text-align:center; margin-top:4px;", {t.t(side_label(side))} }
                    }
                }
            }
        }
    }
}

// ─── ID cards → PDF ───────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
struct Candidate {
    member:      Member,
    family_name: Option<String>,
    sides:       Vec<IdSide>,
}

/// Turn the PDF into a File held by the page, so the later Share tap is a tiny,
/// instant eval and still counts as a user gesture for `navigator.share`.
const STAGE_PDF_JS: &str = r#"
    const [b64, name] = await dioxus.recv();
    const bytes = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
    window.__ffIdPdf = new File([bytes], name, { type: 'application/pdf' });
    dioxus.send('ready');
"#;

const SHARE_PDF_JS: &str = r#"
    const file = window.__ffIdPdf;
    try {
        if (navigator.canShare && navigator.canShare({ files: [file] })) {
            await navigator.share({ files: [file], title: file.name });
            dioxus.send('shared');
        } else {
            const a = document.createElement('a');
            a.href = URL.createObjectURL(file);
            a.download = file.name;
            document.body.appendChild(a);
            a.click();
            a.remove();
            dioxus.send('downloaded');
        }
    } catch (e) {
        dioxus.send(e && e.name === 'AbortError' ? 'cancelled' : 'error: ' + e);
    }
"#;

#[component]
pub fn IdCardsPdf(view: Signal<MemberView>) -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();

    let candidates = use_signal({
        let state = state.clone();
        move || state.with_conn(load_candidates).unwrap_or_default()
    });
    let mut selected = use_signal(Vec::<String>::new);  // member ids, in PDF order
    let mut drag     = use_signal(|| None::<Drag>);
    let mut ready    = use_signal(|| false);
    let mut status   = use_signal(String::new);

    // Any change to the selection makes the staged PDF stale
    use_effect(move || {
        let _ = selected.read();
        ready.set(false);
    });

    let by_id: HashMap<String, Candidate> = candidates.read().iter()
        .map(|c| (c.member.id.clone(), c.clone()))
        .collect();
    let picked: Vec<Candidate> = selected.read().iter().filter_map(|id| by_id.get(id).cloned()).collect();

    // Families with at least one photographed member, for "add whole family"
    let mut families: Vec<String> = Vec::new();
    for c in candidates.read().iter().filter(|c| !c.sides.is_empty()) {
        if let Some(name) = &c.family_name {
            if !families.contains(name) { families.push(name.clone()); }
        }
    }

    let create = {
        let state = state.clone();
        move |_| {
            let ids = selected.read().clone();
            let result = state.with_conn(|conn| -> crate::error::Result<Vec<u8>> {
                let mut people = Vec::new();
                for id in &ids {
                    people.push(CardPair {
                        front: IdCardRepo::get(conn, id, IdSide::Front)?,
                        back:  IdCardRepo::get(conn, id, IdSide::Back)?,
                    });
                }
                id_card_pdf::build(&people)
            });
            match result {
                Ok(pdf) => {
                    let name = format!("CCCD_{}.pdf", chrono::Local::now().format("%Y-%m-%d"));
                    let pages = ids.len().div_ceil(id_card_pdf::ROWS_PER_PAGE);
                    spawn(async move {
                        let mut eval = document::eval(STAGE_PDF_JS);
                        let _ = eval.send((B64.encode(&pdf), name));
                        if eval.recv::<String>().await.is_ok() {
                            ready.set(true);
                            status.set(
                                t.t("PDF ready: {people} people, {pages} page(s)")
                                    .replace("{people}", &ids.len().to_string())
                                    .replace("{pages}", &pages.to_string()),
                            );
                        }
                    });
                }
                Err(e) => status.set(format!("❌ {e}")),
            }
        }
    };

    let share = move |_| {
        spawn(async move {
            let mut eval = document::eval(SHARE_PDF_JS);
            if let Ok(result) = eval.recv::<String>().await {
                if result.starts_with("error") {
                    status.set(format!("❌ {}: {}", t.t("Share failed"), result.trim_start_matches("error: ")));
                }
            }
        });
    };

    rsx! {
        ScreenHeader { title: t.t("ID Cards → PDF"), on_back: move |_| { view.set(MemberView::List); } }
        div {
            style: "padding:16px; display:flex; flex-direction:column; gap:16px; padding-bottom:40px;",

            div { style: "font-size:14px; color:{theme::TEXT_MUTED}; line-height:1.5;",
                for (i, part) in t.t("Tap people in the order you want, then drag ☰ to reorder. Each person is one row (front | back) at real card size, 5 per A4 page.").split('☰').enumerate() {
                    if i > 0 {
                        span { style: "display:inline-flex; vertical-align:middle;",
                            Icon { name: IconName::GripVertical, size: 14, color: theme::TEXT_MUTED.to_string() }
                        }
                    }
                    "{part}"
                }
            }

            // ── Selected, in order ──
            if !picked.is_empty() {
                div {
                    style: "{theme::CARD} display:flex; flex-direction:column; gap:6px;",
                    div { style: "display:flex; justify-content:space-between; align-items:center;",
                        span { style: "font-size:14px; font-weight:600; color:{theme::TEXT};",
                            {t.t("PDF order")}
                        }
                        button {
                            style: "border:none; background:transparent; color:{theme::DANGER}; font-size:13px; cursor:pointer;",
                            onclick: move |_| selected.write().clear(),
                            {t.t("Clear")}
                        }
                    }
                    // Pointer events (not HTML5 drag-and-drop, which iOS ignores for touch).
                    // Listening on the list keeps the drag going when the finger leaves the handle.
                    div {
                        style: "display:flex; flex-direction:column; gap:{ORDER_GAP_PX}px;",
                        onpointermove: move |e| {
                            let Some(mut d) = drag() else { return };
                            let len = selected.read().len();
                            let rows = (e.client_coordinates().y - d.start_y) / ORDER_STEP_PX;
                            let target = (d.from as f64 + rows).round().clamp(0.0, (len - 1) as f64) as usize;
                            if target != d.at {
                                let mut sel = selected.write();
                                let id = sel.remove(d.at);
                                sel.insert(target, id);
                                d.at = target;
                                drag.set(Some(d));
                            }
                        },
                        onpointerup: move |_| drag.set(None),
                        onpointercancel: move |_| drag.set(None),
                        onpointerleave: move |_| drag.set(None),
                        for (i, c) in picked.iter().enumerate() {
                            OrderRow { index: i, candidate: c.clone(), selected, drag }
                        }
                    }
                }
            }

            // ── Quick add a whole family ──
            if !families.is_empty() {
                div { style: "display:flex; flex-wrap:wrap; gap:8px;",
                    for name in families {
                        button {
                            style: "border:1px solid #e2e5f7; background:#e2e5f7; color:#3f51b5; border-radius:999px; padding:8px 14px; font-size:14px; font-weight:600; cursor:pointer; display:inline-flex; align-items:center; gap:4px;",
                            onclick: {
                                let name = name.clone();
                                move |_| {
                                    let ids: Vec<String> = candidates.read().iter()
                                        .filter(|c| !c.sides.is_empty() && c.family_name.as_deref() == Some(name.as_str()))
                                        .map(|c| c.member.id.clone())
                                        .collect();
                                    let mut sel = selected.write();
                                    for id in ids {
                                        if !sel.contains(&id) { sel.push(id); }
                                    }
                                }
                            },
                            Icon { name: IconName::Plus, size: 14 }
                            Icon { name: IconName::House, size: 14 }
                            "{name}"
                        }
                    }
                }
            }

            // ── Everyone ──
            div {
                style: "{theme::CARD} padding:0 16px; overflow:hidden;",
                if candidates.read().is_empty() {
                    div { style: "padding:24px; text-align:center; color:{theme::TEXT_MUTED}; font-size:13px;", {t.t("No members yet")} }
                }
                for c in candidates.read().iter().cloned() {
                    CandidateRow { candidate: c, selected }
                }
            }

            if !status.read().is_empty() {
                div { style: "font-size:13px; color:{theme::PRIMARY}; background:{theme::PRIMARY_SOFT}; border-radius:12px; padding:10px 12px;",
                    IconText {
                        text: status.read().clone(),
                        size: 14,
                        color: if status.read().starts_with('❌') { Some(theme::DANGER.to_string()) } else { None },
                    }
                }
            }

            if ready() {
                button {
                    style: "{theme::BTN_PRIMARY} background:{theme::POSITIVE};",
                    onclick: share,
                    IconText { text: t.t("📤 Share / Save PDF").to_string() }
                }
            } else {
                button {
                    style: if picked.is_empty() {
                        "{theme::BTN_PRIMARY} opacity:.4; cursor:default;"
                    } else {
                        "{theme::BTN_PRIMARY}"
                    },
                    disabled: picked.is_empty(),
                    onclick: create,
                    span { style: "display:inline-flex; align-items:center; justify-content:center;",
                        IconText { text: t.t("📄 Create PDF").to_string() }
                        "\u{a0}({picked.len()})"
                    }
                }
            }
        }
    }
}

#[component]
fn CandidateRow(candidate: Candidate, selected: Signal<Vec<String>>) -> Element {
    let t = use_i18n();
    let id = candidate.member.id.clone();
    let position = selected.read().iter().position(|s| *s == id);
    let has_photos = !candidate.sides.is_empty();
    let missing = if !has_photos {
        Some(t.t("No ID photos"))
    } else if candidate.sides.len() == 1 {
        Some(if candidate.sides[0] == IdSide::Front { t.t("Back side missing") } else { t.t("Front side missing") })
    } else {
        None
    };

    rsx! {
        div {
            style: if has_photos {
                "display:flex; align-items:center; gap:10px; padding:10px 0; {theme::ROW_DIVIDER} cursor:pointer;"
            } else {
                "display:flex; align-items:center; gap:10px; padding:10px 0; {theme::ROW_DIVIDER} opacity:.45;"
            },
            onclick: move |_| {
                if !has_photos { return; }
                let mut sel = selected.write();
                match sel.iter().position(|s| *s == id) {
                    Some(i) => { sel.remove(i); }
                    None    => sel.push(id.clone()),
                }
            },
            div {
                style: if position.is_some() {
                    "width:26px; height:26px; border-radius:50%; background:{theme::PRIMARY}; color:#fff; font-size:13px; font-weight:700; display:flex; align-items:center; justify-content:center; flex-shrink:0;"
                } else {
                    "width:26px; height:26px; border-radius:50%; border:2px solid {theme::BORDER}; flex-shrink:0;"
                },
                if let Some(p) = position { "{p + 1}" }
            }
            InitialsAvatar { name: candidate.member.full_name.clone(), seed: candidate.member.id.clone(), size: 36 }
            div { style: "flex:1; min-width:0;",
                div { style: "font-size:14px; color:{theme::TEXT};", "{candidate.member.full_name}" }
                div { style: "font-size:13px; color:{theme::TEXT_MUTED};",
                    if let Some(f) = &candidate.family_name {
                        span { style: "display:inline-flex; align-items:center; gap:4px;",
                            Icon { name: IconName::House, size: 12 }
                            "{f}"
                        }
                    }
                    if let Some(m) = missing { span { style: "color:#d97706; margin-left:6px;", "{m}" } }
                }
            }
        }
    }
}

const ORDER_ROW_PX:  f64 = 40.0;
const ORDER_GAP_PX:  f64 = 6.0;
const ORDER_STEP_PX: f64 = ORDER_ROW_PX + ORDER_GAP_PX;

/// A row being dragged: where it started, where it is now, and the pointer's start y
#[derive(Clone, Copy, PartialEq)]
struct Drag {
    from:    usize,
    at:      usize,
    start_y: f64,
}

#[component]
fn OrderRow(index: usize, candidate: Candidate, selected: Signal<Vec<String>>, drag: Signal<Option<Drag>>) -> Element {
    let dragging = drag().is_some_and(|d| d.at == index);
    rsx! {
        div {
            style: format!(
                "height:{ORDER_ROW_PX}px; display:flex; align-items:center; gap:8px; border-radius:8px; padding:0 4px; {}",
                if dragging { "background:#e2e5f7; box-shadow:0 2px 8px rgba(63,81,181,.3);" } else { "background:#fff;" }
            ),
            // Drag handle: touch-action:none stops the page scrolling while dragging
            div {
                style: "width:36px; height:100%; display:flex; align-items:center; justify-content:center; color:#8a8d9f; cursor:grab; touch-action:none; user-select:none; -webkit-user-select:none;",
                onpointerdown: move |e| {
                    e.prevent_default();
                    drag.set(Some(Drag { from: index, at: index, start_y: e.client_coordinates().y }));
                },
                Icon { name: IconName::GripVertical, size: 18, color: theme::TEXT_MUTED.to_string() }
            }
            span { style: "width:20px; font-size:13px; font-weight:700; color:#3f51b5; text-align:right;", "{index + 1}." }
            span { style: "flex:1; min-width:0; font-size:14px; color:#1f2133; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                "{candidate.member.full_name}"
            }
            button {
                style: "border:none; background:transparent; cursor:pointer; width:28px; display:flex; align-items:center; justify-content:center; padding:0;",
                onclick: move |_| { selected.write().remove(index); },
                Icon { name: IconName::X, size: 18, color: theme::TEXT_MUTED.to_string() }
            }
        }
    }
}

/// Everyone, family by family (same order as the Family tab), unassigned last
fn load_candidates(conn: &rusqlite::Connection) -> crate::error::Result<Vec<Candidate>> {
    let mut sides = IdCardRepo::sides_by_member(conn)?;
    let mut out = Vec::new();
    let mut push = |member: Member, family_name: Option<String>| {
        let sides = sides.remove(&member.id).unwrap_or_default();
        out.push(Candidate { member, family_name, sides });
    };
    for (family, members) in FamilyRepo::list_with_members(conn)? {
        for m in members { push(m, Some(family.name.clone())); }
    }
    for m in MemberRepo::list_unassigned(conn)? {
        push(m, None);
    }
    Ok(out)
}
