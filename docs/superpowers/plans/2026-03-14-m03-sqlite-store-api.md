# M3: SQLite Schema + Store API — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Implement SQLite database with full schema and CRUD operations for all Inboxly data.

**Architecture:** `inboxly-store` crate owns the database. Uses `rusqlite` with WAL mode for concurrent reads. Schema versioning via `PRAGMA user_version`. All tables defined in the design spec are created, with typed CRUD operations returning `inboxly-core` types. The `Store` struct wraps a `rusqlite::Connection` (single-threaded; connection pooling deferred to when async is needed).

**Tech Stack:** Rust, rusqlite (bundled-sqlcipher feature for bundled SQLite), inboxly-core types

**Prerequisites:** M1 (core types in `inboxly-core`), M2 (config with `data_dir` path)

**Spec reference:** `docs/superpowers/specs/2026-03-14-inboxly-design.md` — "SQLite Schema" section

---

## Context for Implementer

### Workspace layout after M1 + M2

```
inboxly/
├── Cargo.toml              ← workspace root
├── inboxly-core/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── types.rs         ← EmailId, ThreadId, AccountId, BundleId, EmailMeta, Thread, Bundle, etc.
│       ├── config.rs        ← InboxlyConfig with data_dir: PathBuf
│       └── error.rs         ← InboxlyError
└── inboxly-store/           ← THIS MILESTONE CREATES THIS CRATE
```

### Core types you'll use (from `inboxly-core::types`)

```rust
// IDs
AccountId(Uuid)
EmailId(String)       // Message-ID header
ThreadId(Uuid)
BundleId(Uuid)

// Enums
BundleCategory { Social, Promos, Updates, Finance, Purchases, Travel, Forums, LowPriority, Saved, Custom(String) }
BundleVisibility { Bundled, Unbundled, SkipInbox }
BundleThrottle { Immediate, Daily, Weekly }
EmailFlags { read: bool, starred: bool, answered: bool, draft: bool, deleted: bool }

// Structs
Contact { name: Option<String>, address: String }
```

### Design spec SQL schema (authoritative source)

All table definitions come from the "SQLite Schema" section of the design spec. This plan implements every table listed there plus the CRUD operations needed by downstream crates.

---

## Task 1: Create `inboxly-store` crate skeleton

### Step 1.1: Create directory structure

Create the directory `inboxly-store/src/`.

**Command:** `mkdir -p inboxly-store/src`

### Step 1.2: Create `inboxly-store/Cargo.toml`

Create file `inboxly-store/Cargo.toml`:

```toml
[package]
name = "inboxly-store"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
inboxly-core = { path = "../inboxly-core" }
rusqlite = { version = "0.35", features = ["bundled"] }
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tracing = "0.1"

[dev-dependencies]
tempfile = "3"
```

### Step 1.3: Add `inboxly-store` to workspace `Cargo.toml`

In the root `Cargo.toml`, add `"inboxly-store"` to the `workspace.members` array.

### Step 1.4: Create `inboxly-store/src/lib.rs`

Create file `inboxly-store/src/lib.rs`:

```rust
//! SQLite storage backend for Inboxly.
//!
//! Owns the metadata database: emails, threads, accounts, bundles,
//! sync state, reminders, highlights, settings, and offline queue.

mod error;
mod migrations;
mod store;

// Table-specific CRUD modules
mod accounts;
mod bundles;
mod bundle_rules;
mod contacts;
mod emails;
mod highlights;
mod offline_queue;
mod reminders;
mod sender_affinity;
mod settings;
mod sync_state;
mod thread_state;
mod threads;

pub use error::StoreError;
pub use store::Store;
```

### Step 1.5: Create `inboxly-store/src/error.rs`

Create file `inboxly-store/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Database migration failed: expected version {expected}, found {found}")]
    MigrationFailed { expected: u32, found: u32 },

    #[error("Record not found: {0}")]
    NotFound(String),

    #[error("Constraint violation: {0}")]
    Constraint(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, StoreError>;
```

### Step 1.6: Verify it compiles

**Command:** `cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store`

This will fail because the modules don't exist yet — that's expected. Create placeholder files for each module so it compiles:

For each module listed in `lib.rs` (`migrations`, `store`, `accounts`, `bundles`, `bundle_rules`, `contacts`, `emails`, `highlights`, `offline_queue`, `reminders`, `sender_affinity`, `settings`, `sync_state`, `thread_state`, `threads`), create a file `inboxly-store/src/{module}.rs` containing only:

```rust
// TODO: implement in subsequent tasks
```

Then re-run `cargo check -p inboxly-store` — it must pass.

### Step 1.7: Commit

**Command:** `git add inboxly-store/ Cargo.toml && git commit -m "feat(store): add inboxly-store crate skeleton (M3)"`

---

## Task 2: Database initialization + migration system

### Step 2.1: Implement `Store` struct with database open

Replace `inboxly-store/src/store.rs` with:

```rust
use std::path::Path;

use rusqlite::Connection;
use tracing::info;

use crate::error::Result;
use crate::migrations;

/// SQLite metadata store for Inboxly.
///
/// Wraps a single `rusqlite::Connection`. Not `Send` — callers must
/// ensure single-threaded access or wrap in a `Mutex` if shared.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (or create) the database at the given path.
    ///
    /// Enables WAL mode, foreign keys, and runs any pending migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::configure(&conn)?;
        let mut store = Self { conn };
        migrations::run(&mut store)?;
        info!("Store opened at {}", path.display());
        Ok(store)
    }

    /// Open an in-memory database (for tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::configure(&conn)?;
        let mut store = Self { conn };
        migrations::run(&mut store)?;
        info!("Store opened in-memory");
        Ok(store)
    }

    /// Access the raw connection (for modules that need it).
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Execute a closure inside a transaction. Commits on Ok, rolls back on Err.
    pub fn transaction<T, F>(&mut self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let tx = self.conn.transaction()?;
        let result = f(&tx);
        match result {
            Ok(val) => {
                tx.commit()?;
                Ok(val)
            }
            Err(e) => {
                // tx drops here, auto-rollback
                Err(e)
            }
        }
    }

    /// Configure connection pragmas.
    fn configure(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA synchronous = NORMAL;",
        )?;
        Ok(())
    }
}
```

### Step 2.2: Implement migration system

Replace `inboxly-store/src/migrations.rs` with:

```rust
use rusqlite::params;
use tracing::info;

use crate::error::{Result, StoreError};
use crate::store::Store;

/// Current schema version. Bump this when adding a new migration.
const CURRENT_VERSION: u32 = 1;

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

    // Future migrations:
    // if version < 2 { migrate_v1_to_v2(store)?; }

    set_version(store, CURRENT_VERSION)?;
    info!("Database at schema version {CURRENT_VERSION}");
    Ok(())
}

fn get_version(store: &Store) -> Result<u32> {
    let version: u32 = store.conn().query_row(
        "PRAGMA user_version",
        [],
        |row| row.get(0),
    )?;
    Ok(version)
}

fn set_version(store: &Store, version: u32) -> Result<()> {
    // PRAGMA doesn't support parameters, must format directly
    store.conn().execute_batch(&format!("PRAGMA user_version = {version}"))?;
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

        -- Bundles
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
        "
    )?;

    Ok(())
}
```

### Step 2.3: Verify compilation

**Command:** `cargo check -p inboxly-store`

### Step 2.4: Commit

**Command:** `git add -A && git commit -m "feat(store): database init with WAL mode and v1 schema migration (M3)"`

---

## Task 3: Accounts CRUD

### Step 3.1: Implement `inboxly-store/src/accounts.rs`

Replace the placeholder with:

```rust
use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

/// Row representation for the accounts table.
#[derive(Debug, Clone)]
pub struct AccountRow {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub provider: String,
    pub auth_method: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
}

impl Store {
    pub fn insert_account(&self, account: &AccountRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO accounts (id, email, display_name, provider, auth_method, imap_host, imap_port, smtp_host, smtp_port)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                account.id,
                account.email,
                account.display_name,
                account.provider,
                account.auth_method,
                account.imap_host,
                account.imap_port as i64,
                account.smtp_host,
                account.smtp_port as i64,
            ],
        )?;
        Ok(())
    }

    pub fn get_account(&self, id: &str) -> Result<AccountRow> {
        self.conn()
            .query_row(
                "SELECT id, email, display_name, provider, auth_method, imap_host, imap_port, smtp_host, smtp_port
                 FROM accounts WHERE id = ?1",
                params![id],
                |row| {
                    Ok(AccountRow {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        display_name: row.get(2)?,
                        provider: row.get(3)?,
                        auth_method: row.get(4)?,
                        imap_host: row.get(5)?,
                        imap_port: row.get::<_, i64>(6)? as u16,
                        smtp_host: row.get(7)?,
                        smtp_port: row.get::<_, i64>(8)? as u16,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("account {id}"))
                }
                other => StoreError::Sqlite(other),
            })
    }

    pub fn list_accounts(&self) -> Result<Vec<AccountRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, email, display_name, provider, auth_method, imap_host, imap_port, smtp_host, smtp_port
             FROM accounts ORDER BY email",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(AccountRow {
                    id: row.get(0)?,
                    email: row.get(1)?,
                    display_name: row.get(2)?,
                    provider: row.get(3)?,
                    auth_method: row.get(4)?,
                    imap_host: row.get(5)?,
                    imap_port: row.get::<_, i64>(6)? as u16,
                    smtp_host: row.get(7)?,
                    smtp_port: row.get::<_, i64>(8)? as u16,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn update_account(&self, account: &AccountRow) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE accounts SET email = ?2, display_name = ?3, provider = ?4, auth_method = ?5,
             imap_host = ?6, imap_port = ?7, smtp_host = ?8, smtp_port = ?9
             WHERE id = ?1",
            params![
                account.id,
                account.email,
                account.display_name,
                account.provider,
                account.auth_method,
                account.imap_host,
                account.imap_port as i64,
                account.smtp_host,
                account.smtp_port as i64,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("account {}", account.id)));
        }
        Ok(())
    }

    pub fn delete_account(&self, id: &str) -> Result<()> {
        let changed = self.conn().execute(
            "DELETE FROM accounts WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("account {id}")));
        }
        Ok(())
    }
}
```

