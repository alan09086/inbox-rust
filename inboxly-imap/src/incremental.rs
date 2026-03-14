//! Incremental sync logic for keeping a mailbox current after initial sync.
//!
//! Provides:
//! - UID-based new message detection (UIDNEXT comparison)
//! - CONDSTORE flag change detection (CHANGEDSINCE)
//! - Non-CONDSTORE fallback (30-day UID window)
//! - Deleted message detection (UID comparison)
//! - Unified incremental catch-up orchestrator

use std::collections::HashSet;

use chrono::Utc;
use futures::TryStreamExt;
use tokio::sync::mpsc;

use crate::channel::SyncEvent;
use crate::error::ImapError;
use crate::sync::envelope::{flags_to_bitmask, parse_fetch_to_envelope};
use crate::sync::store::batch_insert_envelopes;
use crate::sync::uid_state::{FolderSyncState, load_sync_state, save_sync_state};

/// Result of an incremental sync pass for a single folder.
#[derive(Debug)]
pub struct IncrementalSyncResult {
    /// UIDs of newly fetched messages.
    pub new_uids: Vec<u32>,
    /// Number of flag changes applied.
    pub flag_change_count: u64,
    /// UIDs of messages deleted on the server.
    pub deleted_uids: Vec<u32>,
    /// The server's current UIDNEXT after this sync.
    pub new_uid_next: u32,
    /// The server's current HIGHESTMODSEQ (if CONDSTORE enabled).
    pub new_highest_modseq: Option<u64>,
}

/// Result of checking for new UIDs on the server.
#[derive(Debug)]
pub struct NewUidCheckResult {
    /// UIDs of new messages (empty if none).
    pub new_uids: Vec<u32>,
    /// Server's current UIDNEXT.
    pub server_uid_next: u32,
    /// Server's current HIGHESTMODSEQ (if available).
    pub server_highest_modseq: Option<u64>,
}

/// Check for new messages by comparing stored UIDNEXT with the server's current value.
///
/// If UIDVALIDITY has changed, returns `Err(ImapError::UidValidityChanged)` —
/// the caller must trigger a full re-sync for this folder.
pub async fn check_new_uids<S>(
    session: &mut async_imap::Session<S>,
    folder: &str,
    stored_uid_validity: u32,
    stored_uid_next: u32,
) -> Result<NewUidCheckResult, ImapError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    let mailbox = session.select(folder).await.map_err(ImapError::from)?;

    let server_uid_validity = mailbox.uid_validity.unwrap_or(0);
    let server_uid_next = mailbox.uid_next.unwrap_or(0);
    let server_highest_modseq = mailbox.highest_modseq;

    // UIDVALIDITY changed — all cached UIDs are invalid
    if server_uid_validity != stored_uid_validity {
        return Err(ImapError::UidValidityChanged {
            folder: folder.to_string(),
            old: stored_uid_validity,
            new: server_uid_validity,
        });
    }

    if server_uid_next <= stored_uid_next {
        // No new messages
        return Ok(NewUidCheckResult {
            new_uids: Vec::new(),
            server_uid_next,
            server_highest_modseq,
        });
    }

    // Fetch UIDs in the range stored_uid_next..* (server may have gaps)
    let uid_range = format!("{stored_uid_next}:*");
    let search_result = session
        .uid_search(&uid_range)
        .await
        .map_err(ImapError::from)?;

    // Filter out UIDs below stored_uid_next (IMAP * can match the highest
    // existing UID even if it's below our range)
    let new_uids: Vec<u32> = search_result
        .into_iter()
        .filter(|&uid| uid >= stored_uid_next)
        .collect();

    Ok(NewUidCheckResult {
        new_uids,
        server_uid_next,
        server_highest_modseq,
    })
}

