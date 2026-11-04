use dioxus::prelude::*;
use crate::i18n::use_i18n;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::app::AppState;
use crate::models::{Category, CreditCard, CurrencyInfo, Family, Holding, IdCardImage, Member, Transaction, Wallet};
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
            style: "display:flex; align-items:flex-start; gap:8px; background:{bg}; color:{fg}; border-radius:0; padding:10px 12px; font-size:13px; overflow-wrap:anywhere;",
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
    /// Credit card statement settings; missing in backups made before they existed
    #[serde(default)]
    pub credit_cards: Vec<CreditCard>,
    /// Recurring reminders; missing in older backups
    #[serde(default)]
    pub reminders: Vec<ReminderRow>,
    /// Monthly bills; missing in older backups
    #[serde(default)]
    pub bills: Vec<BillRow>,
    /// Receipt photos attached to transactions; missing in older backups
    #[serde(default)]
    pub transaction_images: Vec<TxnImageRow>,
    /// Whitelisted user settings (see `BACKUP_SETTING_KEYS`); missing in older backups
    #[serde(default)]
    pub settings: Vec<SettingRow>,
}

/// Settings that travel with a backup. Never add `pin_hash` here.
const BACKUP_SETTING_KEYS: [&str; 3] = ["language", "reminder_interval_hours", "reminder_start_time"];

/// Row of the `reminders` table, stored as-is
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReminderRow {
    pub id:           String,
    pub txn_type:     String,
    pub wallet_id:    String,
    pub to_wallet_id: Option<String>,
    pub amount:       f64,
    pub currency:     String,
    pub note:         Option<String>,
    pub start_date:   i64,
    pub repeat_unit:  String,
    pub repeat_every: i64,
    pub created_at:   i64,
    pub done_on:      Option<String>,
    pub is_paused:    i64,
}

/// Row of the `bills` table, stored as-is
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BillRow {
    pub id:         String,
    pub name:       String,
    pub amount:     Option<f64>,
    pub due_day:    i64,
    pub paid_month: Option<String>,
    pub is_paused:  i64,
    pub created_at: i64,
}

/// Row of the `transaction_images` table; the JPEG is base64 in the JSON
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TxnImageRow {
    pub id:             String,
    pub transaction_id: String,
    #[serde(with = "crate::models::id_card::base64_bytes")]
    pub jpeg:           Vec<u8>,
    pub created_at:     i64,
}

/// Row of the `settings` table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingRow {
    pub key:   String,
    pub value: String,
}

/// Dioxus desktop/mobile swaps clicks on file inputs for its own native dialog (app storage only).
/// Stop the click in the capture phase so the WebView's own picker opens (any folder), and send the
/// chosen file back as `{name, content}` (content = null when it couldn't be read).
const PICK_BACKUP_JS: &str = r#"
    const old = window.__ffBackupPick;
    if (old) { document.removeEventListener('click', old.click, true); document.removeEventListener('change', old.change, true); }
    const ours = t => t instanceof HTMLInputElement && t.id === 'ff-backup-import';
    const click = e => { if (ours(e.target)) e.stopPropagation(); };
    const change = e => {
        const input = e.target;
        if (!ours(input)) return;
        e.stopPropagation();
        const file = input.files && input.files[0];
        input.value = '';          // picking the same file again fires change again
        if (!file) return;
        const reader = new FileReader();
        reader.onload = () => dioxus.send({ name: file.name, content: String(reader.result || '') });
        reader.onerror = () => dioxus.send({ name: file.name, content: null });
        reader.readAsText(file);
    };
    document.addEventListener('click', click, true);
    document.addEventListener('change', change, true);
    window.__ffBackupPick = { click, change };
"#;

/// What `PICK_BACKUP_JS` sends for a chosen file
#[derive(serde::Deserialize)]
struct PickedBackup {
    name:    String,
    content: Option<String>,
}

// ─── BackupSettings view ─────────────────────────────────────────────────────

