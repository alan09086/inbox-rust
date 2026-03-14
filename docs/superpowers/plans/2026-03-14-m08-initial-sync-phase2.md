# M8: Initial Sync Phase 2 — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Background body download (RFC822 fetch) to Maildir with tantivy indexing, on-demand single-email fetch, progress reporting, offline action queue, and resume capability after interruption.

**Architecture:** Phase 2 runs as a background tokio task spawned after Phase 1 (M7) completes. It iterates through all emails in `emails` table where `body_downloaded = false`, fetches RFC822 bodies from IMAP in batches of 500 (newest-first by UID descending), writes each body to Maildir (M4's `MaildirStore::write_email()`), indexes the body in tantivy (M5's `SearchIndex::index_email()`), and marks `body_downloaded = true` in SQLite. Progress events are emitted via the existing `tokio::sync::mpsc` channel to the UI. On-demand fetch is triggered when the user opens an email whose body is not yet downloaded. The offline queue stores user actions (done, pin, snooze, move) taken while disconnected, replaying them on reconnect. Resume works by querying SQLite for remaining `body_downloaded = false` rows on restart.

**Prerequisites:** M7 (Phase 1 headers synced, `emails` table populated), M4 (Maildir write), M5 (tantivy index), M6 (IMAP connection).

**Tech Stack:** Rust, `async-imap`, `tokio`, `rusqlite`, `tantivy`, `maildir`

**Crate:** `inboxly-imap` (orchestrator + IMAP fetch), `inboxly-store` (SQLite, Maildir, tantivy interactions)

---

### Task 1: Add `body_downloaded` column to `emails` table

**Files:**
- Modify: `inboxly-store/src/schema.rs` (or wherever the SQLite schema / migrations are defined)

- [ ] **Step 1: Add `body_downloaded` column to the `emails` CREATE TABLE statement**

Add the column after `has_attachments`:

```sql
body_downloaded INTEGER NOT NULL DEFAULT 0
```

This is a boolean flag (0 = body not yet fetched, 1 = RFC822 body written to Maildir and indexed). Phase 1 (M7) inserts rows with `body_downloaded = 0`. Phase 2 sets it to `1` after successful fetch + write + index.

- [ ] **Step 2: Add a migration for existing databases**

If the migration system uses sequential numbered files, create the next migration:

```sql
ALTER TABLE emails ADD COLUMN body_downloaded INTEGER NOT NULL DEFAULT 0;
```

- [ ] **Step 3: Update the `EmailMeta` struct to include `body_downloaded`**

In `inboxly-core/src/types.rs` (or wherever `EmailMeta` is defined):

```rust
pub struct EmailMeta {
    // ... existing fields ...
    pub body_downloaded: bool,
}
```

- [ ] **Step 4: Update all SQLite INSERT/SELECT queries in `inboxly-store` that touch `emails`**

Ensure M7's header-insert code sets `body_downloaded = false` (or 0). Ensure SELECT queries for `EmailMeta` include the new column.

- [ ] **Step 5: Verify it compiles**

```bash
cargo check -p inboxly-store -p inboxly-core
```

- [ ] **Step 6: Commit**

```bash
git add inboxly-core/ inboxly-store/
git commit -m "feat: add body_downloaded column to emails table (M8)"
```

---

### Task 2: Define `SyncEvent` progress variants for Phase 2

**Files:**
- Modify: `inboxly-imap/src/events.rs` (or wherever `SyncEvent` / sync channel types are defined — M7 should have established this)

- [ ] **Step 1: Add Phase 2 progress variants to `SyncEvent` enum**

```rust
/// Progress update for Phase 2 body download.
BodyDownloadProgress {
    account_id: AccountId,
    folder: String,
    downloaded: u64,
    total: u64,
},
/// A single email body was fetched and indexed (for on-demand fetch completion).
BodyFetched {
    email_id: EmailId,
},
/// Phase 2 body download completed for a folder.
BodyDownloadComplete {
    account_id: AccountId,
    folder: String,
},
/// Phase 2 body download encountered a non-fatal error on a single email.
BodyDownloadError {
    email_id: EmailId,
    error: String,
},
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check -p inboxly-imap
```

- [ ] **Step 3: Commit**

```bash
git add inboxly-imap/
git commit -m "feat: add SyncEvent variants for Phase 2 body download progress (M8)"
```

---

### Task 3: Implement batch RFC822 FETCH command

**Files:**
- Create: `inboxly-imap/src/body_fetch.rs`
- Modify: `inboxly-imap/src/lib.rs` (add `mod body_fetch;`)

- [ ] **Step 1: Create `body_fetch.rs` with the batch fetch function**

This function takes an IMAP session reference and a slice of UIDs, fetches RFC822 bodies in a single IMAP command, and returns a `Vec<(u32, Vec<u8>)>` of `(uid, raw_rfc822_bytes)`.

```rust
use async_imap::Session;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::ImapError;

/// Maximum batch size for RFC822 FETCH commands.
pub const BODY_FETCH_BATCH_SIZE: usize = 500;

/// Fetch RFC822 bodies for a batch of UIDs.
///
/// Returns `(uid, raw_bytes)` pairs for each successfully fetched message.
/// UIDs that fail to fetch are logged and skipped (non-fatal).
pub async fn fetch_bodies_batch<S: AsyncRead + AsyncWrite + Unpin>(
    session: &mut Session<S>,
    uids: &[u32],
) -> Result<Vec<(u32, Vec<u8>)>, ImapError> {
    if uids.is_empty() {
        return Ok(Vec::new());
    }

    // Build UID set string: "45000,44999,44998,...,44501"
    let uid_set = uids
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let fetches = session
        .uid_fetch(&uid_set, "RFC822")
        .await
        .map_err(ImapError::Fetch)?;

    let mut results = Vec::with_capacity(uids.len());
    for fetch in fetches.iter() {
        if let (Some(uid), Some(body)) = (fetch.uid, fetch.body()) {
            results.push((uid, body.to_vec()));
        }
    }

    Ok(results)
}

/// Fetch a single email's RFC822 body by UID (for on-demand fetch).
pub async fn fetch_body_single<S: AsyncRead + AsyncWrite + Unpin>(
    session: &mut Session<S>,
    uid: u32,
) -> Result<Option<Vec<u8>>, ImapError> {
    let uid_str = uid.to_string();
    let fetches = session
        .uid_fetch(&uid_str, "RFC822")
        .await
        .map_err(ImapError::Fetch)?;

    for fetch in fetches.iter() {
        if fetch.uid == Some(uid) {
            if let Some(body) = fetch.body() {
                return Ok(Some(body.to_vec()));
            }
        }
    }

    Ok(None)
}
```

- [ ] **Step 2: Add `mod body_fetch;` to `inboxly-imap/src/lib.rs`**

- [ ] **Step 3: Add any needed error variants to `ImapError`**

If `ImapError::Fetch` doesn't exist, add it:

```rust
#[error("IMAP FETCH failed: {0}")]
Fetch(async_imap::error::Error),
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo check -p inboxly-imap
```

- [ ] **Step 5: Commit**

```bash
git add inboxly-imap/
git commit -m "feat: implement batch and single RFC822 FETCH commands (M8)"
```

---

### Task 4: Implement body processing pipeline (Maildir write + tantivy index + SQLite update)

