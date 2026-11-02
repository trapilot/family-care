use dioxus::prelude::*;
use crate::i18n::use_i18n;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::app::AppState;
use crate::models::{Category, CurrencyInfo, Family, Holding, IdCardImage, Member, Transaction, Wallet};
use crate::repository::{CategoryRepo, CurrencyRepo, FamilyRepo, HoldingRepo, IdCardRepo, SettingsRepo, TransactionRepo, WalletRepo};
use crate::views::icons::{emoji_icon, Icon, IconName};
use crate::views::pin::PinVerifyModal;
use crate::views::theme;
use crate::views::ui::IconText;

/// Banner showing an operation result: a check/cross icon chosen from the leading emoji
#[component]
fn StatusBanner(text: String) -> Element {
    let (fg, bg) = if text.starts_with('✅') {
        (theme::POSITIVE, theme::POSITIVE_SOFT)
    } else if text.starts_with('❌') {
        (theme::DANGER, theme::NEGATIVE_SOFT)
    } else {
        (theme::TEXT, theme::BG)
    };
    let (icon, label) = match emoji_icon(&text) {
        Some((name, rest)) => (Some(name), rest.to_string()),
        None => (None, text.clone()),
    };
    rsx! {
        div {
            style: "display:flex; align-items:flex-start; gap:8px; background:{bg}; color:{fg}; border-radius:12px; padding:10px 12px; font-size:13px; overflow-wrap:anywhere;",
            if let Some(name) = icon {
                Icon { name, size: 16, color: fg.to_string() }
            }
            span { "{label}" }
        }
    }
}

// ─── Backup format ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupData {
    pub app_version:  String,
    pub exported_at:  i64,
    pub wallets:      Vec<Wallet>,
    pub holdings:     Vec<Holding>,
    pub categories:   Vec<Category>,
    pub transactions: Vec<Transaction>,
    pub members:      Vec<Member>,
    pub families:     Vec<Family>,
    /// CCCD photos; missing in backups made before photos existed
    #[serde(default)]
    pub id_card_images: Vec<IdCardImage>,
    /// Money types (names, units, order); missing in backups made before multi-currency wallets
    #[serde(default)]
    pub currencies: Vec<CurrencyInfo>,
}

// ─── BackupSettings view ─────────────────────────────────────────────────────

// ─── Action to execute after PIN verification ─────────────────────────────────
#[derive(Clone, PartialEq)]
enum PendingAction {
    Export,
    ImportFromFile(PathBuf),
    ImportFileFromPicker,
}

