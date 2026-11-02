#[cfg(target_os = "android")]
mod android;
mod app;
mod db;
mod error;
mod i18n;
mod models;
mod notify;
mod repository;
mod services;
mod views;

use std::path::PathBuf;

fn main() {
    #[cfg(target_os = "ios")]
    let db_path = {
        let home = std::env::var_os("HOME")
            .expect("Could not find app home directory");

        let dir = PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("FamilyFinance");

        std::fs::create_dir_all(&dir)
            .expect("Failed to create database directory");

        dir.join("family_finance.db")
    };

    // The working directory is "/" (read-only) on Android: use the app's private files dir,
    // found from the process name, which is the package name
    #[cfg(target_os = "android")]
    let db_path = {
        let cmdline = std::fs::read("/proc/self/cmdline").expect("Could not read process name");
        let package = String::from_utf8_lossy(cmdline.split(|b| *b == 0).next().unwrap_or_default()).into_owned();
        let dir = PathBuf::from("/data/data").join(package).join("files");

        std::fs::create_dir_all(&dir)
            .expect("Failed to create database directory");

        dir.join("family_finance.db")
    };

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let db_path = PathBuf::from("family_finance.db");

    let db_path = db_path
        .to_str()
        .expect("Invalid database path");

    let db = db::init(db_path);
    {
        let conn = db.lock().unwrap();
        db::seed(&conn).expect("Failed to seed the database");
    }

    // "Done for today" tapped on a reminder notification (may arrive before any UI exists)
    notify::init(|notification_id| {
        let db = db::get();
        let conn = db.lock().expect("DB lock poisoned");
        let _ = services::ReminderService::handle_done_action(&conn, notification_id);
    });

    launch();
}

/// Add our viewport + base CSS to the default page so it fills the device screen and respects the safe areas.
#[cfg(any(feature = "mobile", feature = "desktop"))]
fn launch() {
    // `mobile` and `desktop` both re-export dioxus_desktop
    #[cfg(feature = "mobile")]
    use dioxus::mobile::Config;
    #[cfg(not(feature = "mobile"))]
    use dioxus::desktop::Config;

    let cfg = Config::new()
        .with_custom_head(include_str!("head.html").to_string())
        .with_background_color((245, 245, 245, 255));

    dioxus::LaunchBuilder::new().with_cfg(cfg).launch(app::App);
}

#[cfg(not(any(feature = "mobile", feature = "desktop")))]
fn launch() {
    dioxus::launch(app::App);
}