**Files:**
- Create: `inboxly-imap/src/body_processor.rs`
- Modify: `inboxly-imap/src/lib.rs` (add `mod body_processor;`)

- [ ] **Step 1: Create `body_processor.rs` with the single-email processing function**

This function takes raw RFC822 bytes for one email, writes to Maildir, indexes in tantivy, and marks `body_downloaded = true` in SQLite. It is called for each email in a batch and also for on-demand fetches.

```rust
use inboxly_core::types::{AccountId, EmailId};
use inboxly_store::maildir::MaildirStore;
use inboxly_store::search::SearchIndex;
use inboxly_store::db::Database;

use crate::error::ImapError;

/// Process a single fetched RFC822 body:
/// 1. Write raw .eml to Maildir
/// 2. Parse body text/html for tantivy indexing
/// 3. Index in tantivy
/// 4. Mark body_downloaded = true in SQLite
///
/// Returns the EmailId on success.
pub async fn process_body(
    email_id: &EmailId,
    imap_uid: u32,
    folder: &str,
    raw_rfc822: &[u8],
    maildir: &MaildirStore,
    search_index: &SearchIndex,
    db: &Database,
) -> Result<(), ImapError> {
    // Step 1: Write to Maildir.
    // M4's MaildirStore should have a method like write_email(path, bytes) or
    // store_raw(folder, unique_name, bytes). Use the maildir_path from the
    // emails table, or generate one from the folder + UID.
    let maildir_path = maildir
        .write_raw_email(folder, imap_uid, raw_rfc822)
        .map_err(|e| ImapError::MaildirWrite(e.to_string()))?;

    // Step 2: Parse body for indexing.
    // Extract plaintext from the RFC822 message for tantivy.
    let body_text = extract_body_text(raw_rfc822);

    // Step 3: Index in tantivy.
    // M5's SearchIndex should have a method like index_body(email_id, body_text).
    search_index
        .index_body(email_id, &body_text)
        .map_err(|e| ImapError::IndexError(e.to_string()))?;

    // Step 4: Update SQLite — set body_downloaded = true, update maildir_path.
    db.mark_body_downloaded(email_id, &maildir_path)
        .map_err(|e| ImapError::DatabaseError(e.to_string()))?;

    Ok(())
}

/// Extract plaintext body from raw RFC822 bytes.
///
/// Handles multipart MIME: prefers text/plain, falls back to stripping HTML
/// from text/html. Returns empty string if no text body found.
fn extract_body_text(raw: &[u8]) -> String {
    // Use the `mail-parser` crate (or equivalent) to parse MIME.
    // This is a simplified sketch — the actual implementation should use
    // mail_parser::MessageParser or similar.
    let message = match mail_parser::MessageParser::default().parse(raw) {
        Some(msg) => msg,
        None => return String::new(),
    };

    // Prefer text/plain body.
    if let Some(text) = message.body_text(0) {
        return text.to_string();
    }

    // Fall back to HTML body, stripped of tags.
    if let Some(html) = message.body_html(0) {
        return strip_html_tags(&html);
    }

    String::new()
}

/// Naive HTML tag stripping for search indexing.
/// For production, consider using `scraper` or `ammonia`.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}
```

- [ ] **Step 2: Add required error variants to `ImapError`**

```rust
#[error("Maildir write failed: {0}")]
MaildirWrite(String),

#[error("Search index error: {0}")]
IndexError(String),

#[error("Database error: {0}")]
DatabaseError(String),
```

- [ ] **Step 3: Add `mail-parser` dependency to `inboxly-imap/Cargo.toml`**

```toml
[dependencies]
mail-parser = "0.9"
```

- [ ] **Step 4: Add the `Database::mark_body_downloaded()` method in `inboxly-store`**

In `inboxly-store/src/db.rs` (or the appropriate store module):

```rust
/// Mark an email's body as downloaded and update its maildir_path.
pub fn mark_body_downloaded(
    &self,
    email_id: &EmailId,
    maildir_path: &Path,
) -> Result<(), StoreError> {
    self.conn.execute(
        "UPDATE emails SET body_downloaded = 1, maildir_path = ?1 WHERE id = ?2",
        rusqlite::params![maildir_path.to_str(), email_id.as_str()],
    )?;
    Ok(())
}
```

- [ ] **Step 5: Add the `SearchIndex::index_body()` method in `inboxly-store`**

