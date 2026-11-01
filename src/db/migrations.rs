use rusqlite::{Connection, Result};

/// The schema only uses CREATE … IF NOT EXISTS / INSERT OR IGNORE, so it runs on every
/// launch: a table added later also appears in existing databases, no migration needed.
/// (Adding a column to an existing table would still need one.)
pub fn run(conn: &Connection) -> Result<()> {
    migrate_v1(conn)?;
    if get_version(conn)? < 1 {
        set_version(conn, 1)?;
    }
    Ok(())
}

fn get_version(conn: &Connection) -> Result<u32> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn set_version(conn: &Connection, version: u32) -> Result<()> {
    conn.execute_batch(&format!("PRAGMA user_version = {version}"))
}

// ─── V1 — full schema ─────────────────────────────────────────────────────────

fn migrate_v1(conn: &Connection) -> Result<()> {
    let _ = conn.execute_batch("
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
    ");

    conn.execute_batch("
        -- ── Money ──
        -- Money types: the fixed VND / USD / GOLD with user-editable display details
        CREATE TABLE IF NOT EXISTS currencies (
            code       TEXT PRIMARY KEY,           -- 'VND' / 'USD' / 'GOLD' (models::Currency)
            name       TEXT NOT NULL,              -- display name, translated via t.tr()
            unit       TEXT NOT NULL,              -- symbol used when formatting amounts
            is_active  INTEGER NOT NULL DEFAULT 1, -- hidden types are left out of pickers
            sort_order INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS wallets (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            wallet_type TEXT NOT NULL,
            broker      TEXT,
            icon        TEXT,
            color       TEXT,
            is_active   INTEGER NOT NULL DEFAULT 1,
            sort_order  INTEGER NOT NULL DEFAULT 0,
            created_at  INTEGER NOT NULL
        );

        -- Balance of one wallet in one money type
        CREATE TABLE IF NOT EXISTS wallet_balances (
            wallet_id TEXT NOT NULL REFERENCES wallets(id),
            currency  TEXT NOT NULL REFERENCES currencies(code),
            balance   REAL NOT NULL DEFAULT 0,
            PRIMARY KEY (wallet_id, currency)
        );

        CREATE TABLE IF NOT EXISTS holdings (
            id            TEXT PRIMARY KEY,
            wallet_id     TEXT NOT NULL REFERENCES wallets(id),
            symbol        TEXT NOT NULL,
            name          TEXT,
            asset_type    TEXT NOT NULL,
            quantity      REAL NOT NULL DEFAULT 0,
            avg_buy_price REAL NOT NULL DEFAULT 0,
            last_price    REAL,
            last_price_at INTEGER,
            created_at    INTEGER NOT NULL,
            UNIQUE(wallet_id, symbol)
        );

        CREATE TABLE IF NOT EXISTS categories (
            id            TEXT PRIMARY KEY,
            name          TEXT NOT NULL,
            icon          TEXT,
            color         TEXT,
            budget_amount REAL,
            parent_id     TEXT REFERENCES categories(id),
            sort_order    INTEGER DEFAULT 0,
            is_system     INTEGER DEFAULT 0,
            created_at    INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS transactions (
            id             TEXT PRIMARY KEY,
            txn_type       TEXT NOT NULL,
            wallet_id      TEXT NOT NULL REFERENCES wallets(id),
            amount         REAL NOT NULL,
            currency       TEXT NOT NULL,
            income_type    TEXT,
            category_id    TEXT REFERENCES categories(id),
            to_wallet_id   TEXT REFERENCES wallets(id),
            to_amount      REAL,
            to_currency    TEXT,
            holding_id     TEXT REFERENCES holdings(id),
            asset_quantity REAL,
            asset_price    REAL,
            note           TEXT,
            txn_date       INTEGER NOT NULL,
            created_at     INTEGER NOT NULL
        );

        -- Photos attached to a transaction (receipts…), JPEG; cleared from Settings → Backup
        CREATE TABLE IF NOT EXISTS transaction_images (
            id             TEXT PRIMARY KEY,
            transaction_id TEXT NOT NULL REFERENCES transactions(id),
            jpeg           BLOB NOT NULL,
            created_at     INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_txn_image_txn ON transaction_images(transaction_id);

        CREATE INDEX IF NOT EXISTS idx_txn_date       ON transactions(txn_date DESC);
        CREATE INDEX IF NOT EXISTS idx_txn_wallet     ON transactions(wallet_id);
        CREATE INDEX IF NOT EXISTS idx_txn_category   ON transactions(category_id);
        CREATE INDEX IF NOT EXISTS idx_txn_type       ON transactions(txn_type);
        CREATE INDEX IF NOT EXISTS idx_holding_wallet ON holdings(wallet_id);

        -- Latest exchange rate per currency, fetched online
        CREATE TABLE IF NOT EXISTS exchange_rates (
            currency   TEXT PRIMARY KEY,   -- 'USD' / 'GOLD'
            rate_vnd   REAL NOT NULL,      -- VND for 1 unit (1 USD, 1 chỉ gold)
            source     TEXT NOT NULL,      -- where it came from, e.g. 'yahoo'
            fetched_at INTEGER NOT NULL    -- unix seconds
        );

        -- ── Family ──
        CREATE TABLE IF NOT EXISTS families (
            id             TEXT PRIMARY KEY,
            name           TEXT NOT NULL,
            common_address TEXT,
            note           TEXT,
            created_at     INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS members (
            id             TEXT PRIMARY KEY,
            family_id      TEXT REFERENCES families(id),
            full_name      TEXT NOT NULL,
            birth_date     INTEGER,
            gender         TEXT,
            phone          TEXT,
            id_number      TEXT,
            id_issue_date  INTEGER,
            id_issue_place TEXT,
            address        TEXT,
            role           TEXT NOT NULL DEFAULT 'member',
            avatar_emoji   TEXT,
            note           TEXT,
            created_at     INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_member_family ON members(family_id);

        -- CCCD photos (front / back), one JPEG per side
        CREATE TABLE IF NOT EXISTS member_id_images (
            member_id  TEXT NOT NULL REFERENCES members(id),
            side       TEXT NOT NULL,                -- 'front' / 'back'
            jpeg       BLOB NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (member_id, side)
        );

        -- ── Notifications ──
        -- Recurring income/transfer reminders
        CREATE TABLE IF NOT EXISTS reminders (
            id           TEXT PRIMARY KEY,
            txn_type     TEXT NOT NULL,              -- 'income' / 'transfer'
            wallet_id    TEXT NOT NULL REFERENCES wallets(id),
            to_wallet_id TEXT REFERENCES wallets(id), -- transfer only
            amount       REAL NOT NULL,
            currency     TEXT NOT NULL,
            note         TEXT,
            start_date   INTEGER NOT NULL,           -- first due day (unix seconds, local noon)
            repeat_unit  TEXT NOT NULL,              -- 'day' / 'week' / 'month' / 'year'
            repeat_every INTEGER NOT NULL DEFAULT 1,
            created_at   INTEGER NOT NULL,
            done_on      TEXT,                       -- 'YYYY-MM-DD' marked done
            is_paused    INTEGER NOT NULL DEFAULT 0
        );

        -- Monthly bills: notify from the due day until that month is marked paid
        CREATE TABLE IF NOT EXISTS bills (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL,
            amount     REAL,                       -- optional, VND
            due_day    INTEGER NOT NULL,           -- 1..31, short months use their last day
            paid_month TEXT,                       -- 'YYYY-MM' marked paid
            is_paused  INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        );

        -- ── Settings ──
        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        -- INSERT OR IGNORE keeps values the user already picked
        INSERT OR IGNORE INTO settings (key, value) VALUES ('app_version', '');
        INSERT OR IGNORE INTO settings (key, value) VALUES ('language', 'en');
        INSERT OR IGNORE INTO settings (key, value) VALUES ('reminder_interval_hours', '4');  -- on a due day, notify every N hours
        INSERT OR IGNORE INTO settings (key, value) VALUES ('reminder_start_time', '08:00');  -- first notification of the day

        INSERT OR IGNORE INTO currencies (code, name, unit, is_active, sort_order) VALUES ('VND',  'Vietnamese Dong', '₫',   1, 0);
        INSERT OR IGNORE INTO currencies (code, name, unit, is_active, sort_order) VALUES ('USD',  'US Dollar',       '$',   0, 1);
        INSERT OR IGNORE INTO currencies (code, name, unit, is_active, sort_order) VALUES ('GOLD', 'Gold',            'chỉ', 0, 2);
    ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_every_table_and_runs_once() {
        let conn = Connection::open_in_memory().unwrap();
        run(&conn).unwrap();
        run(&conn).unwrap();
        assert_eq!(get_version(&conn).unwrap(), 1);

        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name").unwrap();
        let tables: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().collect::<Result<_>>().unwrap();
        assert_eq!(tables, [
            "bills", "categories", "currencies", "exchange_rates", "families", "holdings",
            "member_id_images", "members", "reminders", "settings", "transaction_images",
            "transactions", "wallet_balances", "wallets",
        ]);
    }
}
