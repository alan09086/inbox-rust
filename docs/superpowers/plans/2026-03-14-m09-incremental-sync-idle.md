# M9: Incremental Sync + IDLE

**Crate**: `inboxly-imap`
**Depends on**: M6 (IMAP connection + capability detection), M7 (initial sync phase 1), M8 (initial sync phase 2), M3 (SQLite store API)
**Spec sections**: "IMAP Sync Engine" — Incremental sync, Push sync, Design Decisions
**Branch**: `m09-incremental-sync-idle`

---

## Overview

This milestone adds the two runtime sync modes that keep a mailbox current after initial sync:

1. **Incremental sync** — on app launch (or reconnect), fetches only what changed since last sync using UIDNEXT/CONDSTORE.
2. **Push sync (IDLE)** — holds a persistent IMAP connection for real-time new-mail notification, with timeout handling and catch-up.

Both modes run per-account in independent tokio tasks. A sync lifecycle manager controls start/pause/resume/stop for each account.

## Prerequisites (assumed complete from M6-M8)

From M3 (`inboxly-store`):
- `SyncState { account_id, folder_name, uid_validity, uid_next, highest_modseq, last_sync }` in SQLite
- `Store::get_sync_state(account_id, folder) -> Option<SyncState>`
- `Store::set_sync_state(state: &SyncState) -> Result<()>`
- `Store::get_email_by_uid(account_id, folder, uid) -> Option<EmailMeta>`
- `Store::upsert_email(email: &EmailMeta) -> Result<()>`
- `Store::update_email_flags(account_id, folder, uid, flags) -> Result<()>`
- `Store::mark_email_deleted(account_id, folder, uid) -> Result<()>`
- `Store::get_uids_in_folder(account_id, folder) -> Result<Vec<u32>>`
- `Store::get_uids_since(account_id, folder, since: DateTime<Utc>) -> Result<Vec<u32>>`

From M6 (`inboxly-imap`):
- `ImapConnection` wrapping `async-imap` session with TLS
- `ImapConnection::capabilities() -> &Capabilities` (CONDSTORE, IDLE detection)
- `ImapConnection::select(folder) -> Result<Mailbox>` returning UIDVALIDITY, UIDNEXT, HIGHESTMODSEQ
- Account authentication (password, OAuth2)

From M7-M8 (`inboxly-imap`):
- `fetch_envelopes(session, uid_range, batch_size) -> Result<Vec<EmailMeta>>` — fetches headers/envelope
- `fetch_bodies(session, uids, maildir_root) -> Result<()>` — downloads RFC822 to Maildir
- `process_email(raw, account_id, folder) -> Result<EmailMeta>` — parses raw email into EmailMeta
- `SyncEvent` enum sent via `tokio::sync::mpsc` to UI: `NewEmail`, `EmailFlagsChanged`, `EmailDeleted`, `SyncProgress`, `SyncError`

## File Layout

All new code in `inboxly-imap/src/`:

```
inboxly-imap/src/
├── lib.rs                    ← re-exports
├── connection.rs             ← (M6) ImapConnection
├── auth.rs                   ← (M6) authentication
├── capabilities.rs           ← (M6) capability detection
├── initial_sync.rs           ← (M7-M8) initial sync
├── incremental.rs            ← NEW: incremental sync logic
├── idle.rs                   ← NEW: IDLE command handling
├── sync_loop.rs              ← NEW: per-account sync loop orchestrator
├── sync_manager.rs           ← NEW: multi-account lifecycle manager
└── error.rs                  ← (M6) ImapError variants
```

---

## Tasks

### Task 1: Incremental UID Fetch (UIDNEXT Comparison)

**File**: `inboxly-imap/src/incremental.rs`

Create the core incremental sync function that detects new messages by comparing stored UIDNEXT against the server's current UIDNEXT.

```rust
// inboxly-imap/src/incremental.rs

use crate::connection::ImapConnection;
use crate::error::ImapError;
use inboxly_core::types::{AccountId, SyncState};
use inboxly_store::Store;

/// Result of an incremental sync pass for a single folder.
pub struct IncrementalSyncResult {
    pub new_uids: Vec<u32>,
    pub flag_changes: Vec<(u32, EmailFlags)>,
    pub deleted_uids: Vec<u32>,
    pub new_uid_next: u32,
    pub new_highest_modseq: Option<u64>,
}

/// Check for new messages by comparing stored UIDNEXT with server's current value.
/// Returns UIDs of new messages (stored_uid_next..server_uid_next).
///
/// If UIDVALIDITY has changed, returns Err(ImapError::UidValidityChanged) —
/// the caller must trigger a full re-sync for this folder.
pub async fn check_new_uids(
    conn: &mut ImapConnection,
    folder: &str,
    stored_state: &SyncState,
) -> Result<NewUidCheckResult, ImapError> {
    let mailbox = conn.select(folder).await?;

    // UIDVALIDITY changed — all cached UIDs are invalid
    if mailbox.uid_validity != stored_state.uid_validity {
        return Err(ImapError::UidValidityChanged {
            folder: folder.to_string(),
            old: stored_state.uid_validity,
            new: mailbox.uid_validity,
        });
    }

    let server_uid_next = mailbox.uid_next;
    let stored_uid_next = stored_state.uid_next;

    if server_uid_next <= stored_uid_next {
        // No new messages
        return Ok(NewUidCheckResult {
            new_uid_range: None,
            server_uid_next,
            server_highest_modseq: mailbox.highest_modseq,
        });
    }

    // Fetch UIDs in the range stored_uid_next..* (server may have gaps)
    // Use UID SEARCH to get actual UIDs that exist in the range
    let uid_range = format!("{}:*", stored_uid_next);
    let new_uids = conn.uid_search(&uid_range).await?;

    // Filter out UIDs below stored_uid_next (IMAP * can match the highest existing UID
    // even if it's below our range)
    let new_uids: Vec<u32> = new_uids
        .into_iter()
        .filter(|&uid| uid >= stored_uid_next)
        .collect();

    Ok(NewUidCheckResult {
        new_uid_range: if new_uids.is_empty() { None } else { Some(new_uids) },
        server_uid_next,
        server_highest_modseq: mailbox.highest_modseq,
    })
}

pub struct NewUidCheckResult {
    pub new_uid_range: Option<Vec<u32>>,
    pub server_uid_next: u32,
    pub server_highest_modseq: Option<u64>,
}
```

**Add error variant** in `inboxly-imap/src/error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ImapError {
    // ... existing variants from M6 ...

    #[error("UIDVALIDITY changed for folder '{folder}' (was {old}, now {new}) — full re-sync required")]
    UidValidityChanged { folder: String, old: u32, new: u32 },

    #[error("IDLE interrupted: {0}")]
    IdleInterrupted(String),

    #[error("IDLE not supported by server")]
    IdleNotSupported,

    #[error("Sync task cancelled")]
    SyncCancelled,
}
```

**Tests** (`inboxly-imap/tests/incremental.rs`):
- `test_no_new_uids_when_uid_next_unchanged` — stored_uid_next == server_uid_next returns empty
- `test_new_uids_detected` — server_uid_next > stored returns correct UID range
- `test_uid_validity_changed_returns_error` — mismatched UIDVALIDITY triggers error
- `test_new_uids_filters_below_stored` — UIDs below stored_uid_next are excluded

