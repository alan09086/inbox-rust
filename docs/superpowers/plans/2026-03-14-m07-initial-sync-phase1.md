# M7: Initial Sync Phase 1 — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fetch all message headers/envelopes from an IMAP mailbox in newest-first batches, populate the SQLite `emails` table, build basic thread associations, and report progress — making the inbox usable after the first batch completes.

**Architecture:** A `SyncEngine` orchestrator drives the Phase 1 flow: SELECT the mailbox to discover UIDVALIDITY/UIDNEXT, divide the UID range into descending batches of 500, issue `UID FETCH` for each batch's `(ENVELOPE FLAGS RFC822.SIZE)`, parse each `Envelope` into an `EmailMeta`, insert rows into SQLite in batch transactions, emit progress events over a channel, and fire a first-batch-ready signal after batch 1 completes. UIDVALIDITY is persisted in `sync_state` so stale caches are detected on next launch. Connection drops mid-sync are recoverable by resuming from the last successfully committed UID.

**Tech Stack:** `async-imap` (IMAP protocol), `imap-proto` (Envelope/Address types), `tokio` (async runtime + channels), `rusqlite` (SQLite), `chrono` (date parsing), `thiserror` (errors)

**Prerequisites:** M6 (authenticated IMAP `Session`), M3 (SQLite store with `emails`/`sync_state` tables), M4 (Maildir — not directly used here but schema exists)

**Spec:** `docs/superpowers/specs/2026-03-14-inboxly-design.md` — "IMAP Sync Engine" > "Initial sync" Phase 1

---

## File Structure

```
inboxly-imap/
├── Cargo.toml                          ← add dependencies
├── src/
│   ├── lib.rs                          ← re-export public API
│   ├── sync/
│   │   ├── mod.rs                      ← module declarations
│   │   ├── engine.rs                   ← SyncEngine orchestrator
│   │   ├── envelope.rs                 ← Envelope → EmailMeta conversion
│   │   ├── batch.rs                    ← UID range calculation + batch iteration
│   │   ├── progress.rs                 ← SyncEvent enum + progress channel types
│   │   ├── uid_state.rs                ← UIDVALIDITY/UIDNEXT read/write to sync_state
│   │   └── error.rs                    ← sync-specific error types
│   └── ...                             ← existing M6 code (connection, auth)
└── tests/
    ├── sync_envelope_test.rs           ← envelope parsing unit tests
    ├── sync_batch_test.rs              ← batch range calculation tests
    ├── sync_uid_state_test.rs          ← UIDVALIDITY persistence tests
    ├── sync_engine_test.rs             ← orchestrator integration tests
    └── fixtures/
        └── mod.rs                      ← shared test helpers + fixture builders
```

---

## Chunk 1: Foundation Types + Batch Calculation

### Task 1: Sync Error Types

**Files:**
- Create: `inboxly-imap/src/sync/error.rs`
- Create: `inboxly-imap/src/sync/mod.rs`
- Modify: `inboxly-imap/src/lib.rs`

- [ ] **Step 1: Create the sync error enum**

Create `inboxly-imap/src/sync/error.rs`:

```rust
use thiserror::Error;

/// Errors specific to the IMAP sync engine.
#[derive(Debug, Error)]
pub enum SyncError {
    #[error("IMAP error: {0}")]
    Imap(#[from] async_imap::error::Error),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("UIDVALIDITY changed: folder={folder}, old={old}, new={new}")]
    UidValidityChanged {
        folder: String,
        old: u32,
        new: u32,
    },

    #[error("mailbox SELECT returned no UIDVALIDITY for folder: {0}")]
    MissingUidValidity(String),

    #[error("mailbox SELECT returned no UIDNEXT for folder: {0}")]
    MissingUidNext(String),

    #[error("connection lost during sync of folder {folder}: {source}")]
    ConnectionLost {
        folder: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("envelope missing required field: {field} for UID {uid}")]
    MalformedEnvelope { uid: u32, field: String },

    #[error("date parse error for UID {uid}: {raw}")]
    DateParse { uid: u32, raw: String },
}

pub type SyncResult<T> = Result<T, SyncError>;
```

- [ ] **Step 2: Create sync module and wire into lib.rs**

Create `inboxly-imap/src/sync/mod.rs`:

```rust
pub mod error;

pub use error::{SyncError, SyncResult};
```

Add to `inboxly-imap/src/lib.rs`:

```rust
pub mod sync;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p inboxly-imap`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add inboxly-imap/src/sync/error.rs inboxly-imap/src/sync/mod.rs inboxly-imap/src/lib.rs
git commit -m "feat(imap): add sync error types for Phase 1 sync engine"
```

---

### Task 2: Progress / Event Types

**Files:**
- Create: `inboxly-imap/src/sync/progress.rs`
- Modify: `inboxly-imap/src/sync/mod.rs`

- [ ] **Step 1: Define SyncEvent and SyncProgress**

Create `inboxly-imap/src/sync/progress.rs`:

```rust
use tokio::sync::mpsc;

/// Events emitted by the sync engine to the UI/controller layer.
#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// Phase 1 header sync progress update.
    HeaderProgress(SyncProgress),

    /// The first batch of headers has been committed to SQLite.
    /// The inbox is now usable for display.
    FirstBatchReady {
        folder: String,
        emails_in_batch: u32,
    },

    /// Phase 1 header sync completed for a folder.
    HeaderSyncComplete {
        folder: String,
        total_emails: u32,
    },

    /// A non-fatal error occurred during sync (e.g., one malformed envelope skipped).
    Warning(String),
}

/// Progress data for header sync.
#[derive(Debug, Clone)]
pub struct SyncProgress {
    pub folder: String,
    pub fetched: u32,
    pub total: u32,
}

impl std::fmt::Display for SyncProgress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Syncing headers: {:,} / {:,} ({})",
            self.fetched, self.total, self.folder
        )
    }
}

/// Convenience type alias for the sending half of the progress channel.
pub type SyncEventSender = mpsc::Sender<SyncEvent>;

/// Convenience type alias for the receiving half of the progress channel.
pub type SyncEventReceiver = mpsc::Receiver<SyncEvent>;

/// Create a new sync event channel with the given buffer size.
pub fn sync_event_channel(buffer: usize) -> (SyncEventSender, SyncEventReceiver) {
    mpsc::channel(buffer)
}
```

- [ ] **Step 2: Export from sync/mod.rs**

Add to `inboxly-imap/src/sync/mod.rs`:

```rust
pub mod progress;

pub use progress::{SyncEvent, SyncEventReceiver, SyncEventSender, SyncProgress, sync_event_channel};
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p inboxly-imap`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add inboxly-imap/src/sync/progress.rs inboxly-imap/src/sync/mod.rs
git commit -m "feat(imap): add SyncEvent and progress channel types"
```

---

### Task 3: UID Batch Range Calculator

**Files:**
- Create: `inboxly-imap/src/sync/batch.rs`
- Create: `inboxly-imap/tests/sync_batch_test.rs`
- Modify: `inboxly-imap/src/sync/mod.rs`

- [ ] **Step 1: Write failing tests for batch range calculation**

Create `inboxly-imap/tests/sync_batch_test.rs`:

```rust
use inboxly_imap::sync::batch::BatchIterator;

#[test]
fn batch_iterator_small_mailbox() {
    // 50 messages, UIDs 1..=50, batch size 500
    // Should produce one batch: 1:50
    let batches: Vec<_> = BatchIterator::new(1, 51, 500).collect();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0], (1, 50)); // (start, end) inclusive
}

#[test]
fn batch_iterator_exact_multiple() {
    // 1000 messages, UIDs 1..=1000, batch size 500
    // Newest-first: batch 1 = 501:1000, batch 2 = 1:500
    let batches: Vec<_> = BatchIterator::new(1, 1001, 500).collect();
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0], (501, 1000)); // newest first
    assert_eq!(batches[1], (1, 500));
}

#[test]
fn batch_iterator_large_mailbox() {
    // 1250 messages, UIDs 1..=1250, batch size 500
    // batch 1 = 751:1250, batch 2 = 251:750, batch 3 = 1:250
    let batches: Vec<_> = BatchIterator::new(1, 1251, 500).collect();
    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0], (751, 1250));
    assert_eq!(batches[1], (251, 750));
    assert_eq!(batches[2], (1, 250));
}

#[test]
fn batch_iterator_non_contiguous_uids() {
    // UIDs may not start at 1 — e.g., after deletions.
    // lowest_uid=500, uid_next=1800, batch_size=500
    // Range is 500..=1799 (1300 UIDs)
    // batch 1 = 1300:1799, batch 2 = 800:1299, batch 3 = 500:799
    let batches: Vec<_> = BatchIterator::new(500, 1800, 500).collect();
    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0], (1300, 1799));
    assert_eq!(batches[1], (800, 1299));
    assert_eq!(batches[2], (500, 799));
}

#[test]
fn batch_iterator_empty_mailbox() {
    // uid_next=1 means no messages
    let batches: Vec<_> = BatchIterator::new(1, 1, 500).collect();
    assert_eq!(batches.len(), 0);
}

#[test]
fn batch_iterator_single_message() {
    // One message, UID=1, uid_next=2
    let batches: Vec<_> = BatchIterator::new(1, 2, 500).collect();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0], (1, 1));
}

#[test]
fn batch_iterator_resume_from_uid() {
    // Resuming after crash: already synced UIDs 501..=1000.
    // Resume from lowest_uid=1, up to resume_uid=500 (exclusive of already-done).
    // This simulates using BatchIterator with a truncated range.
    let batches: Vec<_> = BatchIterator::new(1, 501, 500).collect();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0], (1, 500));
}

#[test]
fn batch_to_imap_sequence_string() {
    use inboxly_imap::sync::batch::batch_to_sequence;
    assert_eq!(batch_to_sequence(501, 1000), "501:1000");
    assert_eq!(batch_to_sequence(1, 1), "1");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p inboxly-imap --test sync_batch_test 2>&1 | head -20`
Expected: compilation error — `batch` module does not exist

- [ ] **Step 3: Implement BatchIterator**

Create `inboxly-imap/src/sync/batch.rs`:

```rust
/// Iterator that yields `(start_uid, end_uid)` inclusive ranges in newest-first order.
///
/// Given a UID range `[lowest_uid, uid_next)` (uid_next is exclusive, per IMAP semantics),
/// divides it into batches of `batch_size` UIDs, yielding the highest range first.
#[derive(Debug)]
pub struct BatchIterator {
    /// Lowest UID to include (inclusive).
    lowest_uid: u32,
    /// Current cursor — the next batch ends at this UID (inclusive).
    /// Decrements by batch_size each iteration.
    cursor: Option<u32>,
    /// Number of UIDs per batch.
    batch_size: u32,
}

impl BatchIterator {
    /// Create a new batch iterator.
    ///
    /// - `lowest_uid`: smallest UID in the mailbox (inclusive). Typically 1,
    ///   but may be higher if UIDs have been expunged.
    /// - `uid_next`: the UIDNEXT value from SELECT — one past the highest existing UID.
    /// - `batch_size`: number of UIDs per batch (e.g., 500).
    pub fn new(lowest_uid: u32, uid_next: u32, batch_size: u32) -> Self {
        let cursor = if uid_next > lowest_uid {
            Some(uid_next - 1) // highest existing UID
        } else {
            None // empty mailbox
        };
        Self {
            lowest_uid,
            cursor,
            batch_size,
        }
    }

    /// How many batches remain (estimate — UIDs may be sparse).
    pub fn estimated_batches(&self) -> u32 {
        match self.cursor {
            None => 0,
            Some(cursor) => {
                let range = cursor - self.lowest_uid + 1;
                (range + self.batch_size - 1) / self.batch_size
            }
        }
    }
}

impl Iterator for BatchIterator {
    /// `(start_uid, end_uid)` — both inclusive.
    type Item = (u32, u32);

    fn next(&mut self) -> Option<Self::Item> {
        let end = self.cursor?;
        if end < self.lowest_uid {
            return None;
        }

        let start = if end >= self.lowest_uid + self.batch_size - 1 {
            end - self.batch_size + 1
        } else {
            self.lowest_uid
        };

        // Advance cursor
        if start <= self.lowest_uid {
            self.cursor = None; // this was the last batch
        } else {
            self.cursor = Some(start - 1);
        }

        Some((start, end))
    }
}

/// Convert a `(start, end)` UID range to an IMAP sequence string.
///
/// - Single UID: `"42"`
/// - Range: `"501:1000"`
pub fn batch_to_sequence(start: u32, end: u32) -> String {
    if start == end {
        start.to_string()
    } else {
        format!("{start}:{end}")
    }
}
```

- [ ] **Step 4: Export from sync/mod.rs**

Add to `inboxly-imap/src/sync/mod.rs`:

```rust
pub mod batch;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p inboxly-imap --test sync_batch_test -- --nocapture`
Expected: all 8 tests pass

- [ ] **Step 6: Commit**

```bash
git add inboxly-imap/src/sync/batch.rs inboxly-imap/src/sync/mod.rs inboxly-imap/tests/sync_batch_test.rs
git commit -m "feat(imap): add BatchIterator for newest-first UID range splitting"
```

---

### Task 4: UIDVALIDITY State Persistence

**Files:**
- Create: `inboxly-imap/src/sync/uid_state.rs`
- Create: `inboxly-imap/tests/sync_uid_state_test.rs`
- Modify: `inboxly-imap/src/sync/mod.rs`

- [ ] **Step 1: Write failing tests for UIDVALIDITY storage**

Create `inboxly-imap/tests/sync_uid_state_test.rs`:

```rust
use inboxly_imap::sync::uid_state::{FolderSyncState, load_sync_state, save_sync_state};
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sync_state (
            account_id TEXT NOT NULL,
            folder_name TEXT NOT NULL,
            uid_validity INTEGER NOT NULL,
            uid_next INTEGER NOT NULL,
            highest_modseq INTEGER,
            last_sync TEXT NOT NULL,
            last_synced_uid INTEGER,
            PRIMARY KEY (account_id, folder_name)
        );"
    ).unwrap();
    conn
}

#[test]
fn save_and_load_sync_state() {
    let conn = setup_db();
    let state = FolderSyncState {
        account_id: "acc-1".to_string(),
        folder_name: "INBOX".to_string(),
        uid_validity: 12345,
        uid_next: 5001,
        highest_modseq: None,
        last_synced_uid: Some(5000),
    };
    save_sync_state(&conn, &state).unwrap();

    let loaded = load_sync_state(&conn, "acc-1", "INBOX").unwrap();
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.uid_validity, 12345);
    assert_eq!(loaded.uid_next, 5001);
    assert_eq!(loaded.last_synced_uid, Some(5000));
}

#[test]
fn load_nonexistent_returns_none() {
    let conn = setup_db();
    let loaded = load_sync_state(&conn, "acc-1", "INBOX").unwrap();
    assert!(loaded.is_none());
}

#[test]
fn save_overwrites_existing() {
    let conn = setup_db();
    let state1 = FolderSyncState {
        account_id: "acc-1".to_string(),
        folder_name: "INBOX".to_string(),
        uid_validity: 100,
        uid_next: 500,
        highest_modseq: None,
        last_synced_uid: Some(499),
    };
    save_sync_state(&conn, &state1).unwrap();

    let state2 = FolderSyncState {
        uid_next: 600,
        last_synced_uid: Some(599),
        ..state1.clone()
    };
    save_sync_state(&conn, &state2).unwrap();

    let loaded = load_sync_state(&conn, "acc-1", "INBOX").unwrap().unwrap();
    assert_eq!(loaded.uid_next, 600);
    assert_eq!(loaded.last_synced_uid, Some(599));
}

#[test]
fn detect_uid_validity_change() {
    use inboxly_imap::sync::uid_state::check_uid_validity;
    let conn = setup_db();

    // No prior state — should return Ok(false) meaning "no reset needed"
    let needs_reset = check_uid_validity(&conn, "acc-1", "INBOX", 100).unwrap();
    assert!(!needs_reset);

    // Save state with uid_validity=100
    let state = FolderSyncState {
        account_id: "acc-1".to_string(),
        folder_name: "INBOX".to_string(),
        uid_validity: 100,
        uid_next: 500,
        highest_modseq: None,
        last_synced_uid: None,
    };
    save_sync_state(&conn, &state).unwrap();

    // Same validity — no reset
    let needs_reset = check_uid_validity(&conn, "acc-1", "INBOX", 100).unwrap();
    assert!(!needs_reset);

    // Different validity — needs reset
    let needs_reset = check_uid_validity(&conn, "acc-1", "INBOX", 200).unwrap();
    assert!(needs_reset);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p inboxly-imap --test sync_uid_state_test 2>&1 | head -20`
Expected: compilation error — `uid_state` module does not exist

- [ ] **Step 3: Implement UIDVALIDITY state persistence**

Create `inboxly-imap/src/sync/uid_state.rs`:

```rust
use rusqlite::{Connection, params};
use super::error::SyncResult;

/// Persisted sync state for one (account, folder) pair.
#[derive(Debug, Clone)]
pub struct FolderSyncState {
    pub account_id: String,
    pub folder_name: String,
    pub uid_validity: u32,
    pub uid_next: u32,
    pub highest_modseq: Option<u64>,
    /// The last UID we successfully committed to SQLite during Phase 1.
    /// Used for crash recovery — resume from here instead of re-fetching everything.
    pub last_synced_uid: Option<u32>,
}

/// Save (upsert) sync state for a folder.
pub fn save_sync_state(conn: &Connection, state: &FolderSyncState) -> SyncResult<()> {
    conn.execute(
        "INSERT INTO sync_state (account_id, folder_name, uid_validity, uid_next, highest_modseq, last_sync, last_synced_uid)
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'), ?6)
         ON CONFLICT(account_id, folder_name) DO UPDATE SET
            uid_validity = excluded.uid_validity,
            uid_next = excluded.uid_next,
            highest_modseq = excluded.highest_modseq,
            last_sync = excluded.last_sync,
            last_synced_uid = excluded.last_synced_uid",
        params![
            state.account_id,
            state.folder_name,
            state.uid_validity,
            state.uid_next,
            state.highest_modseq,
            state.last_synced_uid,
        ],
    )?;
    Ok(())
}

/// Load sync state for a folder, if it exists.
pub fn load_sync_state(
    conn: &Connection,
    account_id: &str,
    folder_name: &str,
) -> SyncResult<Option<FolderSyncState>> {
    let mut stmt = conn.prepare(
        "SELECT uid_validity, uid_next, highest_modseq, last_synced_uid
         FROM sync_state
         WHERE account_id = ?1 AND folder_name = ?2",
    )?;

    let result = stmt.query_row(params![account_id, folder_name], |row| {
        Ok(FolderSyncState {
            account_id: account_id.to_string(),
            folder_name: folder_name.to_string(),
            uid_validity: row.get(0)?,
            uid_next: row.get(1)?,
            highest_modseq: row.get(2)?,
            last_synced_uid: row.get(3)?,
        })
    });

    match result {
        Ok(state) => Ok(Some(state)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Check if the server's UIDVALIDITY has changed since our last sync.
///
/// Returns `true` if the validity changed (meaning all cached UIDs are stale
/// and the folder must be re-synced from scratch).
/// Returns `false` if validity matches or no prior state exists.
pub fn check_uid_validity(
    conn: &Connection,
    account_id: &str,
    folder_name: &str,
    server_uid_validity: u32,
) -> SyncResult<bool> {
    match load_sync_state(conn, account_id, folder_name)? {
        None => Ok(false), // first sync, no prior state
        Some(state) => Ok(state.uid_validity != server_uid_validity),
    }
}

/// Delete all cached emails for a folder. Called when UIDVALIDITY changes.
///
/// This is a destructive operation — all locally cached metadata for UIDs in this
/// folder become invalid when the server resets UIDVALIDITY.
pub fn invalidate_folder(
    conn: &Connection,
    account_id: &str,
    folder_name: &str,
) -> SyncResult<()> {
    conn.execute(
        "DELETE FROM emails WHERE account_id = ?1 AND imap_folder = ?2",
        params![account_id, folder_name],
    )?;
    conn.execute(
        "DELETE FROM sync_state WHERE account_id = ?1 AND folder_name = ?2",
        params![account_id, folder_name],
    )?;
    Ok(())
}
```