/// Fetch and process newly discovered UIDs.
///
/// Downloads envelope+flags (Phase 1 style), inserts into SQLite, and
/// emits `SyncEvent::NewEmails` for the UI.
///
/// Returns the number of messages successfully processed.
pub async fn fetch_new_messages<S>(
    session: &mut async_imap::Session<S>,
    db: &rusqlite::Connection,
    account_id: &str,
    folder: &str,
    new_uids: &[u32],
    event_tx: &mpsc::Sender<SyncEvent>,
) -> Result<u32, ImapError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    if new_uids.is_empty() {
        return Ok(0);
    }

    let mut total_processed = 0u32;

    // Batch fetch in groups of 500 (matching initial sync batch size)
    for chunk in new_uids.chunks(500) {
        let uid_set = format_uid_set(chunk);
        if uid_set.is_empty() {
            continue;
        }

        let fetches: Vec<async_imap::types::Fetch> = session
            .uid_fetch(&uid_set, "(ENVELOPE FLAGS RFC822.SIZE)")
            .await
            .map_err(ImapError::from)?
            .try_collect()
            .await
            .map_err(ImapError::from)?;

        let mut envelopes = Vec::with_capacity(fetches.len());
        for fetch in &fetches {
            match parse_fetch_to_envelope(fetch, account_id, folder) {
                Ok(env) => envelopes.push(env),
                Err(e) => {
                    tracing::warn!(
                        account_id,
                        folder,
                        error = %e,
                        "skipped malformed envelope during incremental sync"
                    );
                }
            }
        }

        let inserted = batch_insert_envelopes(db, &envelopes)
            .map_err(|e| ImapError::Store(e.to_string()))? as u32;

        total_processed = total_processed.saturating_add(inserted);
    }

    if total_processed > 0 {
        let _ = event_tx
            .send(SyncEvent::NewEmails {
                account_id: account_id.to_string(),
                folder: folder.to_string(),
                count: u64::from(total_processed),
            })
            .await;
    }

    Ok(total_processed)
}

/// Sync flag changes using CONDSTORE extension (RFC 4551).
///
/// Issues `UID FETCH 1:* (FLAGS) (CHANGEDSINCE <modseq>)` which returns only
/// messages whose flags changed since the stored highest_modseq.
///
/// Returns the new highest_modseq observed.
pub async fn sync_flags_condstore<S>(
    session: &mut async_imap::Session<S>,
    db: &rusqlite::Connection,
    account_id: &str,
    folder: &str,
    highest_modseq: u64,
    event_tx: &mpsc::Sender<SyncEvent>,
) -> Result<(u64, u64), ImapError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    // CHANGEDSINCE returns only changed messages — efficient for 100k+ mailboxes
    let fetch_cmd = format!("1:* (FLAGS) (CHANGEDSINCE {highest_modseq})");
    let fetches: Vec<async_imap::types::Fetch> = session
        .uid_fetch(&fetch_cmd, "(FLAGS)")
        .await
        .map_err(ImapError::from)?
        .try_collect()
        .await
        .map_err(ImapError::from)?;

    let mut new_highest_modseq = highest_modseq;
    let mut change_count = 0u64;

    for item in &fetches {
        let uid = item
            .uid
            .ok_or_else(|| ImapError::Protocol("missing UID in FETCH response".into()))?;
        let flags = flags_to_bitmask(&item.flags().collect::<Vec<_>>());

        // Update local store
        // Ignore missing emails (may not be locally cached yet)
        match update_flags_by_uid_raw(db, account_id, folder, i64::from(uid), i64::from(flags)) {
            Ok(true) => {
                change_count = change_count.saturating_add(1);
            }
            Ok(false) => {
                // Email not locally cached — skip
            }
            Err(e) => {
                tracing::warn!(
                    account_id,
                    folder,
                    uid,
                    error = %e,
                    "failed to update flags for UID"
                );
            }
        }

        // Track highest modseq seen
        if let Some(modseq) = item.modseq {
            new_highest_modseq = new_highest_modseq.max(modseq);
        }
    }

    if change_count > 0 {
        let _ = event_tx
            .send(SyncEvent::FlagsChanged {
                account_id: account_id.to_string(),
                folder: folder.to_string(),
                count: change_count,
            })
            .await;
    }

    Ok((new_highest_modseq, change_count))
}