**Commit**: `feat(imap): add incremental UID fetch with UIDNEXT comparison`

---

### Task 2: New Message Processing (Envelope + Body Download + Index)

**File**: `inboxly-imap/src/incremental.rs`

Add the function that takes newly discovered UIDs, fetches their envelopes and bodies, persists to store, and emits sync events.

```rust
/// Fetch and process newly discovered UIDs.
/// Downloads envelope+flags first (fast), then full body to Maildir.
/// Emits SyncEvent::NewEmail for each processed message.
pub async fn fetch_new_messages(
    conn: &mut ImapConnection,
    store: &Store,
    account_id: AccountId,
    folder: &str,
    new_uids: &[u32],
    maildir_root: &Path,
    event_tx: &mpsc::Sender<SyncEvent>,
) -> Result<u32, ImapError> {
    if new_uids.is_empty() {
        return Ok(0);
    }

    let mut processed = 0u32;

    // Batch fetch in groups of 500 (matching initial sync batch size)
    for chunk in new_uids.chunks(500) {
        let uid_set = format_uid_set(chunk);

        // Phase 1: envelope + flags (lightweight)
        let envelopes = conn
            .uid_fetch(&uid_set, "(ENVELOPE FLAGS RFC822.SIZE)")
            .await?;

        for envelope_data in &envelopes {
            let email_meta = process_envelope(envelope_data, account_id, folder)?;
            store.upsert_email(&email_meta)?;
            event_tx
                .send(SyncEvent::NewEmail(email_meta.clone()))
                .await
                .ok(); // Don't fail sync if UI receiver dropped
        }

        // Phase 2: full body to Maildir
        let body_data = conn.uid_fetch(&uid_set, "(RFC822)").await?;

        for msg in &body_data {
            let uid = msg.uid.ok_or(ImapError::Protocol("missing UID in FETCH response".into()))?;
            write_to_maildir(maildir_root, account_id, folder, uid, &msg.body)?;
            processed += 1;
        }
    }

    // Update tantivy index for new messages (via store)
    store.index_new_emails(account_id, folder, new_uids)?;

    Ok(processed)
}

/// Format a slice of UIDs into an IMAP UID set string: "1,2,5,8:12"
fn format_uid_set(uids: &[u32]) -> String {
    // Build ranges from sorted UIDs for compact representation
    let mut sorted = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let mut parts = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let mut end = start;
        while i + 1 < sorted.len() && sorted[i + 1] == end + 1 {
            end = sorted[i + 1];
            i += 1;
        }
        if start == end {
            parts.push(format!("{}", start));
        } else {
            parts.push(format!("{}:{}", start, end));
        }
        i += 1;
    }
    parts.join(",")
}
```

**Tests**:
- `test_format_uid_set_single` — `[5]` -> `"5"`
- `test_format_uid_set_range` — `[1,2,3,5,6,8]` -> `"1:3,5:6,8"`
- `test_format_uid_set_empty` — `[]` -> `""`
- `test_fetch_new_messages_emits_events` — mock connection, verify SyncEvent::NewEmail count
- `test_fetch_new_messages_batches_large_sets` — 1200 UIDs split into 3 batches of 500/500/200

**Commit**: `feat(imap): add new message fetch + process pipeline for incremental sync`

---

### Task 3: CONDSTORE Flag Change Detection

**File**: `inboxly-imap/src/incremental.rs`

Implement efficient flag sync for servers that advertise the CONDSTORE capability.

```rust
/// Sync flag changes using CONDSTORE extension (RFC 4551).
/// Issues `UID FETCH 1:* (FLAGS) (CHANGEDSINCE <modseq>)` which returns only
/// messages whose flags changed since the stored highest_modseq.
///
/// Returns the new highest_modseq from the server.
pub async fn sync_flags_condstore(
    conn: &mut ImapConnection,
    store: &Store,
    account_id: AccountId,
    folder: &str,
    highest_modseq: u64,
    event_tx: &mpsc::Sender<SyncEvent>,
) -> Result<u64, ImapError> {
    // CHANGEDSINCE returns only changed messages — efficient for 100k+ mailboxes
    let fetch_cmd = format!("1:* (FLAGS) (CHANGEDSINCE {})", highest_modseq);
    let changed = conn.uid_fetch_raw(&fetch_cmd).await?;

    let mut new_highest_modseq = highest_modseq;

    for item in &changed {
        let uid = item.uid.ok_or(ImapError::Protocol("missing UID".into()))?;
        let flags = parse_imap_flags(&item.flags);

        // Update local store
        store.update_email_flags(account_id, folder, uid, &flags)?;

        // Emit event to UI
        event_tx
            .send(SyncEvent::EmailFlagsChanged {
                account_id,
                folder: folder.to_string(),
                uid,
                flags: flags.clone(),
            })
            .await
            .ok();

        // Track highest modseq seen
        if let Some(modseq) = item.modseq {
            new_highest_modseq = new_highest_modseq.max(modseq);
        }
    }

    Ok(new_highest_modseq)
}

/// Parse IMAP flag strings (\Seen, \Flagged, etc.) into our EmailFlags bitmask.
fn parse_imap_flags(flags: &[async_imap::types::Flag<'_>]) -> EmailFlags {
    let mut result = EmailFlags::empty();
    for flag in flags {
        match flag {
            Flag::Seen => result |= EmailFlags::READ,
            Flag::Flagged => result |= EmailFlags::STARRED,
            Flag::Answered => result |= EmailFlags::ANSWERED,
            Flag::Draft => result |= EmailFlags::DRAFT,
            Flag::Deleted => result |= EmailFlags::DELETED,
            _ => {} // Ignore custom flags, \Recent, etc.
        }
    }
    result
}
```

**Tests**:
- `test_parse_imap_flags_seen_flagged` — `\Seen \Flagged` -> `READ | STARRED`
- `test_parse_imap_flags_empty` — no flags -> empty bitmask
- `test_sync_flags_condstore_updates_store` — mock CHANGEDSINCE response, verify store updates
- `test_sync_flags_condstore_returns_new_modseq` — highest modseq from response items propagated
- `test_sync_flags_condstore_emits_events` — verify SyncEvent::EmailFlagsChanged per changed UID

**Commit**: `feat(imap): add CONDSTORE flag sync via CHANGEDSINCE`

---

### Task 4: Non-CONDSTORE Fallback (30-Day UID Window)

**File**: `inboxly-imap/src/incremental.rs`

For servers without CONDSTORE, fetch flags for the last 30 days of UIDs only.

