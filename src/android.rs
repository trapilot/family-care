//! Android native helpers (android/camera-capture Gradle module, bundled by dx): camera capture and
//! ML Kit text recognition.

use std::sync::OnceLock;
use std::time::Duration;

use base64::Engine;
use manganis::jni::objects::{JClass, JValue};

#[manganis::ffi("android/camera-capture")]
extern "Kotlin" {
    pub type CameraCapture;
    // Returns 1 when started, else 0 (manganis' `bool` return conversion doesn't compile with jni 0.21)
    pub fn capture(this: &CameraCapture, max_size: i32, front: i32) -> i32;
    pub fn take_result(this: &CameraCapture) -> Option<String>;
    pub fn recognize_text(this: &CameraCapture, jpeg_base64: String) -> String;
    // Returns 1 when the share sheet was started, else 0
    pub fn share_file(this: &CameraCapture, path: String, chooser_title: String) -> i32;
}

/// The Kotlin helper, created once through the activity's class loader (`find_class` can't see app
/// classes from native threads).
fn plugin() -> Option<&'static CameraCapture> {
    static PLUGIN: OnceLock<Option<CameraCapture>> = OnceLock::new();
    PLUGIN
        .get_or_init(|| {
            manganis::android::with_activity(|env, activity| {
                let created = (|| -> manganis::jni::errors::Result<CameraCapture> {
                    let loader = env
                        .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
                        .l()?;
                    let name = env.new_string("com.example.familymoney.camera.CameraCapture")?;
                    let class = env
                        .call_method(
                            &loader,
                            "loadClass",
                            "(Ljava/lang/String;)Ljava/lang/Class;",
                            &[JValue::Object(&name)],
                        )?
                        .l()?;
                    let obj = env.new_object(
                        JClass::from(class),
                        "(Landroid/app/Activity;)V",
                        &[JValue::Object(activity)],
                    )?;
                    let global = env.new_global_ref(&obj)?;
                    Ok(CameraCapture::from_global_ref(global))
                })();
                match created {
                    Ok(p) => Some(p),
                    Err(e) => {
                        if env.exception_check().unwrap_or(false) {
                            let _ = env.exception_describe();
                            let _ = env.exception_clear();
                        }
                        eprintln!("camera plugin init failed: {e}");
                        None
                    }
                }
            })
        })
        .as_ref()
}

/// Open the front or rear camera and wait for the photo (JPEG, long side <= `max_size`).
/// `None` = user cancelled; `Some(None)` = failed (no camera app / unreadable); `Some(Some(jpeg))` = photo.
pub async fn capture_photo(max_size: u32, front: bool) -> Option<Option<Vec<u8>>> {
    let Some(plugin) = plugin() else { return Some(None) };
    if !matches!(capture(plugin, max_size as i32, front as i32), Ok(1)) {
        return Some(None);
    }
    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        match take_result(plugin) {
            Ok(None) => continue,
            Ok(Some(s)) if s.is_empty() => return None,
            Ok(Some(s)) => {
                return Some(base64::engine::general_purpose::STANDARD.decode(s).ok());
            }
            Err(_) => return Some(None),
        }
    }
}

/// ML Kit text recognition on a JPEG (blocking): the OcrPage JSON, or `None` on failure.
pub fn recognize_text_json(jpeg: &[u8]) -> Option<String> {
    let plugin = plugin()?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(jpeg);
    recognize_text(plugin, b64).ok().filter(|s| !s.is_empty())
}

/// Open the system share sheet (app chooser) for a file under filesDir/backup.
pub fn share_file_sheet(path: &str, chooser_title: &str) -> bool {
    let Some(plugin) = plugin() else { return false };
    matches!(share_file(plugin, path.to_string(), chooser_title.to_string()), Ok(1))
}