/// Sync flags without CONDSTORE — fetches flags for UIDs received in the last
/// 30 days only.
///
/// This is a deliberate trade-off: older flag changes are missed, but we avoid
/// scanning the entire mailbox on every sync.
pub async fn sync_flags_fallback<S>(
    session: &mut async_imap::Session<S>,
    db: &rusqlite::Connection,
    account_id: &str,
    folder: &str,
    event_tx: &mpsc::Sender<SyncEvent>,
) -> Result<u64, ImapError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    let thirty_days_ago = Utc::now().timestamp() - (30 * 24 * 60 * 60);

    // Get locally known UIDs from the last 30 days
    let recent_uids = get_uids_since_raw(db, account_id, folder, thirty_days_ago)
        .map_err(|e| ImapError::Store(e.to_string()))?;

    if recent_uids.is_empty() {
        return Ok(0);
    }

    let uid_set = format_uid_set_i64(&recent_uids);
    if uid_set.is_empty() {
        return Ok(0);
    }

    let fetches: Vec<async_imap::types::Fetch> = session
        .uid_fetch(&uid_set, "(FLAGS)")
        .await
        .map_err(ImapError::from)?
        .try_collect()
        .await
        .map_err(ImapError::from)?;

    let mut change_count = 0u64;

    for item in &fetches {
        let uid = item
            .uid
            .ok_or_else(|| ImapError::Protocol("missing UID in FETCH response".into()))?;
        let server_flags = i64::from(flags_to_bitmask(&item.flags().collect::<Vec<_>>()));

        // Compare with stored flags — only update if different
        if let Ok(Some(local_email)) = get_email_by_uid_raw(db, account_id, folder, i64::from(uid))
            && local_email.flags != server_flags
            && let Ok(true) =
                update_flags_by_uid_raw(db, account_id, folder, i64::from(uid), server_flags)
        {
            change_count = change_count.saturating_add(1);
        }
    }

    if change_count > 0 {
        let _ = event_tx
            .send(SyncEvent::FlagsChanged {
                account_id: account_id.to_string(),
                folder: folder.to_string(),
                count: change_count,
            })
            .await;
    }

    Ok(change_count)
}

/// Unified flag sync dispatcher — chooses CONDSTORE or fallback based on
/// whether a highest_modseq is available (indicating CONDSTORE support).
pub async fn sync_flags<S>(
    session: &mut async_imap::Session<S>,
    db: &rusqlite::Connection,
    account_id: &str,
    folder: &str,
    has_condstore: bool,
    highest_modseq: Option<u64>,
    event_tx: &mpsc::Sender<SyncEvent>,
) -> Result<(Option<u64>, u64), ImapError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    if has_condstore {
        if let Some(modseq) = highest_modseq {
            let (new_modseq, count) =
                sync_flags_condstore(session, db, account_id, folder, modseq, event_tx).await?;
            Ok((Some(new_modseq), count))
        } else {
            // First time with CONDSTORE — no stored modseq yet, skip flag sync.
            // Initial sync already has correct flags. Next incremental can use CHANGEDSINCE.
            Ok((None, 0))
        }
    } else {
        let count = sync_flags_fallback(session, db, account_id, folder, event_tx).await?;
        Ok((None, count))
    }
}

/// Detect messages deleted on the server by comparing locally known UIDs
/// against the server's current UID set.
///
/// Scoped to the last 30 days to avoid scanning the entire mailbox.
pub async fn detect_deleted_messages<S>(
    session: &mut async_imap::Session<S>,
    db: &rusqlite::Connection,
    account_id: &str,
    folder: &str,
    event_tx: &mpsc::Sender<SyncEvent>,
) -> Result<Vec<u32>, ImapError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    let thirty_days_ago = Utc::now().timestamp() - (30 * 24 * 60 * 60);

    let local_uids = get_uids_since_raw(db, account_id, folder, thirty_days_ago)
        .map_err(|e| ImapError::Store(e.to_string()))?;

    if local_uids.is_empty() {
        return Ok(Vec::new());
    }

    // Ask server which of these UIDs still exist
    let uid_set = format_uid_set_i64(&local_uids);
    if uid_set.is_empty() {
        return Ok(Vec::new());
    }

    let server_uids: HashSet<u32> = session
        .uid_search(&uid_set)
        .await
        .map_err(ImapError::from)?
        .into_iter()
        .collect();

    let mut deleted = Vec::new();

    for &local_uid in &local_uids {
        let uid_u32 = u32::try_from(local_uid).unwrap_or(0);
        if uid_u32 > 0 && !server_uids.contains(&uid_u32) {
            // Mark deleted in store — ignore errors for missing emails
            let _ = mark_email_deleted_raw(db, account_id, folder, local_uid);
            deleted.push(uid_u32);
        }
    }

    if !deleted.is_empty() {
        tracing::info!(
            account_id,
            folder,
            count = deleted.len(),
            "detected deleted messages"
        );
        let _ = event_tx
            .send(SyncEvent::EmailsDeleted {
                account_id: account_id.to_string(),
                folder: folder.to_string(),
                count: deleted.len() as u64,
            })
            .await;
    }

    Ok(deleted)
}