```rust
/// Sync flags without CONDSTORE — fetches flags for UIDs received in the last
/// 30 days only. This is a deliberate trade-off: older flag changes are missed,
/// but we avoid scanning the entire mailbox on every sync.
pub async fn sync_flags_fallback(
    conn: &mut ImapConnection,
    store: &Store,
    account_id: AccountId,
    folder: &str,
    event_tx: &mpsc::Sender<SyncEvent>,
) -> Result<(), ImapError> {
    let thirty_days_ago = Utc::now() - chrono::Duration::days(30);

    // Get locally known UIDs from the last 30 days
    let recent_uids = store.get_uids_since(account_id, folder, thirty_days_ago)?;

    if recent_uids.is_empty() {
        return Ok(());
    }

    let uid_set = format_uid_set(&recent_uids);

    // Fetch current flags for these UIDs
    let fetched = conn
        .uid_fetch(&uid_set, "(FLAGS)")
        .await?;

    for item in &fetched {
        let uid = item.uid.ok_or(ImapError::Protocol("missing UID".into()))?;
        let server_flags = parse_imap_flags(&item.flags);

        // Compare with stored flags
        if let Some(local_email) = store.get_email_by_uid(account_id, folder, uid) {
            if local_email.flags != server_flags {
                store.update_email_flags(account_id, folder, uid, &server_flags)?;
                event_tx
                    .send(SyncEvent::EmailFlagsChanged {
                        account_id,
                        folder: folder.to_string(),
                        uid,
                        flags: server_flags,
                    })
                    .await
                    .ok();
            }
        }
    }

    Ok(())
}

/// Unified flag sync dispatcher — chooses CONDSTORE or fallback based on
/// server capabilities.
pub async fn sync_flags(
    conn: &mut ImapConnection,
    store: &Store,
    account_id: AccountId,
    folder: &str,
    sync_state: &SyncState,
    event_tx: &mpsc::Sender<SyncEvent>,
) -> Result<Option<u64>, ImapError> {
    if conn.capabilities().has_condstore() {
        if let Some(modseq) = sync_state.highest_modseq {
            let new_modseq =
                sync_flags_condstore(conn, store, account_id, folder, modseq, event_tx).await?;
            Ok(Some(new_modseq))
        } else {
            // First time with CONDSTORE — no stored modseq yet, skip flag sync
            // (initial sync already has correct flags). Return server's modseq
            // so next incremental can use CHANGEDSINCE.
            let mailbox = conn.examine(folder).await?;
            Ok(mailbox.highest_modseq)
        }
    } else {
        sync_flags_fallback(conn, store, account_id, folder, event_tx).await?;
        Ok(None)
    }
}
```

**Tests**:
- `test_sync_flags_fallback_only_recent_uids` — verify only UIDs from last 30 days are fetched
- `test_sync_flags_fallback_skips_unchanged` — flags match local -> no event emitted
- `test_sync_flags_fallback_detects_change` — \Seen added remotely -> store + event updated
- `test_sync_flags_dispatch_uses_condstore_when_available` — mock CONDSTORE cap -> CHANGEDSINCE used
- `test_sync_flags_dispatch_uses_fallback_when_no_condstore` — no CONDSTORE cap -> 30-day window

**Commit**: `feat(imap): add non-CONDSTORE flag sync fallback with 30-day UID window`

---

### Task 5: Deleted Message Detection

**File**: `inboxly-imap/src/incremental.rs`

Detect messages that were deleted on the server (UID no longer exists).

```rust
/// Detect messages deleted on the server by comparing locally known UIDs
/// against server's current UID set.
///
/// Strategy: UID SEARCH 1:* on the server returns all existing UIDs.
/// Any locally known UID not in the server set is marked deleted.
///
/// For large mailboxes, this is scoped to the same 30-day window used by
/// the non-CONDSTORE fallback. For CONDSTORE servers, VANISHED responses
/// would be ideal but async-imap doesn't support QRESYNC — so we use
/// the search approach.
pub async fn detect_deleted_messages(
    conn: &mut ImapConnection,
    store: &Store,
    account_id: AccountId,
    folder: &str,
    event_tx: &mpsc::Sender<SyncEvent>,
) -> Result<Vec<u32>, ImapError> {
    let thirty_days_ago = Utc::now() - chrono::Duration::days(30);

    // Get locally known UIDs from recent window
    let local_uids = store.get_uids_since(account_id, folder, thirty_days_ago)?;

    if local_uids.is_empty() {
        return Ok(Vec::new());
    }

    // Ask server which of these UIDs still exist
    let uid_set = format_uid_set(&local_uids);
    let server_uids: HashSet<u32> = conn
        .uid_search(&uid_set)
        .await?
        .into_iter()
        .collect();

    let mut deleted = Vec::new();

    for &uid in &local_uids {
        if !server_uids.contains(&uid) {
            store.mark_email_deleted(account_id, folder, uid)?;
            event_tx
                .send(SyncEvent::EmailDeleted {
                    account_id,
                    folder: folder.to_string(),
                    uid,
                })
                .await
                .ok();
            deleted.push(uid);
        }
    }

    if !deleted.is_empty() {
        tracing::info!(
            account_id = %account_id,
            folder = folder,
            count = deleted.len(),
            "detected deleted messages"
        );
    }

    Ok(deleted)
}
```

**Tests**:
- `test_detect_deleted_all_exist` — server has all local UIDs -> empty result
- `test_detect_deleted_some_missing` — server missing UIDs 5,8 -> marks both deleted, emits events
- `test_detect_deleted_empty_folder` — no local UIDs -> returns immediately
- `test_detect_deleted_marks_store` — verify `store.mark_email_deleted` called for each missing UID

**Commit**: `feat(imap): add deleted message detection via UID comparison`

---

### Task 6: IDLE Command and Response Handling

**File**: `inboxly-imap/src/idle.rs`

Implement the IMAP IDLE command wrapper that listens for server push notifications.

```rust
// inboxly-imap/src/idle.rs

use crate::connection::ImapConnection;
use crate::error::ImapError;
use std::time::Duration;
use tokio::time::timeout;

/// Maximum IDLE duration before we proactively reconnect.
/// RFC 2177 recommends clients restart IDLE every 29 minutes.
/// Servers commonly drop connections at 30 min.
const IDLE_TIMEOUT: Duration = Duration::from_secs(29 * 60);

/// Outcome of an IDLE session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdleEvent {
    /// Server reported new messages exist (EXISTS count increased).
    NewMessages { exists: u32 },

    /// Server reported messages were expunged.
    Expunge { seq: u32 },

    /// Server sent a flags update.
    FlagsChanged,

    /// Our 29-minute timeout fired — need to restart IDLE.
    Timeout,

    /// IDLE was cancelled by the sync manager (e.g., pause or stop).
    Cancelled,
}

/// Enter IDLE mode on the currently selected folder.
///
/// Blocks until one of:
/// - Server sends an untagged response (EXISTS, EXPUNGE, FETCH)
/// - 29-minute timeout expires
/// - Cancellation token is triggered
///
/// The caller is responsible for calling `done()` on the idle handle
/// to exit IDLE mode before issuing further commands.
pub async fn idle_wait(
    conn: &mut ImapConnection,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<IdleEvent, ImapError> {
    if !conn.capabilities().has_idle() {
        return Err(ImapError::IdleNotSupported);
    }

    let mut idle_handle = conn.session_mut().idle().await?;

    // async-imap's idle handle gives us a future that resolves on server data
    let idle_future = idle_handle.wait_with_timeout(IDLE_TIMEOUT);

    tokio::select! {
        result = idle_future => {
            match result {
                Ok(idle_result) => {
                    // idle_result is the reason IDLE broke
                    let event = parse_idle_response(idle_result);

                    // Send DONE to exit IDLE
                    idle_handle.done().await?;

                    Ok(event)
                }
                Err(e) => {
                    // Try to send DONE even on error
                    let _ = idle_handle.done().await;
                    Err(ImapError::Idle(e.to_string()))
                }
            }
        }
        _ = cancel.cancelled() => {
            // Cancellation requested — send DONE and return
            let _ = idle_handle.done().await;
            Ok(IdleEvent::Cancelled)
        }
    }
}

/// Parse the IDLE response into our IdleEvent enum.
fn parse_idle_response(response: async_imap::types::IdleResponse) -> IdleEvent {
    match response {
        async_imap::types::IdleResponse::NewData(data) => {
            let response_str = String::from_utf8_lossy(&data);
            if response_str.contains("EXISTS") {
                // Parse "* <n> EXISTS"
                let exists = parse_exists_count(&response_str).unwrap_or(0);
                IdleEvent::NewMessages { exists }
            } else if response_str.contains("EXPUNGE") {
                let seq = parse_expunge_seq(&response_str).unwrap_or(0);
                IdleEvent::Expunge { seq }
            } else if response_str.contains("FETCH") {
                IdleEvent::FlagsChanged
            } else {
                // Unknown untagged response — treat as new messages to be safe
                IdleEvent::NewMessages { exists: 0 }
            }
        }
        async_imap::types::IdleResponse::Timeout => IdleEvent::Timeout,
        async_imap::types::IdleResponse::ManualInterrupt => IdleEvent::Cancelled,
    }
}

/// Parse "* 42 EXISTS" -> 42
fn parse_exists_count(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("* ") {
        if let Some(num_str) = rest.split_whitespace().next() {
            return num_str.parse().ok();
        }
    }
    None
}

/// Parse "* 7 EXPUNGE" -> 7
fn parse_expunge_seq(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("* ") {
        if let Some(num_str) = rest.split_whitespace().next() {
            return num_str.parse().ok();
        }
    }
    None
}
```