#[component]
pub fn BackupSettings() -> Element {
    let t = use_i18n();
    let state   = use_context::<AppState>();
    let status  = use_signal(|| String::new());
    let app_version = use_signal({
        let state = state.clone();
        move || state.with_conn(|conn| SettingsRepo::get_app_version(conn).ok().flatten()).unwrap_or_default()
    });
    let is_ok   = use_signal(|| true);
    let backups = use_signal(|| Vec::<PathBuf>::new());

    // PIN verification modal state
    let mut show_pin_modal = use_signal(|| false);
    let mut pending_action = use_signal(|| None::<PendingAction>);

    // ── Load backups on mount and when needed ──
    let mut load_backups_effect = backups.clone();
    use_effect(move || {
        load_backups_effect.set(list_backup_files());
    });

    // ── Pending action handlers ──
    let handle_export = {
        let state  = state.clone();
        let mut status = status.clone();
        let mut is_ok  = is_ok.clone();
        let mut backups = backups.clone();
        move || {
            let result = state.with_conn(|conn| export_backup(conn));
            match result {
                Ok(json) => {
                    let save_result = save_to_backup_folder(&json);
                    match save_result {
                        Ok(path) => {
                            status.set(format!("✅ {} {}", t.t("Exported to"), path.display()));
                            is_ok.set(true);
                            backups.set(list_backup_files());
                        }
                        Err(e) => {
                            status.set(format!("❌ {}: {e}", t.t("Save failed")));
                            is_ok.set(false);
                        }
                    }
                }
                Err(e) => {
                    status.set(format!("❌ {}: {e}", t.t("Export failed")));
                    is_ok.set(false);
                }
            }
        }
    };

    let handle_import = {
        let state = state.clone();
        let mut status = status.clone();
        let mut is_ok  = is_ok.clone();
        let mut backups = backups.clone();
        move |file_path: PathBuf| {
            let result = read_backup_file(&file_path).and_then(|json| state.with_conn(|conn| import_backup(conn, &json)));
            match result {
                Ok(stats) => {
                    status.set(format!("✅ {} {}: {}", t.t("Imported from"), file_path.display(), stats));
                    is_ok.set(true);
                    backups.set(list_backup_files());
                }
                Err(e) => {
                    status.set(format!("❌ {}: {e}", t.t("Import failed")));
                    is_ok.set(false);
                }
            }
        }
    };

    let handle_verified = {
        let mut show_pin_modal = show_pin_modal.clone();
        let mut pending_action = pending_action.clone();
        let mut handle_export = handle_export.clone();
        let mut handle_import = handle_import.clone();
        move |_| {
            if let Some(action) = pending_action.take() {
                match action {
                    PendingAction::Export => {
                        handle_export();
                    }
                    PendingAction::ImportFromFile(path) => {
                        handle_import(path);
                    }
                    PendingAction::ImportFileFromPicker => {
                        // We'll handle this in the JS eval part when we get there
                    }
                }
            }
            show_pin_modal.set(false);
        }
    };

    // ── Trigger PIN check then export ──
    let export = {
        let mut show_pin_modal = show_pin_modal.clone();
        let mut pending_action = pending_action.clone();
        move |_: MouseEvent| {
            pending_action.set(Some(PendingAction::Export));
            show_pin_modal.set(true);
        }
    };

    // ── Import from saved file ──
    let import = {
        let mut show_pin_modal = show_pin_modal.clone();
        let mut pending_action = pending_action.clone();
        move |file_path: PathBuf| {
            pending_action.set(Some(PendingAction::ImportFromFile(file_path)));
            show_pin_modal.set(true);
        }
    };

    // ── Share backup file ──
    let share = {
        let mut status = status.clone();
        move |file_path: PathBuf| {
            let result = read_backup_file(&file_path);
            match result {
                Ok(json) => {
                    let _ = share_backup(&json, &file_path);
                    status.set(format!("✅ {} {}", t.t("Sharing backup"), file_path.display()));
                }
                Err(e) => {
                    status.set(format!("❌ {}: {e}", t.t("Share failed")));
                }
            }
        }
    };

    // ── Delete backup file ──
    let delete = {
        let mut status = status.clone();
        let mut is_ok  = is_ok.clone();
        let mut backups = backups.clone();
        move |file_path: PathBuf| {
            let result = std::fs::remove_file(&file_path);
            match result {
                Ok(_) => {
                    status.set(format!("✅ {} {}", t.t("Deleted backup"), file_path.display()));
                    is_ok.set(true);
                    backups.set(list_backup_files());
                }
                Err(e) => {
                    status.set(format!("❌ {}: {e}", t.t("Delete failed")));
                    is_ok.set(false);
                }
            }
        }
    };

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:16px;",

            // ── App info ──
            div {
                style: "background:#fff; border-radius:16px; padding:16px;",
                div { style: "font-size:12px; color:{theme::TEXT_MUTED}; margin-bottom:4px;", "APP" }
                div { style: "font-size:15px; font-weight:600; color:{theme::TEXT};", "Family Finance" }
                div { style: "font-size:13px; color:{theme::TEXT_MUTED}; margin-top:2px;", {t.t("Version {version} · SQLite local storage").replace("{version}", &app_version.read())} }
            }

            // ── Export ──
            div {
                style: "background:#fff; border-radius:16px; padding:16px;",
                h2 { style: "margin:0 0 8px; font-size:14px; font-weight:600; color:{theme::TEXT};", {t.t("Export Backup")} }
                p  { style: "margin:0 0 12px; font-size:13px; color:{theme::TEXT_MUTED}; line-height:1.5;",
                    {t.t("Export all wallets, holdings, categories, transactions, members and families as a .bk file.")}
                }
                button {
                    style: "{theme::BTN_PRIMARY} width:100%;",
                    onclick: export,
                    span {
                        style: "display:inline-flex; align-items:center; justify-content:center;",
                        IconText { text: t.t("📤 Export .bk").to_string(), size: 18, color: "#ffffff".to_string(), gap: 8 }
                    }
                }
            }

            // ── Existing backups ──
            if !backups.read().is_empty() {
                div {
                    style: "background:#fff; border-radius:16px; padding:16px;",
                    h2 { style: "margin:0 0 8px; font-size:14px; font-weight:600; color:{theme::TEXT};", {t.t("Saved Backups")} }
                    div { style: "display:flex; flex-direction:column; gap:8px;",
                        for path in backups.read().iter() {
                            BackupFileItem {
                                path: path.clone(),
                                on_import: import.clone(),
                                on_share: share.clone(),
                                on_delete: delete.clone(),
                            }
                        }
                    }
                }
            }

            // ── Import from file ──
            div {
                style: "background:#fff; border-radius:16px; padding:16px;",
                h2 { style: "margin:0 0 8px; font-size:14px; font-weight:600; color:{theme::TEXT};", {t.t("Import from shared file")} }
                p  { style: "margin:0 0 12px; font-size:13px; color:{theme::DANGER}; line-height:1.5; background:{theme::NEGATIVE_SOFT}; border-radius:12px; padding:10px 12px;",
                    IconText { text: t.t("⚠️ Import will REPLACE all existing data. This cannot be undone. Select a shared backup file to import it.").to_string(), color: theme::DANGER.to_string(), gap: 8 }
                }
                label {
                    style: "{theme::BTN_PRIMARY} display:flex; align-items:center; justify-content:center; gap:8px; width:100%; box-sizing:border-box; text-align:center;",
                    IconText { text: t.t("📥 Import File").to_string(), size: 18, color: "#ffffff".to_string(), gap: 8 }
                    input {
                        r#type: "file",
                        accept: ".bk,.ffbackup",
                        style: "display:none;",
                        oninput: move |_e| {
                            let state = state.clone();
                            let status = status.clone();
                            let is_ok = is_ok.clone();
                            let backups = backups.clone();
                            let js = r#"
                                (function() {
                                    const input = document.querySelector('input[type="file"][accept=".bk,.ffbackup"]');
                                    if (input && input.files && input.files.length > 0) {
                                        const file = input.files[0];
                                        const reader = new FileReader();
                                        reader.onload = function(e) {
                                            const content = e.target.result;
                                            const filename = file.name;
                                            // Return both
                                            dioxus.send(JSON.stringify({ filename, content }));
                                        };
                                        reader.onerror = function() {
                                            dioxus.send("error");
                                        };
                                        reader.readAsText(file);
                                    }
                                })();
                            "#;
                            let mut eval = dioxus::document::eval(js);
                            let mut status_clone = status.clone();
                            let mut is_ok_clone = is_ok.clone();
                            let mut backups_clone = backups.clone();
                            let state_clone = state.clone();
                            let _ = dioxus::prelude::spawn(async move {
                                if let Ok(result) = eval.recv::<String>().await {
                                    if result == "error" {
                                        status_clone.set(t.t("❌ Failed to read file").to_string());
                                        is_ok_clone.set(false);
                                        return;
                                    }
                                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&result) {
                                        let filename = data["filename"].as_str().unwrap_or("imported.bk").to_string();
                                        let content = data["content"].as_str().unwrap_or("").to_string();
                                        let save_result = save_to_backup_folder_from_string(&content, &filename);
                                        match save_result {
                                            Ok(path) => {
                                                let import_result = state_clone.with_conn(|conn| import_backup(conn, &content));
                                                match import_result {
                                                    Ok(stats) => {
                                                        status_clone.set(format!("✅ {} {}: {}", t.t("Imported from file"), path.display(), stats));
                                                        is_ok_clone.set(true);
                                                        backups_clone.set(list_backup_files());
                                                    }
                                                    Err(e) => {
                                                        status_clone.set(format!("❌ {}: {e}", t.t("Import failed")));
                                                        is_ok_clone.set(false);
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                status_clone.set(format!("❌ {}: {e}", t.t("Save failed")));
                                                is_ok_clone.set(false);
                                            }
                                        }
                                    }
                                }
                            });
                        },
                    }
                }
            }

            // ── Status ──
            if !status.read().is_empty() {
                StatusBanner { text: status.read().clone() }
            }

            // ── Danger zone ──
            TransactionPhotos {}

            DangerZone {}

            // ── PIN Verify Modal ──
            PinVerifyModal {
                is_visible: show_pin_modal,
                on_cancel: move |_| {
                    pending_action.set(None);
                    show_pin_modal.set(false);
                },
                on_verify: handle_verified,
            }
        }
    }
}