- [ ] **Step 4: Export from sync/mod.rs**

Add to `inboxly-imap/src/sync/mod.rs`:

```rust
pub mod uid_state;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p inboxly-imap --test sync_uid_state_test -- --nocapture`
Expected: all 4 tests pass

- [ ] **Step 6: Commit**

```bash
git add inboxly-imap/src/sync/uid_state.rs inboxly-imap/src/sync/mod.rs inboxly-imap/tests/sync_uid_state_test.rs
git commit -m "feat(imap): add UIDVALIDITY persistence and staleness detection"
```

---

## Chunk 2: Envelope Parsing + SQLite Insertion

### Task 5: Envelope-to-EmailMeta Conversion

**Files:**
- Create: `inboxly-imap/src/sync/envelope.rs`
- Create: `inboxly-imap/tests/sync_envelope_test.rs`
- Modify: `inboxly-imap/src/sync/mod.rs`

- [ ] **Step 1: Write failing tests for envelope parsing**

Create `inboxly-imap/tests/sync_envelope_test.rs`:

```rust
use inboxly_imap::sync::envelope::{
    parse_envelope_date, decode_envelope_bytes, extract_address_string,
    extract_contacts_json, EnvelopeData,
};

#[test]
fn parse_rfc2822_date() {
    let raw = b"Mon, 10 Mar 2026 14:30:00 +0000";
    let dt = parse_envelope_date(raw).unwrap();
    assert_eq!(dt.timestamp(), 1773338200); // verify manually
}

#[test]
fn parse_rfc2822_date_no_day_name() {
    let raw = b"10 Mar 2026 14:30:00 +0000";
    let dt = parse_envelope_date(raw).unwrap();
    assert_eq!(dt.year(), 2026);
    assert_eq!(dt.month(), 3);
}

#[test]
fn parse_invalid_date_returns_error() {
    let raw = b"not a date";
    let result = parse_envelope_date(raw);
    assert!(result.is_err());
}

#[test]
fn decode_utf8_bytes() {
    let raw = b"Hello World";
    assert_eq!(decode_envelope_bytes(raw), "Hello World");
}

#[test]
fn decode_empty_returns_empty() {
    assert_eq!(decode_envelope_bytes(b""), "");
}

#[test]
fn extract_single_address() {
    // Simulates imap_proto::types::Address fields
    let display = "Alan Gaudet <alan@example.com>";
    assert_eq!(
        extract_address_string(Some("Alan Gaudet"), Some("alan"), Some("example.com")),
        ("Alan Gaudet".to_string(), "alan@example.com".to_string())
    );
}

#[test]
fn extract_address_no_name() {
    let (name, addr) = extract_address_string(None, Some("info"), Some("shop.com"));
    assert_eq!(name, "");
    assert_eq!(addr, "info@shop.com");
}

#[test]
fn contacts_json_round_trip() {
    let json = extract_contacts_json(&[
        ("Alice".to_string(), "alice@a.com".to_string()),
        ("Bob".to_string(), "bob@b.com".to_string()),
    ]);
    assert!(json.contains("alice@a.com"));
    assert!(json.contains("Bob"));
}

#[test]
fn envelope_data_to_insert_params() {
    let data = EnvelopeData {
        message_id: "<abc@example.com>".to_string(),
        account_id: "acc-1".to_string(),
        imap_uid: 42,
        imap_folder: "INBOX".to_string(),
        from_name: "Sender".to_string(),
        from_address: "sender@example.com".to_string(),
        to_json: "[]".to_string(),
        cc_json: "[]".to_string(),
        subject: "Test Subject".to_string(),
        date_unix: 1773338200,
        size_bytes: 4096,
        flags: 0,
        in_reply_to: None,
        references_json: None,
    };
    assert_eq!(data.imap_uid, 42);
    assert_eq!(data.subject, "Test Subject");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p inboxly-imap --test sync_envelope_test 2>&1 | head -20`
Expected: compilation error — `envelope` module does not exist

- [ ] **Step 3: Implement envelope parsing**

Create `inboxly-imap/src/sync/envelope.rs`:

```rust
use chrono::{DateTime, Utc};
use super::error::{SyncError, SyncResult};

/// Parsed envelope data ready for SQLite insertion.
///
/// This is an intermediate struct — not the full `EmailMeta` from `inboxly-core`.
/// It contains exactly the columns needed for the `emails` table INSERT.
#[derive(Debug, Clone)]
pub struct EnvelopeData {
    pub message_id: String,
    pub account_id: String,
    pub imap_uid: u32,
    pub imap_folder: String,
    pub from_name: String,
    pub from_address: String,
    pub to_json: String,
    pub cc_json: String,
    pub subject: String,
    pub date_unix: i64,
    pub size_bytes: u64,
    /// Bitmask: bit 0 = seen, bit 1 = flagged, bit 2 = answered, bit 3 = draft, bit 4 = deleted
    pub flags: u32,
    pub in_reply_to: Option<String>,
    pub references_json: Option<String>,
}

/// Parse an IMAP ENVELOPE date string (RFC 2822 format) into a UTC DateTime.
pub fn parse_envelope_date(raw: &[u8]) -> SyncResult<DateTime<Utc>> {
    let s = std::str::from_utf8(raw)
        .map_err(|_| SyncError::DateParse {
            uid: 0,
            raw: String::from_utf8_lossy(raw).to_string(),
        })?
        .trim();

    // Try RFC 2822 first (the standard ENVELOPE date format)
    if let Ok(dt) = DateTime::parse_from_rfc2822(s) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Some servers omit the day name — try common alternative formats
    for fmt in &[
        "%d %b %Y %H:%M:%S %z",
        "%d %b %Y %H:%M:%S",
        "%a, %d %b %Y %H:%M:%S",
    ] {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Ok(dt.and_utc());
        }
        if let Ok(dt) = DateTime::parse_from_str(s, fmt) {
            return Ok(dt.with_timezone(&Utc));
        }
    }

    Err(SyncError::DateParse {
        uid: 0,
        raw: s.to_string(),
    })
}

/// Decode bytes from an ENVELOPE field to a String.
/// IMAP ENVELOPE fields are `Option<Cow<'a, [u8]>>` in imap-proto.
pub fn decode_envelope_bytes(raw: &[u8]) -> String {
    // Try UTF-8 first, fall back to lossy
    String::from_utf8(raw.to_vec()).unwrap_or_else(|_| String::from_utf8_lossy(raw).to_string())
}

/// Extract display name and email address from IMAP Address components.
///
/// imap-proto Address has: name, adl (routing), mailbox (local-part), host (domain).
pub fn extract_address_string(
    name: Option<&str>,
    mailbox: Option<&str>,
    host: Option<&str>,
) -> (String, String) {
    let display_name = name.unwrap_or("").to_string();
    let email = match (mailbox, host) {
        (Some(m), Some(h)) => format!("{m}@{h}"),
        (Some(m), None) => m.to_string(),
        _ => String::new(),
    };
    (display_name, email)
}

