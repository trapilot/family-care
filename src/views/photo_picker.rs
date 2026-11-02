use base64::Engine;
use dioxus::prelude::*;

/// Dioxus desktop/mobile swaps clicks on file inputs for its own native dialog, which
/// is empty on iOS. Our inputs (`data-photo-pick="<key>"`) stop the click in the capture
/// phase so the WebView's own camera / photo-library picker opens; every chosen photo is
/// shrunk to ≤max_size px (1600 by default) JPEG here and sent back as `{key, data}` (data = base64, "" on failure).
/// On Android (`__ffNativeCamera`) camera inputs are not opened by the WebView (it picks the front camera):
/// the click is cancelled and `{key, capture: max}` asks Rust to run the native rear-camera capture.
const PICK_PHOTO_JS: &str = r#"
    const old = window.__ffPhotoPick;
    if (old) {
        document.removeEventListener('click', old.click, true);
        document.removeEventListener('change', old.change, true);
    }
    const ours = t => t instanceof HTMLInputElement && t.dataset.photoPick;
    const click = e => {
        if (!ours(e.target)) return;
        e.stopPropagation();
        if (window.__ffNativeCamera && e.target.hasAttribute('capture')) {
            e.preventDefault();
            dioxus.send({ key: e.target.dataset.photoPick, data: '', capture: Number(e.target.dataset.photoMax) || 1600 });
        }
    };
    const shrink = (key, max, file) => {
        const url = URL.createObjectURL(file);
        const img = new Image();
        img.onload = () => {
            const s = Math.min(1, max / Math.max(img.naturalWidth, img.naturalHeight));
            const c = document.createElement('canvas');
            c.width = Math.round(img.naturalWidth * s);
            c.height = Math.round(img.naturalHeight * s);
            const ctx = c.getContext('2d');
            ctx.fillStyle = '#fff';
            ctx.fillRect(0, 0, c.width, c.height);
            ctx.drawImage(img, 0, 0, c.width, c.height);
            URL.revokeObjectURL(url);
            dioxus.send({ key, data: c.toDataURL('image/jpeg', 0.85).split(',')[1] });
        };
        img.onerror = () => { URL.revokeObjectURL(url); dioxus.send({ key, data: '' }); };
        img.src = url;
    };
    const change = e => {
        const input = e.target;
        if (!ours(input)) return;
        e.stopPropagation();
        const files = Array.from(input.files || []);
        input.value = '';
        const max = Number(input.dataset.photoMax) || 1600;
        files.forEach(f => shrink(input.dataset.photoPick, max, f));
    };
    document.addEventListener('click', click, true);
    document.addEventListener('change', change, true);
    window.__ffPhotoPick = { click, change };
"#;

/// What `PICK_PHOTO_JS` sends for each chosen photo
#[derive(serde::Deserialize)]
struct PickedPhoto {
    key:  String,
    #[serde(default)]
    data: String,
    /// Longest side for a native camera capture request (0 = not a capture request)
    #[serde(default)]
    capture: u32,
}

/// Listen for photos chosen through any `PhotoInput` on screen. `on_photo` gets the input's
/// key and the JPEG bytes, or `None` when the file couldn't be read.
/// One screen at a time: mounting it again takes over from the previous screen.
pub fn use_photo_picker(mut on_photo: impl FnMut(&str, Option<Vec<u8>>) + 'static) {
    use_hook(move || {
        spawn(async move {
            let js = if cfg!(target_os = "android") {
                format!("window.__ffNativeCamera = true;{PICK_PHOTO_JS}")
            } else {
                PICK_PHOTO_JS.to_string()
            };
            let mut eval = document::eval(&js);
            while let Ok(msg) = eval.recv::<PickedPhoto>().await {
                if msg.capture > 0 {
                    #[cfg(target_os = "android")]
                    if let Some(jpeg) = crate::android::capture_photo(msg.capture).await {
                        on_photo(&msg.key, jpeg);
                    }
                    continue;
                }
                let jpeg = base64::engine::general_purpose::STANDARD.decode(&msg.data).ok().filter(|j| !j.is_empty());
                on_photo(&msg.key, jpeg);
            }
        });
    });
}

/// Hidden file input to put inside a `label` (the label is the visible button).
/// `camera`: open the camera directly instead of the photo library.
#[component]
pub fn PhotoInput(
    key_name: String,
    #[props(default)] multiple: bool,
    #[props(default)] camera: bool,
    /// Longest side of the shrunk photo in pixels
    #[props(default = 1600)] max_size: u32,
) -> Element {
    rsx! {
        input {
            r#type: "file",
            accept: "image/*",
            style: "display:none;",
            multiple,
            "capture": if camera { "environment" },
            "data-photo-pick": "{key_name}",
            "data-photo-max": "{max_size}",
        }
    }
}