#[component]
fn BackupFileItem(
    path: PathBuf,
    on_import: EventHandler<PathBuf>,
    on_share: EventHandler<PathBuf>,
    on_delete: EventHandler<PathBuf>,
) -> Element {
    let t = use_i18n();
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
    let path_clone_import = path.clone();
    let path_clone_share = path.clone();
    let path_clone_delete = path.clone();

    rsx! {
        div {
            style: "display:flex; align-items:center; justify-content:space-between; padding:10px 12px; background:#f7f7fc; border-radius:12px;",
            span {
                style: "font-size:13px; color:{theme::TEXT}; overflow-wrap:anywhere;",
                "{filename}"
            }
            div {
                style: "display:flex; gap:8px;",
                button {
                    style: "padding:6px 12px; border:none; border-radius:999px; background:{theme::NEGATIVE_SOFT}; color:{theme::DANGER}; font-size:12px; font-weight:600; cursor:pointer;",
                    onclick: move |_| on_import.call(path_clone_import.clone()),
                    {t.t("Revert")}
                }
                button {
                    style: "padding:6px 12px; border:none; border-radius:999px; background:{theme::PRIMARY}; color:#fff; font-size:12px; font-weight:600; cursor:pointer;",
                    onclick: move |_| on_share.call(path_clone_share.clone()),
                    Icon { name: IconName::Upload, size: 16, color: "#ffffff".to_string() }
                    {t.t("Share")}
                }
                button {
                    style: "padding:6px 12px; border:none; border-radius:999px; background:{theme::BG}; color:{theme::TEXT_MUTED}; font-size:12px; font-weight:600; cursor:pointer;",
                    onclick: move |_| on_delete.call(path_clone_delete.clone()),
                    Icon { name: IconName::Trash, size: 16, color: theme::TEXT_MUTED.to_string() }
                    {t.t("Delete")}
                }
            }
        }
    }
}

