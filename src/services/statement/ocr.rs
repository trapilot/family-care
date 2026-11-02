//! On-device text recognition (Apple Vision on iOS / macOS, ML Kit on Android) for statement photos.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

/// One recognized text line; corners are normalized (0..1) in Vision's convention (origin bottom-left).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrLine {
    pub text: String,
    pub confidence: f32,
    pub top_left: (f64, f64),
    pub top_right: (f64, f64),
    pub bottom_left: (f64, f64),
    pub bottom_right: (f64, f64),
}

/// All lines of one photo plus its pixel size
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrPage {
    pub width: u32,
    pub height: u32,
    pub lines: Vec<OcrLine>,
}

/// Pixel size of a JPEG, read from its SOF marker.
#[cfg_attr(target_os = "android", allow(dead_code))]
fn jpeg_size(jpeg: &[u8]) -> Option<(u32, u32)> {
    if jpeg.len() < 4 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 4 <= jpeg.len() {
        if jpeg[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = jpeg[i + 1];
        // Fill bytes and standalone markers have no length
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if marker == 0x01 || (0xD0..=0xD9).contains(&marker) {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            if i + 9 > jpeg.len() {
                return None;
            }
            let h = u16::from_be_bytes([jpeg[i + 5], jpeg[i + 6]]) as u32;
            let w = u16::from_be_bytes([jpeg[i + 7], jpeg[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}

/// Recognize text in a JPEG (blocking, ~0.5–2 s). Runs Apple Vision on iOS / macOS.
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub fn recognize(jpeg: &[u8]) -> Result<OcrPage> {
    use objc2::rc::autoreleasepool;
    use objc2::runtime::AnyObject;
    use objc2::AnyThread;
    use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};
    use objc2_vision::{
        VNImageRequestHandler, VNRecognizeTextRequest, VNRecognizedTextObservation, VNRequest,
        VNRequestTextRecognitionLevel,
    };

    let (width, height) = jpeg_size(jpeg).ok_or_else(|| AppError::Parse("Unreadable image".into()))?;

    autoreleasepool(|_| {
        let pt = |p: objc2_core_foundation::CGPoint| (p.x, p.y);
        unsafe {
            let req = VNRecognizeTextRequest::new();
            req.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
            let langs = NSArray::from_retained_slice(&[NSString::from_str("vi-VN"), NSString::from_str("en-US")]);
            req.setRecognitionLanguages(&langs);
            req.setUsesLanguageCorrection(false);

            let data = NSData::with_bytes(jpeg);
            let options = NSDictionary::<objc2_vision::VNImageOption, AnyObject>::new();
            let handler = VNImageRequestHandler::initWithData_options(VNImageRequestHandler::alloc(), &data, &options);

            let as_request: &VNRequest = &req;
            let requests = NSArray::from_slice(&[as_request]);
            handler
                .performRequests_error(&requests)
                .map_err(|e| AppError::Validation(e.localizedDescription().to_string()))?;

            let mut lines = Vec::new();
            if let Some(results) = req.results() {
                for obs in results.iter() {
                    let obs: &VNRecognizedTextObservation = &obs;
                    let cands = obs.topCandidates(1);
                    let Some(top) = cands.firstObject() else { continue };
                    lines.push(OcrLine {
                        text: top.string().to_string(),
                        confidence: top.confidence(),
                        top_left: pt(obs.topLeft()),
                        top_right: pt(obs.topRight()),
                        bottom_left: pt(obs.bottomLeft()),
                        bottom_right: pt(obs.bottomRight()),
                    });
                }
            }
            Ok(OcrPage { width, height, lines })
        }
    })
}

/// Recognize text in a JPEG (blocking). Runs ML Kit (android/camera-capture) on Android.
#[cfg(target_os = "android")]
pub fn recognize(jpeg: &[u8]) -> Result<OcrPage> {
    let json = crate::android::recognize_text_json(jpeg)
        .ok_or_else(|| AppError::Validation("Text recognition failed".into()))?;
    serde_json::from_str(&json).map_err(|e| AppError::Parse(e.to_string()))
}

/// Text recognition needs Apple Vision or ML Kit; other targets only compile.
#[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "android")))]
pub fn recognize(_jpeg: &[u8]) -> Result<OcrPage> {
    Err(AppError::Validation("Text recognition is not supported on this device".into()))
}

/// `recognize` on a worker thread, so Vision never blocks the UI thread.
pub async fn recognize_async(jpeg: Vec<u8>) -> Result<OcrPage> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(recognize(&jpeg));
    });
    rx.await
        .map_err(|_| AppError::Validation("Text recognition was interrupted".into()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_size_reads_sof() {
        let bytes = [
            0xFF, 0xD8, // SOI
            0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, // APP0 with 2 payload bytes
            0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x0A, 0x00, 0x14, 0x00, 0x01, 0x01, 0x11, 0x00, // SOF0: h=2560, w=5120
        ];
        assert_eq!(jpeg_size(&bytes), Some((5120, 2560)));
        assert_eq!(jpeg_size(&[0, 1, 2, 3]), None);
        assert_eq!(jpeg_size(&bytes[..10]), None);
    }
}