### Step 3.2: Export `AccountRow` from `lib.rs`

Add to `lib.rs` after the existing `pub use` lines:

```rust
pub use accounts::AccountRow;
```

### Step 3.3: Verify

**Command:** `cargo check -p inboxly-store`

### Step 3.4: Commit

**Command:** `git add -A && git commit -m "feat(store): accounts table CRUD operations (M3)"`

---

## Task 4: Emails CRUD

### Step 4.1: Implement `inboxly-store/src/emails.rs`

Replace the placeholder with:

```rust
use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

/// Row representation for the emails table.
///
/// JSON fields (`to_json`, `cc_json`, `references_json`) are stored as
/// serialised JSON strings. Callers use `serde_json` to parse them into
/// `Vec<Contact>` or `Vec<String>` as needed.
#[derive(Debug, Clone)]
pub struct EmailRow {
    pub id: String,
    pub account_id: String,
    pub thread_id: String,
    pub from_name: Option<String>,
    pub from_address: String,
    pub to_json: String,
    pub cc_json: String,
    pub subject: String,
    pub snippet: String,
    pub date: i64,
    pub maildir_path: String,
    pub flags: i64,
    pub size_bytes: i64,
    pub imap_uid: i64,
    pub imap_folder: String,
    pub has_attachments: bool,
    pub message_id_header: Option<String>,
    pub in_reply_to: Option<String>,
    pub references_json: Option<String>,
}

/// Bitmask constants for `EmailRow::flags`.
pub mod flags {
    pub const READ: i64 = 1;
    pub const STARRED: i64 = 2;
    pub const ANSWERED: i64 = 4;
    pub const DRAFT: i64 = 8;
    pub const DELETED: i64 = 16;
}

impl Store {
    pub fn insert_email(&self, email: &EmailRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO emails (id, account_id, thread_id, from_name, from_address, to_json, cc_json,
             subject, snippet, date, maildir_path, flags, size_bytes, imap_uid, imap_folder,
             has_attachments, message_id_header, in_reply_to, references_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                email.id,
                email.account_id,
                email.thread_id,
                email.from_name,
                email.from_address,
                email.to_json,
                email.cc_json,
                email.subject,
                email.snippet,
                email.date,
                email.maildir_path,
                email.flags,
                email.size_bytes,
                email.imap_uid,
                email.imap_folder,
                email.has_attachments,
                email.message_id_header,
                email.in_reply_to,
                email.references_json,
            ],
        )?;
        Ok(())
    }

    pub fn get_email(&self, id: &str) -> Result<EmailRow> {
        self.conn()
            .query_row(
                "SELECT id, account_id, thread_id, from_name, from_address, to_json, cc_json,
                 subject, snippet, date, maildir_path, flags, size_bytes, imap_uid, imap_folder,
                 has_attachments, message_id_header, in_reply_to, references_json
                 FROM emails WHERE id = ?1",
                params![id],
                Self::row_to_email,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("email {id}"))
                }
                other => StoreError::Sqlite(other),
            })
    }

    /// Get all emails for a thread, ordered by date ascending.
    pub fn get_emails_by_thread(&self, thread_id: &str) -> Result<Vec<EmailRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, account_id, thread_id, from_name, from_address, to_json, cc_json,
             subject, snippet, date, maildir_path, flags, size_bytes, imap_uid, imap_folder,
             has_attachments, message_id_header, in_reply_to, references_json
             FROM emails WHERE thread_id = ?1 ORDER BY date ASC",
        )?;
        let rows = stmt
            .query_map(params![thread_id], Self::row_to_email)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get all emails for an account in a folder, ordered by date descending.
    pub fn get_emails_by_folder(
        &self,
        account_id: &str,
        folder: &str,
    ) -> Result<Vec<EmailRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, account_id, thread_id, from_name, from_address, to_json, cc_json,
             subject, snippet, date, maildir_path, flags, size_bytes, imap_uid, imap_folder,
             has_attachments, message_id_header, in_reply_to, references_json
             FROM emails WHERE account_id = ?1 AND imap_folder = ?2 ORDER BY date DESC",
        )?;
        let rows = stmt
            .query_map(params![account_id, folder], Self::row_to_email)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Look up an email by IMAP UID (unique within account + folder).
    pub fn get_email_by_uid(
        &self,
        account_id: &str,
        folder: &str,
        uid: i64,
    ) -> Result<Option<EmailRow>> {
        let result = self.conn().query_row(
            "SELECT id, account_id, thread_id, from_name, from_address, to_json, cc_json,
             subject, snippet, date, maildir_path, flags, size_bytes, imap_uid, imap_folder,
             has_attachments, message_id_header, in_reply_to, references_json
             FROM emails WHERE account_id = ?1 AND imap_folder = ?2 AND imap_uid = ?3",
            params![account_id, folder, uid],
            Self::row_to_email,
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Sqlite(e)),
        }
    }

    /// Update flags for an email.
    pub fn update_email_flags(&self, id: &str, flags: i64) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE emails SET flags = ?2 WHERE id = ?1",
            params![id, flags],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("email {id}")));
        }
        Ok(())
    }

    /// Reassign an email to a different thread (used during thread unification).
    pub fn update_email_thread(&self, email_id: &str, new_thread_id: &str) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE emails SET thread_id = ?2 WHERE id = ?1",
            params![email_id, new_thread_id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("email {email_id}")));
        }
        Ok(())
    }

    pub fn delete_email(&self, id: &str) -> Result<()> {
        let changed = self.conn().execute(
            "DELETE FROM emails WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("email {id}")));
        }
        Ok(())
    }

    /// Get the highest IMAP UID stored for a given account + folder.
    pub fn get_max_uid(&self, account_id: &str, folder: &str) -> Result<Option<i64>> {
        let result = self.conn().query_row(
            "SELECT MAX(imap_uid) FROM emails WHERE account_id = ?1 AND imap_folder = ?2",
            params![account_id, folder],
            |row| row.get::<_, Option<i64>>(0),
        )?;
        Ok(result)
    }

    fn row_to_email(row: &rusqlite::Row<'_>) -> rusqlite::Result<EmailRow> {
        Ok(EmailRow {
            id: row.get(0)?,
            account_id: row.get(1)?,
            thread_id: row.get(2)?,
            from_name: row.get(3)?,
            from_address: row.get(4)?,
            to_json: row.get(5)?,
            cc_json: row.get(6)?,
            subject: row.get(7)?,
            snippet: row.get(8)?,
            date: row.get(9)?,
            maildir_path: row.get(10)?,
            flags: row.get(11)?,
            size_bytes: row.get(12)?,
            imap_uid: row.get(13)?,
            imap_folder: row.get(14)?,
            has_attachments: row.get(15)?,
            message_id_header: row.get(16)?,
            in_reply_to: row.get(17)?,
            references_json: row.get(18)?,
        })
    }
}
```

### Step 4.2: Export `EmailRow` and `flags` from `lib.rs`

Add to `lib.rs`:

```rust
pub use emails::{EmailRow, flags};
```

### Step 4.3: Verify

**Command:** `cargo check -p inboxly-store`

### Step 4.4: Commit

**Command:** `git add -A && git commit -m "feat(store): emails table CRUD with UID lookup and thread reassignment (M3)"`

---

## Task 5: Threads CRUD

### Step 5.1: Implement `inboxly-store/src/threads.rs`

Replace the placeholder with:

```rust
use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct ThreadRow {
    pub id: String,
    pub account_id: String,
    pub subject: String,
    pub newest_date: i64,
    pub oldest_date: i64,
    pub email_count: i64,
    pub unread_count: i64,
    pub has_attachments: bool,
    pub snippet: String,
}

impl Store {
    pub fn insert_thread(&self, thread: &ThreadRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                thread.id,
                thread.account_id,
                thread.subject,
                thread.newest_date,
                thread.oldest_date,
                thread.email_count,
                thread.unread_count,
                thread.has_attachments,
                thread.snippet,
            ],
        )?;
        Ok(())
    }

    pub fn get_thread(&self, id: &str) -> Result<ThreadRow> {
        self.conn()
            .query_row(
                "SELECT id, account_id, subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet
                 FROM threads WHERE id = ?1",
                params![id],
                Self::row_to_thread,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("thread {id}"))
                }
                other => StoreError::Sqlite(other),
            })
    }

    /// Get threads for an account, ordered by newest_date descending.
    /// `limit` and `offset` support pagination.
    pub fn get_threads_by_account(
        &self,
        account_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ThreadRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, account_id, subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet
             FROM threads WHERE account_id = ?1 ORDER BY newest_date DESC LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt
            .query_map(params![account_id, limit, offset], Self::row_to_thread)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Update thread aggregate metadata (called after email insert/delete/flag change).
    pub fn update_thread(&self, thread: &ThreadRow) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE threads SET subject = ?2, newest_date = ?3, oldest_date = ?4,
             email_count = ?5, unread_count = ?6, has_attachments = ?7, snippet = ?8
             WHERE id = ?1",
            params![
                thread.id,
                thread.subject,
                thread.newest_date,
                thread.oldest_date,
                thread.email_count,
                thread.unread_count,
                thread.has_attachments,
                thread.snippet,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("thread {}", thread.id)));
        }
        Ok(())
    }

    pub fn delete_thread(&self, id: &str) -> Result<()> {
        let changed = self.conn().execute(
            "DELETE FROM threads WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("thread {id}")));
        }
        Ok(())
    }

    /// Insert or update a thread (upsert). Used during sync when a new email
    /// may create a thread or update an existing one.
    pub fn upsert_thread(&self, thread: &ThreadRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                subject = excluded.subject,
                newest_date = excluded.newest_date,
                oldest_date = excluded.oldest_date,
                email_count = excluded.email_count,
                unread_count = excluded.unread_count,
                has_attachments = excluded.has_attachments,
                snippet = excluded.snippet",
            params![
                thread.id,
                thread.account_id,
                thread.subject,
                thread.newest_date,
                thread.oldest_date,
                thread.email_count,
                thread.unread_count,
                thread.has_attachments,
                thread.snippet,
            ],
        )?;
        Ok(())
    }

    fn row_to_thread(row: &rusqlite::Row<'_>) -> rusqlite::Result<ThreadRow> {
        Ok(ThreadRow {
            id: row.get(0)?,
            account_id: row.get(1)?,
            subject: row.get(2)?,
            newest_date: row.get(3)?,
            oldest_date: row.get(4)?,
            email_count: row.get(5)?,
            unread_count: row.get(6)?,
            has_attachments: row.get(7)?,
            snippet: row.get(8)?,
        })
    }
}
```

### Step 5.2: Export `ThreadRow` from `lib.rs`

Add to `lib.rs`:

```rust
pub use threads::ThreadRow;
```

### Step 5.3: Verify

**Command:** `cargo check -p inboxly-store`

### Step 5.4: Commit

**Command:** `git add -A && git commit -m "feat(store): threads table CRUD with upsert and pagination (M3)"`

---

## Task 6: Thread state CRUD

### Step 6.1: Implement `inboxly-store/src/thread_state.rs`

Replace the placeholder with:

```rust
use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct ThreadStateRow {
    pub thread_id: String,
    pub pinned: bool,
    pub done: bool,
    pub snoozed_until: Option<i64>,
    pub snoozed_location_json: Option<String>,
    pub bundle_id: Option<String>,
}

impl Store {
    pub fn insert_thread_state(&self, state: &ThreadStateRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO thread_state (thread_id, pinned, done, snoozed_until, snoozed_location_json, bundle_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                state.thread_id,
                state.pinned,
                state.done,
                state.snoozed_until,
                state.snoozed_location_json,
                state.bundle_id,
            ],
        )?;
        Ok(())
    }

    pub fn get_thread_state(&self, thread_id: &str) -> Result<ThreadStateRow> {
        self.conn()
            .query_row(
                "SELECT thread_id, pinned, done, snoozed_until, snoozed_location_json, bundle_id
                 FROM thread_state WHERE thread_id = ?1",
                params![thread_id],
                Self::row_to_thread_state,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("thread_state {thread_id}"))
                }
                other => StoreError::Sqlite(other),
            })
    }

    /// Get or create thread state. Returns existing row or inserts default and returns it.
    pub fn get_or_create_thread_state(&self, thread_id: &str) -> Result<ThreadStateRow> {
        match self.get_thread_state(thread_id) {
            Ok(state) => Ok(state),
            Err(StoreError::NotFound(_)) => {
                let state = ThreadStateRow {
                    thread_id: thread_id.to_string(),
                    pinned: false,
                    done: false,
                    snoozed_until: None,
                    snoozed_location_json: None,
                    bundle_id: None,
                };
                self.insert_thread_state(&state)?;
                Ok(state)
            }
            Err(e) => Err(e),
        }
    }

    pub fn set_thread_pinned(&self, thread_id: &str, pinned: bool) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE thread_state SET pinned = ?2 WHERE thread_id = ?1",
            params![thread_id, pinned],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("thread_state {thread_id}")));
        }
        Ok(())
    }

    pub fn set_thread_done(&self, thread_id: &str, done: bool) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE thread_state SET done = ?2 WHERE thread_id = ?1",
            params![thread_id, done],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("thread_state {thread_id}")));
        }
        Ok(())
    }

    pub fn set_thread_snoozed(
        &self,
        thread_id: &str,
        until: Option<i64>,
        location_json: Option<&str>,
    ) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE thread_state SET snoozed_until = ?2, snoozed_location_json = ?3 WHERE thread_id = ?1",
            params![thread_id, until, location_json],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("thread_state {thread_id}")));
        }
        Ok(())
    }

    pub fn set_thread_bundle(&self, thread_id: &str, bundle_id: Option<&str>) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE thread_state SET bundle_id = ?2 WHERE thread_id = ?1",
            params![thread_id, bundle_id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("thread_state {thread_id}")));
        }
        Ok(())
    }

    /// Get all pinned, not-done threads (for the Pinned section of the inbox feed).
    pub fn get_pinned_threads(&self) -> Result<Vec<ThreadStateRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT thread_id, pinned, done, snoozed_until, snoozed_location_json, bundle_id
             FROM thread_state WHERE pinned = 1 AND done = 0",
        )?;
        let rows = stmt
            .query_map([], Self::row_to_thread_state)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get all snoozed threads (for the Snoozed view and the snooze scheduler).
    pub fn get_snoozed_threads(&self) -> Result<Vec<ThreadStateRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT thread_id, pinned, done, snoozed_until, snoozed_location_json, bundle_id
             FROM thread_state WHERE snoozed_until IS NOT NULL",
        )?;
        let rows = stmt
            .query_map([], Self::row_to_thread_state)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get all threads assigned to a bundle.
    pub fn get_threads_by_bundle(&self, bundle_id: &str) -> Result<Vec<ThreadStateRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT thread_id, pinned, done, snoozed_until, snoozed_location_json, bundle_id
             FROM thread_state WHERE bundle_id = ?1",
        )?;
        let rows = stmt
            .query_map(params![bundle_id], Self::row_to_thread_state)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_thread_state(&self, thread_id: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM thread_state WHERE thread_id = ?1",
            params![thread_id],
        )?;
        Ok(())
    }

    fn row_to_thread_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<ThreadStateRow> {
        Ok(ThreadStateRow {
            thread_id: row.get(0)?,
            pinned: row.get(1)?,
            done: row.get(2)?,
            snoozed_until: row.get(3)?,
            snoozed_location_json: row.get(4)?,
            bundle_id: row.get(5)?,
        })
    }
}
```

### Step 6.2: Export `ThreadStateRow` from `lib.rs`

Add to `lib.rs`:

```rust
pub use thread_state::ThreadStateRow;
```

### Step 6.3: Verify

**Command:** `cargo check -p inboxly-store`

### Step 6.4: Commit

**Command:** `git add -A && git commit -m "feat(store): thread_state table CRUD with pin/done/snooze/bundle helpers (M3)"`

---

## Task 7: Sync state CRUD

### Step 7.1: Implement `inboxly-store/src/sync_state.rs`

Replace the placeholder with:

```rust
use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct SyncStateRow {
    pub account_id: String,
    pub folder_name: String,
    pub uid_validity: Option<i64>,
    pub uid_next: Option<i64>,
    pub highest_modseq: Option<i64>,
    pub last_sync: Option<i64>,
}

impl Store {
    pub fn upsert_sync_state(&self, state: &SyncStateRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO sync_state (account_id, folder_name, uid_validity, uid_next, highest_modseq, last_sync)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(account_id, folder_name) DO UPDATE SET
                uid_validity = excluded.uid_validity,
                uid_next = excluded.uid_next,
                highest_modseq = excluded.highest_modseq,
                last_sync = excluded.last_sync",
            params![
                state.account_id,
                state.folder_name,
                state.uid_validity,
                state.uid_next,
                state.highest_modseq,
                state.last_sync,
            ],
        )?;
        Ok(())
    }

    pub fn get_sync_state(
        &self,
        account_id: &str,
        folder_name: &str,
    ) -> Result<Option<SyncStateRow>> {
        let result = self.conn().query_row(
            "SELECT account_id, folder_name, uid_validity, uid_next, highest_modseq, last_sync
             FROM sync_state WHERE account_id = ?1 AND folder_name = ?2",
            params![account_id, folder_name],
            |row| {
                Ok(SyncStateRow {
                    account_id: row.get(0)?,
                    folder_name: row.get(1)?,
                    uid_validity: row.get(2)?,
                    uid_next: row.get(3)?,
                    highest_modseq: row.get(4)?,
                    last_sync: row.get(5)?,
                })
            },
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Sqlite(e)),
        }
    }

    /// Get all sync states for an account (one per synced folder).
    pub fn get_sync_states_for_account(&self, account_id: &str) -> Result<Vec<SyncStateRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT account_id, folder_name, uid_validity, uid_next, highest_modseq, last_sync
             FROM sync_state WHERE account_id = ?1",
        )?;
        let rows = stmt
            .query_map(params![account_id], |row| {
                Ok(SyncStateRow {
                    account_id: row.get(0)?,
                    folder_name: row.get(1)?,
                    uid_validity: row.get(2)?,
                    uid_next: row.get(3)?,
                    highest_modseq: row.get(4)?,
                    last_sync: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_sync_state(&self, account_id: &str, folder_name: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM sync_state WHERE account_id = ?1 AND folder_name = ?2",
            params![account_id, folder_name],
        )?;
        Ok(())
    }

    /// Delete all sync state for an account (used when removing an account).
    pub fn delete_sync_states_for_account(&self, account_id: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM sync_state WHERE account_id = ?1",
            params![account_id],
        )?;
        Ok(())
    }
}
```

### Step 7.2: Export `SyncStateRow` from `lib.rs`

Add to `lib.rs`:

```rust
pub use sync_state::SyncStateRow;
```

### Step 7.3: Verify and commit

**Command:** `cargo check -p inboxly-store && git add -A && git commit -m "feat(store): sync_state table CRUD with upsert (M3)"`

---

## Task 8: Contacts CRUD

### Step 8.1: Implement `inboxly-store/src/contacts.rs`

Replace the placeholder with:

```rust
use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct ContactRow {
    pub address: String,
    pub display_name: Option<String>,
    pub avatar_letter: Option<String>,
    pub avatar_color_index: i64,
    pub last_seen: i64,
}

impl Store {
    /// Insert or update a contact. Called on email ingest to keep the contact
    /// cache fresh without re-parsing headers.
    pub fn upsert_contact(&self, contact: &ContactRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO contacts (address, display_name, avatar_letter, avatar_color_index, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(address) DO UPDATE SET
                display_name = COALESCE(excluded.display_name, contacts.display_name),
                avatar_letter = COALESCE(excluded.avatar_letter, contacts.avatar_letter),
                avatar_color_index = excluded.avatar_color_index,
                last_seen = MAX(excluded.last_seen, contacts.last_seen)",
            params![
                contact.address,
                contact.display_name,
                contact.avatar_letter,
                contact.avatar_color_index,
                contact.last_seen,
            ],
        )?;
        Ok(())
    }

    pub fn get_contact(&self, address: &str) -> Result<Option<ContactRow>> {
        let result = self.conn().query_row(
            "SELECT address, display_name, avatar_letter, avatar_color_index, last_seen
             FROM contacts WHERE address = ?1",
            params![address],
            |row| {
                Ok(ContactRow {
                    address: row.get(0)?,
                    display_name: row.get(1)?,
                    avatar_letter: row.get(2)?,
                    avatar_color_index: row.get(3)?,
                    last_seen: row.get(4)?,
                })
            },
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Sqlite(e)),
        }
    }

    /// Search contacts by address or display name prefix (for autocomplete).
    pub fn search_contacts(&self, query: &str, limit: i64) -> Result<Vec<ContactRow>> {
        let pattern = format!("{query}%");
        let mut stmt = self.conn().prepare(
            "SELECT address, display_name, avatar_letter, avatar_color_index, last_seen
             FROM contacts
             WHERE address LIKE ?1 OR display_name LIKE ?1
             ORDER BY last_seen DESC
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![pattern, limit], |row| {
                Ok(ContactRow {
                    address: row.get(0)?,
                    display_name: row.get(1)?,
                    avatar_letter: row.get(2)?,
                    avatar_color_index: row.get(3)?,
                    last_seen: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_contact(&self, address: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM contacts WHERE address = ?1",
            params![address],
        )?;
        Ok(())
    }
}
```

### Step 8.2: Export `ContactRow` from `lib.rs`

Add to `lib.rs`:

```rust
pub use contacts::ContactRow;
```

### Step 8.3: Verify and commit

**Command:** `cargo check -p inboxly-store && git add -A && git commit -m "feat(store): contacts table CRUD with upsert and search (M3)"`

---

## Task 9: Bundles CRUD

### Step 9.1: Implement `inboxly-store/src/bundles.rs`

Replace the placeholder with:

```rust
use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct BundleRow {
    pub id: String,
    pub category: String,
    pub name: String,
    pub color: String,
    pub badge_color: String,
    pub visibility: String,
    pub throttle: String,
    pub sort_order: i64,
}

impl Store {
    pub fn insert_bundle(&self, bundle: &BundleRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO bundles (id, category, name, color, badge_color, visibility, throttle, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                bundle.id,
                bundle.category,
                bundle.name,
                bundle.color,
                bundle.badge_color,
                bundle.visibility,
                bundle.throttle,
                bundle.sort_order,
            ],
        )?;
        Ok(())
    }

    pub fn get_bundle(&self, id: &str) -> Result<BundleRow> {
        self.conn()
            .query_row(
                "SELECT id, category, name, color, badge_color, visibility, throttle, sort_order
                 FROM bundles WHERE id = ?1",
                params![id],
                Self::row_to_bundle,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("bundle {id}"))
                }
                other => StoreError::Sqlite(other),
            })
    }

    /// Get all bundles ordered by sort_order.
    pub fn list_bundles(&self) -> Result<Vec<BundleRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, category, name, color, badge_color, visibility, throttle, sort_order
             FROM bundles ORDER BY sort_order ASC",
        )?;
        let rows = stmt
            .query_map([], Self::row_to_bundle)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Find a bundle by category name (e.g., "Social", "Promos").
    pub fn get_bundle_by_category(&self, category: &str) -> Result<Option<BundleRow>> {
        let result = self.conn().query_row(
            "SELECT id, category, name, color, badge_color, visibility, throttle, sort_order
             FROM bundles WHERE category = ?1",
            params![category],
            Self::row_to_bundle,
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Sqlite(e)),
        }
    }

    pub fn update_bundle(&self, bundle: &BundleRow) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE bundles SET category = ?2, name = ?3, color = ?4, badge_color = ?5,
             visibility = ?6, throttle = ?7, sort_order = ?8
             WHERE id = ?1",
            params![
                bundle.id,
                bundle.category,
                bundle.name,
                bundle.color,
                bundle.badge_color,
                bundle.visibility,
                bundle.throttle,
                bundle.sort_order,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("bundle {}", bundle.id)));
        }
        Ok(())
    }

    pub fn delete_bundle(&self, id: &str) -> Result<()> {
        let changed = self.conn().execute(
            "DELETE FROM bundles WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("bundle {id}")));
        }
        Ok(())
    }

    fn row_to_bundle(row: &rusqlite::Row<'_>) -> rusqlite::Result<BundleRow> {
        Ok(BundleRow {
            id: row.get(0)?,
            category: row.get(1)?,
            name: row.get(2)?,
            color: row.get(3)?,
            badge_color: row.get(4)?,
            visibility: row.get(5)?,
            throttle: row.get(6)?,
            sort_order: row.get(7)?,
        })
    }
}
```

### Step 9.2: Export `BundleRow` from `lib.rs`

Add to `lib.rs`:

```rust
pub use bundles::BundleRow;
```

### Step 9.3: Verify and commit

**Command:** `cargo check -p inboxly-store && git add -A && git commit -m "feat(store): bundles table CRUD with category lookup (M3)"`

---

## Task 10: Bundle rules CRUD

### Step 10.1: Implement `inboxly-store/src/bundle_rules.rs`

Replace the placeholder with:

```rust
use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct BundleRuleRow {
    pub id: String,
    pub bundle_id: String,
    pub field: String,      // "From", "To", "Subject", "Header", "Body"
    pub operator: String,   // "Contains", "Equals", "Matches", "Domain"
    pub value: String,
    pub priority: i64,
}

impl Store {
    pub fn insert_bundle_rule(&self, rule: &BundleRuleRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO bundle_rules (id, bundle_id, field, operator, value, priority)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                rule.id,
                rule.bundle_id,
                rule.field,
                rule.operator,
                rule.value,
                rule.priority,
            ],
        )?;
        Ok(())
    }

    pub fn get_bundle_rule(&self, id: &str) -> Result<BundleRuleRow> {
        self.conn()
            .query_row(
                "SELECT id, bundle_id, field, operator, value, priority
                 FROM bundle_rules WHERE id = ?1",
                params![id],
                Self::row_to_bundle_rule,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("bundle_rule {id}"))
                }
                other => StoreError::Sqlite(other),
            })
    }

    /// Get all rules for a bundle, ordered by priority descending (highest first).
    pub fn get_rules_for_bundle(&self, bundle_id: &str) -> Result<Vec<BundleRuleRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, bundle_id, field, operator, value, priority
             FROM bundle_rules WHERE bundle_id = ?1 ORDER BY priority DESC",
        )?;
        let rows = stmt
            .query_map(params![bundle_id], Self::row_to_bundle_rule)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get ALL rules across all bundles, ordered by priority descending.
    /// Used by the bundler engine to evaluate all rules against an email.
    pub fn get_all_bundle_rules(&self) -> Result<Vec<BundleRuleRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, bundle_id, field, operator, value, priority
             FROM bundle_rules ORDER BY priority DESC",
        )?;
        let rows = stmt
            .query_map([], Self::row_to_bundle_rule)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn update_bundle_rule(&self, rule: &BundleRuleRow) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE bundle_rules SET bundle_id = ?2, field = ?3, operator = ?4, value = ?5, priority = ?6
             WHERE id = ?1",
            params![
                rule.id,
                rule.bundle_id,
                rule.field,
                rule.operator,
                rule.value,
                rule.priority,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("bundle_rule {}", rule.id)));
        }
        Ok(())
    }

    pub fn delete_bundle_rule(&self, id: &str) -> Result<()> {
        let changed = self.conn().execute(
            "DELETE FROM bundle_rules WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("bundle_rule {id}")));
        }
        Ok(())
    }

    fn row_to_bundle_rule(row: &rusqlite::Row<'_>) -> rusqlite::Result<BundleRuleRow> {
        Ok(BundleRuleRow {
            id: row.get(0)?,
            bundle_id: row.get(1)?,
            field: row.get(2)?,
            operator: row.get(3)?,
            value: row.get(4)?,
            priority: row.get(5)?,
        })
    }
}
```

### Step 10.2: Export `BundleRuleRow` from `lib.rs`

Add to `lib.rs`:

```rust
pub use bundle_rules::BundleRuleRow;
```

### Step 10.3: Verify and commit

**Command:** `cargo check -p inboxly-store && git add -A && git commit -m "feat(store): bundle_rules table CRUD (M3)"`

---

## Task 11: Sender affinity CRUD

### Step 11.1: Implement `inboxly-store/src/sender_affinity.rs`

Replace the placeholder with:

```rust
use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct SenderAffinityRow {
    pub sender_domain: String,
    pub sender_address: String,
    pub bundle_category: String,
    pub confidence: f64,
    pub learned_at: i64,
}

impl Store {
    /// Insert or update sender affinity. Confidence increases on repeated actions.
    pub fn upsert_sender_affinity(&self, affinity: &SenderAffinityRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO sender_affinity (sender_domain, sender_address, bundle_category, confidence, learned_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(sender_address, bundle_category) DO UPDATE SET
                confidence = excluded.confidence,
                learned_at = excluded.learned_at",
            params![
                affinity.sender_domain,
                affinity.sender_address,
                affinity.bundle_category,
                affinity.confidence,
                affinity.learned_at,
            ],
        )?;
        Ok(())
    }

    /// Get the best affinity for a sender address (highest confidence).
    pub fn get_sender_affinity(&self, sender_address: &str) -> Result<Option<SenderAffinityRow>> {
        let result = self.conn().query_row(
            "SELECT sender_domain, sender_address, bundle_category, confidence, learned_at
             FROM sender_affinity WHERE sender_address = ?1
             ORDER BY confidence DESC LIMIT 1",
            params![sender_address],
            Self::row_to_sender_affinity,
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Sqlite(e)),
        }
    }

    /// Get all affinities for a domain (for domain-level fallback when no exact address match).
    pub fn get_affinities_by_domain(&self, domain: &str) -> Result<Vec<SenderAffinityRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT sender_domain, sender_address, bundle_category, confidence, learned_at
             FROM sender_affinity WHERE sender_domain = ?1
             ORDER BY confidence DESC",
        )?;
        let rows = stmt
            .query_map(params![domain], Self::row_to_sender_affinity)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_sender_affinity(&self, sender_address: &str, bundle_category: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM sender_affinity WHERE sender_address = ?1 AND bundle_category = ?2",
            params![sender_address, bundle_category],
        )?;
        Ok(())
    }

    fn row_to_sender_affinity(row: &rusqlite::Row<'_>) -> rusqlite::Result<SenderAffinityRow> {
        Ok(SenderAffinityRow {
            sender_domain: row.get(0)?,
            sender_address: row.get(1)?,
            bundle_category: row.get(2)?,
            confidence: row.get(3)?,
            learned_at: row.get(4)?,
        })
    }
}
```

### Step 11.2: Export `SenderAffinityRow` from `lib.rs`

Add to `lib.rs`:

```rust
pub use sender_affinity::SenderAffinityRow;
```

### Step 11.3: Verify and commit

**Command:** `cargo check -p inboxly-store && git add -A && git commit -m "feat(store): sender_affinity table CRUD with domain lookup (M3)"`

---

## Task 12: Reminders CRUD

### Step 12.1: Implement `inboxly-store/src/reminders.rs`

Replace the placeholder with:

```rust
use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct ReminderRow {
    pub id: String,
    pub title: String,
    pub due_at: Option<i64>,
    pub location_lat: Option<f64>,
    pub location_lng: Option<f64>,
    pub location_label: Option<String>,
    pub recurring: Option<String>,
    pub done: bool,
}

impl Store {
    pub fn insert_reminder(&self, reminder: &ReminderRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO reminders (id, title, due_at, location_lat, location_lng, location_label, recurring, done)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                reminder.id,
                reminder.title,
                reminder.due_at,
                reminder.location_lat,
                reminder.location_lng,
                reminder.location_label,
                reminder.recurring,
                reminder.done,
            ],
        )?;
        Ok(())
    }

    pub fn get_reminder(&self, id: &str) -> Result<ReminderRow> {
        self.conn()
            .query_row(
                "SELECT id, title, due_at, location_lat, location_lng, location_label, recurring, done
                 FROM reminders WHERE id = ?1",
                params![id],
                Self::row_to_reminder,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("reminder {id}"))
                }
                other => StoreError::Sqlite(other),
            })
    }

    /// Get all active (not done) reminders, ordered by due_at ascending.
    pub fn get_active_reminders(&self) -> Result<Vec<ReminderRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, title, due_at, location_lat, location_lng, location_label, recurring, done
             FROM reminders WHERE done = 0 ORDER BY due_at ASC",
        )?;
        let rows = stmt
            .query_map([], Self::row_to_reminder)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get reminders that are due (due_at <= now). Used by the snooze scheduler.
    pub fn get_due_reminders(&self, now: i64) -> Result<Vec<ReminderRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, title, due_at, location_lat, location_lng, location_label, recurring, done
             FROM reminders WHERE done = 0 AND due_at IS NOT NULL AND due_at <= ?1",
        )?;
        let rows = stmt
            .query_map(params![now], Self::row_to_reminder)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get location-based reminders (for geofence checking).
    pub fn get_location_reminders(&self) -> Result<Vec<ReminderRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, title, due_at, location_lat, location_lng, location_label, recurring, done
             FROM reminders WHERE done = 0 AND location_lat IS NOT NULL",
        )?;
        let rows = stmt
            .query_map([], Self::row_to_reminder)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn set_reminder_done(&self, id: &str, done: bool) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE reminders SET done = ?2 WHERE id = ?1",
            params![id, done],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("reminder {id}")));
        }
        Ok(())
    }

    pub fn update_reminder(&self, reminder: &ReminderRow) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE reminders SET title = ?2, due_at = ?3, location_lat = ?4, location_lng = ?5,
             location_label = ?6, recurring = ?7, done = ?8
             WHERE id = ?1",
            params![
                reminder.id,
                reminder.title,
                reminder.due_at,
                reminder.location_lat,
                reminder.location_lng,
                reminder.location_label,
                reminder.recurring,
                reminder.done,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("reminder {}", reminder.id)));
        }
        Ok(())
    }

    pub fn delete_reminder(&self, id: &str) -> Result<()> {
        let changed = self.conn().execute(
            "DELETE FROM reminders WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("reminder {id}")));
        }
        Ok(())
    }

    fn row_to_reminder(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReminderRow> {
        Ok(ReminderRow {
            id: row.get(0)?,
            title: row.get(1)?,
            due_at: row.get(2)?,
            location_lat: row.get(3)?,
            location_lng: row.get(4)?,
            location_label: row.get(5)?,
            recurring: row.get(6)?,
            done: row.get(7)?,
        })
    }
}
```

