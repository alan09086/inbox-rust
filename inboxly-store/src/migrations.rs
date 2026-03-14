use tracing::info;

use crate::error::{Result, StoreError};
use crate::store::Store;

/// Current schema version. Bump this when adding a new migration.
const CURRENT_VERSION: u32 = 2;

/// Run all pending migrations.
pub fn run(store: &mut Store) -> Result<()> {
    let version = get_version(store)?;

    if version > CURRENT_VERSION {
        return Err(StoreError::MigrationFailed {
            expected: CURRENT_VERSION,
            found: version,
        });
    }

    if version < 1 {
        migrate_v0_to_v1(store)?;
    }

    if version < 2 {
        migrate_v1_to_v2(store)?;
    }

    set_version(store, CURRENT_VERSION)?;
    info!("Database at schema version {CURRENT_VERSION}");
    Ok(())
}

fn get_version(store: &Store) -> Result<u32> {
    let version: u32 = store
        .conn()
        .query_row("PRAGMA user_version", [], |row| row.get(0))?;
    Ok(version)
}

fn set_version(store: &Store, version: u32) -> Result<()> {
    // PRAGMA doesn't support parameters, must format directly
    store
        .conn()
        .execute_batch(&format!("PRAGMA user_version = {version}"))?;
    Ok(())
}