In `inboxly-store/src/search.rs` (or wherever M5's tantivy index is defined):

```rust
/// Index (or update) the body text for an email that was previously
/// indexed with headers only.
pub fn index_body(
    &self,
    email_id: &EmailId,
    body_text: &str,
) -> Result<(), StoreError> {
    let mut writer = self.index_writer.lock().unwrap();

    // Delete the old header-only document.
    let id_term = Term::from_field_text(self.fields.email_id, email_id.as_str());
    writer.delete_term(id_term.clone());

    // Re-add with body text included.
    // The full document should include all header fields (from, to, subject, date)
    // plus the body_text field. Fetch the header data from SQLite or from a
    // cached struct passed in. For simplicity, this method adds only the body
    // field — the caller should provide the full document.
    //
    // In practice, this should be:
    //   index_email_full(email_meta, body_text)
    // which builds the complete tantivy document.
    //
    // Placeholder: the actual API will depend on M5's index schema.
    let mut doc = tantivy::Document::new();
    doc.add_text(self.fields.email_id, email_id.as_str());
    doc.add_text(self.fields.body_text, body_text);
    writer.add_document(doc)?;
    writer.commit()?;

    Ok(())
}
```

**Note:** The exact tantivy API depends on M5's implementation. The key contract is: given an `EmailId` and body text, update the search index to include the body content. If M5 already provides `index_email()` that accepts an `EmailMeta` + optional body text, use that instead. Adapt to the actual M5 API signatures.

- [ ] **Step 6: Add `mod body_processor;` to `inboxly-imap/src/lib.rs`**

- [ ] **Step 7: Verify it compiles**

```bash
cargo check -p inboxly-imap -p inboxly-store
```

- [ ] **Step 8: Commit**

```bash
git add inboxly-imap/ inboxly-store/
git commit -m "feat: implement body processing pipeline — Maildir write, tantivy index, SQLite update (M8)"
```

---

### Task 5: Implement Phase 2 orchestrator (background task)

**Files:**
- Create: `inboxly-imap/src/phase2.rs`
- Modify: `inboxly-imap/src/lib.rs` (add `mod phase2;`)

- [ ] **Step 1: Create `phase2.rs` with the main orchestrator function**

This is the top-level async function spawned as a tokio task after Phase 1 completes. It loops through all un-downloaded emails in batches, fetching and processing each batch.

```rust
use std::sync::Arc;
use tokio::sync::mpsc;

use inboxly_core::types::AccountId;
use inboxly_store::db::Database;
use inboxly_store::maildir::MaildirStore;
use inboxly_store::search::SearchIndex;

use crate::body_fetch::{fetch_bodies_batch, BODY_FETCH_BATCH_SIZE};
use crate::body_processor::process_body;
use crate::events::SyncEvent;
use crate::error::ImapError;

/// Run Phase 2 body download for a single account + folder.
///
/// Fetches RFC822 bodies for all emails with `body_downloaded = false`,
/// in batches of 500, newest-first (descending UID order).
///
/// Emits `SyncEvent::BodyDownloadProgress` after each batch.
/// Emits `SyncEvent::BodyDownloadComplete` when finished.
///
/// This function is designed to be spawned as `tokio::spawn(phase2_download(...))`.
/// It does NOT block the UI — progress is communicated via the event channel.
///
/// **Resume capability:** On restart, this function queries SQLite for remaining
/// `body_downloaded = false` rows. Already-fetched emails are skipped automatically.
///
/// **Cancellation:** Respects `CancellationToken` for graceful shutdown. Each batch
/// checks the token before starting the next IMAP FETCH.
pub async fn phase2_download<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    account_id: AccountId,
    folder: String,
    session: Arc<tokio::sync::Mutex<Session<S>>>,
    db: Arc<Database>,
    maildir: Arc<MaildirStore>,
    search_index: Arc<SearchIndex>,
    event_tx: mpsc::Sender<SyncEvent>,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), ImapError> {
    // Query total count of bodies to download (for progress denominator).
    let total = db.count_emails_without_body(&account_id, &folder)?;
    if total == 0 {
        event_tx
            .send(SyncEvent::BodyDownloadComplete {
                account_id: account_id.clone(),
                folder: folder.clone(),
            })
            .await
            .ok();
        return Ok(());
    }

    let mut downloaded: u64 = 0;

    loop {
        // Check for cancellation before each batch.
        if cancel.is_cancelled() {
            tracing::info!(
                account_id = %account_id,
                folder = %folder,
                "Phase 2 download cancelled after {downloaded}/{total} bodies"
            );
            return Ok(());
        }

        // Get next batch of UIDs to fetch (newest-first, up to BODY_FETCH_BATCH_SIZE).
        let batch_uids = db.get_uids_without_body(
            &account_id,
            &folder,
            BODY_FETCH_BATCH_SIZE,
        )?;

        if batch_uids.is_empty() {
            break; // All done.
        }

        // Fetch RFC822 bodies from IMAP.
        let bodies = {
            let mut sess = session.lock().await;
            fetch_bodies_batch(&mut sess, &batch_uids).await?
        };

        // Process each fetched body.
        for (uid, raw_bytes) in &bodies {
            // Look up email_id from UID.
            let email_id = match db.get_email_id_by_uid(&account_id, &folder, *uid)? {
                Some(id) => id,
                None => {
                    tracing::warn!(uid = uid, "No email found for UID, skipping");
                    continue;
                }
            };

            match process_body(
                &email_id,
                *uid,
                &folder,
                raw_bytes,
                &maildir,
                &search_index,
                &db,
            )
            .await
            {
                Ok(()) => {
                    downloaded += 1;
                }
                Err(e) => {
                    // Non-fatal: log error, emit event, continue with next email.
                    tracing::error!(
                        email_id = %email_id,
                        uid = uid,
                        error = %e,
                        "Failed to process body, skipping"
                    );
                    event_tx
                        .send(SyncEvent::BodyDownloadError {
                            email_id,
                            error: e.to_string(),
                        })
                        .await
                        .ok();
                }
            }
        }

        // Emit progress after each batch.
        event_tx
            .send(SyncEvent::BodyDownloadProgress {
                account_id: account_id.clone(),
                folder: folder.clone(),
                downloaded,
                total,
            })
            .await
            .ok();
    }

    // Emit completion event.
    event_tx
        .send(SyncEvent::BodyDownloadComplete {
            account_id: account_id.clone(),
            folder: folder.clone(),
        })
        .await
        .ok();

    tracing::info!(
        account_id = %account_id,
        folder = %folder,
        downloaded = downloaded,
        "Phase 2 body download complete"
    );

    Ok(())
}
```

- [ ] **Step 2: Add required `Database` query methods in `inboxly-store`**

In `inboxly-store/src/db.rs`:

```rust
/// Count emails in a folder that have not had their body downloaded yet.
pub fn count_emails_without_body(
    &self,
    account_id: &AccountId,
    folder: &str,
) -> Result<u64, StoreError> {
    let count: u64 = self.conn.query_row(
        "SELECT COUNT(*) FROM emails WHERE account_id = ?1 AND imap_folder = ?2 AND body_downloaded = 0",
        rusqlite::params![account_id.as_str(), folder],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Get UIDs of emails without bodies, ordered by UID descending (newest first).
/// Returns at most `limit` UIDs.
pub fn get_uids_without_body(
    &self,
    account_id: &AccountId,
    folder: &str,
    limit: usize,
) -> Result<Vec<u32>, StoreError> {
    let mut stmt = self.conn.prepare(
        "SELECT imap_uid FROM emails
         WHERE account_id = ?1 AND imap_folder = ?2 AND body_downloaded = 0
         ORDER BY imap_uid DESC
         LIMIT ?3"
    )?;
    let uids = stmt
        .query_map(
            rusqlite::params![account_id.as_str(), folder, limit as i64],
            |row| row.get(0),
        )?
        .collect::<Result<Vec<u32>, _>>()?;
    Ok(uids)
}

/// Look up an email's EmailId by its IMAP UID within an account + folder.
pub fn get_email_id_by_uid(
    &self,
    account_id: &AccountId,
    folder: &str,
    uid: u32,
) -> Result<Option<EmailId>, StoreError> {
    let result = self.conn.query_row(
        "SELECT id FROM emails WHERE account_id = ?1 AND imap_folder = ?2 AND imap_uid = ?3",
        rusqlite::params![account_id.as_str(), folder, uid],
        |row| row.get::<_, String>(0),
    );
    match result {
        Ok(id) => Ok(Some(EmailId(id))),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(StoreError::from(e)),
    }
}
```

- [ ] **Step 3: Add `tokio-util` dependency to `inboxly-imap/Cargo.toml` for `CancellationToken`**

```toml
[dependencies]
tokio-util = { version = "0.7", features = ["rt"] }
```

- [ ] **Step 4: Add `mod phase2;` to `inboxly-imap/src/lib.rs`**

- [ ] **Step 5: Verify it compiles**

```bash
cargo check -p inboxly-imap -p inboxly-store
```

- [ ] **Step 6: Commit**

```bash
git add inboxly-imap/ inboxly-store/
git commit -m "feat: implement Phase 2 background body download orchestrator (M8)"
```

---

### Task 6: Implement on-demand body fetch (user opens email before Phase 2 reaches it)

**Files:**
- Create: `inboxly-imap/src/on_demand.rs`
- Modify: `inboxly-imap/src/lib.rs` (add `mod on_demand;`)

- [ ] **Step 1: Create `on_demand.rs` with the on-demand fetch function**

When the user opens an email whose body hasn't been downloaded yet (`body_downloaded = false`), the UI sends a request through the sync command channel. This function handles that request: fetch the single RFC822 body from IMAP, process it (Maildir + tantivy + SQLite), and emit a `BodyFetched` event so the UI can display the body.

```rust
use std::sync::Arc;
use tokio::sync::mpsc;

use inboxly_core::types::{AccountId, EmailId};
use inboxly_store::db::Database;
use inboxly_store::maildir::MaildirStore;
use inboxly_store::search::SearchIndex;

use crate::body_fetch::fetch_body_single;
use crate::body_processor::process_body;
use crate::events::SyncEvent;
use crate::error::ImapError;

/// Fetch a single email's body on demand.
///
/// Called when the user opens an email whose body has not been downloaded
/// by Phase 2 yet. This takes priority over the background batch download.
///
/// Returns the raw RFC822 bytes on success (so the caller can parse and
/// display immediately without re-reading from Maildir).
pub async fn fetch_body_on_demand<S: AsyncRead + AsyncWrite + Unpin>(
    email_id: &EmailId,
    account_id: &AccountId,
    folder: &str,
    imap_uid: u32,
    session: &Arc<tokio::sync::Mutex<Session<S>>>,
    db: &Database,
    maildir: &MaildirStore,
    search_index: &SearchIndex,
    event_tx: &mpsc::Sender<SyncEvent>,
) -> Result<Vec<u8>, ImapError> {
    // Check if body was already downloaded (race with Phase 2).
    if db.is_body_downloaded(email_id)? {
        // Body already exists — read from Maildir and return.
        let path = db.get_maildir_path(email_id)?
            .ok_or_else(|| ImapError::NotFound(email_id.clone()))?;
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| ImapError::MaildirRead(e.to_string()))?;
        return Ok(bytes);
    }

    // Fetch from IMAP.
    let raw_bytes = {
        let mut sess = session.lock().await;
        fetch_body_single(&mut sess, imap_uid)
            .await?
            .ok_or_else(|| ImapError::NotFound(email_id.clone()))?
    };

    // Process: Maildir write + tantivy index + SQLite update.
    process_body(
        email_id,
        imap_uid,
        folder,
        &raw_bytes,
        maildir,
        search_index,
        db,
    )
    .await?;

    // Notify UI that the body is ready.
    event_tx
        .send(SyncEvent::BodyFetched {
            email_id: email_id.clone(),
        })
        .await
        .ok();

    Ok(raw_bytes)
}
```

- [ ] **Step 2: Add helper methods to `Database`**

In `inboxly-store/src/db.rs`:

```rust
/// Check if an email's body has been downloaded.
pub fn is_body_downloaded(&self, email_id: &EmailId) -> Result<bool, StoreError> {
    let downloaded: bool = self.conn.query_row(
        "SELECT body_downloaded FROM emails WHERE id = ?1",
        rusqlite::params![email_id.as_str()],
        |row| row.get(0),
    )?;
    Ok(downloaded)
}

/// Get the maildir_path for an email.
pub fn get_maildir_path(&self, email_id: &EmailId) -> Result<Option<PathBuf>, StoreError> {
    let path: Option<String> = self.conn.query_row(
        "SELECT maildir_path FROM emails WHERE id = ?1",
        rusqlite::params![email_id.as_str()],
        |row| row.get(0),
    )?;
    Ok(path.map(PathBuf::from))
}
```

- [ ] **Step 3: Add error variants**

```rust
#[error("Email not found: {0}")]
NotFound(EmailId),

#[error("Maildir read failed: {0}")]
MaildirRead(String),
```

- [ ] **Step 4: Add `mod on_demand;` to `inboxly-imap/src/lib.rs`**

- [ ] **Step 5: Verify it compiles**

```bash
cargo check -p inboxly-imap -p inboxly-store
```

- [ ] **Step 6: Commit**

```bash
git add inboxly-imap/ inboxly-store/
git commit -m "feat: implement on-demand body fetch for emails opened before Phase 2 reaches them (M8)"
```

---

### Task 7: Define offline queue action types and SQLite operations

**Files:**
- Modify: `inboxly-store/src/db.rs` (or create `inboxly-store/src/offline_queue.rs`)
- Modify: `inboxly-core/src/types.rs` (add `OfflineAction` enum)

- [ ] **Step 1: Define `OfflineAction` enum in `inboxly-core`**

```rust
use serde::{Deserialize, Serialize};

/// An action taken by the user while offline (or during sync).
/// Queued in SQLite and replayed against IMAP when connectivity is restored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OfflineAction {
    /// Mark email as read (set \Seen flag on IMAP).
    MarkRead {
        account_id: AccountId,
        folder: String,
        imap_uid: u32,
    },
    /// Mark email as unread (clear \Seen flag).
    MarkUnread {
        account_id: AccountId,
        folder: String,
        imap_uid: u32,
    },
    /// Star/flag an email (set \Flagged).
    Star {
        account_id: AccountId,
        folder: String,
        imap_uid: u32,
    },
    /// Unstar an email (clear \Flagged).
    Unstar {
        account_id: AccountId,
        folder: String,
        imap_uid: u32,
    },
    /// Archive / "Done" — move to archive or set \Deleted + expunge
    /// depending on provider. For Gmail: move to All Mail.
    MarkDone {
        account_id: AccountId,
        folder: String,
        imap_uid: u32,
    },
    /// Move email to trash.
    MoveToTrash {
        account_id: AccountId,
        folder: String,
        imap_uid: u32,
    },
    /// Move email to a different IMAP folder.
    MoveToFolder {
        account_id: AccountId,
        from_folder: String,
        to_folder: String,
        imap_uid: u32,
    },
    /// Mark as answered (set \Answered flag) — set after sending a reply.
    MarkAnswered {
        account_id: AccountId,
        folder: String,
        imap_uid: u32,
    },
    /// Send a queued draft (composed offline).
    SendDraft {
        account_id: AccountId,
        draft_maildir_path: String,
    },
}
```

- [ ] **Step 2: Create the `offline_queue` table (if not already in schema)**

The design spec defines this table. Ensure it exists in the schema:

```sql
CREATE TABLE IF NOT EXISTS offline_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    action TEXT NOT NULL,          -- serialized OfflineAction variant name
    payload_json TEXT NOT NULL,    -- JSON serialized OfflineAction
    created_at INTEGER NOT NULL    -- unix epoch
);
```

- [ ] **Step 3: Implement queue operations in `inboxly-store`**

```rust
use chrono::Utc;
use inboxly_core::types::OfflineAction;

impl Database {
    /// Enqueue an offline action for later replay.
    pub fn enqueue_offline_action(&self, action: &OfflineAction) -> Result<i64, StoreError> {
        let action_name = action.variant_name();
        let payload = serde_json::to_string(action)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let now = Utc::now().timestamp();

        self.conn.execute(
            "INSERT INTO offline_queue (action, payload_json, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![action_name, payload, now],
        )?;

        let id = self.conn.last_insert_rowid();
        Ok(id)
    }

    /// Dequeue all pending offline actions, ordered by creation time (FIFO).
    pub fn dequeue_all_offline_actions(&self) -> Result<Vec<(i64, OfflineAction)>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, payload_json FROM offline_queue ORDER BY created_at ASC"
        )?;
        let actions = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                let json: String = row.get(1)?;
                Ok((id, json))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut result = Vec::with_capacity(actions.len());
        for (id, json) in actions {
            let action: OfflineAction = serde_json::from_str(&json)
                .map_err(|e| StoreError::Deserialization(e.to_string()))?;
            result.push((id, action));
        }
        Ok(result)
    }

    /// Remove a successfully replayed offline action.
    pub fn remove_offline_action(&self, id: i64) -> Result<(), StoreError> {
        self.conn.execute(
            "DELETE FROM offline_queue WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(())
    }

    /// Count pending offline actions.
    pub fn count_offline_actions(&self) -> Result<u64, StoreError> {
        let count: u64 = self.conn.query_row(
            "SELECT COUNT(*) FROM offline_queue",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

impl OfflineAction {
    /// Return a short name for the action variant (for the `action` column).
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::MarkRead { .. } => "mark_read",
            Self::MarkUnread { .. } => "mark_unread",
            Self::Star { .. } => "star",
            Self::Unstar { .. } => "unstar",
            Self::MarkDone { .. } => "mark_done",
            Self::MoveToTrash { .. } => "move_to_trash",
            Self::MoveToFolder { .. } => "move_to_folder",
            Self::MarkAnswered { .. } => "mark_answered",
            Self::SendDraft { .. } => "send_draft",
        }
    }
}
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo check -p inboxly-core -p inboxly-store
```

- [ ] **Step 5: Commit**

```bash
git add inboxly-core/ inboxly-store/
git commit -m "feat: define OfflineAction types and offline queue SQLite operations (M8)"
```

---

### Task 8: Implement offline action replay on reconnect

**Files:**
- Create: `inboxly-imap/src/offline_replay.rs`
- Modify: `inboxly-imap/src/lib.rs` (add `mod offline_replay;`)

- [ ] **Step 1: Create `offline_replay.rs` with the replay function**

This function drains the offline queue and replays each action against IMAP. Called on reconnect (after incremental sync or at startup when connectivity is restored).

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

use inboxly_core::types::OfflineAction;
use inboxly_store::db::Database;

use crate::error::ImapError;

/// Replay all queued offline actions against the IMAP server.
///
/// Actions are replayed in FIFO order. Each successfully replayed action
/// is removed from the queue. Failed actions remain in the queue for
/// the next replay attempt.
///
/// Returns the count of successfully replayed actions.
pub async fn replay_offline_queue<S: AsyncRead + AsyncWrite + Unpin>(
    session: &Arc<Mutex<Session<S>>>,
    db: &Database,
) -> Result<u64, ImapError> {
    let actions = db.dequeue_all_offline_actions()
        .map_err(|e| ImapError::DatabaseError(e.to_string()))?;

    if actions.is_empty() {
        return Ok(0);
    }

    tracing::info!(count = actions.len(), "Replaying offline queue");

    let mut replayed = 0u64;

    for (id, action) in &actions {
        match replay_single_action(session, action).await {
            Ok(()) => {
                db.remove_offline_action(*id)
                    .map_err(|e| ImapError::DatabaseError(e.to_string()))?;
                replayed += 1;
            }
            Err(e) => {
                // Log and continue — action stays in queue for next attempt.
                tracing::error!(
                    action_id = id,
                    action = ?action,
                    error = %e,
                    "Failed to replay offline action, will retry"
                );
            }
        }
    }

    tracing::info!(
        replayed = replayed,
        remaining = actions.len() as u64 - replayed,
        "Offline queue replay complete"
    );

    Ok(replayed)
}

/// Replay a single offline action against IMAP.
async fn replay_single_action<S: AsyncRead + AsyncWrite + Unpin>(
    session: &Arc<Mutex<Session<S>>>,
    action: &OfflineAction,
) -> Result<(), ImapError> {
    let mut sess = session.lock().await;

    match action {
        OfflineAction::MarkRead { folder, imap_uid, .. } => {
            sess.select(folder).await.map_err(ImapError::Select)?;
            sess.uid_store(imap_uid.to_string(), "+FLAGS (\\Seen)")
                .await
                .map_err(ImapError::Store)?;
        }
        OfflineAction::MarkUnread { folder, imap_uid, .. } => {
            sess.select(folder).await.map_err(ImapError::Select)?;
            sess.uid_store(imap_uid.to_string(), "-FLAGS (\\Seen)")
                .await
                .map_err(ImapError::Store)?;
        }
        OfflineAction::Star { folder, imap_uid, .. } => {
            sess.select(folder).await.map_err(ImapError::Select)?;
            sess.uid_store(imap_uid.to_string(), "+FLAGS (\\Flagged)")
                .await
                .map_err(ImapError::Store)?;
        }
        OfflineAction::Unstar { folder, imap_uid, .. } => {
            sess.select(folder).await.map_err(ImapError::Select)?;
            sess.uid_store(imap_uid.to_string(), "-FLAGS (\\Flagged)")
                .await
                .map_err(ImapError::Store)?;
        }
        OfflineAction::MarkDone { folder, imap_uid, .. } => {
            // Archive: move to archive folder (Gmail: [Gmail]/All Mail)
            // or mark \Deleted + expunge for standard IMAP.
            // For v1, use COPY + STORE \Deleted + EXPUNGE pattern.
            sess.select(folder).await.map_err(ImapError::Select)?;
            // TODO: Determine archive folder per provider (M9/M25 refinement).
            // For now, just set \Seen (done = read + out of inbox).
            sess.uid_store(imap_uid.to_string(), "+FLAGS (\\Seen \\Deleted)")
                .await
                .map_err(ImapError::Store)?;
            sess.expunge().await.map_err(ImapError::Expunge)?;
        }
        OfflineAction::MoveToTrash { folder, imap_uid, .. } => {
            sess.select(folder).await.map_err(ImapError::Select)?;
            // COPY to Trash, then delete from source.
            sess.uid_copy(imap_uid.to_string(), "Trash")
                .await
                .map_err(ImapError::Copy)?;
            sess.uid_store(imap_uid.to_string(), "+FLAGS (\\Deleted)")
                .await
                .map_err(ImapError::Store)?;
            sess.expunge().await.map_err(ImapError::Expunge)?;
        }
        OfflineAction::MoveToFolder { from_folder, to_folder, imap_uid, .. } => {
            sess.select(from_folder).await.map_err(ImapError::Select)?;
            sess.uid_copy(imap_uid.to_string(), to_folder)
                .await
                .map_err(ImapError::Copy)?;
            sess.uid_store(imap_uid.to_string(), "+FLAGS (\\Deleted)")
                .await
                .map_err(ImapError::Store)?;
            sess.expunge().await.map_err(ImapError::Expunge)?;
        }
        OfflineAction::MarkAnswered { folder, imap_uid, .. } => {
            sess.select(folder).await.map_err(ImapError::Select)?;
            sess.uid_store(imap_uid.to_string(), "+FLAGS (\\Answered)")
                .await
                .map_err(ImapError::Store)?;
        }
        OfflineAction::SendDraft { draft_maildir_path, .. } => {
            // Draft sending is handled by SMTP (M23), not IMAP.
            // Queue the draft for the SMTP sender.
            // For now, log a warning — SMTP integration comes in M23.
            tracing::warn!(
                path = draft_maildir_path,
                "SendDraft replay deferred to M23 (SMTP integration)"
            );
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Add IMAP error variants for STORE, COPY, EXPUNGE, SELECT**

```rust
#[error("IMAP SELECT failed: {0}")]
Select(async_imap::error::Error),

#[error("IMAP STORE failed: {0}")]
Store(async_imap::error::Error),

#[error("IMAP COPY failed: {0}")]
Copy(async_imap::error::Error),

#[error("IMAP EXPUNGE failed: {0}")]
Expunge(async_imap::error::Error),
```

- [ ] **Step 3: Add `mod offline_replay;` to `inboxly-imap/src/lib.rs`**

- [ ] **Step 4: Verify it compiles**

```bash
cargo check -p inboxly-imap
```

- [ ] **Step 5: Commit**

```bash
git add inboxly-imap/
git commit -m "feat: implement offline action replay against IMAP on reconnect (M8)"
```

---

### Task 9: Wire Phase 2 launch into the sync engine

**Files:**
- Modify: `inboxly-imap/src/sync.rs` (or wherever the main sync orchestrator from M7 lives)

- [ ] **Step 1: After Phase 1 completes, spawn Phase 2 as a background tokio task**

In the sync engine's main function (established in M7), after Phase 1 header sync finishes, spawn Phase 2:

```rust
// Phase 1 complete — inbox is usable.
event_tx
    .send(SyncEvent::Phase1Complete {
        account_id: account_id.clone(),
    })
    .await
    .ok();

// Spawn Phase 2 as background task — does NOT block UI.
let phase2_cancel = cancel.child_token();
let phase2_handle = tokio::spawn({
    let account_id = account_id.clone();
    let session = Arc::clone(&session);
    let db = Arc::clone(&db);
    let maildir = Arc::clone(&maildir);
    let search_index = Arc::clone(&search_index);
    let event_tx = event_tx.clone();

    async move {
        // Download bodies for each synced folder.
        for folder in &synced_folders {
            if let Err(e) = crate::phase2::phase2_download(
                account_id.clone(),
                folder.clone(),
                Arc::clone(&session),
                Arc::clone(&db),
                Arc::clone(&maildir),
                Arc::clone(&search_index),
                event_tx.clone(),
                phase2_cancel.clone(),
            )
            .await
            {
                tracing::error!(
                    account_id = %account_id,
                    folder = %folder,
                    error = %e,
                    "Phase 2 body download failed"
                );
            }
        }
    }
});
```

- [ ] **Step 2: Wire on-demand fetch into the sync command handler**

Add a command variant for on-demand fetch requests from the UI:

```rust
/// Commands the UI can send to the sync engine.
pub enum SyncCommand {
    // ... existing variants from M7 ...

    /// Fetch a single email's body on demand (user opened it before Phase 2 reached it).
    FetchBodyOnDemand {
        email_id: EmailId,
        account_id: AccountId,
        folder: String,
        imap_uid: u32,
        /// Channel to send the raw RFC822 bytes back to the requester.
        reply: tokio::sync::oneshot::Sender<Result<Vec<u8>, ImapError>>,
    },
}
```

In the command handler loop, add:

```rust
SyncCommand::FetchBodyOnDemand { email_id, account_id, folder, imap_uid, reply } => {
    let result = crate::on_demand::fetch_body_on_demand(
        &email_id,
        &account_id,
        &folder,
        imap_uid,
        &session,
        &db,
        &maildir,
        &search_index,
        &event_tx,
    )
    .await;
    let _ = reply.send(result);
}
```

- [ ] **Step 3: Wire offline queue replay into reconnect logic**

After establishing/re-establishing an IMAP connection, replay any queued actions:

```rust
// After IMAP connection established (or re-established after disconnect):
match crate::offline_replay::replay_offline_queue(&session, &db).await {
    Ok(count) if count > 0 => {
        tracing::info!(count = count, "Replayed offline actions");
    }
    Ok(_) => {} // No queued actions.
    Err(e) => {
        tracing::error!(error = %e, "Failed to replay offline queue");
    }
}
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo check -p inboxly-imap
```

- [ ] **Step 5: Commit**

```bash
git add inboxly-imap/
git commit -m "feat: wire Phase 2 launch, on-demand fetch, and offline replay into sync engine (M8)"
```

---

### Task 10: Unit tests for body processing and offline queue

**Files:**
- Create: `inboxly-imap/src/body_processor_tests.rs` (or `#[cfg(test)] mod tests` in `body_processor.rs`)
- Create: `inboxly-store/src/offline_queue_tests.rs` (or `#[cfg(test)] mod tests` in `db.rs`)

- [ ] **Step 1: Test `extract_body_text` with plain text email**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_body_text_plain() {
        let raw = b"From: alice@example.com\r\n\
            To: bob@example.com\r\n\
            Subject: Hello\r\n\
            Content-Type: text/plain\r\n\
            \r\n\
            This is the body text.\r\n";
        let text = extract_body_text(raw);
        assert!(text.contains("This is the body text."));
    }
}
```

- [ ] **Step 2: Test `extract_body_text` with HTML email**

```rust
#[test]
fn test_extract_body_text_html_fallback() {
    let raw = b"From: alice@example.com\r\n\
        To: bob@example.com\r\n\
        Subject: Hello\r\n\
        Content-Type: text/html\r\n\
        \r\n\
        <html><body><p>Hello <b>World</b></p></body></html>\r\n";
    let text = extract_body_text(raw);
    assert!(text.contains("Hello"));
    assert!(text.contains("World"));
    assert!(!text.contains("<p>"));
    assert!(!text.contains("<b>"));
}
```

- [ ] **Step 3: Test `extract_body_text` with multipart MIME**

```rust
#[test]
fn test_extract_body_text_multipart() {
    let raw = b"From: alice@example.com\r\n\
        To: bob@example.com\r\n\
        Subject: Hello\r\n\
        Content-Type: multipart/alternative; boundary=\"boundary42\"\r\n\
        \r\n\
        --boundary42\r\n\
        Content-Type: text/plain\r\n\
        \r\n\
        Plain text version.\r\n\
        --boundary42\r\n\
        Content-Type: text/html\r\n\
        \r\n\
        <html><body>HTML version.</body></html>\r\n\
        --boundary42--\r\n";
    let text = extract_body_text(raw);
    // Should prefer text/plain.
    assert!(text.contains("Plain text version."));
}
```

- [ ] **Step 4: Test `strip_html_tags`**

```rust
#[test]
fn test_strip_html_tags() {
    assert_eq!(strip_html_tags("<p>Hello</p>"), "Hello");
    assert_eq!(strip_html_tags("<b>Bold</b> and <i>italic</i>"), "Bold and italic");
    assert_eq!(strip_html_tags("No tags here"), "No tags here");
    assert_eq!(strip_html_tags(""), "");
    assert_eq!(
        strip_html_tags("<div class=\"x\">Content</div>"),
        "Content"
    );
}
```

- [ ] **Step 5: Test empty/malformed input**

```rust
#[test]
fn test_extract_body_text_empty() {
    let text = extract_body_text(b"");
    assert!(text.is_empty());
}

#[test]
fn test_extract_body_text_headers_only() {
    let raw = b"From: alice@example.com\r\n\
        Subject: No body\r\n\
        \r\n";
    let text = extract_body_text(raw);
    assert!(text.is_empty());
}
```

- [ ] **Step 6: Commit**

```bash
git add inboxly-imap/
git commit -m "test: add unit tests for body text extraction (M8)"
```

---

### Task 11: Unit tests for offline queue operations

**Files:**
- Modify: `inboxly-store/src/db.rs` (add `#[cfg(test)]` module) or create test file

- [ ] **Step 1: Test enqueue and dequeue round-trip**

```rust
#[cfg(test)]
mod offline_queue_tests {
    use super::*;
    use inboxly_core::types::{AccountId, OfflineAction};

    fn test_db() -> Database {
        // Create an in-memory SQLite database with schema applied.
        Database::open_in_memory().expect("Failed to create test database")
    }

    #[test]
    fn test_enqueue_dequeue_roundtrip() {
        let db = test_db();
        let action = OfflineAction::MarkRead {
            account_id: AccountId::new(),
            folder: "INBOX".to_string(),
            imap_uid: 42,
        };

        let id = db.enqueue_offline_action(&action).unwrap();
        assert!(id > 0);

        let actions = db.dequeue_all_offline_actions().unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, id);

        // Verify the action deserialized correctly.
        match &actions[0].1 {
            OfflineAction::MarkRead { imap_uid, folder, .. } => {
                assert_eq!(*imap_uid, 42);
                assert_eq!(folder, "INBOX");
            }
            other => panic!("Expected MarkRead, got {:?}", other),
        }
    }
}
```

- [ ] **Step 2: Test FIFO ordering**

```rust
#[test]
fn test_offline_queue_fifo_order() {
    let db = test_db();
    let account = AccountId::new();

    let actions = vec![
        OfflineAction::MarkRead { account_id: account.clone(), folder: "INBOX".into(), imap_uid: 1 },
        OfflineAction::Star { account_id: account.clone(), folder: "INBOX".into(), imap_uid: 2 },
        OfflineAction::MarkDone { account_id: account.clone(), folder: "INBOX".into(), imap_uid: 3 },
    ];

    for action in &actions {
        db.enqueue_offline_action(action).unwrap();
    }

    let dequeued = db.dequeue_all_offline_actions().unwrap();
    assert_eq!(dequeued.len(), 3);

    // Verify FIFO order: MarkRead (uid 1), Star (uid 2), MarkDone (uid 3).
    match &dequeued[0].1 {
        OfflineAction::MarkRead { imap_uid, .. } => assert_eq!(*imap_uid, 1),
        other => panic!("Expected MarkRead, got {:?}", other),
    }
    match &dequeued[1].1 {
        OfflineAction::Star { imap_uid, .. } => assert_eq!(*imap_uid, 2),
        other => panic!("Expected Star, got {:?}", other),
    }
    match &dequeued[2].1 {
        OfflineAction::MarkDone { imap_uid, .. } => assert_eq!(*imap_uid, 3),
        other => panic!("Expected MarkDone, got {:?}", other),
    }
}
```

- [ ] **Step 3: Test remove action**

```rust
#[test]
fn test_remove_offline_action() {
    let db = test_db();
    let action = OfflineAction::Star {
        account_id: AccountId::new(),
        folder: "INBOX".into(),
        imap_uid: 99,
    };

    let id = db.enqueue_offline_action(&action).unwrap();
    assert_eq!(db.count_offline_actions().unwrap(), 1);

    db.remove_offline_action(id).unwrap();
    assert_eq!(db.count_offline_actions().unwrap(), 0);

    let remaining = db.dequeue_all_offline_actions().unwrap();
    assert!(remaining.is_empty());
}
```

- [ ] **Step 4: Test empty queue**

```rust
#[test]
fn test_empty_offline_queue() {
    let db = test_db();
    let actions = db.dequeue_all_offline_actions().unwrap();
    assert!(actions.is_empty());
    assert_eq!(db.count_offline_actions().unwrap(), 0);
}
```

- [ ] **Step 5: Test all action variants serialize/deserialize correctly**

```rust
#[test]
fn test_all_offline_action_variants_roundtrip() {
    let db = test_db();
    let account = AccountId::new();

    let variants = vec![
        OfflineAction::MarkRead { account_id: account.clone(), folder: "INBOX".into(), imap_uid: 1 },
        OfflineAction::MarkUnread { account_id: account.clone(), folder: "INBOX".into(), imap_uid: 2 },
        OfflineAction::Star { account_id: account.clone(), folder: "INBOX".into(), imap_uid: 3 },
        OfflineAction::Unstar { account_id: account.clone(), folder: "INBOX".into(), imap_uid: 4 },
        OfflineAction::MarkDone { account_id: account.clone(), folder: "INBOX".into(), imap_uid: 5 },
        OfflineAction::MoveToTrash { account_id: account.clone(), folder: "INBOX".into(), imap_uid: 6 },
        OfflineAction::MoveToFolder {
            account_id: account.clone(),
            from_folder: "INBOX".into(),
            to_folder: "Archive".into(),
            imap_uid: 7,
        },
        OfflineAction::MarkAnswered { account_id: account.clone(), folder: "INBOX".into(), imap_uid: 8 },
        OfflineAction::SendDraft {
            account_id: account.clone(),
            draft_maildir_path: "/tmp/draft.eml".into(),
        },
    ];

    for action in &variants {
        db.enqueue_offline_action(action).unwrap();
    }

    let dequeued = db.dequeue_all_offline_actions().unwrap();
    assert_eq!(dequeued.len(), 9);

    // Verify variant names match.
    let expected_names = [
        "mark_read", "mark_unread", "star", "unstar",
        "mark_done", "move_to_trash", "move_to_folder",
        "mark_answered", "send_draft",
    ];
    for (i, (_, action)) in dequeued.iter().enumerate() {
        assert_eq!(action.variant_name(), expected_names[i]);
    }
}
```

- [ ] **Step 6: Verify tests pass**

```bash
cargo test -p inboxly-store -- offline_queue
```

- [ ] **Step 7: Commit**

```bash
git add inboxly-store/
git commit -m "test: add unit tests for offline queue enqueue/dequeue/remove (M8)"
```

---

### Task 12: Integration tests for Phase 2 resume and progress

**Files:**
- Create: `inboxly-imap/tests/phase2_integration.rs`

- [ ] **Step 1: Test resume capability — interrupted download resumes from where it left off**

This test simulates a partial Phase 2 download, then verifies that restarting Phase 2 only fetches the remaining emails.

```rust
//! Integration tests for Phase 2 body download.
//!
//! These tests use an in-memory SQLite database and mock IMAP session.
//! They verify resume, progress reporting, and error handling.

use inboxly_core::types::{AccountId, EmailId};
use inboxly_store::db::Database;

/// Simulate Phase 2 being interrupted after downloading 3 of 5 emails,
/// then verify that resume only fetches the remaining 2.
#[tokio::test]
async fn test_phase2_resume_after_interruption() {
    let db = Database::open_in_memory().unwrap();
    let account_id = AccountId::new();

    // Insert 5 emails with body_downloaded = false (simulating Phase 1 output).
    for uid in 1..=5 {
        db.insert_test_email(&account_id, "INBOX", uid, false).unwrap();
    }

    // Mark 3 as downloaded (simulating partial Phase 2 run).
    for uid in 3..=5 {
        let email_id = db.get_email_id_by_uid(&account_id, "INBOX", uid).unwrap().unwrap();
        db.mark_body_downloaded(&email_id, &format!("/tmp/{uid}.eml").into()).unwrap();
    }

    // Query remaining — should be UIDs 1 and 2.
    let remaining = db.get_uids_without_body(&account_id, "INBOX", 500).unwrap();
    assert_eq!(remaining.len(), 2);
    assert!(remaining.contains(&1));
    assert!(remaining.contains(&2));
}
```

- [ ] **Step 2: Test newest-first ordering**

```rust
#[tokio::test]
async fn test_phase2_fetches_newest_first() {
    let db = Database::open_in_memory().unwrap();
    let account_id = AccountId::new();

    // Insert 10 emails.
    for uid in 1..=10 {
        db.insert_test_email(&account_id, "INBOX", uid, false).unwrap();
    }

    // Get first batch — should be in descending UID order.
    let batch = db.get_uids_without_body(&account_id, "INBOX", 5).unwrap();
    assert_eq!(batch, vec![10, 9, 8, 7, 6]);
}
```

- [ ] **Step 3: Test progress counting**

```rust
#[tokio::test]
async fn test_phase2_progress_count() {
    let db = Database::open_in_memory().unwrap();
    let account_id = AccountId::new();

    for uid in 1..=100 {
        db.insert_test_email(&account_id, "INBOX", uid, false).unwrap();
    }

    let total = db.count_emails_without_body(&account_id, "INBOX").unwrap();
    assert_eq!(total, 100);

    // Mark 40 as downloaded.
    for uid in 61..=100 {
        let email_id = db.get_email_id_by_uid(&account_id, "INBOX", uid).unwrap().unwrap();
        db.mark_body_downloaded(&email_id, &format!("/tmp/{uid}.eml").into()).unwrap();
    }

    let remaining = db.count_emails_without_body(&account_id, "INBOX").unwrap();
    assert_eq!(remaining, 60);
}
```

- [ ] **Step 4: Test `is_body_downloaded` check (for on-demand race condition)**

```rust
#[tokio::test]
async fn test_is_body_downloaded() {
    let db = Database::open_in_memory().unwrap();
    let account_id = AccountId::new();

    db.insert_test_email(&account_id, "INBOX", 42, false).unwrap();
    let email_id = db.get_email_id_by_uid(&account_id, "INBOX", 42).unwrap().unwrap();

    assert!(!db.is_body_downloaded(&email_id).unwrap());

    db.mark_body_downloaded(&email_id, &"/tmp/42.eml".into()).unwrap();

    assert!(db.is_body_downloaded(&email_id).unwrap());
}
```

- [ ] **Step 5: Add `Database::insert_test_email()` helper (test-only)**

In `inboxly-store/src/db.rs`:

```rust
#[cfg(any(test, feature = "test-helpers"))]
impl Database {
    /// Insert a minimal test email row for integration testing.
    pub fn insert_test_email(
        &self,
        account_id: &AccountId,
        folder: &str,
        imap_uid: u32,
        body_downloaded: bool,
    ) -> Result<EmailId, StoreError> {
        let email_id = EmailId(format!("test-{}-{}-{}", account_id, folder, imap_uid));
        self.conn.execute(
            "INSERT INTO emails (id, account_id, imap_folder, imap_uid, body_downloaded, \
             subject, snippet, date, flags, size_bytes, from_name, from_address, \
             to_json, cc_json, has_attachments, message_id_header, thread_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, '', '', 0, 0, 0, '', '', '[]', '[]', 0, ?1, ?1)",
            rusqlite::params![
                email_id.as_str(),
                account_id.as_str(),
                folder,
                imap_uid,
                body_downloaded as i32,
            ],
        )?;
        Ok(email_id)
    }
}
```

- [ ] **Step 6: Verify tests pass**

```bash
cargo test -p inboxly-imap -- phase2
cargo test -p inboxly-store
```

- [ ] **Step 7: Commit**

```bash
git add inboxly-imap/ inboxly-store/
git commit -m "test: add integration tests for Phase 2 resume, ordering, and progress (M8)"
```

---

### Task 13: Run full workspace build and clippy

- [ ] **Step 1: Run full workspace check**

```bash
cargo test --workspace && cargo clippy --workspace -- -D warnings
```

Expected: All tests pass, no clippy warnings.

- [ ] **Step 2: Fix any issues found by clippy or tests**

- [ ] **Step 3: Final commit if any fixes were needed**

```bash
git add -A
git commit -m "fix: address clippy warnings and test failures (M8)"
```

---

## Summary of Files Modified/Created

### New files:
- `inboxly-imap/src/body_fetch.rs` — batch and single RFC822 FETCH commands
- `inboxly-imap/src/body_processor.rs` — Maildir write + tantivy index + SQLite update pipeline
- `inboxly-imap/src/phase2.rs` — background orchestrator task
- `inboxly-imap/src/on_demand.rs` — on-demand single-email body fetch
- `inboxly-imap/src/offline_replay.rs` — offline queue replay against IMAP
- `inboxly-imap/tests/phase2_integration.rs` — integration tests

### Modified files:
- `inboxly-core/src/types.rs` — `body_downloaded` field on `EmailMeta`, `OfflineAction` enum
- `inboxly-store/src/schema.rs` — `body_downloaded` column, `offline_queue` table
- `inboxly-store/src/db.rs` — `mark_body_downloaded()`, `count_emails_without_body()`, `get_uids_without_body()`, `get_email_id_by_uid()`, `is_body_downloaded()`, `get_maildir_path()`, offline queue methods, test helpers
- `inboxly-store/src/search.rs` — `index_body()` method
- `inboxly-imap/src/events.rs` — Phase 2 `SyncEvent` variants
- `inboxly-imap/src/error.rs` — new error variants
- `inboxly-imap/src/sync.rs` — Phase 2 spawn, on-demand command, offline replay wiring
- `inboxly-imap/src/lib.rs` — new module declarations
- `inboxly-imap/Cargo.toml` — `mail-parser`, `tokio-util` dependencies

### Dependencies added:
- `mail-parser = "0.9"` (RFC822/MIME parsing for body text extraction)
- `tokio-util = "0.7"` (CancellationToken for graceful shutdown)

## Key Design Decisions

1. **Newest-first download order** — Users are most likely to open recent emails. Downloading newest UIDs first means the most relevant emails get their bodies earliest.

2. **Non-fatal per-email errors** — A single email failing to download (malformed MIME, transient IMAP error) does not abort the entire Phase 2. The error is logged, an event is emitted, and the next email proceeds. The failed email can be retried on next Phase 2 run (it remains `body_downloaded = false`).

3. **Shared IMAP session via `Arc<Mutex<Session>>`** — Phase 2 and on-demand fetch compete for the IMAP connection. The mutex serializes access. On-demand fetches take priority by design: when the user opens an email, the on-demand handler acquires the lock, fetches the single body, then releases. Phase 2 batches wait.

4. **Tantivy document update strategy** — When Phase 1 indexes headers only, and Phase 2 adds the body, the tantivy document is deleted and re-added with all fields (headers + body). This avoids partial-update complexity. The commit-per-batch (not per-email) amortizes tantivy writer overhead — commit after each batch of 500.

5. **Offline queue uses JSON serialization** — `serde_json` for the `OfflineAction` payload makes the queue inspectable and debuggable. The `action` column stores a human-readable variant name for quick filtering (`SELECT * FROM offline_queue WHERE action = 'mark_done'`).

6. **Resume is automatic** — No explicit checkpoint or cursor needed. The `body_downloaded` column in SQLite IS the resume state. On restart, `get_uids_without_body()` returns exactly the emails that still need fetching.
