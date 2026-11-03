//! iOS share sheet (UIActivityViewController) for backup files.

use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString, NSURL};
use objc2_ui_kit::{UIActivityViewController, UIApplication, UIViewController, UIWindowScene};

/// Present the share sheet for the file at `path`. Returns true when the sheet was presented (or
/// queued on the main thread), false when no view controller could present it.
pub fn share_file(path: &str) -> bool {
    match MainThreadMarker::new() {
        Some(mtm) => present(mtm, path),
        None => {
            let path = path.to_string();
            DispatchQueue::main().exec_async(move || {
                if let Some(mtm) = MainThreadMarker::new() {
                    present(mtm, &path);
                }
            });
            true
        }
    }
}

fn present(mtm: MainThreadMarker, path: &str) -> bool {
    let Some(presenter) = top_view_controller(mtm) else { return false };
    let ns_path = NSString::from_str(path);
    let url = NSURL::fileURLWithPath(&ns_path);
    let items: Retained<NSArray<AnyObject>> = NSArray::from_retained_slice(&[Retained::into_super(Retained::into_super(url))]);
    let avc = unsafe {
        UIActivityViewController::initWithActivityItems_applicationActivities(
            UIActivityViewController::alloc(mtm),
            &items,
            None,
        )
    };
    // iPad: the sheet is a popover and crashes without an anchor
    unsafe {
        if let Some(popover) = avc.popoverPresentationController() {
            if let Some(view) = presenter.view() {
                let b = view.bounds();
                popover.setSourceView(Some(&view));
                popover.setSourceRect(NSRect::new(
                    NSPoint::new(b.origin.x + b.size.width / 2.0, b.origin.y + b.size.height / 2.0),
                    NSSize::new(1.0, 1.0),
                ));
            }
        }
        presenter.presentViewController_animated_completion(&avc, true, None);
    }
    true
}

/// Top-most presented view controller of the key window of the first window scene.
fn top_view_controller(mtm: MainThreadMarker) -> Option<Retained<UIViewController>> {
    unsafe {
        let app = UIApplication::sharedApplication(mtm);
        let scenes = app.connectedScenes();
        let mut root = None;
        for scene in scenes.iter() {
            let Ok(ws) = scene.downcast::<UIWindowScene>() else { continue };
            let windows = ws.windows();
            let window = windows.iter().find(|w| w.isKeyWindow()).or_else(|| windows.firstObject());
            if let Some(vc) = window.and_then(|w| w.rootViewController()) {
                root = Some(vc);
                break;
            }
        }
        let mut top = root?;
        while let Some(next) = top.presentedViewController() {
            top = next;
        }
        Some(top)
    }
}