/// Perform a full incremental catch-up for a single folder.
///
/// Called on app launch, after IDLE wakeup, and after IDLE timeout.
///
/// Steps:
/// 1. SELECT folder, check UIDVALIDITY
/// 2. Fetch new UIDs (UIDNEXT comparison)
/// 3. Download and process new messages
/// 4. Sync flag changes (CONDSTORE or fallback)
/// 5. Detect deleted messages
/// 6. Update sync_state in store
pub async fn incremental_sync_folder<S>(
    session: &mut async_imap::Session<S>,
    db: &rusqlite::Connection,
    account_id: &str,
    folder: &str,
    has_condstore: bool,
    event_tx: &mpsc::Sender<SyncEvent>,
) -> Result<IncrementalSyncResult, ImapError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + std::fmt::Debug,
{
    let sync_state = load_sync_state(db, account_id, folder)
        .map_err(|e| ImapError::Store(e.to_string()))?
        .ok_or_else(|| ImapError::NoSyncState {
            account_id: account_id.to_string(),
            folder: folder.to_string(),
        })?;

    let stored_uid_validity = sync_state.uid_validity;
    let stored_uid_next = sync_state.uid_next;

    // Step 1-2: Check for new UIDs (also SELECT the folder)
    let uid_check = check_new_uids(session, folder, stored_uid_validity, stored_uid_next).await?;

    let mut result = IncrementalSyncResult {
        new_uids: Vec::new(),
        flag_change_count: 0,
        deleted_uids: Vec::new(),
        new_uid_next: uid_check.server_uid_next,
        new_highest_modseq: uid_check.server_highest_modseq,
    };

    // Step 3: Fetch and process new messages
    if !uid_check.new_uids.is_empty() {
        let count = fetch_new_messages(
            session,
            db,
            account_id,
            folder,
            &uid_check.new_uids,
            event_tx,
        )
        .await?;
        tracing::info!(
            account_id,
            folder,
            count,
            "fetched new messages during incremental sync"
        );
        result.new_uids = uid_check.new_uids;
    }

    // Step 4: Sync flag changes
    let (new_modseq, flag_count) = sync_flags(
        session,
        db,
        account_id,
        folder,
        has_condstore,
        sync_state.highest_modseq,
        event_tx,
    )
    .await?;
    result.flag_change_count = flag_count;
    if let Some(modseq) = new_modseq {
        result.new_highest_modseq = Some(modseq);
    }

    // Step 5: Detect deleted messages
    result.deleted_uids =
        detect_deleted_messages(session, db, account_id, folder, event_tx).await?;

    // Step 6: Update sync state
    let updated_state = FolderSyncState {
        account_id: account_id.to_string(),
        folder_name: folder.to_string(),
        uid_validity: stored_uid_validity,
        uid_next: result.new_uid_next,
        highest_modseq: result.new_highest_modseq,
        last_synced_uid: sync_state.last_synced_uid,
    };
    save_sync_state(db, &updated_state).map_err(|e| ImapError::Store(e.to_string()))?;

    // Emit incremental sync complete event
    let _ = event_tx
        .send(SyncEvent::IncrementalSyncComplete {
            account_id: account_id.to_string(),
            folder: folder.to_string(),
            new_emails: result.new_uids.len() as u64,
            flag_changes: result.flag_change_count,
            deleted: result.deleted_uids.len() as u64,
        })
        .await;

    Ok(result)
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Format a slice of u32 UIDs into an IMAP UID set string: "1,2,5,8:12".
///
/// Builds ranges from sorted UIDs for compact representation.
pub fn format_uid_set(uids: &[u32]) -> String {
    if uids.is_empty() {
        return String::new();
    }

    let mut sorted = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let mut parts = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let mut end = start;
        while i + 1 < sorted.len() && sorted[i + 1] == end.saturating_add(1) {
            end = sorted[i + 1];
            i += 1;
        }
        if start == end {
            parts.push(start.to_string());
        } else {
            parts.push(format!("{start}:{end}"));
        }
        i += 1;
    }
    parts.join(",")
}