### Step 12.2: Export `ReminderRow` from `lib.rs`

Add to `lib.rs`:

```rust
pub use reminders::ReminderRow;
```

### Step 12.3: Verify and commit

**Command:** `cargo check -p inboxly-store && git add -A && git commit -m "feat(store): reminders table CRUD with due/location queries (M3)"`

---

## Task 13: Highlights CRUD

### Step 13.1: Implement `inboxly-store/src/highlights.rs`

Replace the placeholder with:

```rust
use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

/// Row representation for the highlights table.
///
/// `highlight_type` is one of: "TrackingNumber", "Flight", "Hotel", "Event", "Payment"
/// `data_json` contains the type-specific fields as JSON (see Highlight enum in core).
#[derive(Debug, Clone)]
pub struct HighlightRow {
    pub id: Option<i64>,     // AUTOINCREMENT, None on insert
    pub thread_id: String,
    pub highlight_type: String,
    pub data_json: String,
}

impl Store {
    pub fn insert_highlight(&self, highlight: &HighlightRow) -> Result<i64> {
        self.conn().execute(
            "INSERT INTO highlights (thread_id, highlight_type, data_json)
             VALUES (?1, ?2, ?3)",
            params![
                highlight.thread_id,
                highlight.highlight_type,
                highlight.data_json,
            ],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    /// Get all highlights for a thread.
    pub fn get_highlights_for_thread(&self, thread_id: &str) -> Result<Vec<HighlightRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, thread_id, highlight_type, data_json
             FROM highlights WHERE thread_id = ?1",
        )?;
        let rows = stmt
            .query_map(params![thread_id], |row| {
                Ok(HighlightRow {
                    id: row.get(0)?,
                    thread_id: row.get(1)?,
                    highlight_type: row.get(2)?,
                    data_json: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get all highlights of a given type (e.g., all "Flight" highlights for trip assembly).
    pub fn get_highlights_by_type(&self, highlight_type: &str) -> Result<Vec<HighlightRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, thread_id, highlight_type, data_json
             FROM highlights WHERE highlight_type = ?1",
        )?;
        let rows = stmt
            .query_map(params![highlight_type], |row| {
                Ok(HighlightRow {
                    id: row.get(0)?,
                    thread_id: row.get(1)?,
                    highlight_type: row.get(2)?,
                    data_json: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Delete all highlights for a thread (used when re-extracting after body download).
    pub fn delete_highlights_for_thread(&self, thread_id: &str) -> Result<u64> {
        let changed = self.conn().execute(
            "DELETE FROM highlights WHERE thread_id = ?1",
            params![thread_id],
        )?;
        Ok(changed as u64)
    }

    pub fn delete_highlight(&self, id: i64) -> Result<()> {
        let changed = self.conn().execute(
            "DELETE FROM highlights WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("highlight {id}")));
        }
        Ok(())
    }
}
```

### Step 13.2: Export `HighlightRow` from `lib.rs`

Add to `lib.rs`:

```rust
pub use highlights::HighlightRow;
```

### Step 13.3: Verify and commit

**Command:** `cargo check -p inboxly-store && git add -A && git commit -m "feat(store): highlights table CRUD with type-based query (M3)"`

---

## Task 14: Settings CRUD

### Step 14.1: Implement `inboxly-store/src/settings.rs`

Replace the placeholder with:

```rust
use rusqlite::params;

use crate::error::Result;
use crate::store::Store;

impl Store {
    /// Get a setting value by key. Returns None if not set.
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let result = self.conn().query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Set a setting value. Inserts or updates.
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Delete a setting.
    pub fn delete_setting(&self, key: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM settings WHERE key = ?1",
            params![key],
        )?;
        Ok(())
    }

    /// Get all settings as key-value pairs.
    pub fn get_all_settings(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn().prepare(
            "SELECT key, value FROM settings ORDER BY key",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
```

### Step 14.2: Verify and commit

**Command:** `cargo check -p inboxly-store && git add -A && git commit -m "feat(store): settings table CRUD (M3)"`

---

## Task 15: Offline queue CRUD

### Step 15.1: Implement `inboxly-store/src/offline_queue.rs`

Replace the placeholder with:

```rust
use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct OfflineQueueRow {
    pub id: Option<i64>,     // AUTOINCREMENT, None on insert
    pub action: String,
    pub payload_json: String,
    pub created_at: i64,
}

impl Store {
    /// Enqueue an offline action for replay on reconnect.
    pub fn enqueue_offline_action(&self, action: &str, payload_json: &str) -> Result<i64> {
        self.conn().execute(
            "INSERT INTO offline_queue (action, payload_json) VALUES (?1, ?2)",
            params![action, payload_json],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    /// Get all queued actions in FIFO order.
    pub fn get_offline_queue(&self) -> Result<Vec<OfflineQueueRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, action, payload_json, created_at
             FROM offline_queue ORDER BY id ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(OfflineQueueRow {
                    id: row.get(0)?,
                    action: row.get(1)?,
                    payload_json: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Remove a successfully replayed action from the queue.
    pub fn dequeue_offline_action(&self, id: i64) -> Result<()> {
        let changed = self.conn().execute(
            "DELETE FROM offline_queue WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("offline_queue {id}")));
        }
        Ok(())
    }

    /// Clear the entire offline queue (e.g., after successful full replay).
    pub fn clear_offline_queue(&self) -> Result<u64> {
        let changed = self.conn().execute("DELETE FROM offline_queue", [])?;
        Ok(changed as u64)
    }

    /// Count pending offline actions.
    pub fn count_offline_queue(&self) -> Result<i64> {
        let count: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM offline_queue",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}
```

### Step 15.2: Export `OfflineQueueRow` from `lib.rs`

Add to `lib.rs`:

```rust
pub use offline_queue::OfflineQueueRow;
```

### Step 15.3: Verify and commit

**Command:** `cargo check -p inboxly-store && git add -A && git commit -m "feat(store): offline_queue table CRUD with FIFO ordering (M3)"`

---

## Task 16: Database rebuild capability

### Step 16.1: Add `rebuild` method to `Store`

Add to the end of `inboxly-store/src/store.rs`, inside the `impl Store` block:

```rust
    /// Drop and recreate all tables. Used for full database rebuild from Maildir.
    ///
    /// **Destructive** — all data is lost. Caller is responsible for re-indexing
    /// from Maildir files after calling this.
    pub fn rebuild(&mut self) -> Result<()> {
        tracing::warn!("Rebuilding database — dropping all tables");
        self.conn.execute_batch(
            "DROP TABLE IF EXISTS offline_queue;
             DROP TABLE IF EXISTS highlights;
             DROP TABLE IF EXISTS settings;
             DROP TABLE IF EXISTS reminders;
             DROP TABLE IF EXISTS sender_affinity;
             DROP TABLE IF EXISTS bundle_rules;
             DROP TABLE IF EXISTS thread_state;
             DROP TABLE IF EXISTS sync_state;
             DROP TABLE IF EXISTS emails;
             DROP TABLE IF EXISTS threads;
             DROP TABLE IF EXISTS bundles;
             DROP TABLE IF EXISTS contacts;
             DROP TABLE IF EXISTS accounts;
             PRAGMA user_version = 0;"
        )?;
        crate::migrations::run(self)?;
        tracing::info!("Database rebuilt successfully");
        Ok(())
    }
```

### Step 16.2: Verify and commit

**Command:** `cargo check -p inboxly-store && git add -A && git commit -m "feat(store): database rebuild capability for Maildir recovery (M3)"`

---

## Task 17: Final `lib.rs` assembly

### Step 17.1: Write the final `inboxly-store/src/lib.rs`

Ensure `lib.rs` has the complete set of module declarations and public exports:

```rust
//! SQLite storage backend for Inboxly.
//!
//! Owns the metadata database: emails, threads, accounts, bundles,
//! sync state, reminders, highlights, settings, and offline queue.

mod error;
mod migrations;
mod store;

mod accounts;
mod bundles;
mod bundle_rules;
mod contacts;
mod emails;
mod highlights;
mod offline_queue;
mod reminders;
mod sender_affinity;
mod settings;
mod sync_state;
mod thread_state;
mod threads;

pub use error::{StoreError, Result};
pub use store::Store;

pub use accounts::AccountRow;
pub use bundles::BundleRow;
pub use bundle_rules::BundleRuleRow;
pub use contacts::ContactRow;
pub use emails::{EmailRow, flags};
pub use highlights::HighlightRow;
pub use offline_queue::OfflineQueueRow;
pub use reminders::ReminderRow;
pub use sender_affinity::SenderAffinityRow;
pub use sync_state::SyncStateRow;
pub use thread_state::ThreadStateRow;
pub use threads::ThreadRow;
```