**Tests**:
- `test_parse_exists_count` — `"* 42 EXISTS"` -> `Some(42)`
- `test_parse_exists_count_malformed` — `"* EXISTS"` -> `None`
- `test_parse_expunge_seq` — `"* 7 EXPUNGE"` -> `Some(7)`
- `test_idle_event_timeout` — verify timeout variant after IDLE_TIMEOUT
- `test_idle_event_cancelled` — verify cancelled variant when token is triggered
- `test_idle_not_supported_error` — no IDLE capability -> `IdleNotSupported`

**Commit**: `feat(imap): add IDLE command handler with timeout and cancellation`

---

### Task 7: IDLE Timeout and Reconnect Loop

**File**: `inboxly-imap/src/idle.rs`

Build the persistent IDLE loop that restarts after timeout or server disconnects, with exponential backoff on errors.

```rust
/// Configuration for the IDLE reconnect loop.
pub struct IdleLoopConfig {
    /// Initial backoff delay on connection failure.
    pub initial_backoff: Duration,
    /// Maximum backoff delay.
    pub max_backoff: Duration,
    /// Backoff multiplier (2.0 = double each failure).
    pub backoff_multiplier: f64,
    /// Maximum consecutive failures before giving up and falling back to polling.
    pub max_consecutive_failures: u32,
}

impl Default for IdleLoopConfig {
    fn default() -> Self {
        Self {
            initial_backoff: Duration::from_secs(5),
            max_backoff: Duration::from_secs(300), // 5 minutes
            backoff_multiplier: 2.0,
            max_consecutive_failures: 10,
        }
    }
}

/// Run a persistent IDLE loop for a single folder.
///
/// This function:
/// 1. Enters IDLE mode
/// 2. On server notification (EXISTS/EXPUNGE/FETCH) -> signals the sync loop for incremental catch-up
/// 3. On 29-min timeout -> re-enters IDLE (server keeps connection alive)
/// 4. On connection loss -> reconnects with exponential backoff, does catch-up, re-enters IDLE
/// 5. On cancellation -> exits cleanly
///
/// Returns only when cancelled or max_consecutive_failures exceeded.
pub async fn idle_loop(
    conn_factory: &dyn ConnectionFactory,
    account_id: AccountId,
    folder: &str,
    wakeup_tx: mpsc::Sender<IdleWakeup>,
    cancel: CancellationToken,
    config: IdleLoopConfig,
) -> Result<(), ImapError> {
    let mut consecutive_failures = 0u32;
    let mut backoff = config.initial_backoff;
    let mut conn = conn_factory.connect(account_id).await?;

    conn.select(folder).await?;

    loop {
        if cancel.is_cancelled() {
            return Ok(());
        }

        match idle_wait(&mut conn, cancel.clone()).await {
            Ok(IdleEvent::NewMessages { exists }) => {
                consecutive_failures = 0;
                backoff = config.initial_backoff;
                tracing::debug!(account_id = %account_id, folder, exists, "IDLE: new messages");
                wakeup_tx
                    .send(IdleWakeup::NewMail { account_id, folder: folder.to_string() })
                    .await
                    .ok();
            }

            Ok(IdleEvent::Expunge { seq }) => {
                consecutive_failures = 0;
                backoff = config.initial_backoff;
                tracing::debug!(account_id = %account_id, folder, seq, "IDLE: message expunged");
                wakeup_tx
                    .send(IdleWakeup::Expunge { account_id, folder: folder.to_string() })
                    .await
                    .ok();
            }

            Ok(IdleEvent::FlagsChanged) => {
                consecutive_failures = 0;
                backoff = config.initial_backoff;
                tracing::debug!(account_id = %account_id, folder, "IDLE: flags changed");
                wakeup_tx
                    .send(IdleWakeup::FlagsChanged { account_id, folder: folder.to_string() })
                    .await
                    .ok();
            }

            Ok(IdleEvent::Timeout) => {
                // 29-min timeout — normal, just re-enter IDLE
                consecutive_failures = 0;
                tracing::trace!(account_id = %account_id, folder, "IDLE: 29-min timeout, re-entering");
                // Do a quick incremental check before re-entering IDLE
                wakeup_tx
                    .send(IdleWakeup::TimeoutCatchup { account_id, folder: folder.to_string() })
                    .await
                    .ok();
                // Re-select folder to refresh state
                conn.select(folder).await?;
                continue;
            }

            Ok(IdleEvent::Cancelled) => {
                tracing::info!(account_id = %account_id, folder, "IDLE: cancelled");
                return Ok(());
            }

            Err(e) => {
                consecutive_failures += 1;
                tracing::warn!(
                    account_id = %account_id,
                    folder,
                    error = %e,
                    consecutive_failures,
                    "IDLE: error, will reconnect"
                );

                if consecutive_failures >= config.max_consecutive_failures {
                    tracing::error!(
                        account_id = %account_id,
                        folder,
                        "IDLE: max failures reached, giving up"
                    );
                    return Err(ImapError::IdleInterrupted(format!(
                        "exceeded {} consecutive IDLE failures",
                        config.max_consecutive_failures
                    )));
                }

                // Exponential backoff with cancellation check
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = cancel.cancelled() => return Ok(()),
                }
                backoff = Duration::from_secs_f64(
                    (backoff.as_secs_f64() * config.backoff_multiplier)
                        .min(config.max_backoff.as_secs_f64()),
                );

                // Reconnect
                match conn_factory.connect(account_id).await {
                    Ok(new_conn) => {
                        conn = new_conn;
                        conn.select(folder).await?;
                    }
                    Err(e) => {
                        tracing::warn!(account_id = %account_id, "IDLE reconnect failed: {}", e);
                        continue; // Will retry with next backoff
                    }
                }
            }
        }
    }
}

/// Signal from the IDLE loop to the sync loop.
#[derive(Debug, Clone)]
pub enum IdleWakeup {
    NewMail { account_id: AccountId, folder: String },
    Expunge { account_id: AccountId, folder: String },
    FlagsChanged { account_id: AccountId, folder: String },
    TimeoutCatchup { account_id: AccountId, folder: String },
}

/// Trait for creating IMAP connections — enables testing with mock connections.
#[async_trait::async_trait]
pub trait ConnectionFactory: Send + Sync {
    async fn connect(&self, account_id: AccountId) -> Result<ImapConnection, ImapError>;
}
```