// ─── Danger zone (reset data) ─────────────────────────────────────────────────

/// How much space transaction photos take, and a button to delete them all
#[component]
fn TransactionPhotos() -> Element {
    let t = use_i18n();
    let state = use_context::<AppState>();
    let load = {
        let state = state.clone();
        move || state.with_conn(|conn| crate::repository::TxnImageRepo::stats(conn).unwrap_or((0, 0)))
    };
    let mut stats = use_signal({ let load = load.clone(); move || load() });
    let mut confirm = use_signal(|| false);
    let mut status  = use_signal(String::new);

    let (count, bytes) = stats();
    let size = format!("{:.1} MB", bytes as f64 / 1_048_576.0);

    rsx! {
        div {
            style: "background:#fff; border-radius:16px; padding:16px;",
            h2 { style: "margin:0 0 6px; font-size:14px; font-weight:600; color:{theme::TEXT};", {t.t("Transaction photos")} }
            p { style: "margin:0 0 12px; font-size:13px; color:{theme::TEXT_MUTED}; line-height:1.5;",
                {t.t("{count} photos · {size}. Deleting them makes the database smaller; the transactions stay.")
                    .replace("{count}", &count.to_string()).replace("{size}", &size)}
            }
            button {
                style: if count > 0 {
                    "{theme::BTN_DANGER} width:100%;"
                } else {
                    "{theme::BTN_SECONDARY} width:100%; opacity:.45; cursor:default;"
                },
                disabled: count == 0,
                onclick: move |_| confirm.set(true),
                span {
                    style: "display:inline-flex; align-items:center; justify-content:center;",
                    IconText { text: t.t("🗑 Clear all transaction photos").to_string(), size: 18, color: if count > 0 { theme::DANGER.to_string() } else { theme::PRIMARY.to_string() }, gap: 8 }
                }
            }
            if !status.read().is_empty() {
                div { style: "margin-top:10px;", StatusBanner { text: status.read().clone() } }
            }

            if confirm() {
                div {
                    style: "{theme::OVERLAY}",
                    div {
                        style: "{theme::DIALOG}",
                        h3 { style: "margin:0 0 8px; font-size:16px; font-weight:600; color:{theme::TEXT};", {t.t("Delete all transaction photos?")} }
                        p { style: "margin:0 0 18px; font-size:14px; color:{theme::TEXT_MUTED}; line-height:1.5;",
                            {t.t("{count} photos will be permanently deleted. Transactions and ID card photos are kept.").replace("{count}", &count.to_string())}
                        }
                        div { style: "display:flex; gap:10px;",
                            button {
                                style: "{theme::BTN_SECONDARY} flex:1;",
                                onclick: move |_| confirm.set(false),
                                {t.t("Cancel")}
                            }
                            button {
                                style: "{theme::BTN_DANGER} flex:1;",
                                onclick: {
                                    let state = state.clone();
                                    let load = load.clone();
                                    move |_| {
                                        confirm.set(false);
                                        match state.with_conn(crate::repository::TxnImageRepo::clear_all) {
                                            Ok(())  => status.set(t.t("✅ Transaction photos deleted").to_string()),
                                            Err(e)  => status.set(format!("❌ {}: {e}", t.t("Delete failed"))),
                                        }
                                        stats.set(load());
                                    }
                                },
                                {t.t("Delete")}
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DangerZone() -> Element {
    let t = use_i18n();
    let state   = use_context::<AppState>();
    let mut show_pin_modal = use_signal(|| false);
    let mut confirmed = use_signal(|| false);
    let mut status = use_signal(String::new);

    let handle_delete = {
        let state = state.clone();
        move || {
            let result = state.with_conn(|conn| {
                delete_all_data(conn)?;
                // Cancel the notifications of the reminders just deleted
                crate::services::ReminderService::resync(conn)
            });
            match result {
                Ok(())  => status.set(t.t("✅ All data deleted").to_string()),
                Err(e)  => status.set(format!("❌ {}: {e}", t.t("Delete failed"))),
            }
        }
    };

    let handle_verified = {
        let mut show_pin_modal = show_pin_modal.clone();
        let mut handle_delete = handle_delete.clone();
        move |_| {
            handle_delete();
            show_pin_modal.set(false);
        }
    };

    rsx! {
        div {
            style: "background:#fff; border-radius:16px; padding:16px; border:1px solid #fde3ea;",
            h2 {
                style: "margin:0 0 8px; font-size:14px; font-weight:600; color:{theme::DANGER}; display:flex; align-items:center; gap:8px;",
                IconText { text: t.t("⚠️ Danger Zone").to_string(), color: theme::DANGER.to_string(), gap: 8 }
            }
            p  { style: "margin:0 0 12px; font-size:13px; color:#8a8d9f;", {t.t("Permanently delete all data. Cannot be undone.")} }

            button {
                style: "{theme::BTN_DANGER} width:100%;",
                onclick: move |_| {
                    confirmed.set(true);
                    show_pin_modal.set(true);
                },
                {t.t("Delete All Data")}
            }

            if !status.read().is_empty() {
                div { style: "margin-top:10px;", StatusBanner { text: status.read().clone() } }
            }

            PinVerifyModal {
                is_visible: show_pin_modal,
                on_cancel: move |_| {
                    confirmed.set(false);
                    show_pin_modal.set(false);
                },
                on_verify: handle_verified,
            }
        }
    }
}

// ─── Backup logic ─────────────────────────────────────────────────────────────

/// Wipe all user data in one transaction. Rows that reference others go first
/// (transactions/reminders → wallets, ID photos → members), otherwise SQLite's
/// foreign keys reject deleting the wallets and members.
fn delete_all_data(conn: &rusqlite::Connection) -> crate::error::Result<()> {
    conn.execute_batch("
        BEGIN IMMEDIATE;
        DELETE FROM transaction_images;
        DELETE FROM transactions;
        DELETE FROM reminders;
        DELETE FROM bills;
        DELETE FROM holdings;
        DELETE FROM member_id_images;
        DELETE FROM members;
        DELETE FROM families;
        DELETE FROM wallet_balances;
        DELETE FROM wallets;
        DELETE FROM categories WHERE is_system = 0;
        COMMIT;
    ").map_err(|e| {
        let _ = conn.execute_batch("ROLLBACK");
        e.into()
    })
}

fn export_backup(conn: &rusqlite::Connection) -> crate::error::Result<String> {
    let wallets      = WalletRepo::list_active(conn)?;
    let holdings     = HoldingRepo::list_all_active(conn)?;
    let categories   = CategoryRepo::list(conn)?;
    let transactions = TransactionRepo::list_all(conn)?;
    let members      = load_members(conn);
    let families     = FamilyRepo::list(conn)?;
    let id_card_images = IdCardRepo::list_all(conn)?;
    let currencies   = CurrencyRepo::list(conn)?;

    let data = BackupData {
        app_version:  SettingsRepo::get_app_version(conn)?.unwrap_or_default(),
        exported_at:  crate::views::now_ts(),
        wallets,
        holdings,
        categories,
        transactions,
        members,
        families,
        id_card_images,
        currencies,
    };

    Ok(serde_json::to_string_pretty(&data)?)
}

/// Parse a backup; wallets from before multi-currency wallets carry one `currency` + `balance`
/// instead of `balances`, so those are converted first.
fn parse_backup(json: &str) -> crate::error::Result<BackupData> {
    let mut value: serde_json::Value = serde_json::from_str(json)?;
    if let Some(wallets) = value.get_mut("wallets").and_then(|w| w.as_array_mut()) {
        for w in wallets {
            let Some(obj) = w.as_object_mut() else { continue };
            if obj.contains_key("balances") { continue; }
            if let (Some(currency), Some(balance)) = (obj.get("currency").cloned(), obj.get("balance").cloned()) {
                obj.insert("balances".into(), serde_json::json!([{ "currency": currency, "balance": balance }]));
            }
        }
    }
    Ok(serde_json::from_value(value)?)
}

fn import_backup(conn: &rusqlite::Connection, json: &str) -> crate::error::Result<String> {
    let data = parse_backup(json)?;

    conn.execute_batch("BEGIN IMMEDIATE")?;

    let result = (|| -> crate::error::Result<()> {
        // Clear existing data
        conn.execute_batch("
            DELETE FROM transaction_images;
            DELETE FROM transactions;
            DELETE FROM holdings;
            DELETE FROM wallet_balances;
            DELETE FROM wallets;
            DELETE FROM categories WHERE is_system = 0;
            DELETE FROM member_id_images;
            DELETE FROM members;
            DELETE FROM families;
        ")?;

        // Insert families first because members have foreign key
        for f in &data.families {
            conn.execute(
                "INSERT OR REPLACE INTO families
                 (id,name,common_address,note,created_at)
                 VALUES (?1,?2,?3,?4,?5)",
                rusqlite::params![
                    f.id, f.name, f.common_address, f.note, f.created_at,
                ],
            )?;
        }

        // Insert wallets
        for w in &data.wallets {
            conn.execute(
                "INSERT OR REPLACE INTO wallets
                 (id,name,wallet_type,broker,icon,color,is_active,sort_order,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                rusqlite::params![
                    w.id, w.name, w.wallet_type.to_string(),
                    w.broker, w.icon, w.color,
                    w.is_active as i32, w.sort_order, w.created_at,
                ],
            )?;
            for b in &w.balances {
                conn.execute(
                    "INSERT OR REPLACE INTO wallet_balances (wallet_id,currency,balance) VALUES (?1,?2,?3)",
                    rusqlite::params![w.id, b.currency.to_string(), crate::models::decimal_to_f64(b.balance)],
                )?;
            }
        }

        // Money types: the rows always exist (seeded), only their details are restored
        for c in &data.currencies {
            conn.execute(
                "UPDATE currencies SET name = ?1, unit = ?2, is_active = ?3, sort_order = ?4 WHERE code = ?5",
                rusqlite::params![c.name, c.unit, c.is_active as i32, c.sort_order, c.code.to_string()],
            )?;
        }

        // Insert holdings
        for h in &data.holdings {
            conn.execute(
                "INSERT OR REPLACE INTO holdings
                 (id,wallet_id,symbol,name,asset_type,quantity,avg_buy_price,last_price,last_price_at,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                rusqlite::params![
                    h.id, h.wallet_id, h.symbol, h.name, h.asset_type.to_string(),
                    h.quantity_f64(), h.avg_buy_price_f64(),
                    h.last_price.map(|p| crate::models::decimal_to_f64(p)),
                    h.last_price_at, h.created_at,
                ],
            )?;
        }

        // Insert categories (skip system — already seeded)
        for c in data.categories.iter().filter(|c| !c.is_system) {
            conn.execute(
                "INSERT OR REPLACE INTO categories
                 (id,name,icon,color,budget_amount,parent_id,sort_order,is_system,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,0,?8)",
                rusqlite::params![
                    c.id, c.name, c.icon, c.color,
                    c.budget_f64(), c.parent_id, c.sort_order, c.created_at,
                ],
            )?;
        }

        // Insert transactions (raw — no balance recalculation)
        for t in &data.transactions {
            conn.execute(
                "INSERT OR REPLACE INTO transactions
                 (id,txn_type,wallet_id,amount,currency,income_type,category_id,
                  to_wallet_id,to_amount,to_currency,holding_id,asset_quantity,asset_price,
                  note,txn_date,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                rusqlite::params![
                    t.id, t.txn_type.to_string(), t.wallet_id, t.amount_f64(),
                    t.currency.to_string(),
                    t.income_type.as_ref().map(|i| i.to_string()),
                    t.category_id,
                    t.to_wallet_id,
                    t.to_amount.map(crate::models::decimal_to_f64),
                    t.to_currency.as_ref().map(|c| c.to_string()),
                    t.holding_id,
                    t.asset_quantity.map(crate::models::decimal_to_f64),
                    t.asset_price.map(crate::models::decimal_to_f64),
                    t.note, t.txn_date, t.created_at,
                ],
            )?;
        }

        // Insert members (now includes family_id)
        for m in &data.members {
            conn.execute(
                "INSERT OR REPLACE INTO members
                 (id,family_id,full_name,birth_date,gender,phone,id_number,id_issue_date,id_issue_place,address,role,avatar_emoji,note,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                rusqlite::params![
                    m.id, m.family_id, m.full_name, m.birth_date, m.gender.as_ref().map(|g| g.to_string()).unwrap_or_default(), m.phone,
                    m.id_number, m.id_issue_date, m.id_issue_place, m.address, m.role.to_string(), m.avatar_emoji,
                    m.note, m.created_at,
                ],
            )?;
        }

        for img in &data.id_card_images {
            conn.execute(
                "INSERT OR REPLACE INTO member_id_images (member_id,side,jpeg,updated_at) VALUES (?1,?2,?3,?4)",
                rusqlite::params![img.member_id, img.side.to_string(), img.jpeg, img.updated_at],
            )?;
        }

        Ok(())
    })();

    match result {
        Ok(_) => {
            conn.execute_batch("COMMIT")?;
            CurrencyRepo::refresh_cache(conn)?;
            Ok(format!(
                "{} families, {} wallets, {} holdings, {} categories, {} transactions, {} members, {} ID photos",
                data.families.len(), data.wallets.len(), data.holdings.len(),
                data.categories.len(), data.transactions.len(), data.members.len(),
                data.id_card_images.len(),
            ))
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

fn load_members(conn: &rusqlite::Connection) -> Vec<Member> {
    use crate::models::Member;
    let mut stmt = match conn.prepare(
        "SELECT id,family_id,full_name,birth_date,gender,phone,id_number,id_issue_date,id_issue_place,address,role,avatar_emoji,note,created_at FROM members"
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map([], Member::from_row)
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

fn ensure_backup_folder() -> std::io::Result<PathBuf> {
    let backup_dir = PathBuf::from("backup");
    if !backup_dir.exists() {
        std::fs::create_dir_all(&backup_dir)?;
    }
    Ok(backup_dir)
}

fn list_backup_files() -> Vec<PathBuf> {
    let backup_dir = match ensure_backup_folder() {
        Ok(d) => d,
        Err(_) => return vec![],
    };

    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&backup_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("bk") {
                files.push(path);
            }
        }
    }

    // Sort by modified time descending
    files.sort_by(|a, b| {
        let a_time = a.metadata().and_then(|m| m.modified()).ok();
        let b_time = b.metadata().and_then(|m| m.modified()).ok();
        b_time.cmp(&a_time)
    });

    files
}

fn save_to_backup_folder(json: &str) -> crate::error::Result<PathBuf> {
    use std::io::Write;
    use chrono::{Local, TimeZone};
    let backup_dir = ensure_backup_folder()?;
    let ts = crate::views::now_ts();
    let datetime = Local.timestamp_opt(ts, 0).single().unwrap_or_else(|| Local::now());
    let filename = format!("{}.bk", datetime.format("%Y_%m_%d_%H_%M_%S"));
    let path = backup_dir.join(filename);
    let mut f = std::fs::File::create(&path)?;
    f.write_all(json.as_bytes())?;
    Ok(path)
}

fn save_to_backup_folder_from_string(json: &str, original_filename: &str) -> crate::error::Result<PathBuf> {
    use std::io::Write;
    let backup_dir = ensure_backup_folder()?;
    let mut path = backup_dir.join(original_filename);
    let mut counter = 1;
    while path.exists() {
        let stem = std::path::Path::new(original_filename).file_stem().and_then(|s| s.to_str()).unwrap_or("imported");
        let ext = std::path::Path::new(original_filename).extension().and_then(|s| s.to_str()).unwrap_or("bk");
        path = backup_dir.join(format!("{}_{}.{}", stem, counter, ext));
        counter += 1;
    }
    let mut f = std::fs::File::create(&path)?;
    f.write_all(json.as_bytes())?;
    Ok(path)
}

fn read_backup_file(path: &PathBuf) -> crate::error::Result<String> {
    Ok(std::fs::read_to_string(path)?)
}

fn share_backup(json: &str, path: &PathBuf) -> crate::error::Result<()> {
    // On web/desktop: try to download or copy to clipboard
    // On mobile: use native share sheet
    // For now, we'll just copy to clipboard and log
    let js = format!(r#"
        async function shareBackup() {{
            try {{
                // Try to copy to clipboard first
                await navigator.clipboard.writeText('{}');
                // Try to use Web Share API if available
                if (navigator.share) {{
                    const file = new File([`{}`], '{}', {{type: 'application/json'}});
                    await navigator.share({{
                        title: 'Family Finance Backup',
                        text: 'Family Finance Backup File',
                        files: [file]
                    }});
                }}
            }} catch (e) {{
                console.log('Sharing failed:', e);
            }}
        }}
        shareBackup();
    "#, json.replace('\\', "\\\\").replace('\'', "\\'"), json.replace('\\', "\\\\").replace('\'', "\\'"), path.file_name().unwrap_or_default().to_str().unwrap_or_default());
    
    let _ = dioxus::document::eval(&js);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_icon_strips_leading_emoji() {
        assert_eq!(emoji_icon("✅ Done").map(|(_, r)| r), Some("Done"));
        assert_eq!(emoji_icon("❌ Failed: x").map(|(_, r)| r), Some("Failed: x"));
        assert_eq!(emoji_icon("⚠️ Danger Zone").map(|(_, r)| r), Some("Danger Zone"));
        assert_eq!(emoji_icon("📤 Export .bk").map(|(_, r)| r), Some("Export .bk"));
        assert_eq!(emoji_icon("plain"), None);
    }

    use crate::models::{Currency, NewReminder, NewWallet, NewTransaction, RepeatUnit, TransactionType, WalletType};
    use crate::repository::{ReminderRepo, WalletRepo};
    use crate::services::TransactionService;
    use rust_decimal::Decimal;

    fn count(conn: &rusqlite::Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn delete_all_clears_wallets_even_with_reminders() {
        for foreign_keys in ["ON", "OFF"] {
            let conn = rusqlite::Connection::open_in_memory().unwrap();
            crate::db::migrations::run(&conn).unwrap();
            conn.execute_batch(&format!("PRAGMA foreign_keys = {foreign_keys}")).unwrap();

            let wallet = WalletRepo::create(&conn, &NewWallet {
                name: "Cash".into(), wallet_type: WalletType::Cash, broker: None, icon: None, color: None, sort_order: 0,
            }).unwrap();
            TransactionService::execute(&conn,
                &NewTransaction::opening_balance(&wallet.id, Decimal::from(5_000), Currency::VND, 1)).unwrap();
            ReminderRepo::create(&conn, &NewReminder {
                txn_type: TransactionType::Income, wallet_id: wallet.id.clone(), to_wallet_id: None,
                amount: Decimal::from(1_000), currency: Currency::VND, note: None,
                start_date: 1, repeat_unit: RepeatUnit::Month, repeat_every: 1,
            }).unwrap();

            delete_all_data(&conn).unwrap();
            for table in ["wallets", "wallet_balances", "transactions", "transaction_images", "reminders", "bills", "holdings", "members", "families", "member_id_images"] {
                assert_eq!(count(&conn, table), 0, "{table} not empty (foreign_keys {foreign_keys})");
            }
        }
    }

    #[test]
    fn export_then_import_keeps_every_money_type_of_a_wallet() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let wallet = WalletRepo::create(&conn, &NewWallet {
            name: "Cash".into(), wallet_type: WalletType::Cash, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        for (amount, currency) in [(5_000, Currency::VND), (3, Currency::GOLD)] {
            TransactionService::execute(&conn,
                &NewTransaction::opening_balance(&wallet.id, Decimal::from(amount), currency, 1)).unwrap();
        }

        let json = export_backup(&conn).unwrap();
        import_backup(&conn, &json).unwrap();

        let w = WalletRepo::find_by_id(&conn, &wallet.id).unwrap().unwrap();
        assert_eq!(w.balance(&Currency::VND), Decimal::from(5_000));
        assert_eq!(w.balance(&Currency::GOLD), Decimal::from(3));
        assert_eq!(w.balance(&Currency::USD), Decimal::ZERO);
    }

    #[test]
    fn legacy_backup_wallet_balance_is_restored() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let json = r#"{
            "app_version": "0.1.0", "exported_at": 1,
            "wallets": [{
                "id": "w1", "name": "HSBC", "wallet_type": "bank", "currency": "USD", "balance": 12.5,
                "broker": null, "icon": null, "color": null, "is_active": true, "sort_order": 0, "created_at": 1
            }],
            "holdings": [], "categories": [], "transactions": [], "members": [], "families": []
        }"#;

        import_backup(&conn, json).unwrap();

        let w = WalletRepo::find_by_id(&conn, "w1").unwrap().unwrap();
        assert_eq!(w.balance(&Currency::USD), Decimal::try_from(12.5).unwrap());
    }
}
