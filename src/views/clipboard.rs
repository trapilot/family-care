//! Clipboard copy shared by the screens.

use dioxus::prelude::*;

/// Clipboard copy with a textarea fallback for WebViews without the async clipboard API.
/// The text is passed through `dioxus.recv()`, so it never has to be escaped into the script.
const COPY_JS: &str = r#"
const n = await dioxus.recv();
let ok = false;
try { await navigator.clipboard.writeText(n); ok = true; } catch (e) {
  try {
    const ta = document.createElement('textarea');
    ta.value = n; ta.style.position = 'fixed'; ta.style.opacity = '0';
    document.body.appendChild(ta); ta.select();
    ok = document.execCommand('copy');
    document.body.removeChild(ta);
  } catch (e2) {}
}
dioxus.send(ok);
"#;

/// Copy `text` to the clipboard; true when it was copied.
pub async fn copy_to_clipboard(text: &str) -> bool {
    let mut eval = document::eval(COPY_JS);
    if eval.send(text.to_string()).is_err() {
        return false;
    }
    matches!(eval.recv::<bool>().await, Ok(true))
}