**Tests**:
- `test_idle_loop_reconnects_on_error` — mock connection fails, verify backoff and reconnect
- `test_idle_loop_exponential_backoff` — verify delays double up to max
- `test_idle_loop_max_failures_exits` — 10 consecutive failures -> returns error
- `test_idle_loop_resets_backoff_on_success` — failure then success resets backoff to initial
- `test_idle_loop_exits_on_cancel` — cancel token triggered -> clean exit
- `test_idle_loop_timeout_triggers_catchup` — 29-min timeout sends TimeoutCatchup wakeup

**Commit**: `feat(imap): add IDLE reconnect loop with exponential backoff`

---

### Task 8: Post-IDLE Incremental Catch-Up

**File**: `inboxly-imap/src/incremental.rs`

Combine all incremental operations into a single catch-up pass, used after IDLE breaks or on app launch.

```rust
/// Perform a full incremental catch-up for a single folder.
/// Called on app launch, after IDLE wakeup, and after IDLE timeout.
///
/// Steps:
/// 1. SELECT folder, check UIDVALIDITY
/// 2. Fetch new UIDs (UIDNEXT comparison)
/// 3. Download and process new messages
/// 4. Sync flag changes (CONDSTORE or fallback)
/// 5. Detect deleted messages
/// 6. Update sync_state in store
pub async fn incremental_sync_folder(
    conn: &mut ImapConnection,
    store: &Store,
    account_id: AccountId,
    folder: &str,
    maildir_root: &Path,
    event_tx: &mpsc::Sender<SyncEvent>,
) -> Result<IncrementalSyncResult, ImapError> {
    let sync_state = store
        .get_sync_state(account_id, folder)
        .ok_or_else(|| ImapError::NoSyncState {
            account_id,
            folder: folder.to_string(),
        })?;

    // Step 1-2: Check for new UIDs
    let uid_check = check_new_uids(conn, folder, &sync_state).await?;

    let mut result = IncrementalSyncResult {
        new_uids: Vec::new(),
        flag_changes: Vec::new(),
        deleted_uids: Vec::new(),
        new_uid_next: uid_check.server_uid_next,
        new_highest_modseq: uid_check.server_highest_modseq,
    };

    // Step 3: Fetch and process new messages
    if let Some(new_uids) = uid_check.new_uid_range {
        let count = fetch_new_messages(
            conn,
            store,
            account_id,
            folder,
            &new_uids,
            maildir_root,
            event_tx,
        )
        .await?;
        tracing::info!(
            account_id = %account_id,
            folder,
            count,
            "fetched new messages"
        );
        result.new_uids = new_uids;
    }

    // Step 4: Sync flag changes
    let new_modseq = sync_flags(
        conn,
        store,
        account_id,
        folder,
        &sync_state,
        event_tx,
    )
    .await?;
    if let Some(modseq) = new_modseq {
        result.new_highest_modseq = Some(modseq);
    }

    // Step 5: Detect deleted messages
    result.deleted_uids = detect_deleted_messages(
        conn,
        store,
        account_id,
        folder,
        event_tx,
    )
    .await?;

    // Step 6: Update sync state
    let updated_state = SyncState {
        account_id,
        folder_name: folder.to_string(),
        uid_validity: sync_state.uid_validity,
        uid_next: result.new_uid_next,
        highest_modseq: result.new_highest_modseq,
        last_sync: Utc::now(),
    };
    store.set_sync_state(&updated_state)?;

    event_tx
        .send(SyncEvent::SyncProgress {
            account_id,
            folder: folder.to_string(),
            status: SyncStatus::UpToDate,
        })
        .await
        .ok();

    Ok(result)
}
```

**Tests**:
- `test_incremental_sync_folder_full_pass` — mock all stages, verify correct ordering and state update
- `test_incremental_sync_folder_no_changes` — nothing new on server -> sync_state updated with same values
- `test_incremental_sync_folder_uid_validity_change` — triggers UidValidityChanged error
- `test_incremental_sync_folder_updates_last_sync` — verify last_sync timestamp updated

**Commit**: `feat(imap): add unified incremental catch-up for post-IDLE and launch sync`

---

### Task 9: Per-Account Sync Task Spawning

**File**: `inboxly-imap/src/sync_loop.rs`

Build the per-account sync loop that orchestrates incremental sync and IDLE for all synced folders.