/// Initial schema: all tables.
fn migrate_v0_to_v1(store: &mut Store) -> Result<()> {
    info!("Running migration v0 -> v1: creating all tables");

    store.conn().execute_batch(
        "
        -- Accounts
        CREATE TABLE IF NOT EXISTS accounts (
            id                TEXT PRIMARY KEY NOT NULL,
            email             TEXT NOT NULL,
            display_name      TEXT NOT NULL,
            provider          TEXT NOT NULL,
            auth_method       TEXT NOT NULL,
            imap_host         TEXT NOT NULL,
            imap_port         INTEGER NOT NULL,
            smtp_host         TEXT NOT NULL,
            smtp_port         INTEGER NOT NULL,
            created_at        INTEGER NOT NULL DEFAULT (unixepoch())
        );

        -- Emails (most-queried table)
        CREATE TABLE IF NOT EXISTS emails (
            id                  TEXT PRIMARY KEY NOT NULL,
            account_id          TEXT NOT NULL REFERENCES accounts(id),
            thread_id           TEXT NOT NULL,
            from_name           TEXT,
            from_address        TEXT NOT NULL,
            to_json             TEXT NOT NULL DEFAULT '[]',
            cc_json             TEXT NOT NULL DEFAULT '[]',
            subject             TEXT NOT NULL DEFAULT '',
            snippet             TEXT NOT NULL DEFAULT '',
            date                INTEGER NOT NULL,
            maildir_path        TEXT NOT NULL,
            flags               INTEGER NOT NULL DEFAULT 0,
            size_bytes          INTEGER NOT NULL DEFAULT 0,
            imap_uid            INTEGER NOT NULL DEFAULT 0,
            imap_folder         TEXT NOT NULL DEFAULT 'INBOX',
            has_attachments     INTEGER NOT NULL DEFAULT 0,
            body_downloaded     INTEGER NOT NULL DEFAULT 0,
            message_id_header   TEXT,
            in_reply_to         TEXT,
            references_json     TEXT,
            UNIQUE(account_id, imap_folder, imap_uid)
        );
        CREATE INDEX IF NOT EXISTS idx_emails_thread_id ON emails(thread_id);
        CREATE INDEX IF NOT EXISTS idx_emails_account_id ON emails(account_id);
        CREATE INDEX IF NOT EXISTS idx_emails_date ON emails(date DESC);
        CREATE INDEX IF NOT EXISTS idx_emails_from_address ON emails(from_address);

        -- Threads (aggregated metadata)
        CREATE TABLE IF NOT EXISTS threads (
            id                TEXT PRIMARY KEY NOT NULL,
            account_id        TEXT NOT NULL REFERENCES accounts(id),
            subject           TEXT NOT NULL DEFAULT '',
            newest_date       INTEGER NOT NULL,
            oldest_date       INTEGER NOT NULL,
            email_count       INTEGER NOT NULL DEFAULT 0,
            unread_count      INTEGER NOT NULL DEFAULT 0,
            has_attachments   INTEGER NOT NULL DEFAULT 0,
            snippet           TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS idx_threads_account_id ON threads(account_id);
        CREATE INDEX IF NOT EXISTS idx_threads_newest_date ON threads(newest_date DESC);

        -- Bundles (defined before thread_state since thread_state references it)
        CREATE TABLE IF NOT EXISTS bundles (
            id                TEXT PRIMARY KEY NOT NULL,
            category          TEXT NOT NULL,
            name              TEXT NOT NULL,
            color             TEXT NOT NULL DEFAULT '#000000',
            badge_color       TEXT NOT NULL DEFAULT '#eeeeee',
            visibility        TEXT NOT NULL DEFAULT 'Bundled',
            throttle          TEXT NOT NULL DEFAULT 'Immediate',
            sort_order        INTEGER NOT NULL DEFAULT 0
        );

        -- Thread state (pins, done, snooze, bundle assignment)
        CREATE TABLE IF NOT EXISTS thread_state (
            thread_id             TEXT PRIMARY KEY NOT NULL REFERENCES threads(id),
            pinned                INTEGER NOT NULL DEFAULT 0,
            done                  INTEGER NOT NULL DEFAULT 0,
            snoozed_until         INTEGER,
            snoozed_location_json TEXT,
            bundle_id             TEXT REFERENCES bundles(id)
        );

        -- Sync state (per account + folder)
        CREATE TABLE IF NOT EXISTS sync_state (
            account_id        TEXT NOT NULL REFERENCES accounts(id),
            folder_name       TEXT NOT NULL,
            uid_validity      INTEGER,
            uid_next          INTEGER,
            highest_modseq    INTEGER,
            last_sync         INTEGER,
            PRIMARY KEY (account_id, folder_name)
        );

        -- Contacts
        CREATE TABLE IF NOT EXISTS contacts (
            address           TEXT PRIMARY KEY NOT NULL,
            display_name      TEXT,
            avatar_letter     TEXT,
            avatar_color_index INTEGER NOT NULL DEFAULT 0,
            last_seen         INTEGER NOT NULL
        );

        -- Bundle rules
        CREATE TABLE IF NOT EXISTS bundle_rules (
            id                TEXT PRIMARY KEY NOT NULL,
            bundle_id         TEXT NOT NULL REFERENCES bundles(id) ON DELETE CASCADE,
            field             TEXT NOT NULL,
            operator          TEXT NOT NULL,
            value             TEXT NOT NULL,
            priority          INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_bundle_rules_bundle_id ON bundle_rules(bundle_id);

        -- Sender affinity (learned categorisation)
        CREATE TABLE IF NOT EXISTS sender_affinity (
            sender_domain     TEXT NOT NULL,
            sender_address    TEXT NOT NULL,
            bundle_category   TEXT NOT NULL,
            confidence        REAL NOT NULL DEFAULT 0.0,
            learned_at        INTEGER NOT NULL,
            PRIMARY KEY (sender_address, bundle_category)
        );
        CREATE INDEX IF NOT EXISTS idx_sender_affinity_domain ON sender_affinity(sender_domain);

        -- Reminders
        CREATE TABLE IF NOT EXISTS reminders (
            id                TEXT PRIMARY KEY NOT NULL,
            title             TEXT NOT NULL,
            due_at            INTEGER,
            location_lat      REAL,
            location_lng      REAL,
            location_label    TEXT,
            recurring         TEXT,
            done              INTEGER NOT NULL DEFAULT 0
        );

        -- Highlights (per-thread extracted data)
        CREATE TABLE IF NOT EXISTS highlights (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            thread_id         TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
            highlight_type    TEXT NOT NULL,
            data_json         TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_highlights_thread_id ON highlights(thread_id);

        -- Settings (key-value)
        CREATE TABLE IF NOT EXISTS settings (
            key               TEXT PRIMARY KEY NOT NULL,
            value             TEXT NOT NULL
        );

        -- Offline queue (actions to replay on reconnect)
        CREATE TABLE IF NOT EXISTS offline_queue (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            action            TEXT NOT NULL,
            payload_json      TEXT NOT NULL,
            created_at        INTEGER NOT NULL DEFAULT (unixepoch())
        );
        ",
    )?;

    Ok(())
}

/// M8: Add body_downloaded column and index for Phase 2 sync.
///
/// The column may already exist if the database was created fresh with v0->v1
/// that includes it. We check first to avoid a "duplicate column" error.
fn migrate_v1_to_v2(store: &mut Store) -> Result<()> {
    info!("Running migration v1 -> v2: add body_downloaded column to emails");

    // Check if column already exists (fresh installs already have it in v0->v1).
    let has_column: bool = {
        let mut stmt = store.conn().prepare("PRAGMA table_info(emails)")?;
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();
        columns.iter().any(|c| c == "body_downloaded")
    };

    if !has_column {
        store.conn().execute_batch(
            "ALTER TABLE emails ADD COLUMN body_downloaded INTEGER NOT NULL DEFAULT 0;",
        )?;
    }

    // Index is idempotent (IF NOT EXISTS).
    store.conn().execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_emails_body_downloaded
             ON emails(account_id, imap_folder, body_downloaded)
             WHERE body_downloaded = 0;",
    )?;

    Ok(())
}