### Step 17.2: Verify the full crate compiles

**Command:** `cargo check -p inboxly-store`

### Step 17.3: Commit

**Command:** `git add -A && git commit -m "feat(store): finalize lib.rs exports for all table row types (M3)"`

---

## Task 18: Integration tests

### Step 18.1: Create test file `inboxly-store/tests/integration.rs`

```rust
use inboxly_store::*;

fn test_store() -> Store {
    Store::open_in_memory().expect("failed to open in-memory store")
}

fn sample_account() -> AccountRow {
    AccountRow {
        id: "acct-001".into(),
        email: "alice@example.com".into(),
        display_name: "Alice".into(),
        provider: "generic".into(),
        auth_method: "password".into(),
        imap_host: "imap.example.com".into(),
        imap_port: 993,
        smtp_host: "smtp.example.com".into(),
        smtp_port: 587,
    }
}

fn sample_thread(account_id: &str) -> ThreadRow {
    ThreadRow {
        id: "thread-001".into(),
        account_id: account_id.into(),
        subject: "Hello World".into(),
        newest_date: 1710000000,
        oldest_date: 1710000000,
        email_count: 1,
        unread_count: 1,
        has_attachments: false,
        snippet: "Hey there...".into(),
    }
}

fn sample_email(account_id: &str, thread_id: &str) -> EmailRow {
    EmailRow {
        id: "<msg001@example.com>".into(),
        account_id: account_id.into(),
        thread_id: thread_id.into(),
        from_name: Some("Bob".into()),
        from_address: "bob@example.com".into(),
        to_json: r#"[{"address":"alice@example.com","name":"Alice"}]"#.into(),
        cc_json: "[]".into(),
        subject: "Hello World".into(),
        snippet: "Hey there...".into(),
        date: 1710000000,
        maildir_path: "/mail/cur/msg001:2,S".into(),
        flags: flags::READ,
        size_bytes: 4096,
        imap_uid: 42,
        imap_folder: "INBOX".into(),
        has_attachments: false,
        message_id_header: Some("<msg001@example.com>".into()),
        in_reply_to: None,
        references_json: None,
    }
}

// === Account tests ===

#[test]
fn test_account_crud() {
    let store = test_store();
    let account = sample_account();

    store.insert_account(&account).unwrap();
    let fetched = store.get_account("acct-001").unwrap();
    assert_eq!(fetched.email, "alice@example.com");
    assert_eq!(fetched.imap_port, 993);

    let accounts = store.list_accounts().unwrap();
    assert_eq!(accounts.len(), 1);

    store.delete_account("acct-001").unwrap();
    assert!(store.get_account("acct-001").is_err());
}

#[test]
fn test_account_not_found() {
    let store = test_store();
    let result = store.get_account("nonexistent");
    assert!(matches!(result, Err(StoreError::NotFound(_))));
}

// === Email tests ===

#[test]
fn test_email_crud() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();
    store.insert_thread(&sample_thread("acct-001")).unwrap();

    let email = sample_email("acct-001", "thread-001");
    store.insert_email(&email).unwrap();

    let fetched = store.get_email("<msg001@example.com>").unwrap();
    assert_eq!(fetched.subject, "Hello World");
    assert_eq!(fetched.imap_uid, 42);

    // Test UID lookup
    let by_uid = store.get_email_by_uid("acct-001", "INBOX", 42).unwrap();
    assert!(by_uid.is_some());

    // Test flag update
    store.update_email_flags("<msg001@example.com>", flags::READ | flags::STARRED).unwrap();
    let updated = store.get_email("<msg001@example.com>").unwrap();
    assert_eq!(updated.flags, flags::READ | flags::STARRED);

    // Test thread reassignment
    let thread2 = ThreadRow { id: "thread-002".into(), ..sample_thread("acct-001") };
    store.insert_thread(&thread2).unwrap();
    store.update_email_thread("<msg001@example.com>", "thread-002").unwrap();
    let moved = store.get_email("<msg001@example.com>").unwrap();
    assert_eq!(moved.thread_id, "thread-002");

    store.delete_email("<msg001@example.com>").unwrap();
    assert!(store.get_email("<msg001@example.com>").is_err());
}

#[test]
fn test_email_unique_constraint() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();
    store.insert_thread(&sample_thread("acct-001")).unwrap();

    let email = sample_email("acct-001", "thread-001");
    store.insert_email(&email).unwrap();

    // Same account_id + imap_folder + imap_uid should fail
    let mut dup = email.clone();
    dup.id = "<msg002@example.com>".into();
    let result = store.insert_email(&dup);
    assert!(result.is_err());
}

// === Thread tests ===

#[test]
fn test_thread_crud() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();

    let thread = sample_thread("acct-001");
    store.insert_thread(&thread).unwrap();

    let fetched = store.get_thread("thread-001").unwrap();
    assert_eq!(fetched.subject, "Hello World");

    // Test upsert
    let mut updated = thread.clone();
    updated.email_count = 5;
    updated.snippet = "Updated snippet".into();
    store.upsert_thread(&updated).unwrap();
    let re_fetched = store.get_thread("thread-001").unwrap();
    assert_eq!(re_fetched.email_count, 5);
    assert_eq!(re_fetched.snippet, "Updated snippet");
}

// === Thread state tests ===

#[test]
fn test_thread_state_crud() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();
    store.insert_thread(&sample_thread("acct-001")).unwrap();

    let state = store.get_or_create_thread_state("thread-001").unwrap();
    assert!(!state.pinned);
    assert!(!state.done);

    store.set_thread_pinned("thread-001", true).unwrap();
    let pinned = store.get_thread_state("thread-001").unwrap();
    assert!(pinned.pinned);

    let pinned_list = store.get_pinned_threads().unwrap();
    assert_eq!(pinned_list.len(), 1);

    store.set_thread_done("thread-001", true).unwrap();
    // Done threads are excluded from pinned query
    let pinned_list = store.get_pinned_threads().unwrap();
    assert_eq!(pinned_list.len(), 0);
}

// === Sync state tests ===

#[test]
fn test_sync_state_upsert() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();

    let state = SyncStateRow {
        account_id: "acct-001".into(),
        folder_name: "INBOX".into(),
        uid_validity: Some(12345),
        uid_next: Some(100),
        highest_modseq: Some(999),
        last_sync: Some(1710000000),
    };
    store.upsert_sync_state(&state).unwrap();

    let fetched = store.get_sync_state("acct-001", "INBOX").unwrap().unwrap();
    assert_eq!(fetched.uid_next, Some(100));

    // Update via upsert
    let updated = SyncStateRow { uid_next: Some(200), ..state };
    store.upsert_sync_state(&updated).unwrap();
    let re_fetched = store.get_sync_state("acct-001", "INBOX").unwrap().unwrap();
    assert_eq!(re_fetched.uid_next, Some(200));
}

// === Contacts tests ===

#[test]
fn test_contact_upsert_and_search() {
    let store = test_store();
    let contact = ContactRow {
        address: "bob@example.com".into(),
        display_name: Some("Bob".into()),
        avatar_letter: Some("B".into()),
        avatar_color_index: 1,
        last_seen: 1710000000,
    };
    store.upsert_contact(&contact).unwrap();

    let fetched = store.get_contact("bob@example.com").unwrap().unwrap();
    assert_eq!(fetched.display_name, Some("Bob".into()));

    // Search
    let results = store.search_contacts("bob", 10).unwrap();
    assert_eq!(results.len(), 1);

    let results = store.search_contacts("xyz", 10).unwrap();
    assert!(results.is_empty());
}

// === Bundle tests ===

#[test]
fn test_bundle_crud() {
    let store = test_store();
    let bundle = BundleRow {
        id: "bundle-001".into(),
        category: "Social".into(),
        name: "Social".into(),
        color: "#d23f31".into(),
        badge_color: "#faebea".into(),
        visibility: "Bundled".into(),
        throttle: "Immediate".into(),
        sort_order: 0,
    };
    store.insert_bundle(&bundle).unwrap();

    let fetched = store.get_bundle("bundle-001").unwrap();
    assert_eq!(fetched.category, "Social");

    let by_cat = store.get_bundle_by_category("Social").unwrap();
    assert!(by_cat.is_some());

    let all = store.list_bundles().unwrap();
    assert_eq!(all.len(), 1);
}

// === Bundle rules tests ===

#[test]
fn test_bundle_rule_crud() {
    let store = test_store();
    let bundle = BundleRow {
        id: "bundle-001".into(),
        category: "Social".into(),
        name: "Social".into(),
        color: "#d23f31".into(),
        badge_color: "#faebea".into(),
        visibility: "Bundled".into(),
        throttle: "Immediate".into(),
        sort_order: 0,
    };
    store.insert_bundle(&bundle).unwrap();

    let rule = BundleRuleRow {
        id: "rule-001".into(),
        bundle_id: "bundle-001".into(),
        field: "From".into(),
        operator: "Domain".into(),
        value: "facebookmail.com".into(),
        priority: 10,
    };
    store.insert_bundle_rule(&rule).unwrap();

    let rules = store.get_rules_for_bundle("bundle-001").unwrap();
    assert_eq!(rules.len(), 1);

    let all_rules = store.get_all_bundle_rules().unwrap();
    assert_eq!(all_rules.len(), 1);
}

// === Sender affinity tests ===

#[test]
fn test_sender_affinity() {
    let store = test_store();
    let affinity = SenderAffinityRow {
        sender_domain: "example.com".into(),
        sender_address: "news@example.com".into(),
        bundle_category: "Promos".into(),
        confidence: 0.85,
        learned_at: 1710000000,
    };
    store.upsert_sender_affinity(&affinity).unwrap();

    let fetched = store.get_sender_affinity("news@example.com").unwrap().unwrap();
    assert_eq!(fetched.bundle_category, "Promos");
    assert!((fetched.confidence - 0.85).abs() < f64::EPSILON);

    let domain_results = store.get_affinities_by_domain("example.com").unwrap();
    assert_eq!(domain_results.len(), 1);
}

// === Reminder tests ===

#[test]
fn test_reminder_crud() {
    let store = test_store();
    let reminder = ReminderRow {
        id: "rem-001".into(),
        title: "Buy milk".into(),
        due_at: Some(1710100000),
        location_lat: None,
        location_lng: None,
        location_label: None,
        recurring: None,
        done: false,
    };
    store.insert_reminder(&reminder).unwrap();

    let active = store.get_active_reminders().unwrap();
    assert_eq!(active.len(), 1);

    let due = store.get_due_reminders(1710200000).unwrap();
    assert_eq!(due.len(), 1);

    let not_due = store.get_due_reminders(1710000000).unwrap();
    assert!(not_due.is_empty());

    store.set_reminder_done("rem-001", true).unwrap();
    let active = store.get_active_reminders().unwrap();
    assert!(active.is_empty());
}

// === Highlight tests ===

#[test]
fn test_highlight_crud() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();
    store.insert_thread(&sample_thread("acct-001")).unwrap();

    let highlight = HighlightRow {
        id: None,
        thread_id: "thread-001".into(),
        highlight_type: "TrackingNumber".into(),
        data_json: r#"{"carrier":"UPS","number":"1Z999AA10123456784"}"#.into(),
    };
    let id = store.insert_highlight(&highlight).unwrap();
    assert!(id > 0);

    let highlights = store.get_highlights_for_thread("thread-001").unwrap();
    assert_eq!(highlights.len(), 1);

    let by_type = store.get_highlights_by_type("TrackingNumber").unwrap();
    assert_eq!(by_type.len(), 1);

    let deleted = store.delete_highlights_for_thread("thread-001").unwrap();
    assert_eq!(deleted, 1);
}

// === Settings tests ===

#[test]
fn test_settings_crud() {
    let store = test_store();

    assert!(store.get_setting("theme").unwrap().is_none());

    store.set_setting("theme", "dark").unwrap();
    assert_eq!(store.get_setting("theme").unwrap().unwrap(), "dark");

    // Upsert
    store.set_setting("theme", "light").unwrap();
    assert_eq!(store.get_setting("theme").unwrap().unwrap(), "light");

    let all = store.get_all_settings().unwrap();
    assert_eq!(all.len(), 1);

    store.delete_setting("theme").unwrap();
    assert!(store.get_setting("theme").unwrap().is_none());
}

// === Offline queue tests ===

#[test]
fn test_offline_queue() {
    let store = test_store();

    let id1 = store.enqueue_offline_action("mark_done", r#"{"thread_id":"t1"}"#).unwrap();
    let id2 = store.enqueue_offline_action("pin", r#"{"thread_id":"t2"}"#).unwrap();
    assert!(id2 > id1);

    let queue = store.get_offline_queue().unwrap();
    assert_eq!(queue.len(), 2);
    assert_eq!(queue[0].action, "mark_done");
    assert_eq!(queue[1].action, "pin");

    store.dequeue_offline_action(id1).unwrap();
    assert_eq!(store.count_offline_queue().unwrap(), 1);

    store.clear_offline_queue().unwrap();
    assert_eq!(store.count_offline_queue().unwrap(), 0);
}

// === Transaction tests ===

#[test]
fn test_transaction_commit() {
    let mut store = test_store();
    store.insert_account(&sample_account()).unwrap();

    store.transaction(|conn| {
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet)
             VALUES ('tx-thread', 'acct-001', 'Transaction test', 0, 0, 0, 0, 0, '')",
            [],
        )?;
        Ok(())
    }).unwrap();

    let thread = store.get_thread("tx-thread").unwrap();
    assert_eq!(thread.subject, "Transaction test");
}

#[test]
fn test_transaction_rollback() {
    let mut store = test_store();
    store.insert_account(&sample_account()).unwrap();
    store.insert_thread(&sample_thread("acct-001")).unwrap();

    let result: std::result::Result<(), StoreError> = store.transaction(|conn| {
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet)
             VALUES ('tx-thread-2', 'acct-001', 'Will rollback', 0, 0, 0, 0, 0, '')",
            [],
        )?;
        // Force an error
        Err(StoreError::Constraint("intentional failure".into()))
    });
    assert!(result.is_err());

    // Thread should not exist
    assert!(store.get_thread("tx-thread-2").is_err());
}

// === Rebuild test ===

#[test]
fn test_rebuild() {
    let mut store = test_store();
    store.insert_account(&sample_account()).unwrap();
    store.set_setting("theme", "dark").unwrap();

    store.rebuild().unwrap();

    // All data gone
    assert!(store.list_accounts().unwrap().is_empty());
    assert!(store.get_setting("theme").unwrap().is_none());

    // But tables still exist — can insert again
    store.insert_account(&sample_account()).unwrap();
    assert_eq!(store.list_accounts().unwrap().len(), 1);
}
```