/// Format a slice of i64 UIDs into an IMAP UID set string.
fn format_uid_set_i64(uids: &[i64]) -> String {
    let u32_uids: Vec<u32> = uids
        .iter()
        .filter_map(|&uid| u32::try_from(uid).ok())
        .collect();
    format_uid_set(&u32_uids)
}

// ---------------------------------------------------------------------------
// Raw SQLite helpers (operating on Connection directly, not Store)
// These allow the incremental sync code to work with the same Arc<Mutex<Connection>>
// pattern used by the Phase 1 engine, without needing a full Store instance.
// ---------------------------------------------------------------------------

/// Get UIDs since a given unix timestamp (raw connection version).
fn get_uids_since_raw(
    conn: &rusqlite::Connection,
    account_id: &str,
    folder: &str,
    since_unix: i64,
) -> std::result::Result<Vec<i64>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT imap_uid FROM emails
         WHERE account_id = ?1 AND imap_folder = ?2 AND date >= ?3
         ORDER BY imap_uid ASC",
    )?;
    let uids = stmt
        .query_map(rusqlite::params![account_id, folder, since_unix], |row| {
            row.get(0)
        })?
        .collect::<std::result::Result<Vec<i64>, _>>()?;
    Ok(uids)
}

/// Get an email by UID (raw connection version).
fn get_email_by_uid_raw(
    conn: &rusqlite::Connection,
    account_id: &str,
    folder: &str,
    uid: i64,
) -> std::result::Result<Option<EmailFlagsRow>, rusqlite::Error> {
    let result = conn.query_row(
        "SELECT flags FROM emails WHERE account_id = ?1 AND imap_folder = ?2 AND imap_uid = ?3",
        rusqlite::params![account_id, folder, uid],
        |row| Ok(EmailFlagsRow { flags: row.get(0)? }),
    );
    match result {
        Ok(row) => Ok(Some(row)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Minimal struct for flag comparison — avoids loading entire EmailRow.
struct EmailFlagsRow {
    flags: i64,
}

/// Update flags for an email by UID (raw connection version).
///
/// Returns `true` if the row was found and updated, `false` if no matching row exists.
fn update_flags_by_uid_raw(
    conn: &rusqlite::Connection,
    account_id: &str,
    folder: &str,
    uid: i64,
    new_flags: i64,
) -> std::result::Result<bool, rusqlite::Error> {
    let changed = conn.execute(
        "UPDATE emails SET flags = ?4
         WHERE account_id = ?1 AND imap_folder = ?2 AND imap_uid = ?3",
        rusqlite::params![account_id, folder, uid, new_flags],
    )?;
    Ok(changed > 0)
}

/// Mark an email as deleted by UID (raw connection version).
fn mark_email_deleted_raw(
    conn: &rusqlite::Connection,
    account_id: &str,
    folder: &str,
    uid: i64,
) -> std::result::Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE emails SET flags = flags | 16
         WHERE account_id = ?1 AND imap_folder = ?2 AND imap_uid = ?3",
        rusqlite::params![account_id, folder, uid],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_uid_set_single() {
        assert_eq!(format_uid_set(&[5]), "5");
    }

    #[test]
    fn test_format_uid_set_range() {
        assert_eq!(format_uid_set(&[1, 2, 3, 5, 6, 8]), "1:3,5:6,8");
    }

    #[test]
    fn test_format_uid_set_empty() {
        assert_eq!(format_uid_set(&[]), "");
    }

    #[test]
    fn test_format_uid_set_unsorted() {
        assert_eq!(format_uid_set(&[8, 1, 3, 2, 5, 6]), "1:3,5:6,8");
    }

    #[test]
    fn test_format_uid_set_duplicates() {
        assert_eq!(format_uid_set(&[1, 1, 2, 2, 3]), "1:3");
    }

    #[test]
    fn test_format_uid_set_single_range() {
        assert_eq!(format_uid_set(&[10, 11, 12, 13]), "10:13");
    }

    #[test]
    fn test_format_uid_set_all_singles() {
        assert_eq!(format_uid_set(&[1, 3, 5, 7]), "1,3,5,7");
    }

    #[test]
    fn test_format_uid_set_i64_conversion() {
        assert_eq!(format_uid_set_i64(&[1, 2, 3, 5i64]), "1:3,5");
    }

    #[test]
    fn test_format_uid_set_i64_filters_negative() {
        // Negative values should be filtered out (can't be valid UIDs)
        assert_eq!(format_uid_set_i64(&[-1, 1, 2, 3]), "1:3");
    }

    #[test]
    fn test_get_uids_since_raw_empty_db() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
        // Create the emails table
        conn.execute_batch(
            "CREATE TABLE emails (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                thread_id TEXT NOT NULL DEFAULT '',
                from_name TEXT,
                from_address TEXT NOT NULL DEFAULT '',
                to_json TEXT NOT NULL DEFAULT '[]',
                cc_json TEXT NOT NULL DEFAULT '[]',
                subject TEXT NOT NULL DEFAULT '',
                snippet TEXT NOT NULL DEFAULT '',
                date INTEGER NOT NULL DEFAULT 0,
                maildir_path TEXT NOT NULL DEFAULT '',
                flags INTEGER NOT NULL DEFAULT 0,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                imap_uid INTEGER NOT NULL DEFAULT 0,
                imap_folder TEXT NOT NULL DEFAULT '',
                has_attachments INTEGER NOT NULL DEFAULT 0,
                body_downloaded INTEGER NOT NULL DEFAULT 0,
                message_id_header TEXT,
                in_reply_to TEXT,
                references_json TEXT
            );",
        )
        .expect("create table");

        let uids = get_uids_since_raw(&conn, "acc1", "INBOX", 0).expect("query");
        assert!(uids.is_empty());
    }

    #[test]
    fn test_get_uids_since_raw_filters_by_date() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(
            "CREATE TABLE emails (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                thread_id TEXT NOT NULL DEFAULT '',
                from_name TEXT,
                from_address TEXT NOT NULL DEFAULT '',
                to_json TEXT NOT NULL DEFAULT '[]',
                cc_json TEXT NOT NULL DEFAULT '[]',
                subject TEXT NOT NULL DEFAULT '',
                snippet TEXT NOT NULL DEFAULT '',
                date INTEGER NOT NULL DEFAULT 0,
                maildir_path TEXT NOT NULL DEFAULT '',
                flags INTEGER NOT NULL DEFAULT 0,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                imap_uid INTEGER NOT NULL DEFAULT 0,
                imap_folder TEXT NOT NULL DEFAULT '',
                has_attachments INTEGER NOT NULL DEFAULT 0,
                body_downloaded INTEGER NOT NULL DEFAULT 0,
                message_id_header TEXT,
                in_reply_to TEXT,
                references_json TEXT
            );",
        )
        .expect("create table");

        // Insert emails: one old (date=100), one recent (date=1000), one very recent (date=2000)
        conn.execute(
            "INSERT INTO emails (id, account_id, imap_uid, imap_folder, date)
             VALUES ('old', 'acc1', 10, 'INBOX', 100)",
            [],
        )
        .expect("insert old");
        conn.execute(
            "INSERT INTO emails (id, account_id, imap_uid, imap_folder, date)
             VALUES ('recent', 'acc1', 20, 'INBOX', 1000)",
            [],
        )
        .expect("insert recent");
        conn.execute(
            "INSERT INTO emails (id, account_id, imap_uid, imap_folder, date)
             VALUES ('very_recent', 'acc1', 30, 'INBOX', 2000)",
            [],
        )
        .expect("insert very_recent");

        // Since 500 — should get UIDs 20 and 30
        let uids = get_uids_since_raw(&conn, "acc1", "INBOX", 500).expect("query");
        assert_eq!(uids, vec![20, 30]);

        // Since 1500 — should get only UID 30
        let uids = get_uids_since_raw(&conn, "acc1", "INBOX", 1500).expect("query");
        assert_eq!(uids, vec![30]);
    }

    #[test]
    fn test_mark_email_deleted_raw() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(
            "CREATE TABLE emails (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                thread_id TEXT NOT NULL DEFAULT '',
                from_name TEXT,
                from_address TEXT NOT NULL DEFAULT '',
                to_json TEXT NOT NULL DEFAULT '[]',
                cc_json TEXT NOT NULL DEFAULT '[]',
                subject TEXT NOT NULL DEFAULT '',
                snippet TEXT NOT NULL DEFAULT '',
                date INTEGER NOT NULL DEFAULT 0,
                maildir_path TEXT NOT NULL DEFAULT '',
                flags INTEGER NOT NULL DEFAULT 0,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                imap_uid INTEGER NOT NULL DEFAULT 0,
                imap_folder TEXT NOT NULL DEFAULT '',
                has_attachments INTEGER NOT NULL DEFAULT 0,
                body_downloaded INTEGER NOT NULL DEFAULT 0,
                message_id_header TEXT,
                in_reply_to TEXT,
                references_json TEXT
            );",
        )
        .expect("create table");

        conn.execute(
            "INSERT INTO emails (id, account_id, imap_uid, imap_folder, flags)
             VALUES ('msg1', 'acc1', 10, 'INBOX', 1)",
            [],
        )
        .expect("insert");

        mark_email_deleted_raw(&conn, "acc1", "INBOX", 10).expect("mark deleted");

        // flags should now have bit 4 (DELETED=16) set: 1 | 16 = 17
        let flags: i64 = conn
            .query_row("SELECT flags FROM emails WHERE id = 'msg1'", [], |row| {
                row.get(0)
            })
            .expect("query flags");
        assert_eq!(flags, 17); // READ(1) | DELETED(16)
    }

    #[test]
    fn test_get_email_by_uid_raw() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(
            "CREATE TABLE emails (
                id TEXT PRIMARY KEY,
                account_id TEXT NOT NULL,
                thread_id TEXT NOT NULL DEFAULT '',
                from_name TEXT,
                from_address TEXT NOT NULL DEFAULT '',
                to_json TEXT NOT NULL DEFAULT '[]',
                cc_json TEXT NOT NULL DEFAULT '[]',
                subject TEXT NOT NULL DEFAULT '',
                snippet TEXT NOT NULL DEFAULT '',
                date INTEGER NOT NULL DEFAULT 0,
                maildir_path TEXT NOT NULL DEFAULT '',
                flags INTEGER NOT NULL DEFAULT 0,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                imap_uid INTEGER NOT NULL DEFAULT 0,
                imap_folder TEXT NOT NULL DEFAULT '',
                has_attachments INTEGER NOT NULL DEFAULT 0,
                body_downloaded INTEGER NOT NULL DEFAULT 0,
                message_id_header TEXT,
                in_reply_to TEXT,
                references_json TEXT
            );",
        )
        .expect("create table");

        conn.execute(
            "INSERT INTO emails (id, account_id, imap_uid, imap_folder, flags)
             VALUES ('msg1', 'acc1', 10, 'INBOX', 3)",
            [],
        )
        .expect("insert");

        let result = get_email_by_uid_raw(&conn, "acc1", "INBOX", 10).expect("query");
        assert!(result.is_some());
        assert_eq!(result.expect("should exist").flags, 3);

        let result = get_email_by_uid_raw(&conn, "acc1", "INBOX", 99).expect("query");
        assert!(result.is_none());
    }
}