// ─── Action to execute after PIN verification ─────────────────────────────────
#[derive(Clone, PartialEq)]
enum PendingAction {
    Export,
    ImportFromFile(PathBuf),
    /// A file chosen with the system picker (name + text content)
    ImportFromContent { name: String, content: String },
}

#[component]
pub fn BackupSettings() -> Element {
    let t = use_i18n();

    let state   = use_context::<AppState>();
    let mut status  = use_signal(|| String::new());
    let app_version = use_signal({
        let state = state.clone();
        move || state.with_conn(|conn| SettingsRepo::get_app_version(conn).ok().flatten()).unwrap_or_default()
    });
    let mut is_ok   = use_signal(|| true);
    let backups = use_signal(|| Vec::<PathBuf>::new());

    // PIN verification modal state
    let mut show_pin_modal = use_signal(|| false);
    let mut pending_action = use_signal(|| None::<PendingAction>);

    // Picked backup file: read in JS, then confirmed with the PIN like the other actions
    use_hook(move || {
        spawn(async move {
            let mut eval = document::eval(PICK_BACKUP_JS);
            while let Ok(msg) = eval.recv::<PickedBackup>().await {
                match msg.content {
                    Some(content) => {
                        pending_action.set(Some(PendingAction::ImportFromContent { name: msg.name, content }));
                        show_pin_modal.set(true);
                    }
                    None => {
                        status.set(t.t("❌ Failed to read file").to_string());
                        is_ok.set(false);
                    }
                }
            }
        });
    });

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
                    apply_imported_language(&state, t);
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

    let handle_import_content = {
        let state = state.clone();
        let mut status = status.clone();
        let mut is_ok  = is_ok.clone();
        let mut backups = backups.clone();
        move |name: String, content: String| {
            match state.with_conn(|conn| import_backup(conn, &content)) {
                Ok(stats) => {
                    apply_imported_language(&state, t);
                    // Best effort: keep a copy in the app's backup list (the import already succeeded)
                    let _ = save_to_backup_folder_from_string(&content, &name);
                    backups.set(list_backup_files());
                    status.set(format!("✅ {} {}: {}", t.t("Imported from file"), name, stats));
                    is_ok.set(true);
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
        let mut handle_import_content = handle_import_content.clone();
        move |_| {
            if let Some(action) = pending_action.take() {
                match action {
                    PendingAction::Export => {
                        handle_export();
                    }
                    PendingAction::ImportFromFile(path) => {
                        handle_import(path);
                    }
                    PendingAction::ImportFromContent { name, content } => {
                        handle_import_content(name, content);
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
            match crate::views::share::share_file(&file_path, "application/octet-stream", t.t("Share backup")) {
                Ok(()) => {
                    status.set(format!("✅ {}", t.t("Choose an app to share the backup")));
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
                style: "background:#fff; border-radius:0; padding:16px;",
                div { style: "font-size:12px; color:{theme::TEXT_MUTED}; margin-bottom:4px;", "APP" }
                div { style: "font-size:15px; font-weight:600; color:{theme::TEXT};", "Family Money" }
                div { style: "font-size:13px; color:{theme::TEXT_MUTED}; margin-top:2px;", {t.t("Version {version} · SQLite local storage").replace("{version}", &app_version.read())} }
            }

            // ── Export ──
            div {
                style: "background:#fff; border-radius:0; padding:16px;",
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
                    style: "background:#fff; border-radius:0; padding:16px;",
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
                style: "background:#fff; border-radius:0; padding:16px;",
                h2 { style: "margin:0 0 8px; font-size:14px; font-weight:600; color:{theme::TEXT};", {t.t("Import from file")} }
                p  { style: "margin:0 0 12px; font-size:13px; color:{theme::DANGER}; line-height:1.5; background:{theme::NEGATIVE_SOFT}; border-radius:0; padding:10px 12px;",
                    IconText { text: t.t("⚠️ Import will REPLACE all existing data. This cannot be undone. Pick a backup file (.bk) to import it.").to_string(), color: theme::DANGER.to_string(), gap: 8 }
                }
                label {
                    style: "{theme::BTN_PRIMARY} display:flex; align-items:center; justify-content:center; gap:8px; width:100%; box-sizing:border-box; text-align:center;",
                    IconText { text: t.t("📥 Import File").to_string(), size: 18, color: "#ffffff".to_string(), gap: 8 }
                    input {
                        id: "ff-backup-import",
                        r#type: "file",
                        style: "display:none;",
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
            style: "display:flex; align-items:center; justify-content:space-between; padding:10px 12px; background:#f7f7fc; border-radius:0;",
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
            style: "background:#fff; border-radius:0; padding:16px;",
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
            style: "background:#fff; border-radius:0; padding:16px; border:1px solid #fde3ea;",
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
        DELETE FROM credit_cards;
        DELETE FROM wallets;
        DELETE FROM categories WHERE is_system = 0;
        COMMIT;
    ").map_err(|e| {
        let _ = conn.execute_batch("ROLLBACK");
        e.into()
    })
}

fn export_backup(conn: &rusqlite::Connection) -> crate::error::Result<String> {
    let wallets      = WalletRepo::list_all(conn)?;
    let holdings     = HoldingRepo::list_all(conn)?;
    let categories   = CategoryRepo::list(conn)?;
    let transactions = TransactionRepo::list_all(conn)?;
    let members      = load_members(conn);
    let families     = FamilyRepo::list(conn)?;
    let id_card_images = IdCardRepo::list_all(conn)?;
    let currencies   = CurrencyRepo::list(conn)?;
    let credit_cards = {
        let mut stmt = conn.prepare("SELECT * FROM credit_cards")?;
        let rows = stmt.query_map([], CreditCard::from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let reminders = {
        let mut stmt = conn.prepare(
            "SELECT id,txn_type,wallet_id,to_wallet_id,amount,currency,note,start_date,repeat_unit,repeat_every,created_at,done_on,is_paused FROM reminders")?;
        let rows = stmt.query_map([], |r| Ok(ReminderRow {
            id: r.get(0)?, txn_type: r.get(1)?, wallet_id: r.get(2)?, to_wallet_id: r.get(3)?,
            amount: r.get(4)?, currency: r.get(5)?, note: r.get(6)?, start_date: r.get(7)?,
            repeat_unit: r.get(8)?, repeat_every: r.get(9)?, created_at: r.get(10)?,
            done_on: r.get(11)?, is_paused: r.get(12)?,
        }))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let bills = {
        let mut stmt = conn.prepare("SELECT id,name,amount,due_day,paid_month,is_paused,created_at FROM bills")?;
        let rows = stmt.query_map([], |r| Ok(BillRow {
            id: r.get(0)?, name: r.get(1)?, amount: r.get(2)?, due_day: r.get(3)?,
            paid_month: r.get(4)?, is_paused: r.get(5)?, created_at: r.get(6)?,
        }))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let transaction_images = {
        let mut stmt = conn.prepare("SELECT id,transaction_id,jpeg,created_at FROM transaction_images")?;
        let rows = stmt.query_map([], |r| Ok(TxnImageRow {
            id: r.get(0)?, transaction_id: r.get(1)?, jpeg: r.get(2)?, created_at: r.get(3)?,
        }))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let settings = {
        use rusqlite::OptionalExtension;
        let mut out = Vec::new();
        for key in BACKUP_SETTING_KEYS {
            let value: Option<String> = conn
                .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
                .optional()?;
            if let Some(value) = value { out.push(SettingRow { key: key.to_string(), value }); }
        }
        out
    };

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
        credit_cards,
        reminders,
        bills,
        transaction_images,
        settings,
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

/// Apply the imported language right away (the backup restores the language setting)
fn apply_imported_language(state: &AppState, mut t: crate::i18n::I18n) {
    if let Some(code) = state.with_conn(|conn| SettingsRepo::get_language(conn).ok().flatten()) {
        t.set_lang(crate::i18n::Lang::from_code(&code));
    }
}

fn import_backup(conn: &rusqlite::Connection, json: &str) -> crate::error::Result<String> {
    let data = parse_backup(json)?;

    conn.execute_batch("BEGIN IMMEDIATE")?;

    let result = (|| -> crate::error::Result<()> {
        // Clear existing data
        conn.execute_batch("
            DELETE FROM transaction_images;
            DELETE FROM reminders;
            DELETE FROM bills;
            DELETE FROM transactions;
            DELETE FROM holdings;
            DELETE FROM wallet_balances;
            DELETE FROM credit_cards;
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

        // Reminders (after the wallets they reference)
        for r in &data.reminders {
            conn.execute(
                "INSERT OR REPLACE INTO reminders
                 (id,txn_type,wallet_id,to_wallet_id,amount,currency,note,start_date,repeat_unit,repeat_every,created_at,done_on,is_paused)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                rusqlite::params![
                    r.id, r.txn_type, r.wallet_id, r.to_wallet_id, r.amount, r.currency, r.note,
                    r.start_date, r.repeat_unit, r.repeat_every, r.created_at, r.done_on, r.is_paused,
                ],
            )?;
        }

        for b in &data.bills {
            conn.execute(
                "INSERT OR REPLACE INTO bills (id,name,amount,due_day,paid_month,is_paused,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                rusqlite::params![b.id, b.name, b.amount, b.due_day, b.paid_month, b.is_paused, b.created_at],
            )?;
        }

        // Only whitelisted settings; anything else in the file (e.g. pin_hash) is ignored
        for s in data.settings.iter().filter(|s| BACKUP_SETTING_KEYS.contains(&s.key.as_str())) {
            conn.execute(
                "INSERT OR REPLACE INTO settings (key,value) VALUES (?1,?2)",
                rusqlite::params![s.key, s.value],
            )?;
        }

        // Credit card settings (after the wallets they reference)
        for c in &data.credit_cards {
            conn.execute(
                "INSERT OR REPLACE INTO credit_cards
                 (wallet_id,statement_day,due_day,credit_limit,paid_month,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                rusqlite::params![
                    c.wallet_id, c.statement_day, c.due_day,
                    c.credit_limit.map(crate::models::decimal_to_f64),
                    c.paid_month, c.created_at,
                ],
            )?;
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

        // Receipt photos (after the transactions they reference)
        for img in &data.transaction_images {
            conn.execute(
                "INSERT OR REPLACE INTO transaction_images (id,transaction_id,jpeg,created_at) VALUES (?1,?2,?3,?4)",
                rusqlite::params![img.id, img.transaction_id, img.jpeg, img.created_at],
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
            // Notifications follow the imported reminders/bills/cards; never fail an import over them
            let _ = crate::services::ReminderService::resync(conn);
            Ok(format!(
                "{} families, {} wallets, {} holdings, {} categories, {} transactions, {} members, {} ID photos, {} reminders, {} bills, {} transaction photos",
                data.families.len(), data.wallets.len(), data.holdings.len(),
                data.categories.len(), data.transactions.len(), data.members.len(),
                data.id_card_images.len(),
                data.reminders.len(), data.bills.len(), data.transaction_images.len(),
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
    // Android's cwd is read-only: app_files_dir() is the app's private files dir there
    let backup_dir = crate::views::native::app_files_dir()?.join("backup");
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

/// Backup file name: `YYYYMMDD_HHmmss.bk`.
fn backup_file_name(dt: chrono::NaiveDateTime) -> String {
    format!("{}.bk", dt.format("%Y%m%d_%H%M%S"))
}

fn save_to_backup_folder(json: &str) -> crate::error::Result<PathBuf> {
    use std::io::Write;
    use chrono::{Local, TimeZone};
    let backup_dir = ensure_backup_folder()?;
    let ts = crate::views::now_ts();
    let datetime = Local.timestamp_opt(ts, 0).single().unwrap_or_else(|| Local::now());
    let name = backup_file_name(datetime.naive_local());
    let mut path = backup_dir.join(&name);
    // Second precision can collide: never overwrite an existing backup
    let stem = name.trim_end_matches(".bk").to_string();
    let mut counter = 2;
    while path.exists() {
        path = backup_dir.join(format!("{stem}_{counter}.bk"));
        counter += 1;
    }
    let mut f = std::fs::File::create(&path)?;
    f.write_all(json.as_bytes())?;
    Ok(path)
}

fn save_to_backup_folder_from_string(json: &str, original_filename: &str) -> crate::error::Result<PathBuf> {
    use std::io::Write;
    let backup_dir = ensure_backup_folder()?;
    // Keep only the file name (no folder escape) and make sure it ends in .bk so the list shows it
    let mut name = std::path::Path::new(original_filename)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("imported.bk")
        .to_string();
    if std::path::Path::new(&name).extension().and_then(|e| e.to_str()) != Some("bk") {
        name.push_str(".bk");
    }
    let mut path = backup_dir.join(&name);
    let mut counter = 1;
    while path.exists() {
        let stem = std::path::Path::new(&name).file_stem().and_then(|s| s.to_str()).unwrap_or("imported");
        path = backup_dir.join(format!("{}_{}.bk", stem, counter));
        counter += 1;
    }
    let mut f = std::fs::File::create(&path)?;
    f.write_all(json.as_bytes())?;
    Ok(path)
}

fn read_backup_file(path: &PathBuf) -> crate::error::Result<String> {
    Ok(std::fs::read_to_string(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_file_name_format() {
        let dt = chrono::NaiveDate::from_ymd_opt(2026, 11, 3).unwrap().and_hms_opt(9, 5, 7).unwrap();
        assert_eq!(backup_file_name(dt), "20261103_090507.bk");
    }

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

            crate::repository::CreditCardRepo::upsert(&conn, &wallet.id, 5, 25, None).unwrap();

            delete_all_data(&conn).unwrap();
            for table in ["wallets", "wallet_balances", "transactions", "transaction_images", "reminders", "bills", "credit_cards", "holdings", "members", "families", "member_id_images"] {
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
    fn export_then_import_keeps_credit_card_settings() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let wallet = WalletRepo::create(&conn, &NewWallet {
            name: "Visa".into(), wallet_type: WalletType::CreditCard, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        crate::repository::CreditCardRepo::upsert(&conn, &wallet.id, 10, 28, Some(Decimal::from(20_000_000))).unwrap();
        crate::repository::CreditCardRepo::set_paid_month(&conn, &wallet.id, Some("2026-10")).unwrap();
        let before = crate::repository::CreditCardRepo::get(&conn, &wallet.id).unwrap().unwrap();

        let json = export_backup(&conn).unwrap();
        import_backup(&conn, &json).unwrap();

        let after = crate::repository::CreditCardRepo::get(&conn, &wallet.id).unwrap().unwrap();
        assert_eq!(after, before);
        assert_eq!(after.paid_month.as_deref(), Some("2026-10"));
    }

    #[test]
    fn transaction_on_a_removed_wallet_survives_export_import() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        crate::repository::CategoryRepo::seed_defaults(&conn).unwrap();
        let category = crate::repository::CategoryRepo::list(&conn).unwrap()[0].id.clone();
        let wallet = WalletRepo::create(&conn, &NewWallet {
            name: "Old".into(), wallet_type: WalletType::Cash, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        TransactionService::execute(&conn, &NewTransaction::opening_balance(&wallet.id, Decimal::from(1_000), Currency::VND, 1)).unwrap();
        TransactionService::execute(&conn, &NewTransaction::expense(&wallet.id, Decimal::from(100), Currency::VND, &category, 2)).unwrap();
        WalletRepo::soft_delete(&conn, &wallet.id).unwrap();

        let json = export_backup(&conn).unwrap();
        import_backup(&conn, &json).unwrap();

        let w = WalletRepo::find_by_id(&conn, &wallet.id).unwrap().unwrap();
        assert!(!w.is_active);
        assert_eq!(w.balance(&Currency::VND), Decimal::from(900));
        assert_eq!(count(&conn, "transactions"), 2);
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

    #[test]
    fn export_then_import_restores_reminders_bills_photos_and_settings() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();

        let wallet = WalletRepo::create(&conn, &NewWallet {
            name: "Cash".into(), wallet_type: WalletType::Cash, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        TransactionService::execute(&conn,
            &NewTransaction::opening_balance(&wallet.id, Decimal::from(5_000), Currency::VND, 1)).unwrap();
        let txn_id: String = conn.query_row("SELECT id FROM transactions LIMIT 1", [], |r| r.get(0)).unwrap();
        let photo = vec![0xFFu8, 0xD8, 1, 2, 3, 0xFF, 0xD9];
        crate::repository::TxnImageRepo::add(&conn, &txn_id, &photo).unwrap();
        ReminderRepo::create(&conn, &NewReminder {
            txn_type: TransactionType::Income, wallet_id: wallet.id.clone(), to_wallet_id: None,
            amount: Decimal::from(1_000), currency: Currency::VND, note: Some("rent".into()),
            start_date: 1, repeat_unit: RepeatUnit::Month, repeat_every: 1,
        }).unwrap();
        crate::repository::BillRepo::create(&conn, &crate::models::NewBill {
            name: "Internet".into(), amount: Some(Decimal::from(300_000)), due_day: 15,
        }).unwrap();
        SettingsRepo::set_language(&conn, "vi").unwrap();
        SettingsRepo::set_reminder_interval_hours(&conn, 7).unwrap();
        SettingsRepo::set_pin(&conn, "1111").unwrap();

        let json = export_backup(&conn).unwrap();
        assert!(!json.contains("pin_hash"));

        delete_all_data(&conn).unwrap();
        SettingsRepo::set_language(&conn, "en").unwrap();
        SettingsRepo::set_reminder_interval_hours(&conn, 4).unwrap();

        let msg = import_backup(&conn, &json).unwrap();
        assert!(msg.contains("1 reminders, 1 bills, 1 transaction photos"), "{msg}");
        for table in ["reminders", "bills", "transaction_images", "transactions", "wallets"] {
            assert_eq!(count(&conn, table), 1, "{table}");
        }
        assert_eq!(crate::repository::TxnImageRepo::list(&conn, &txn_id).unwrap(), vec![photo]);
        assert_eq!(SettingsRepo::get_language(&conn).unwrap().as_deref(), Some("vi"));
        assert_eq!(SettingsRepo::get_reminder_interval_hours(&conn).unwrap(), 7);
        // The PIN is untouched by an import
        assert!(SettingsRepo::verify_pin(&conn, "1111").unwrap());
    }

    #[test]
    fn import_succeeds_over_existing_reminders_and_ignores_pin_in_file() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();

        let wallet = WalletRepo::create(&conn, &NewWallet {
            name: "Cash".into(), wallet_type: WalletType::Cash, broker: None, icon: None, color: None, sort_order: 0,
        }).unwrap();
        ReminderRepo::create(&conn, &NewReminder {
            txn_type: TransactionType::Income, wallet_id: wallet.id.clone(), to_wallet_id: None,
            amount: Decimal::from(1_000), currency: Currency::VND, note: None,
            start_date: 1, repeat_unit: RepeatUnit::Month, repeat_every: 1,
        }).unwrap();
        crate::repository::BillRepo::create(&conn, &crate::models::NewBill {
            name: "Old".into(), amount: None, due_day: 1,
        }).unwrap();
        SettingsRepo::set_pin(&conn, "1111").unwrap();

        let json = r#"{
            "app_version": "0.1.0", "exported_at": 1,
            "wallets": [], "holdings": [], "categories": [], "transactions": [], "members": [], "families": [],
            "settings": [{"key": "pin_hash", "value": "evil"}, {"key": "language", "value": "vi"}]
        }"#;
        import_backup(&conn, json).unwrap();

        assert_eq!(count(&conn, "reminders"), 0);
        assert_eq!(count(&conn, "bills"), 0);
        assert_eq!(SettingsRepo::get_language(&conn).unwrap().as_deref(), Some("vi"));
        assert!(SettingsRepo::verify_pin(&conn, "1111").unwrap());
    }
}