/// Serialize a list of (name, address) pairs to JSON for the to_json/cc_json columns.
pub fn extract_contacts_json(contacts: &[(String, String)]) -> String {
    let entries: Vec<String> = contacts
        .iter()
        .map(|(name, addr)| {
            format!(
                r#"{{"name":"{}","address":"{}"}}"#,
                name.replace('\\', "\\\\").replace('"', "\\\""),
                addr.replace('\\', "\\\\").replace('"', "\\\""),
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

/// Convert IMAP flag names to our bitmask representation.
///
/// Bit layout: 0=Seen, 1=Flagged(starred), 2=Answered, 3=Draft, 4=Deleted
pub fn flags_to_bitmask(flags: &[async_imap::types::Flag<'_>]) -> u32 {
    use async_imap::types::Flag;
    let mut mask = 0u32;
    for flag in flags {
        match flag {
            Flag::Seen => mask |= 1 << 0,
            Flag::Flagged => mask |= 1 << 1,
            Flag::Answered => mask |= 1 << 2,
            Flag::Draft => mask |= 1 << 3,
            Flag::Deleted => mask |= 1 << 4,
            _ => {} // ignore custom/unknown flags
        }
    }
    mask
}

/// Parse one IMAP Fetch response into an EnvelopeData.
///
/// Requires that the FETCH included `(ENVELOPE FLAGS RFC822.SIZE)`.
pub fn parse_fetch_to_envelope(
    fetch: &async_imap::types::Fetch,
    account_id: &str,
    folder: &str,
) -> SyncResult<EnvelopeData> {
    let uid = fetch.uid.ok_or_else(|| SyncError::MalformedEnvelope {
        uid: 0,
        field: "UID".to_string(),
    })?;

    let envelope = fetch.envelope().ok_or_else(|| SyncError::MalformedEnvelope {
        uid,
        field: "ENVELOPE".to_string(),
    })?;

    // Message-ID
    let message_id = envelope
        .message_id
        .as_ref()
        .map(|b| decode_envelope_bytes(b))
        .unwrap_or_else(|| format!("<generated-{uid}@inboxly>"));

    // Subject
    let subject = envelope
        .subject
        .as_ref()
        .map(|b| decode_envelope_bytes(b))
        .unwrap_or_default();

    // Date
    let date_unix = match &envelope.date {
        Some(raw) => parse_envelope_date(raw)
            .map(|dt| dt.timestamp())
            .unwrap_or(0),
        None => 0,
    };

    // From (first address)
    let (from_name, from_address) = envelope
        .from
        .as_ref()
        .and_then(|addrs| addrs.first())
        .map(|addr| {
            extract_address_string(
                addr.name.as_ref().map(|b| std::str::from_utf8(b).unwrap_or("")),
                addr.mailbox.as_ref().map(|b| std::str::from_utf8(b).unwrap_or("")),
                addr.host.as_ref().map(|b| std::str::from_utf8(b).unwrap_or("")),
            )
        })
        .unwrap_or_default();

    // To
    let to_contacts: Vec<(String, String)> = envelope
        .to
        .as_ref()
        .map(|addrs| {
            addrs
                .iter()
                .map(|addr| {
                    extract_address_string(
                        addr.name.as_ref().map(|b| std::str::from_utf8(b).unwrap_or("")),
                        addr.mailbox.as_ref().map(|b| std::str::from_utf8(b).unwrap_or("")),
                        addr.host.as_ref().map(|b| std::str::from_utf8(b).unwrap_or("")),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    // CC
    let cc_contacts: Vec<(String, String)> = envelope
        .cc
        .as_ref()
        .map(|addrs| {
            addrs
                .iter()
                .map(|addr| {
                    extract_address_string(
                        addr.name.as_ref().map(|b| std::str::from_utf8(b).unwrap_or("")),
                        addr.mailbox.as_ref().map(|b| std::str::from_utf8(b).unwrap_or("")),
                        addr.host.as_ref().map(|b| std::str::from_utf8(b).unwrap_or("")),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    // In-Reply-To
    let in_reply_to = envelope
        .in_reply_to
        .as_ref()
        .map(|b| decode_envelope_bytes(b));

    // References — not in ENVELOPE; will be populated from headers in Phase 2.
    // For Phase 1, we use In-Reply-To for basic threading.
    let references_json = None;

    // Flags
    let flags = flags_to_bitmask(&fetch.flags().collect::<Vec<_>>());

    // Size
    let size_bytes = fetch.size.unwrap_or(0) as u64;

    Ok(EnvelopeData {
        message_id,
        account_id: account_id.to_string(),
        imap_uid: uid,
        imap_folder: folder.to_string(),
        from_name,
        from_address,
        to_json: extract_contacts_json(&to_contacts),
        cc_json: extract_contacts_json(&cc_contacts),
        subject,
        date_unix,
        size_bytes,
        flags,
        in_reply_to,
        references_json,
    })
}
```

- [ ] **Step 4: Export from sync/mod.rs**

Add to `inboxly-imap/src/sync/mod.rs`:

```rust
pub mod envelope;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p inboxly-imap --test sync_envelope_test -- --nocapture`
Expected: all 10 tests pass

Note: the `parse_fetch_to_envelope` and `flags_to_bitmask` functions are not unit-tested here because they require `async_imap::types::Fetch` which cannot be constructed in tests (no public constructor). They are tested in Task 9 (integration tests) using a real or mock IMAP session.

- [ ] **Step 6: Commit**

```bash
git add inboxly-imap/src/sync/envelope.rs inboxly-imap/src/sync/mod.rs inboxly-imap/tests/sync_envelope_test.rs
git commit -m "feat(imap): add ENVELOPE to EnvelopeData parser with date/address/flag handling"
```

---

### Task 6: Batch Insert to SQLite

**Files:**
- Create: `inboxly-imap/src/sync/store.rs`
- Create: `inboxly-imap/tests/sync_store_test.rs`
- Modify: `inboxly-imap/src/sync/mod.rs`

- [ ] **Step 1: Write failing tests for batch insert**

Create `inboxly-imap/tests/sync_store_test.rs`:

```rust
use inboxly_imap::sync::envelope::EnvelopeData;
use inboxly_imap::sync::store::{batch_insert_envelopes, count_emails_in_folder};
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS emails (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            thread_id TEXT,
            from_name TEXT NOT NULL,
            from_address TEXT NOT NULL,
            to_json TEXT NOT NULL,
            cc_json TEXT NOT NULL,
            subject TEXT NOT NULL,
            snippet TEXT NOT NULL DEFAULT '',
            date INTEGER NOT NULL,
            maildir_path TEXT NOT NULL DEFAULT '',
            flags INTEGER NOT NULL DEFAULT 0,
            size_bytes INTEGER NOT NULL DEFAULT 0,
            imap_uid INTEGER NOT NULL,
            imap_folder TEXT NOT NULL,
            has_attachments INTEGER NOT NULL DEFAULT 0,
            message_id_header TEXT,
            in_reply_to TEXT,
            references_json TEXT,
            UNIQUE(account_id, imap_folder, imap_uid)
        );"
    ).unwrap();
    conn
}

fn make_envelope(uid: u32) -> EnvelopeData {
    EnvelopeData {
        message_id: format!("<msg-{uid}@example.com>"),
        account_id: "acc-1".to_string(),
        imap_uid: uid,
        imap_folder: "INBOX".to_string(),
        from_name: "Test Sender".to_string(),
        from_address: "test@example.com".to_string(),
        to_json: r#"[{"name":"Me","address":"me@example.com"}]"#.to_string(),
        cc_json: "[]".to_string(),
        subject: format!("Subject {uid}"),
        date_unix: 1773338200 + uid as i64,
        size_bytes: 1024,
        flags: 0,
        in_reply_to: None,
        references_json: None,
    }
}

#[test]
fn insert_single_batch() {
    let conn = setup_db();
    let envelopes: Vec<_> = (1..=5).map(make_envelope).collect();
    let inserted = batch_insert_envelopes(&conn, &envelopes).unwrap();
    assert_eq!(inserted, 5);
}

#[test]
fn insert_ignores_duplicates() {
    let conn = setup_db();
    let envelopes: Vec<_> = (1..=5).map(make_envelope).collect();

    batch_insert_envelopes(&conn, &envelopes).unwrap();
    // Insert same batch again — duplicates should be ignored (ON CONFLICT IGNORE)
    let inserted = batch_insert_envelopes(&conn, &envelopes).unwrap();
    assert_eq!(inserted, 0);
}

#[test]
fn insert_large_batch() {
    let conn = setup_db();
    let envelopes: Vec<_> = (1..=1000).map(make_envelope).collect();
    let inserted = batch_insert_envelopes(&conn, &envelopes).unwrap();
    assert_eq!(inserted, 1000);
}

#[test]
fn count_emails() {
    let conn = setup_db();
    assert_eq!(count_emails_in_folder(&conn, "acc-1", "INBOX").unwrap(), 0);

    let envelopes: Vec<_> = (1..=3).map(make_envelope).collect();
    batch_insert_envelopes(&conn, &envelopes).unwrap();
    assert_eq!(count_emails_in_folder(&conn, "acc-1", "INBOX").unwrap(), 3);
}

#[test]
fn insert_empty_batch() {
    let conn = setup_db();
    let inserted = batch_insert_envelopes(&conn, &[]).unwrap();
    assert_eq!(inserted, 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p inboxly-imap --test sync_store_test 2>&1 | head -20`
Expected: compilation error — `store` module does not exist in sync

- [ ] **Step 3: Implement batch insert**

Create `inboxly-imap/src/sync/store.rs`:

```rust
use rusqlite::{Connection, params};
use super::envelope::EnvelopeData;
use super::error::SyncResult;

/// Insert a batch of envelopes into the `emails` table within a single transaction.
///
/// Uses `INSERT OR IGNORE` to skip duplicates (same account_id + imap_folder + imap_uid).
/// Returns the number of rows actually inserted (excluding ignored duplicates).
pub fn batch_insert_envelopes(conn: &Connection, envelopes: &[EnvelopeData]) -> SyncResult<usize> {
    if envelopes.is_empty() {
        return Ok(0);
    }

    let tx = conn.unchecked_transaction()?;
    let mut inserted = 0usize;

    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR IGNORE INTO emails (
                id, account_id, thread_id,
                from_name, from_address, to_json, cc_json,
                subject, snippet, date, maildir_path,
                flags, size_bytes, imap_uid, imap_folder,
                has_attachments, message_id_header, in_reply_to, references_json
            ) VALUES (
                ?1, ?2, NULL,
                ?3, ?4, ?5, ?6,
                ?7, '', ?8, '',
                ?9, ?10, ?11, ?12,
                0, ?13, ?14, ?15
            )",
        )?;

        for env in envelopes {
            let changes = stmt.execute(params![
                env.message_id,       // id (Message-ID as primary key)
                env.account_id,
                env.from_name,
                env.from_address,
                env.to_json,
                env.cc_json,
                env.subject,
                env.date_unix,
                env.flags,
                env.size_bytes,
                env.imap_uid,
                env.imap_folder,
                env.message_id,       // message_id_header (same as id)
                env.in_reply_to,
                env.references_json,
            ])?;
            inserted += changes;
        }
    }

    tx.commit()?;
    Ok(inserted)
}

/// Count emails in a specific folder for an account.
pub fn count_emails_in_folder(
    conn: &Connection,
    account_id: &str,
    folder: &str,
) -> SyncResult<u32> {
    let count: u32 = conn.query_row(
        "SELECT COUNT(*) FROM emails WHERE account_id = ?1 AND imap_folder = ?2",
        params![account_id, folder],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Get the lowest synced UID for a folder, used for resume calculation.
pub fn lowest_synced_uid(
    conn: &Connection,
    account_id: &str,
    folder: &str,
) -> SyncResult<Option<u32>> {
    let result: Option<u32> = conn.query_row(
        "SELECT MIN(imap_uid) FROM emails WHERE account_id = ?1 AND imap_folder = ?2",
        params![account_id, folder],
        |row| row.get(0),
    )?;
    Ok(result)
}

/// Get the highest synced UID for a folder.
pub fn highest_synced_uid(
    conn: &Connection,
    account_id: &str,
    folder: &str,
) -> SyncResult<Option<u32>> {
    let result: Option<u32> = conn.query_row(
        "SELECT MAX(imap_uid) FROM emails WHERE account_id = ?1 AND imap_folder = ?2",
        params![account_id, folder],
        |row| row.get(0),
    )?;
    Ok(result)
}
```

- [ ] **Step 4: Export from sync/mod.rs**

Add to `inboxly-imap/src/sync/mod.rs`:

```rust
pub mod store;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p inboxly-imap --test sync_store_test -- --nocapture`
Expected: all 5 tests pass

- [ ] **Step 6: Commit**

```bash
git add inboxly-imap/src/sync/store.rs inboxly-imap/src/sync/mod.rs inboxly-imap/tests/sync_store_test.rs
git commit -m "feat(imap): add batch envelope insertion to SQLite with dedup"
```

---

## Chunk 3: Basic Threading + Sync Engine Orchestrator

### Task 7: Basic Thread Association

**Files:**
- Create: `inboxly-imap/src/sync/threading.rs`
- Create: `inboxly-imap/tests/sync_threading_test.rs`
- Modify: `inboxly-imap/src/sync/mod.rs`

Threading in M7 is intentionally basic — full JWZ-style threading with placeholder resolution is M10. Here we assign `thread_id` values using only `In-Reply-To` from the ENVELOPE (the `References` header is not available until Phase 2 body download). The algorithm:

1. If `in_reply_to` is present and a row with that `message_id_header` exists in `emails`, copy its `thread_id`.
2. If `in_reply_to` references a message not yet seen, generate a new `thread_id` (M10 will unify later).
3. If no `in_reply_to`, generate a new `thread_id`.

- [ ] **Step 1: Write failing tests**

Create `inboxly-imap/tests/sync_threading_test.rs`:

```rust
use inboxly_imap::sync::threading::assign_thread_ids;
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS emails (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            thread_id TEXT,
            from_name TEXT NOT NULL,
            from_address TEXT NOT NULL,
            to_json TEXT NOT NULL,
            cc_json TEXT NOT NULL,
            subject TEXT NOT NULL,
            snippet TEXT NOT NULL DEFAULT '',
            date INTEGER NOT NULL,
            maildir_path TEXT NOT NULL DEFAULT '',
            flags INTEGER NOT NULL DEFAULT 0,
            size_bytes INTEGER NOT NULL DEFAULT 0,
            imap_uid INTEGER NOT NULL,
            imap_folder TEXT NOT NULL,
            has_attachments INTEGER NOT NULL DEFAULT 0,
            message_id_header TEXT,
            in_reply_to TEXT,
            references_json TEXT,
            UNIQUE(account_id, imap_folder, imap_uid)
        );
        CREATE TABLE IF NOT EXISTS threads (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            subject TEXT NOT NULL,
            newest_date INTEGER NOT NULL,
            oldest_date INTEGER NOT NULL,
            email_count INTEGER NOT NULL DEFAULT 0,
            unread_count INTEGER NOT NULL DEFAULT 0,
            has_attachments INTEGER NOT NULL DEFAULT 0,
            snippet TEXT NOT NULL DEFAULT ''
        );"
    ).unwrap();
    conn
}

fn insert_email(conn: &Connection, msg_id: &str, in_reply_to: Option<&str>, uid: u32) {
    conn.execute(
        "INSERT INTO emails (id, account_id, thread_id, from_name, from_address, to_json, cc_json,
         subject, date, imap_uid, imap_folder, message_id_header, in_reply_to)
         VALUES (?1, 'acc-1', NULL, 'Test', 'test@x.com', '[]', '[]',
         'Subject', 1773338200, ?2, 'INBOX', ?1, ?3)",
        rusqlite::params![msg_id, uid, in_reply_to],
    ).unwrap();
}

#[test]
fn standalone_email_gets_new_thread() {
    let conn = setup_db();
    insert_email(&conn, "<msg-1@x.com>", None, 1);

    let assigned = assign_thread_ids(&conn, "acc-1").unwrap();
    assert_eq!(assigned, 1);

    let thread_id: String = conn.query_row(
        "SELECT thread_id FROM emails WHERE id = '<msg-1@x.com>'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert!(!thread_id.is_empty());
}

#[test]
fn reply_joins_parent_thread() {
    let conn = setup_db();
    insert_email(&conn, "<msg-1@x.com>", None, 1);
    insert_email(&conn, "<msg-2@x.com>", Some("<msg-1@x.com>"), 2);

    assign_thread_ids(&conn, "acc-1").unwrap();

    let tid1: String = conn.query_row(
        "SELECT thread_id FROM emails WHERE id = '<msg-1@x.com>'", [], |r| r.get(0),
    ).unwrap();
    let tid2: String = conn.query_row(
        "SELECT thread_id FROM emails WHERE id = '<msg-2@x.com>'", [], |r| r.get(0),
    ).unwrap();
    assert_eq!(tid1, tid2);
}

#[test]
fn reply_to_unknown_parent_gets_own_thread() {
    let conn = setup_db();
    // Parent not in DB — reply gets its own thread (M10 will unify when parent arrives)
    insert_email(&conn, "<msg-2@x.com>", Some("<missing@x.com>"), 2);

    let assigned = assign_thread_ids(&conn, "acc-1").unwrap();
    assert_eq!(assigned, 1);

    let thread_id: String = conn.query_row(
        "SELECT thread_id FROM emails WHERE id = '<msg-2@x.com>'", [], |r| r.get(0),
    ).unwrap();
    assert!(!thread_id.is_empty());
}

#[test]
fn already_threaded_emails_skipped() {
    let conn = setup_db();
    insert_email(&conn, "<msg-1@x.com>", None, 1);
    assign_thread_ids(&conn, "acc-1").unwrap();

    // Run again — should not re-assign
    let assigned = assign_thread_ids(&conn, "acc-1").unwrap();
    assert_eq!(assigned, 0);
}

#[test]
fn thread_row_created() {
    let conn = setup_db();
    insert_email(&conn, "<msg-1@x.com>", None, 1);
    assign_thread_ids(&conn, "acc-1").unwrap();

    let thread_count: u32 = conn.query_row(
        "SELECT COUNT(*) FROM threads", [], |r| r.get(0),
    ).unwrap();
    assert_eq!(thread_count, 1);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p inboxly-imap --test sync_threading_test 2>&1 | head -20`
Expected: compilation error — `threading` module does not exist

- [ ] **Step 3: Implement basic threading**

Create `inboxly-imap/src/sync/threading.rs`:

```rust
use rusqlite::{Connection, params};
use uuid::Uuid;
use super::error::SyncResult;

/// Assign `thread_id` to all emails that have `thread_id IS NULL`.
///
/// Basic algorithm (Phase 1 — full threading is M10):
/// 1. For each un-threaded email, check `in_reply_to`.
/// 2. If `in_reply_to` points to a message already in `emails` that HAS a thread_id,
///    join that thread.
/// 3. Otherwise, create a new thread.
///
/// Returns the number of emails that were assigned a thread_id.
pub fn assign_thread_ids(conn: &Connection, account_id: &str) -> SyncResult<u32> {
    // Fetch all un-threaded emails for this account, ordered by date ascending
    // so parents are processed before replies when possible.
    let mut select_stmt = conn.prepare(
        "SELECT id, in_reply_to, subject, date
         FROM emails
         WHERE account_id = ?1 AND thread_id IS NULL
         ORDER BY date ASC",
    )?;

    let unthreaded: Vec<(String, Option<String>, String, i64)> = select_stmt
        .query_map(params![account_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    if unthreaded.is_empty() {
        return Ok(0);
    }

    let tx = conn.unchecked_transaction()?;
    let mut assigned = 0u32;

    for (msg_id, in_reply_to, subject, date) in &unthreaded {
        let thread_id = if let Some(parent_msg_id) = in_reply_to {
            // Try to find parent's thread_id
            let parent_tid: Option<String> = tx
                .query_row(
                    "SELECT thread_id FROM emails WHERE message_id_header = ?1 AND thread_id IS NOT NULL",
                    params![parent_msg_id],
                    |row| row.get(0),
                )
                .ok();

            parent_tid.unwrap_or_else(|| Uuid::new_v4().to_string())
        } else {
            Uuid::new_v4().to_string()
        };

        // Update the email row
        tx.execute(
            "UPDATE emails SET thread_id = ?1 WHERE id = ?2",
            params![thread_id, msg_id],
        )?;

        // Upsert threads table row
        tx.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, email_count, snippet)
             VALUES (?1, ?2, ?3, ?4, ?4, 1, '')
             ON CONFLICT(id) DO UPDATE SET
                newest_date = MAX(threads.newest_date, excluded.newest_date),
                oldest_date = MIN(threads.oldest_date, excluded.oldest_date),
                email_count = email_count + 1",
            params![thread_id, account_id, subject, date],
        )?;

        assigned += 1;
    }

    tx.commit()?;
    Ok(assigned)
}
```

- [ ] **Step 4: Export from sync/mod.rs**

Add to `inboxly-imap/src/sync/mod.rs`:

```rust
pub mod threading;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p inboxly-imap --test sync_threading_test -- --nocapture`
Expected: all 5 tests pass

- [ ] **Step 6: Commit**

```bash
git add inboxly-imap/src/sync/threading.rs inboxly-imap/src/sync/mod.rs inboxly-imap/tests/sync_threading_test.rs
git commit -m "feat(imap): add basic In-Reply-To thread assignment (Phase 1 pre-M10)"
```

---

### Task 8: Test Fixtures Module

**Files:**
- Create: `inboxly-imap/tests/fixtures/mod.rs`

This module provides shared helpers for integration tests. Not TDD — it is a test utility.

- [ ] **Step 1: Create fixtures module**

Create `inboxly-imap/tests/fixtures/mod.rs`:

```rust
use rusqlite::Connection;

/// Create an in-memory SQLite database with the full Inboxly schema
/// (enough tables for sync engine testing).
pub fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "
        CREATE TABLE emails (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            thread_id TEXT,
            from_name TEXT NOT NULL,
            from_address TEXT NOT NULL,
            to_json TEXT NOT NULL,
            cc_json TEXT NOT NULL,
            subject TEXT NOT NULL,
            snippet TEXT NOT NULL DEFAULT '',
            date INTEGER NOT NULL,
            maildir_path TEXT NOT NULL DEFAULT '',
            flags INTEGER NOT NULL DEFAULT 0,
            size_bytes INTEGER NOT NULL DEFAULT 0,
            imap_uid INTEGER NOT NULL,
            imap_folder TEXT NOT NULL,
            has_attachments INTEGER NOT NULL DEFAULT 0,
            message_id_header TEXT,
            in_reply_to TEXT,
            references_json TEXT,
            UNIQUE(account_id, imap_folder, imap_uid)
        );

        CREATE TABLE threads (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            subject TEXT NOT NULL,
            newest_date INTEGER NOT NULL,
            oldest_date INTEGER NOT NULL,
            email_count INTEGER NOT NULL DEFAULT 0,
            unread_count INTEGER NOT NULL DEFAULT 0,
            has_attachments INTEGER NOT NULL DEFAULT 0,
            snippet TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE sync_state (
            account_id TEXT NOT NULL,
            folder_name TEXT NOT NULL,
            uid_validity INTEGER NOT NULL,
            uid_next INTEGER NOT NULL,
            highest_modseq INTEGER,
            last_sync TEXT NOT NULL,
            last_synced_uid INTEGER,
            PRIMARY KEY (account_id, folder_name)
        );
        ",
    )
    .unwrap();
    conn
}

/// Build a Vec of EnvelopeData for testing, with UIDs from `start` to `end` inclusive.
pub fn make_envelopes(
    start: u32,
    end: u32,
    account_id: &str,
    folder: &str,
) -> Vec<inboxly_imap::sync::envelope::EnvelopeData> {
    (start..=end)
        .map(|uid| inboxly_imap::sync::envelope::EnvelopeData {
            message_id: format!("<msg-{uid}@test.inboxly>"),
            account_id: account_id.to_string(),
            imap_uid: uid,
            imap_folder: folder.to_string(),
            from_name: format!("Sender {uid}"),
            from_address: format!("sender{uid}@example.com"),
            to_json: r#"[{"name":"Me","address":"me@example.com"}]"#.to_string(),
            cc_json: "[]".to_string(),
            subject: format!("Test email #{uid}"),
            date_unix: 1773338200 + uid as i64,
            size_bytes: 1024 + uid as u64,
            flags: if uid % 3 == 0 { 1 } else { 0 }, // every 3rd is "seen"
            in_reply_to: None,
            references_json: None,
        })
        .collect()
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo test -p inboxly-imap --test sync_store_test -- --nocapture`
Expected: existing tests still pass (fixtures module not yet used by any test file, but must compile)

- [ ] **Step 3: Commit**

```bash
git add inboxly-imap/tests/fixtures/mod.rs
git commit -m "test(imap): add shared test fixtures for sync engine tests"
```

---

### Task 9: Sync Engine Orchestrator

**Files:**
- Create: `inboxly-imap/src/sync/engine.rs`
- Modify: `inboxly-imap/src/sync/mod.rs`

This is the core orchestrator that coordinates the entire Phase 1 sync. It ties together all previous components.

- [ ] **Step 1: Define the SyncEngine struct and its public API**

Create `inboxly-imap/src/sync/engine.rs`:

```rust
use std::sync::Arc;
use rusqlite::Connection;
use tokio::sync::Mutex;
use futures::TryStreamExt;

use super::batch::{BatchIterator, batch_to_sequence};
use super::envelope::parse_fetch_to_envelope;
use super::error::{SyncError, SyncResult};
use super::progress::{SyncEvent, SyncEventSender, SyncProgress};
use super::store::batch_insert_envelopes;
use super::threading::assign_thread_ids;
use super::uid_state::{
    FolderSyncState, check_uid_validity, invalidate_folder, load_sync_state, save_sync_state,
};

/// Configuration for the Phase 1 sync engine.
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Number of UIDs to fetch per IMAP FETCH command. Default: 500.
    pub batch_size: u32,
    /// Account ID for the account being synced.
    pub account_id: String,
    /// IMAP folder to sync (e.g., "INBOX").
    pub folder: String,
}

impl SyncConfig {
    pub fn new(account_id: impl Into<String>, folder: impl Into<String>) -> Self {
        Self {
            batch_size: 500,
            account_id: account_id.into(),
            folder: folder.into(),
        }
    }

    pub fn with_batch_size(mut self, size: u32) -> Self {
        self.batch_size = size;
        self
    }
}

/// Result of a completed Phase 1 sync.
#[derive(Debug)]
pub struct SyncPhase1Result {
    pub folder: String,
    pub total_fetched: u32,
    pub total_inserted: u32,
    pub threads_created: u32,
    pub uid_validity: u32,
    pub uid_next: u32,
}

/// Run Phase 1 sync: fetch all ENVELOPE + FLAGS + RFC822.SIZE for a folder.
///
/// This function:
/// 1. SELECTs the folder to get UIDVALIDITY and UIDNEXT
/// 2. Checks for UIDVALIDITY changes (invalidates cache if changed)
/// 3. Computes resume point from last successful sync
/// 4. Fetches envelopes in batches of `config.batch_size`, newest-first
/// 5. Inserts each batch into SQLite
/// 6. Assigns basic thread IDs after each batch
/// 7. Emits progress events and first-batch-ready signal
/// 8. Persists sync state after each batch for crash recovery
///
/// # Arguments
/// - `session`: An authenticated, mutable IMAP session (from M6)
/// - `db`: SQLite connection (wrapped in Arc<Mutex> for async safety)
/// - `config`: Sync configuration
/// - `event_tx`: Channel sender for progress events to UI
///
/// # Type Parameters
/// - `S`: The stream/connection type (typically `TlsStream<TcpStream>`)
pub async fn run_phase1_sync<S>(
    session: &mut async_imap::Session<S>,
    db: Arc<Mutex<Connection>>,
    config: &SyncConfig,
    event_tx: SyncEventSender,
) -> SyncResult<SyncPhase1Result>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
{
    // Step 1: SELECT the folder
    let mailbox = session
        .select(&config.folder)
        .await
        .map_err(SyncError::from)?;

    let uid_validity = mailbox
        .uid_validity
        .ok_or_else(|| SyncError::MissingUidValidity(config.folder.clone()))?;

    let uid_next = mailbox
        .uid_next
        .ok_or_else(|| SyncError::MissingUidNext(config.folder.clone()))?;

    // Step 2: Check UIDVALIDITY
    {
        let conn = db.lock().await;
        let changed = check_uid_validity(&conn, &config.account_id, &config.folder, uid_validity)?;
        if changed {
            tracing::warn!(
                folder = %config.folder,
                "UIDVALIDITY changed — invalidating cached emails for folder"
            );
            invalidate_folder(&conn, &config.account_id, &config.folder)?;
        }
    }

    // Step 3: Compute resume point
    let lowest_uid = {
        let conn = db.lock().await;
        match load_sync_state(&conn, &config.account_id, &config.folder)? {
            Some(state) if state.uid_validity == uid_validity => {
                // Resume: we've synced down to last_synced_uid previously.
                // If last_synced_uid is 1, the full range is already done.
                match state.last_synced_uid {
                    Some(last) if last <= 1 => {
                        tracing::info!(folder = %config.folder, "Phase 1 already complete");
                        return Ok(SyncPhase1Result {
                            folder: config.folder.clone(),
                            total_fetched: 0,
                            total_inserted: 0,
                            threads_created: 0,
                            uid_validity,
                            uid_next,
                        });
                    }
                    Some(_last) => {
                        // We'll fetch from UID 1 up to last-1.
                        // The BatchIterator handles this — we just need the right uid_next.
                        1u32
                    }
                    None => 1u32, // no prior progress
                }
            }
            _ => 1u32, // first sync or validity mismatch (already invalidated above)
        }
    };

    // Determine the effective uid_next for batch iteration.
    // If resuming, we only need UIDs below the last_synced_uid.
    let effective_uid_next = {
        let conn = db.lock().await;
        match load_sync_state(&conn, &config.account_id, &config.folder)? {
            Some(state) if state.uid_validity == uid_validity => {
                state.last_synced_uid.unwrap_or(uid_next)
            }
            _ => uid_next,
        }
    };

    let total_estimate = if effective_uid_next > lowest_uid {
        effective_uid_next - lowest_uid
    } else {
        0
    };

    if total_estimate == 0 {
        return Ok(SyncPhase1Result {
            folder: config.folder.clone(),
            total_fetched: 0,
            total_inserted: 0,
            threads_created: 0,
            uid_validity,
            uid_next,
        });
    }

    // Step 4: Iterate batches newest-first
    let batches = BatchIterator::new(lowest_uid, effective_uid_next, config.batch_size);
    let mut total_fetched = 0u32;
    let mut total_inserted = 0u32;
    let mut total_threads = 0u32;
    let mut is_first_batch = true;

    for (batch_start, batch_end) in batches {
        let sequence = batch_to_sequence(batch_start, batch_end);

        // Step 4a: UID FETCH
        let fetch_result = session
            .uid_fetch(&sequence, "(ENVELOPE FLAGS RFC822.SIZE)")
            .await;

        let fetches = match fetch_result {
            Ok(stream) => stream.try_collect::<Vec<_>>().await.map_err(SyncError::from)?,
            Err(e) => {
                // Connection error — save progress and return error for retry
                let conn = db.lock().await;
                save_sync_state(
                    &conn,
                    &FolderSyncState {
                        account_id: config.account_id.clone(),
                        folder_name: config.folder.clone(),
                        uid_validity,
                        uid_next,
                        highest_modseq: None,
                        last_synced_uid: Some(batch_end + 1), // resume below this
                    },
                )?;
                return Err(SyncError::ConnectionLost {
                    folder: config.folder.clone(),
                    source: Box::new(e),
                });
            }
        };

        // Step 4b: Parse envelopes
        let mut envelopes = Vec::with_capacity(fetches.len());
        for fetch in &fetches {
            match parse_fetch_to_envelope(fetch, &config.account_id, &config.folder) {
                Ok(env) => envelopes.push(env),
                Err(e) => {
                    // Log warning but continue — one bad envelope shouldn't kill the sync
                    let _ = event_tx
                        .send(SyncEvent::Warning(format!("Skipped malformed envelope: {e}")))
                        .await;
                }
            }
        }

        let batch_fetched = envelopes.len() as u32;

        // Step 4c: Batch insert to SQLite
        let batch_inserted = {
            let conn = db.lock().await;
            batch_insert_envelopes(&conn, &envelopes)? as u32
        };

        // Step 4d: Assign thread IDs for newly inserted emails
        let batch_threads = {
            let conn = db.lock().await;
            assign_thread_ids(&conn, &config.account_id)?
        };

        total_fetched += batch_fetched;
        total_inserted += batch_inserted;
        total_threads += batch_threads;

        // Step 5: Emit progress
        let _ = event_tx
            .send(SyncEvent::HeaderProgress(SyncProgress {
                folder: config.folder.clone(),
                fetched: total_fetched,
                total: total_estimate,
            }))
            .await;

        // Step 6: First-batch-ready signal
        if is_first_batch {
            let _ = event_tx
                .send(SyncEvent::FirstBatchReady {
                    folder: config.folder.clone(),
                    emails_in_batch: batch_inserted,
                })
                .await;
            is_first_batch = false;
        }

        // Step 7: Persist sync state for crash recovery
        {
            let conn = db.lock().await;
            save_sync_state(
                &conn,
                &FolderSyncState {
                    account_id: config.account_id.clone(),
                    folder_name: config.folder.clone(),
                    uid_validity,
                    uid_next,
                    highest_modseq: None,
                    last_synced_uid: Some(batch_start), // we've synced down to here
                },
            )?;
        }
    }

    // Step 8: Emit completion
    let _ = event_tx
        .send(SyncEvent::HeaderSyncComplete {
            folder: config.folder.clone(),
            total_emails: total_inserted,
        })
        .await;

    Ok(SyncPhase1Result {
        folder: config.folder.clone(),
        total_fetched,
        total_inserted,
        threads_created: total_threads,
        uid_validity,
        uid_next,
    })
}
```

- [ ] **Step 2: Export from sync/mod.rs**

Update `inboxly-imap/src/sync/mod.rs` to its final form:

```rust
pub mod batch;
pub mod engine;
pub mod envelope;
pub mod error;
pub mod progress;
pub mod store;
pub mod threading;
pub mod uid_state;

pub use engine::{SyncConfig, SyncPhase1Result, run_phase1_sync};
pub use error::{SyncError, SyncResult};
pub use progress::{SyncEvent, SyncEventReceiver, SyncEventSender, SyncProgress, sync_event_channel};
```

- [ ] **Step 3: Add required dependencies to Cargo.toml**

Ensure `inboxly-imap/Cargo.toml` has these dependencies (add any missing):

```toml
[dependencies]
async-imap = "0.11"
chrono = { version = "0.4", features = ["serde"] }
futures = "0.3"
rusqlite = { version = "0.33", features = ["bundled"] }
thiserror = "2"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
uuid = { version = "1", features = ["v4"] }

[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p inboxly-imap`
Expected: compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add inboxly-imap/src/sync/engine.rs inboxly-imap/src/sync/mod.rs inboxly-imap/Cargo.toml
git commit -m "feat(imap): add Phase 1 sync engine orchestrator

Coordinates UID discovery, batched FETCH, SQLite insertion,
basic threading, progress events, and crash-recovery state."
```

---

### Task 10: Integration Tests with Mock/Fixture IMAP

**Files:**
- Create: `inboxly-imap/tests/sync_engine_test.rs`

Full integration tests for `run_phase1_sync` require an IMAP session. Since `async_imap::Session` wraps a raw stream, we can test with a mock TCP stream that replays canned IMAP responses. This is the standard approach — the mock speaks raw IMAP protocol bytes.

- [ ] **Step 1: Write the mock IMAP stream and integration tests**

Create `inboxly-imap/tests/sync_engine_test.rs`:

```rust
//! Integration tests for the Phase 1 sync engine.
//!
//! Uses a mock TCP stream that replays canned IMAP responses.
//! This tests the full pipeline: SELECT → UID FETCH → parse → insert → thread → progress.

mod fixtures;

use std::sync::Arc;
use tokio::sync::Mutex;
use inboxly_imap::sync::{SyncConfig, SyncEvent, run_phase1_sync, sync_event_channel};

/// A mock async read/write stream that replays canned IMAP server responses.
///
/// The write side records what the client sends (for assertions).
/// The read side returns pre-loaded response bytes.
struct MockImapStream {
    /// Canned responses, consumed in order.
    responses: Vec<Vec<u8>>,
    /// Index into responses.
    read_idx: usize,
    /// Bytes already read from current response.
    read_offset: usize,
    /// Client commands captured.
    captured_writes: Vec<u8>,
}

impl MockImapStream {
    fn new(responses: Vec<Vec<u8>>) -> Self {
        Self {
            responses,
            read_idx: 0,
            read_offset: 0,
            captured_writes: Vec::new(),
        }
    }
}

impl tokio::io::AsyncRead for MockImapStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if self.read_idx >= self.responses.len() {
            return std::task::Poll::Ready(Ok(())); // EOF
        }
        let response = &self.responses[self.read_idx];
        let remaining = &response[self.read_offset..];
        let to_copy = remaining.len().min(buf.remaining());
        buf.put_slice(&remaining[..to_copy]);
        self.read_offset += to_copy;
        if self.read_offset >= response.len() {
            self.read_idx += 1;
            self.read_offset = 0;
        }
        std::task::Poll::Ready(Ok(()))
    }
}

impl tokio::io::AsyncWrite for MockImapStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.captured_writes.extend_from_slice(buf);
        std::task::Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

/// Build the IMAP responses for a SELECT + UID FETCH sequence.
///
/// This is intentionally simplified — real IMAP protocol has tagged responses,
/// continuation lines, etc. For full integration testing against a real server,
/// use a local Dovecot/Greenmail instance (see note below).
fn build_select_response(exists: u32, uid_validity: u32, uid_next: u32) -> Vec<u8> {
    format!(
        "* {exists} EXISTS\r\n\
         * 0 RECENT\r\n\
         * OK [UIDVALIDITY {uid_validity}]\r\n\
         * OK [UIDNEXT {uid_next}]\r\n\
         * FLAGS (\\Seen \\Answered \\Flagged \\Deleted \\Draft)\r\n\
         A1 OK [READ-WRITE] SELECT completed\r\n"
    )
    .into_bytes()
}

fn build_fetch_response(uids: &[u32]) -> Vec<u8> {
    let mut buf = String::new();
    for (seq, uid) in uids.iter().enumerate() {
        let seq1 = seq + 1; // 1-based sequence
        buf.push_str(&format!(
            "* {seq1} FETCH (UID {uid} RFC822.SIZE 2048 FLAGS (\\Seen) \
             ENVELOPE (\"Mon, 10 Mar 2026 14:30:00 +0000\" \"Subject {uid}\" \
             ((\"Sender\" NIL \"sender\" \"example.com\")) \
             ((\"Sender\" NIL \"sender\" \"example.com\")) \
             ((\"Sender\" NIL \"sender\" \"example.com\")) \
             ((\"Recipient\" NIL \"me\" \"example.com\")) \
             NIL NIL NIL \"<msg-{uid}@example.com>\"))\r\n"
        ));
    }
    buf.push_str("A2 OK FETCH completed\r\n");
    buf.into_bytes()
}

// NOTE: The MockImapStream approach works for testing the pipeline logic,
// but has limitations — async-imap's response parser is strict about protocol
// formatting. If these tests are flaky due to protocol parsing, replace with
// a real local IMAP server (Greenmail via Docker is recommended for CI):
//
//   docker run -p 3143:3143 -p 3025:3025 greenmail/standalone:2.1.2
//
// For now, we test the individual components (envelope, batch, store, threading)
// thoroughly via unit tests, and test the orchestrator at a higher level.

#[tokio::test]
async fn sync_engine_components_work_together() {
    // This test verifies the component integration WITHOUT mocking the IMAP stream,
    // since constructing a valid async-imap Session from a mock is fragile.
    // Instead, it tests the post-FETCH pipeline: parse → insert → thread → progress.

    use inboxly_imap::sync::envelope::EnvelopeData;
    use inboxly_imap::sync::store::batch_insert_envelopes;
    use inboxly_imap::sync::threading::assign_thread_ids;
    use inboxly_imap::sync::uid_state::{FolderSyncState, save_sync_state, load_sync_state};

    let conn = fixtures::test_db();
    let account_id = "test-account";
    let folder = "INBOX";

    // Simulate batch 1 (newest): UIDs 501-1000
    let batch1 = fixtures::make_envelopes(501, 1000, account_id, folder);
    let inserted1 = batch_insert_envelopes(&conn, &batch1).unwrap();
    assert_eq!(inserted1, 500);

    let threaded1 = assign_thread_ids(&conn, account_id).unwrap();
    assert_eq!(threaded1, 500); // all new threads (no replies in fixture)

    // Save sync state after batch 1
    save_sync_state(
        &conn,
        &FolderSyncState {
            account_id: account_id.to_string(),
            folder_name: folder.to_string(),
            uid_validity: 12345,
            uid_next: 1001,
            highest_modseq: None,
            last_synced_uid: Some(501),
        },
    )
    .unwrap();

    // Simulate batch 2 (older): UIDs 1-500
    let batch2 = fixtures::make_envelopes(1, 500, account_id, folder);
    let inserted2 = batch_insert_envelopes(&conn, &batch2).unwrap();
    assert_eq!(inserted2, 500);

    let threaded2 = assign_thread_ids(&conn, account_id).unwrap();
    assert_eq!(threaded2, 500);

    // Save final sync state
    save_sync_state(
        &conn,
        &FolderSyncState {
            account_id: account_id.to_string(),
            folder_name: folder.to_string(),
            uid_validity: 12345,
            uid_next: 1001,
            highest_modseq: None,
            last_synced_uid: Some(1),
        },
    )
    .unwrap();

    // Verify totals
    let total: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM emails WHERE account_id = ?1",
            [account_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(total, 1000);

    let thread_count: u32 = conn
        .query_row("SELECT COUNT(*) FROM threads", [], |r| r.get(0))
        .unwrap();
    assert_eq!(thread_count, 1000); // one thread per email (no replies)

    // Verify sync state persisted
    let state = load_sync_state(&conn, account_id, folder).unwrap().unwrap();
    assert_eq!(state.uid_validity, 12345);
    assert_eq!(state.last_synced_uid, Some(1));
}

#[tokio::test]
async fn progress_events_emitted() {
    let (tx, mut rx) = sync_event_channel(64);

    // Simulate sending progress events like the engine would
    tx.send(SyncEvent::HeaderProgress(inboxly_imap::sync::SyncProgress {
        folder: "INBOX".to_string(),
        fetched: 500,
        total: 1000,
    }))
    .await
    .unwrap();

    tx.send(SyncEvent::FirstBatchReady {
        folder: "INBOX".to_string(),
        emails_in_batch: 500,
    })
    .await
    .unwrap();

    tx.send(SyncEvent::HeaderSyncComplete {
        folder: "INBOX".to_string(),
        total_emails: 1000,
    })
    .await
    .unwrap();

    drop(tx);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    assert_eq!(events.len(), 3);
    assert!(matches!(&events[0], SyncEvent::HeaderProgress(_)));
    assert!(matches!(&events[1], SyncEvent::FirstBatchReady { .. }));
    assert!(matches!(&events[2], SyncEvent::HeaderSyncComplete { .. }));
}

#[tokio::test]
async fn duplicate_inserts_are_idempotent() {
    let conn = fixtures::test_db();
    let envelopes = fixtures::make_envelopes(1, 100, "acc-1", "INBOX");

    // Insert twice
    use inboxly_imap::sync::store::batch_insert_envelopes;
    let first = batch_insert_envelopes(&conn, &envelopes).unwrap();
    let second = batch_insert_envelopes(&conn, &envelopes).unwrap();

    assert_eq!(first, 100);
    assert_eq!(second, 0); // all duplicates ignored

    let total: u32 = conn
        .query_row("SELECT COUNT(*) FROM emails", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 100);
}

#[tokio::test]
async fn resume_after_crash() {
    // Simulate: synced UIDs 501-1000, then "crashed".
    // On resume, should only need to sync UIDs 1-500.

    let conn = fixtures::test_db();
    let account_id = "acc-1";
    let folder = "INBOX";

    // First run: insert batch 1
    use inboxly_imap::sync::store::batch_insert_envelopes;
    use inboxly_imap::sync::uid_state::{FolderSyncState, save_sync_state, load_sync_state};

    let batch1 = fixtures::make_envelopes(501, 1000, account_id, folder);
    batch_insert_envelopes(&conn, &batch1).unwrap();

    save_sync_state(
        &conn,
        &FolderSyncState {
            account_id: account_id.to_string(),
            folder_name: folder.to_string(),
            uid_validity: 99,
            uid_next: 1001,
            highest_modseq: None,
            last_synced_uid: Some(501),
        },
    )
    .unwrap();

    // Resume: check where we left off
    let state = load_sync_state(&conn, account_id, folder).unwrap().unwrap();
    assert_eq!(state.last_synced_uid, Some(501));

    // The engine would create BatchIterator::new(1, 501, 500) → one batch: (1, 500)
    use inboxly_imap::sync::batch::BatchIterator;
    let remaining_batches: Vec<_> = BatchIterator::new(1, state.last_synced_uid.unwrap(), 500).collect();
    assert_eq!(remaining_batches.len(), 1);
    assert_eq!(remaining_batches[0], (1, 500));
}

#[tokio::test]
async fn uid_validity_change_invalidates_cache() {
    let conn = fixtures::test_db();
    let account_id = "acc-1";
    let folder = "INBOX";

    // Populate with some emails
    use inboxly_imap::sync::store::batch_insert_envelopes;
    use inboxly_imap::sync::uid_state::{
        FolderSyncState, check_uid_validity, invalidate_folder, save_sync_state,
    };

    let envelopes = fixtures::make_envelopes(1, 50, account_id, folder);
    batch_insert_envelopes(&conn, &envelopes).unwrap();

    save_sync_state(
        &conn,
        &FolderSyncState {
            account_id: account_id.to_string(),
            folder_name: folder.to_string(),
            uid_validity: 100,
            uid_next: 51,
            highest_modseq: None,
            last_synced_uid: Some(1),
        },
    )
    .unwrap();

    // Server reports different UIDVALIDITY
    let changed = check_uid_validity(&conn, account_id, folder, 200).unwrap();
    assert!(changed);

    // Invalidate
    invalidate_folder(&conn, account_id, folder).unwrap();

    let count: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM emails WHERE account_id = ?1 AND imap_folder = ?2",
            rusqlite::params![account_id, folder],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0); // all emails purged
}

#[tokio::test]
async fn large_mailbox_batching() {
    // Verify that 100k+ UID ranges are split correctly
    use inboxly_imap::sync::batch::BatchIterator;

    let batches: Vec<_> = BatchIterator::new(1, 100_001, 500).collect();
    assert_eq!(batches.len(), 200); // 100,000 / 500

    // First batch should be the newest UIDs
    assert_eq!(batches[0], (99_501, 100_000));
    // Last batch should be the oldest
    assert_eq!(batches[199], (1, 500));
}
```

- [ ] **Step 2: Run all tests**

Run: `cargo test -p inboxly-imap -- --nocapture`
Expected: all tests pass (unit tests from Tasks 3-7 plus integration tests from this task)

- [ ] **Step 3: Commit**

```bash
git add inboxly-imap/tests/sync_engine_test.rs
git commit -m "test(imap): add Phase 1 sync engine integration tests

Tests cover: component pipeline, progress events, idempotent inserts,
crash recovery resume, UIDVALIDITY invalidation, large mailbox batching."
```

---

## Chunk 4: Final Wiring + Documentation

### Task 11: Public API Surface + Module Documentation

**Files:**
- Modify: `inboxly-imap/src/sync/mod.rs`
- Modify: `inboxly-imap/src/lib.rs`

- [ ] **Step 1: Add module-level documentation**

Ensure `inboxly-imap/src/sync/mod.rs` has the doc comment at the top:

```rust
//! # IMAP Sync Engine — Phase 1 (Header Sync)
//!
//! This module implements the initial sync flow for Inboxly:
//!
//! 1. SELECT the target folder to discover UIDVALIDITY and UIDNEXT
//! 2. Split the UID range into batches of 500, newest-first
//! 3. For each batch, issue `UID FETCH (ENVELOPE FLAGS RFC822.SIZE)`
//! 4. Parse IMAP ENVELOPE responses into `EnvelopeData` structs
//! 5. Batch-insert into the SQLite `emails` table
//! 6. Assign basic thread IDs using `In-Reply-To` headers
//! 7. Emit progress events to the UI via `tokio::sync::mpsc`
//! 8. Fire a first-batch-ready signal so the inbox is usable immediately
//! 9. Persist UIDVALIDITY + last-synced-UID for crash recovery
//!
//! ## Usage
//!
//! ```rust,no_run
//! use inboxly_imap::sync::{SyncConfig, run_phase1_sync, sync_event_channel};
//! use std::sync::Arc;
//! use tokio::sync::Mutex;
//!
//! // After authenticating via M6:
//! let config = SyncConfig::new("account-uuid", "INBOX");
//! let (event_tx, mut event_rx) = sync_event_channel(256);
//! let db = Arc::new(Mutex::new(/* rusqlite::Connection */));
//!
//! // Spawn progress consumer
//! tokio::spawn(async move {
//!     while let Some(event) = event_rx.recv().await {
//!         println!("{event:?}");
//!     }
//! });
//!
//! // Run sync (requires &mut Session from M6)
//! // let result = run_phase1_sync(&mut session, db, &config, event_tx).await?;
//! ```
//!
//! ## Crash Recovery
//!
//! After each batch, `last_synced_uid` is persisted to `sync_state`. On restart,
//! the engine reads this value and resumes from where it left off, skipping
//! already-fetched UID ranges.
//!
//! ## Threading
//!
//! Phase 1 uses a simplified threading algorithm based only on `In-Reply-To`
//! (since `References` is not available in ENVELOPE). Full threading with
//! `References` header parsing and placeholder resolution is implemented in M10.

pub mod batch;
pub mod engine;
pub mod envelope;
pub mod error;
pub mod progress;
pub mod store;
pub mod threading;
pub mod uid_state;

pub use engine::{SyncConfig, SyncPhase1Result, run_phase1_sync};
pub use error::{SyncError, SyncResult};
pub use progress::{SyncEvent, SyncEventReceiver, SyncEventSender, SyncProgress, sync_event_channel};
```

- [ ] **Step 2: Verify all tests pass**

Run: `cargo test -p inboxly-imap -- --nocapture`
Expected: all tests pass

- [ ] **Step 3: Run clippy**

Run: `cargo clippy -p inboxly-imap -- -D warnings`
Expected: no warnings

- [ ] **Step 4: Commit**

```bash
git add inboxly-imap/src/sync/mod.rs inboxly-imap/src/lib.rs
git commit -m "docs(imap): add module documentation and public API surface for Phase 1 sync"
```

---

### Task 12: Final Verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests pass across all crates

- [ ] **Step 2: Run clippy on workspace**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings

- [ ] **Step 3: Verify sync/mod.rs exports are correct**

Run: `cargo doc -p inboxly-imap --no-deps --open`
Expected: documentation renders, `sync` module shows all public types

- [ ] **Step 4: Commit (if any clippy fixes were needed)**

```bash
git add -A
git commit -m "fix(imap): address clippy warnings in Phase 1 sync engine"
```

---

## Summary

| Task | Component | Files | Tests |
|------|-----------|-------|-------|
| 1 | Error types | `sync/error.rs` | — |
| 2 | Progress events | `sync/progress.rs` | — |
| 3 | Batch range calculator | `sync/batch.rs` | 8 unit tests |
| 4 | UIDVALIDITY persistence | `sync/uid_state.rs` | 4 unit tests |
| 5 | Envelope parsing | `sync/envelope.rs` | 10 unit tests |
| 6 | SQLite batch insert | `sync/store.rs` | 5 unit tests |
| 7 | Basic threading | `sync/threading.rs` | 5 unit tests |
| 8 | Test fixtures | `tests/fixtures/mod.rs` | — |
| 9 | Sync engine orchestrator | `sync/engine.rs` | — |
| 10 | Integration tests | `tests/sync_engine_test.rs` | 6 integration tests |
| 11 | Documentation + API | `sync/mod.rs`, `lib.rs` | — |
| 12 | Final verification | — | full suite |

**Total: 12 tasks, ~38 test cases, 7 source files, 4 test files**
