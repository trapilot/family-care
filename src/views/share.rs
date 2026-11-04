//! Native share sheet for files written under the app's files dir.

use std::path::Path;

use crate::error::{AppError, Result};

/// Open the native share sheet for `path`. The file contents never leave the app except through
/// the app the user picks.
pub fn share_file(path: &Path, mime: &str, chooser_title: &str) -> Result<()> {
    if !path.exists() {
        return Err(AppError::NotFound(path.display().to_string()));
    }
    open_share_sheet(&path.to_string_lossy(), mime, chooser_title)
}

#[cfg(target_os = "android")]
fn open_share_sheet(path: &str, mime: &str, chooser_title: &str) -> Result<()> {
    if crate::android::share_file_sheet(path, chooser_title, mime) {
        Ok(())
    } else {
        Err(AppError::Validation("Couldn't open the share sheet".into()))
    }
}

#[cfg(target_os = "ios")]
fn open_share_sheet(path: &str, _mime: &str, _chooser_title: &str) -> Result<()> {
    if crate::share_ios::share_file(path) {
        Ok(())
    } else {
        Err(AppError::Validation("Couldn't open the share sheet".into()))
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn open_share_sheet(_path: &str, _mime: &str, _chooser_title: &str) -> Result<()> {
    Err(AppError::Validation("Sharing is only available on the phone".into()))
}