```rust
// inboxly-imap/src/sync_loop.rs

use crate::idle::{idle_loop, IdleLoopConfig, IdleWakeup, ConnectionFactory};
use crate::incremental::incremental_sync_folder;
use crate::connection::ImapConnection;
use crate::error::ImapError;
use inboxly_core::types::AccountId;
use inboxly_store::Store;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// The well-known folders we sync (from spec).
const SYNCED_FOLDERS: &[&str] = &["INBOX", "Sent", "Drafts", "Trash", "Spam"];

/// Per-account sync loop.
///
/// Lifecycle:
/// 1. On start: incremental catch-up for all synced folders
/// 2. Enter IDLE on INBOX (primary notification target)
/// 3. On IDLE wakeup: incremental catch-up, re-enter IDLE
/// 4. Periodic full catch-up for non-INBOX folders (every 5 min)
/// 5. On pause: exit IDLE, stop syncing, retain state
/// 6. On resume: incremental catch-up, re-enter IDLE
/// 7. On stop: exit IDLE, clean up, task exits
pub async fn account_sync_loop(
    conn_factory: Arc<dyn ConnectionFactory>,
    store: Arc<Store>,
    account_id: AccountId,
    maildir_root: PathBuf,
    event_tx: mpsc::Sender<SyncEvent>,
    cancel: CancellationToken,
    pause: Arc<tokio::sync::Notify>,
    paused: Arc<AtomicBool>,
) -> Result<(), ImapError> {
    tracing::info!(account_id = %account_id, "sync loop started");

    // Phase 1: Initial incremental catch-up for all folders
    {
        let mut conn = conn_factory.connect(account_id).await?;
        // Resolve folder names via LIST + SPECIAL-USE
        let folder_map = resolve_folder_names(&mut conn).await?;

        for &canonical in SYNCED_FOLDERS {
            let imap_name = folder_map.get(canonical).unwrap_or(&canonical.to_string());

            match incremental_sync_folder(
                &mut conn,
                &store,
                account_id,
                imap_name,
                &maildir_root,
                &event_tx,
            )
            .await
            {
                Ok(result) => {
                    tracing::info!(
                        account_id = %account_id,
                        folder = imap_name,
                        new = result.new_uids.len(),
                        deleted = result.deleted_uids.len(),
                        "incremental sync complete"
                    );
                }
                Err(ImapError::UidValidityChanged { folder, .. }) => {
                    tracing::warn!(
                        account_id = %account_id,
                        folder,
                        "UIDVALIDITY changed — need full re-sync (not implemented in M9, skip folder)"
                    );
                    // TODO: trigger full re-sync for this folder (M7 initial sync)
                    continue;
                }
                Err(e) => {
                    tracing::error!(
                        account_id = %account_id,
                        folder = imap_name,
                        error = %e,
                        "incremental sync failed"
                    );
                    event_tx
                        .send(SyncEvent::SyncError {
                            account_id,
                            error: e.to_string(),
                        })
                        .await
                        .ok();
                }
            }
        }
    } // conn dropped — IDLE needs its own connection

    // Phase 2: IDLE loop on INBOX + periodic catch-up for other folders
    let (wakeup_tx, mut wakeup_rx) = mpsc::channel::<IdleWakeup>(32);

    // Spawn IDLE task for INBOX
    let idle_cancel = cancel.child_token();
    let idle_conn_factory = conn_factory.clone();
    let inbox_folder = "INBOX".to_string(); // TODO: use resolved name
    let idle_handle = tokio::spawn({
        let idle_cancel = idle_cancel.clone();
        async move {
            idle_loop(
                idle_conn_factory.as_ref(),
                account_id,
                &inbox_folder,
                wakeup_tx,
                idle_cancel,
                IdleLoopConfig::default(),
            )
            .await
        }
    });

    // Periodic sync interval for non-INBOX folders
    let mut periodic_interval = tokio::time::interval(Duration::from_secs(300)); // 5 min
    periodic_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            // IDLE wakeup — new mail or changes on INBOX
            Some(wakeup) = wakeup_rx.recv() => {
                tracing::debug!(account_id = %account_id, ?wakeup, "IDLE wakeup received");

                // Quick incremental catch-up on INBOX
                let mut conn = conn_factory.connect(account_id).await?;
                let folder = match &wakeup {
                    IdleWakeup::NewMail { folder, .. }
                    | IdleWakeup::Expunge { folder, .. }
                    | IdleWakeup::FlagsChanged { folder, .. }
                    | IdleWakeup::TimeoutCatchup { folder, .. } => folder.as_str(),
                };
                if let Err(e) = incremental_sync_folder(
                    &mut conn, &store, account_id, folder, &maildir_root, &event_tx,
                ).await {
                    tracing::warn!(account_id = %account_id, folder, error = %e, "post-IDLE catch-up failed");
                }
            }

            // Periodic catch-up for non-INBOX folders
            _ = periodic_interval.tick() => {
                let mut conn = match conn_factory.connect(account_id).await {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(account_id = %account_id, error = %e, "periodic sync connect failed");
                        continue;
                    }
                };
                for &folder in &SYNCED_FOLDERS[1..] { // Skip INBOX (handled by IDLE)
                    if cancel.is_cancelled() { break; }
                    if let Err(e) = incremental_sync_folder(
                        &mut conn, &store, account_id, folder, &maildir_root, &event_tx,
                    ).await {
                        tracing::warn!(account_id = %account_id, folder, error = %e, "periodic sync failed");
                    }
                }
            }

            // Pause signal
            _ = pause.notified(), if !paused.load(Ordering::Relaxed) => {
                tracing::info!(account_id = %account_id, "sync loop paused");
                paused.store(true, Ordering::Relaxed);
                idle_cancel.cancel(); // Stop IDLE while paused

                // Wait for resume
                pause.notified().await;
                paused.store(false, Ordering::Relaxed);
                tracing::info!(account_id = %account_id, "sync loop resumed");

                // Restart IDLE (TODO: respawn idle task)
            }

            // Cancellation
            _ = cancel.cancelled() => {
                tracing::info!(account_id = %account_id, "sync loop cancelled");
                idle_cancel.cancel();
                break;
            }
        }
    }

    // Wait for IDLE task to finish
    let _ = idle_handle.await;
    tracing::info!(account_id = %account_id, "sync loop stopped");
    Ok(())
}

/// Resolve well-known folder names using IMAP LIST with SPECIAL-USE attributes (RFC 6154).
/// Falls back to common name matching if SPECIAL-USE not supported.
async fn resolve_folder_names(
    conn: &mut ImapConnection,
) -> Result<HashMap<String, String>, ImapError> {
    let mut map = HashMap::new();

    let folders = conn.list("", "*").await?;

    for folder in &folders {
        // Check SPECIAL-USE attributes
        for attr in &folder.attributes {
            match attr {
                // Map SPECIAL-USE attributes to our canonical names
                NameAttribute::Custom(ref s) if s == "\\Sent" => {
                    map.insert("Sent".to_string(), folder.name().to_string());
                }
                NameAttribute::Custom(ref s) if s == "\\Drafts" => {
                    map.insert("Drafts".to_string(), folder.name().to_string());
                }
                NameAttribute::Custom(ref s) if s == "\\Trash" => {
                    map.insert("Trash".to_string(), folder.name().to_string());
                }
                NameAttribute::Custom(ref s) if s == "\\Junk" => {
                    map.insert("Spam".to_string(), folder.name().to_string());
                }
                _ => {}
            }
        }

        // Fallback: common name matching (Gmail uses [Gmail]/Sent Mail, etc.)
        let name_lower = folder.name().to_lowercase();
        if !map.contains_key("Sent")
            && (name_lower == "sent" || name_lower.ends_with("/sent mail"))
        {
            map.insert("Sent".to_string(), folder.name().to_string());
        }
        if !map.contains_key("Drafts") && name_lower == "drafts" {
            map.insert("Drafts".to_string(), folder.name().to_string());
        }
        if !map.contains_key("Trash") && (name_lower == "trash" || name_lower == "bin") {
            map.insert("Trash".to_string(), folder.name().to_string());
        }
        if !map.contains_key("Spam") && (name_lower == "spam" || name_lower == "junk") {
            map.insert("Spam".to_string(), folder.name().to_string());
        }
    }

    // INBOX is always INBOX per IMAP spec (case-insensitive)
    map.insert("INBOX".to_string(), "INBOX".to_string());

    Ok(map)
}
```

**Tests**:
- `test_resolve_folder_names_special_use` — SPECIAL-USE attrs map correctly
- `test_resolve_folder_names_gmail_fallback` — `[Gmail]/Sent Mail` resolved as Sent
- `test_resolve_folder_names_generic_fallback` — lowercase matching (spam, junk, trash, bin)
- `test_account_sync_loop_initial_catchup` — verify all 5 folders get incremental sync on start
- `test_account_sync_loop_idle_wakeup_triggers_catchup` — wakeup -> incremental sync runs
- `test_account_sync_loop_cancellation` — cancel token -> loop exits cleanly

**Commit**: `feat(imap): add per-account sync loop with IDLE + periodic catch-up`

---

### Task 10: Sync Lifecycle Management (Start/Pause/Resume/Stop)

**File**: `inboxly-imap/src/sync_manager.rs`

Build the top-level sync manager that owns per-account sync tasks and exposes lifecycle control to the application.

