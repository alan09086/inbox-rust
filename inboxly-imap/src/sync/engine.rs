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
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + std::fmt::Debug,
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

    let total_estimate = effective_uid_next.saturating_sub(lowest_uid);

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

        let fetches: Vec<async_imap::types::Fetch> = match fetch_result {
            Ok(stream) => stream
                .try_collect::<Vec<async_imap::types::Fetch>>()
                .await
                .map_err(SyncError::from)?,
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
