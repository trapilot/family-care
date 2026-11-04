//! Horizontal swipe gestures (month change, swipe-from-edge to go back).
//!
//! Dioxus touch events don't expose the event target, and a swipe must be ignored on horizontal
//! scrollers, inputs, dialogs and hidden screens, so the detection runs in JS and reports
//! "left" / "right" / "back" to Rust (same eval pattern as the backup file picker).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use dioxus::prelude::*;

/// Installs `touchstart`/`touchend` listeners on the document. `__ID__` is replaced by a JSON
/// string literal. A previous listener with the same key is removed first (re-mount).
const SWIPE_JS: &str = r#"
const id = __ID__;
const reg = (window.__ffSwipe = window.__ffSwipe || {});
const old = reg[id];
if (old) {
    document.removeEventListener('touchstart', old.start);
    document.removeEventListener('touchend', old.end);
}
let sx = 0, sy = 0, st = 0, target = null, active = false;
// A fixed element only blocks when it doesn't contain the screen's anchor (dialogs/overlays);
// the app shell is position:fixed and wraps every screen, so it must not block.
const blocked = (node, anchor) => {
    let el = node && node.nodeType === 1 ? node : (node ? node.parentElement : null);
    if (el && el.closest && el.closest('input, textarea, select, [contenteditable], [data-noswipe]')) return true;
    while (el && el !== document.documentElement) {
        const cs = getComputedStyle(el);
        if (cs.position === 'fixed' && !el.contains(anchor)) return true;
        if ((cs.overflowX === 'auto' || cs.overflowX === 'scroll') && el.scrollWidth > el.clientWidth + 1) return true;
        el = el.parentElement;
    }
    return false;
};
const start = (ev) => {
    if (ev.touches.length !== 1) { active = false; return; }
    const t = ev.touches[0];
    sx = t.clientX; sy = t.clientY; st = Date.now(); target = ev.target; active = true;
};
const end = (ev) => {
    if (!active) return;
    active = false;
    const t = ev.changedTouches && ev.changedTouches[0];
    if (!t) return;
    const anchor = document.getElementById(id);
    if (!anchor || anchor.offsetParent === null) return;
    if (blocked(target, anchor)) return;
    const dx = t.clientX - sx, dy = t.clientY - sy, dt = Date.now() - st;
    // Generous time limit: slow drags (e.g. mouse on the emulator) still count as swipes
    if (Math.abs(dx) < 70 || Math.abs(dx) <= 1.8 * Math.abs(dy) || dt > 2000) return;
    if (sx <= 28) { if (dx > 0) dioxus.send('back'); return; }
    dioxus.send(dx < 0 ? 'left' : 'right');
};
document.addEventListener('touchstart', start, { passive: true });
document.addEventListener('touchend', end, { passive: true });
reg[id] = { start, end };
"#;

const REMOVE_JS: &str = r#"
const id = __ID__;
const reg = window.__ffSwipe;
if (reg && reg[id]) {
    document.removeEventListener('touchstart', reg[id].start);
    document.removeEventListener('touchend', reg[id].end);
    delete reg[id];
}
"#;

#[derive(Clone, Default)]
struct Handlers {
    left:  Option<EventHandler<()>>,
    right: Option<EventHandler<()>>,
    back:  Option<EventHandler<()>>,
}

/// A fresh id for the root element of a screen header (`use_swipe`'s `anchor_id`).
pub fn use_swipe_anchor() -> String {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    use_hook(|| format!("ff-swipe-{}", NEXT.fetch_add(1, Ordering::Relaxed)))
}

/// Installs a document-level swipe listener while the calling component is mounted.
/// `anchor_id`: id of an element of the calling screen; gestures are ignored while it is hidden
/// (offsetParent == null), so a screen kept mounted with display:none doesn't react.
/// Swipes starting at the left edge only trigger `on_back`; other swipes trigger `on_left` / `on_right`.
pub fn use_swipe(
    anchor_id: String,
    on_left: Option<EventHandler<()>>,
    on_right: Option<EventHandler<()>>,
    on_back: Option<EventHandler<()>>,
) {
    // The loop below lives across renders: keep the latest handlers in a shared cell
    let handlers = use_hook(|| Rc::new(RefCell::new(Handlers::default())));
    *handlers.borrow_mut() = Handlers { left: on_left, right: on_right, back: on_back };

    let id_lit = serde_json::to_string(&anchor_id).unwrap_or_else(|_| "\"\"".into());
    use_hook({
        let handlers = handlers.clone();
        let id_lit = id_lit.clone();
        move || {
            spawn(async move {
                let mut eval = document::eval(&SWIPE_JS.replace("__ID__", &id_lit));
                while let Ok(msg) = eval.recv::<String>().await {
                    let h = handlers.borrow().clone();
                    let handler = match msg.as_str() {
                        "left"  => h.left,
                        "right" => h.right,
                        "back"  => h.back,
                        _       => None,
                    };
                    if let Some(handler) = handler {
                        handler.call(());
                    }
                }
            });
        }
    });

    use_drop(move || {
        let _ = document::eval(&REMOVE_JS.replace("__ID__", &id_lit));
    });
}