```rust
// inboxly-imap/src/sync_manager.rs

use crate::sync_loop::account_sync_loop;
use crate::idle::ConnectionFactory;
use crate::error::ImapError;
use inboxly_core::types::AccountId;
use inboxly_store::Store;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Handle for a running account sync task.
struct AccountSyncHandle {
    cancel: CancellationToken,
    pause_notify: Arc<tokio::sync::Notify>,
    paused: Arc<AtomicBool>,
    join_handle: JoinHandle<Result<(), ImapError>>,
}

/// Manages sync lifecycle for all accounts.
///
/// Thread-safe: all methods take `&self` and use interior mutability.
pub struct SyncManager {
    accounts: tokio::sync::Mutex<HashMap<AccountId, AccountSyncHandle>>,
    store: Arc<Store>,
    conn_factory: Arc<dyn ConnectionFactory>,
    event_tx: mpsc::Sender<SyncEvent>,
    /// Master cancellation token — cancelling this stops ALL accounts.
    master_cancel: CancellationToken,
}

impl SyncManager {
    pub fn new(
        store: Arc<Store>,
        conn_factory: Arc<dyn ConnectionFactory>,
        event_tx: mpsc::Sender<SyncEvent>,
    ) -> Self {
        Self {
            accounts: tokio::sync::Mutex::new(HashMap::new()),
            store,
            conn_factory,
            event_tx,
            master_cancel: CancellationToken::new(),
        }
    }

    /// Start syncing for an account. If already running, this is a no-op.
    pub async fn start(&self, account_id: AccountId, maildir_root: PathBuf) -> Result<(), ImapError> {
        let mut accounts = self.accounts.lock().await;

        if accounts.contains_key(&account_id) {
            tracing::warn!(account_id = %account_id, "sync already running");
            return Ok(());
        }

        let cancel = self.master_cancel.child_token();
        let pause_notify = Arc::new(tokio::sync::Notify::new());
        let paused = Arc::new(AtomicBool::new(false));

        let conn_factory = self.conn_factory.clone();
        let store = self.store.clone();
        let event_tx = self.event_tx.clone();
        let cancel_clone = cancel.clone();
        let pause_clone = pause_notify.clone();
        let paused_clone = paused.clone();

        let join_handle = tokio::spawn(async move {
            account_sync_loop(
                conn_factory,
                store,
                account_id,
                maildir_root,
                event_tx,
                cancel_clone,
                pause_clone,
                paused_clone,
            )
            .await
        });

        accounts.insert(
            account_id,
            AccountSyncHandle {
                cancel,
                pause_notify,
                paused,
                join_handle,
            },
        );

        tracing::info!(account_id = %account_id, "sync started");
        Ok(())
    }

    /// Pause syncing for an account. IDLE is stopped, periodic sync is suspended.
    /// State is retained — resume picks up where it left off.
    pub async fn pause(&self, account_id: AccountId) -> Result<(), ImapError> {
        let accounts = self.accounts.lock().await;

        if let Some(handle) = accounts.get(&account_id) {
            if handle.paused.load(Ordering::Relaxed) {
                tracing::warn!(account_id = %account_id, "sync already paused");
                return Ok(());
            }
            handle.pause_notify.notify_one();
            tracing::info!(account_id = %account_id, "sync pause requested");
            Ok(())
        } else {
            Err(ImapError::SyncNotRunning(account_id))
        }
    }

    /// Resume a paused account's sync. Triggers incremental catch-up + IDLE restart.
    pub async fn resume(&self, account_id: AccountId) -> Result<(), ImapError> {
        let accounts = self.accounts.lock().await;

        if let Some(handle) = accounts.get(&account_id) {
            if !handle.paused.load(Ordering::Relaxed) {
                tracing::warn!(account_id = %account_id, "sync not paused");
                return Ok(());
            }
            handle.pause_notify.notify_one();
            tracing::info!(account_id = %account_id, "sync resume requested");
            Ok(())
        } else {
            Err(ImapError::SyncNotRunning(account_id))
        }
    }

    /// Stop syncing for an account. Task is cancelled and cleaned up.
    pub async fn stop(&self, account_id: AccountId) -> Result<(), ImapError> {
        let mut accounts = self.accounts.lock().await;

        if let Some(handle) = accounts.remove(&account_id) {
            handle.cancel.cancel();
            match handle.join_handle.await {
                Ok(Ok(())) => tracing::info!(account_id = %account_id, "sync stopped cleanly"),
                Ok(Err(e)) => tracing::warn!(account_id = %account_id, error = %e, "sync stopped with error"),
                Err(e) => tracing::error!(account_id = %account_id, error = %e, "sync task panicked"),
            }
            Ok(())
        } else {
            Err(ImapError::SyncNotRunning(account_id))
        }
    }

    /// Stop all account syncs. Used on application shutdown.
    pub async fn stop_all(&self) {
        self.master_cancel.cancel();

        let mut accounts = self.accounts.lock().await;
        let ids: Vec<AccountId> = accounts.keys().copied().collect();

        for id in ids {
            if let Some(handle) = accounts.remove(&id) {
                let _ = handle.join_handle.await;
            }
        }

        tracing::info!("all sync tasks stopped");
    }

    /// Check if an account's sync is currently running.
    pub async fn is_running(&self, account_id: AccountId) -> bool {
        let accounts = self.accounts.lock().await;
        accounts.contains_key(&account_id)
    }

    /// Check if an account's sync is paused.
    pub async fn is_paused(&self, account_id: AccountId) -> bool {
        let accounts = self.accounts.lock().await;
        accounts
            .get(&account_id)
            .map(|h| h.paused.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// Get IDs of all accounts currently syncing.
    pub async fn running_accounts(&self) -> Vec<AccountId> {
        let accounts = self.accounts.lock().await;
        accounts.keys().copied().collect()
    }
}
```

**Add error variants** in `inboxly-imap/src/error.rs`:

```rust
#[error("sync not running for account {0}")]
SyncNotRunning(AccountId),

#[error("no sync state found for account {0} folder {1}")]
NoSyncState { account_id: AccountId, folder: String },
```

**Tests**:
- `test_sync_manager_start_spawns_task` — start -> is_running returns true
- `test_sync_manager_start_idempotent` — start twice -> no error, single task
- `test_sync_manager_stop_cleans_up` — stop -> is_running returns false, task exits
- `test_sync_manager_stop_nonexistent_errors` — stop unknown account -> SyncNotRunning
- `test_sync_manager_pause_resume` — pause -> is_paused true -> resume -> is_paused false
- `test_sync_manager_stop_all` — multiple accounts -> stop_all -> all stopped
- `test_sync_manager_running_accounts` — start 3 -> running_accounts returns 3 IDs

**Commit**: `feat(imap): add SyncManager for multi-account sync lifecycle`

---

### Task 11: Integration Tests

**File**: `inboxly-imap/tests/integration/incremental_sync.rs`

End-to-end tests that verify the complete incremental sync + IDLE flow using a mock IMAP server or recorded session data.

