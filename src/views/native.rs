//! Small cross-platform helpers around native phone features: the dialer and the app's private files dir.

use std::path::PathBuf;

/// Keep only digits and a leading `+` of a phone number
pub fn dial_digits(number: &str) -> String {
    let mut out = String::new();
    for (i, c) in number.trim().chars().enumerate() {
        if c.is_ascii_digit() || (c == '+' && i == 0) {
            out.push(c);
        }
    }
    out
}

/// Open the phone dialer prefilled with `number`. Returns false when it couldn't be opened
/// (unsupported platform, empty number, no dialer app).
pub fn dial(number: &str) -> bool {
    let number = dial_digits(number);
    if number.is_empty() {
        return false;
    }
    dial_native(&number)
}

#[cfg(target_os = "android")]
fn dial_native(number: &str) -> bool {
    crate::android::dial_number(number)
}

#[cfg(target_os = "ios")]
fn dial_native(number: &str) -> bool {
    crate::share_ios::dial(number)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn dial_native(_number: &str) -> bool {
    false
}

/// The app's writable files folder: `/data/data/<package>/files` on Android (the cwd is read-only
/// there, same derivation as main.rs), the current directory elsewhere.
pub fn app_files_dir() -> std::io::Result<PathBuf> {
    #[cfg(target_os = "android")]
    {
        let cmdline = std::fs::read("/proc/self/cmdline")?;
        let package = String::from_utf8_lossy(cmdline.split(|b| *b == 0).next().unwrap_or_default()).into_owned();
        Ok(PathBuf::from("/data/data").join(package).join("files"))
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(PathBuf::new())
    }
}

#[cfg(test)]
mod tests {
    use super::dial_digits;

    #[test]
    fn dial_digits_keeps_digits_and_leading_plus() {
        assert_eq!(dial_digits("+84 912-345 678"), "+84912345678");
        assert_eq!(dial_digits("(090) 123.4567"), "0901234567");
        assert_eq!(dial_digits("09+12"), "0912");
        assert_eq!(dial_digits("abc"), "");
    }
}