### Step 18.2: Run all tests

**Command:** `cargo test -p inboxly-store`

All 17 tests must pass.

### Step 18.3: Run clippy

**Command:** `cargo clippy -p inboxly-store -- -D warnings`

Fix any warnings.

### Step 18.4: Commit

**Command:** `git add -A && git commit -m "test(store): integration tests for all tables, transactions, and rebuild (M3)"`

---

## Task 19: Final verification

### Step 19.1: Full workspace check

**Command:** `cargo check --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings`

All must pass.

### Step 19.2: Verify public API surface

Confirm `inboxly-store` exports exactly these public items:

- `Store` — open, open_in_memory, transaction, rebuild
- `StoreError`, `Result`
- `AccountRow`
- `EmailRow`, `flags` module
- `ThreadRow`
- `ThreadStateRow`
- `SyncStateRow`
- `ContactRow`
- `BundleRow`
- `BundleRuleRow`
- `SenderAffinityRow`
- `ReminderRow`
- `HighlightRow`
- `OfflineQueueRow`

### Step 19.3: Commit (if any fixes were needed)

**Command:** `git add -A && git commit -m "fix(store): address clippy/test feedback (M3)"`

---

## Summary

| Task | Description | Files | Tests |
|------|-------------|-------|-------|
| 1 | Crate skeleton | `Cargo.toml`, `lib.rs`, `error.rs` + placeholders | compile |
| 2 | DB init + migrations | `store.rs`, `migrations.rs` | compile |
| 3 | Accounts CRUD | `accounts.rs` | 2 |
| 4 | Emails CRUD | `emails.rs` | 2 |
| 5 | Threads CRUD | `threads.rs` | 1 |
| 6 | Thread state CRUD | `thread_state.rs` | 1 |
| 7 | Sync state CRUD | `sync_state.rs` | 1 |
| 8 | Contacts CRUD | `contacts.rs` | 1 |
| 9 | Bundles CRUD | `bundles.rs` | 1 |
| 10 | Bundle rules CRUD | `bundle_rules.rs` | 1 |
| 11 | Sender affinity CRUD | `sender_affinity.rs` | 1 |
| 12 | Reminders CRUD | `reminders.rs` | 1 |
| 13 | Highlights CRUD | `highlights.rs` | 1 |
| 14 | Settings CRUD | `settings.rs` | 1 |
| 15 | Offline queue CRUD | `offline_queue.rs` | 1 |
| 16 | Database rebuild | `store.rs` (extend) | 1 |
| 17 | Final lib.rs assembly | `lib.rs` | compile |
| 18 | Integration tests | `tests/integration.rs` | 17 total |
| 19 | Final verification | — | full workspace |

**Total: 19 tasks, ~17 integration tests, 16 source files**