```rust
// inboxly-imap/tests/integration/incremental_sync.rs

/// Test infrastructure: MockImapServer
///
/// A lightweight in-memory IMAP server that:
/// - Responds to SELECT with configurable UIDVALIDITY, UIDNEXT, HIGHESTMODSEQ
/// - Responds to UID FETCH with preconfigured message envelopes and bodies
/// - Responds to UID SEARCH with preconfigured UID sets
/// - Supports IDLE with configurable notification delay
/// - Listens on localhost with a random port
///
/// Alternative: use recorded IMAP sessions via `wiremock`-style fixtures
/// if building a full mock server is too complex for M9.

mod mock_server;

use inboxly_imap::incremental::incremental_sync_folder;
use inboxly_imap::sync_manager::SyncManager;
use inboxly_store::Store;

/// Full incremental sync flow: new messages + flag changes + deletions.
#[tokio::test]
async fn test_full_incremental_sync_flow() {
    // Setup:
    // - SQLite store with sync_state: uid_next=100, highest_modseq=50
    // - Mock server: uid_next=103 (3 new), modseq=55 (2 flag changes), UID 95 deleted
    //
    // Verify:
    // - 3 new emails persisted to store
    // - 2 flag updates applied
    // - UID 95 marked deleted
    // - sync_state updated: uid_next=103, highest_modseq=55
    // - SyncEvents emitted: 3x NewEmail, 2x FlagsChanged, 1x EmailDeleted
}

/// CONDSTORE flag sync — only changed flags returned.
#[tokio::test]
async fn test_condstore_flag_sync() {
    // Setup:
    // - Server supports CONDSTORE, stored modseq=50
    // - FETCH (FLAGS) (CHANGEDSINCE 50) returns UIDs 42 and 67 with new flags
    //
    // Verify:
    // - Only UIDs 42 and 67 updated in store
    // - New highest_modseq persisted
}

/// Non-CONDSTORE fallback — 30-day window only.
#[tokio::test]
async fn test_fallback_flag_sync_30_day_window() {
    // Setup:
    // - Server does NOT support CONDSTORE
    // - Store has UIDs spanning 60 days
    //
    // Verify:
    // - Only UIDs from last 30 days are fetched
    // - Older UIDs are NOT fetched (verify no FETCH command for old UIDs)
}

/// UIDVALIDITY change detected.
#[tokio::test]
async fn test_uid_validity_change_detected() {
    // Setup:
    // - Stored UIDVALIDITY=1000, server returns UIDVALIDITY=1001
    //
    // Verify:
    // - UidValidityChanged error returned
    // - No messages fetched or modified
}

/// IDLE wakeup triggers incremental catch-up.
#[tokio::test]
async fn test_idle_wakeup_triggers_catchup() {
    // Setup:
    // - Sync loop running with IDLE
    // - Mock server sends "* 15 EXISTS" after 100ms
    //
    // Verify:
    // - Incremental sync runs within 1 second
    // - New messages appear in store
}

/// IDLE timeout (29 min simulated) reconnects cleanly.
#[tokio::test]
async fn test_idle_timeout_reconnects() {
    // Setup:
    // - Override IDLE_TIMEOUT to 1 second for testing
    // - Mock server does not send any IDLE data
    //
    // Verify:
    // - After ~1 second, IDLE restarts
    // - TimeoutCatchup wakeup emitted
}

/// Sync manager lifecycle: start, pause, resume, stop.
#[tokio::test]
async fn test_sync_manager_lifecycle() {
    // Setup:
    // - SyncManager with mock connection factory
    // - Start account sync
    //
    // Verify sequence:
    // 1. start -> is_running=true, is_paused=false
    // 2. pause -> is_paused=true (IDLE stops)
    // 3. resume -> is_paused=false (IDLE restarts, catch-up runs)
    // 4. stop -> is_running=false (task exits)
}

/// Multiple accounts sync independently.
#[tokio::test]
async fn test_multi_account_independent_sync() {
    // Setup:
    // - 3 accounts, each with different mock servers
    // - Account B's server is slow (500ms delay)
    //
    // Verify:
    // - Accounts A and C complete sync before B
    // - All 3 accounts eventually sync
    // - stop_all stops all 3
}

/// Deleted message detection with empty recent window.
#[tokio::test]
async fn test_deleted_detection_no_recent_messages() {
    // Setup:
    // - All local messages are older than 30 days
    //
    // Verify:
    // - detect_deleted_messages returns empty (doesn't scan old messages)
}

/// Folder name resolution with Gmail SPECIAL-USE.
#[tokio::test]
async fn test_folder_resolution_gmail() {
    // Setup:
    // - LIST response includes [Gmail]/Sent Mail with \Sent attribute
    //
    // Verify:
    // - "Sent" resolves to "[Gmail]/Sent Mail"
}
```

**Commit**: `test(imap): add integration tests for incremental sync + IDLE + lifecycle`

---

### Task 12: Module Exports and Documentation

**File**: `inboxly-imap/src/lib.rs`

Wire up the new modules and add public API documentation.

```rust
// Add to inboxly-imap/src/lib.rs

pub mod incremental;
pub mod idle;
pub mod sync_loop;
pub mod sync_manager;

// Re-export main public types
pub use idle::{IdleEvent, IdleWakeup, IdleLoopConfig, ConnectionFactory};
pub use incremental::{
    IncrementalSyncResult, NewUidCheckResult,
    check_new_uids, fetch_new_messages, sync_flags, detect_deleted_messages,
    incremental_sync_folder,
};
pub use sync_loop::account_sync_loop;
pub use sync_manager::SyncManager;
```

Update `inboxly-imap/Cargo.toml` to add new dependencies:

```toml
[dependencies]
# ... existing from M6 ...
tokio-util = { version = "0.7", features = ["rt"] }  # CancellationToken
async-trait = "0.1"
tracing = "0.1"
```

**Commit**: `feat(imap): wire up incremental sync + IDLE modules in lib.rs`

---

## Verification Checklist

Before marking M9 complete:

- [ ] `cargo test --workspace` — all tests pass (including M1-M8 tests)
- [ ] `cargo clippy --workspace -- -D warnings` — no warnings
- [ ] `cargo doc --workspace --no-deps` — docs build without warnings
- [ ] Incremental sync fetches only new UIDs (not full mailbox)
- [ ] CONDSTORE path uses CHANGEDSINCE (not full flag scan)
- [ ] Non-CONDSTORE fallback scopes to 30-day window
- [ ] UIDVALIDITY change returns clear error (no silent data corruption)
- [ ] IDLE timeout at 29 minutes (not 30 — avoids server-side disconnect race)
- [ ] IDLE reconnect uses exponential backoff
- [ ] Cancellation token propagates cleanly (no orphaned tasks)
- [ ] SyncManager start/pause/resume/stop all work
- [ ] Multiple accounts sync independently
- [ ] All SyncEvents emitted correctly for UI consumption
- [ ] No Iced or UI types in any `inboxly-imap` API

## Commit Sequence

1. `feat(imap): add incremental UID fetch with UIDNEXT comparison`
2. `feat(imap): add new message fetch + process pipeline for incremental sync`
3. `feat(imap): add CONDSTORE flag sync via CHANGEDSINCE`
4. `feat(imap): add non-CONDSTORE flag sync fallback with 30-day UID window`
5. `feat(imap): add deleted message detection via UID comparison`
6. `feat(imap): add IDLE command handler with timeout and cancellation`
7. `feat(imap): add IDLE reconnect loop with exponential backoff`
8. `feat(imap): add unified incremental catch-up for post-IDLE and launch sync`
9. `feat(imap): add per-account sync loop with IDLE + periodic catch-up`
10. `feat(imap): add SyncManager for multi-account sync lifecycle`
11. `test(imap): add integration tests for incremental sync + IDLE + lifecycle`
12. `feat(imap): wire up incremental sync + IDLE modules in lib.rs`
