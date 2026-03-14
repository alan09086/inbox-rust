//! Phase 2 background body download orchestrator.
//!
//! Spawned as a tokio task after Phase 1 header sync completes.
//! Iterates through all emails with `body_downloaded = false` in batches,
//! fetching RFC822 bodies, writing to Maildir, indexing in tantivy,
//! and updating SQLite.
//!
//! ## Data flow
//!
//! ```text
//!   SQLite (body_downloaded=0)
//!       │
//!       ▼
//!   get_uids_without_body(account, folder, 500)
//!       │
//!       ▼
//!   fetch_bodies_batch(session, uids)
//!       │
//!       ▼
//!   for each (uid, raw_bytes):
//!       ├──► process_body(email_id, folder, raw, flags, maildir, store)
//!       ├──► extract_body_text(raw) → body_text
//!       ├──► search_index.update_email(meta, body_text, None) + commit()
//!       └──► emit BodyDownloadProgress
//!
//!   emit BodyDownloadComplete
//! ```
//!
//! ## Resume capability
//!
//! No explicit checkpoint needed — `body_downloaded` column IS the resume
//! state. On restart, `get_uids_without_body()` returns exactly the
//! emails that still need fetching.

use std::fmt::Debug;
use std::sync::Arc;

use async_imap::Session;
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use tokio_util::sync::CancellationToken;
use tokio::io::{AsyncRead, AsyncWrite};

use inboxly_core::EmailFlags;
use inboxly_store::Store;
use inboxly_store::maildir_store::MaildirStore;
use inboxly_store::search::SearchIndex;

use crate::body_fetch::{fetch_bodies_batch, BODY_FETCH_BATCH_SIZE};
use crate::body_processor::{extract_body_text, process_body};
use crate::channel::SyncEvent;
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
pub async fn phase2_download<S>(
    account_id: String,
    folder: String,
    session: Arc<AsyncMutex<Session<S>>>,
    store: Arc<Store>,
    maildir: Arc<MaildirStore>,
    search_index: Arc<SearchIndex>,
    event_tx: mpsc::Sender<SyncEvent>,
    cancel: CancellationToken,
) -> Result<(), ImapError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Debug,
{
    // Query total count of bodies to download (for progress denominator).
    let total = store
        .count_emails_without_body(&account_id, &folder)
        .map_err(|e| ImapError::DatabaseError(e.to_string()))?;

    if total == 0 {
        let _ = event_tx
            .send(SyncEvent::BodyDownloadComplete {
                account_id: account_id.clone(),
                folder: folder.clone(),
            })
            .await;
        return Ok(());
    }

    tracing::info!(
        account_id = %account_id,
        folder = %folder,
        total = total,
        "Phase 2: starting body download"
    );

    let mut downloaded: u64 = 0;

    loop {
        // Check for cancellation before each batch.
        if cancel.is_cancelled() {
            tracing::info!(
                account_id = %account_id,
                folder = %folder,
                downloaded = downloaded,
                total = total,
                "Phase 2 download cancelled"
            );
            return Ok(());
        }

        // Get next batch of UIDs to fetch (newest-first, up to BODY_FETCH_BATCH_SIZE).
        let batch_uids = store
            .get_uids_without_body(&account_id, &folder, BODY_FETCH_BATCH_SIZE)
            .map_err(|e| ImapError::DatabaseError(e.to_string()))?;

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
            let uid_i64 = i64::from(*uid);

            // Look up email_id from UID.
            let email_id = match store
                .get_email_id_by_uid(&account_id, &folder, uid_i64)
                .map_err(|e| ImapError::DatabaseError(e.to_string()))?
            {
                Some(id) => id,
                None => {
                    tracing::warn!(uid = uid, "no email found for UID, skipping");
                    continue;
                }
            };

            // Default flags for new body — we don't have the flags here,
            // so use default (unread). The IMAP flags are already in SQLite
            // from Phase 1; we just need to write the .eml to disk.
            let flags = EmailFlags::default();

            match process_body(
                &email_id,
                &folder,
                raw_bytes,
                &flags,
                &maildir,
                &store,
            ) {
                Ok(maildir_path) => {
                    // Extract body text and update search index.
                    let body_text = extract_body_text(raw_bytes);

                    // Get the full EmailMeta to pass to search index.
                    // For now, we update via the email row data.
                    if let Ok(Some(email_row)) = store.get_email_by_uid(
                        &account_id,
                        &folder,
                        uid_i64,
                    ) {
                        // Build a minimal EmailMeta for search update.
                        // The search index only needs the fields it indexes.
                        if let Ok(email_meta) = row_to_search_meta(&email_row, &maildir_path) {
                            if let Err(e) = search_index.update_email(
                                &email_meta,
                                Some(&body_text),
                                None,
                            ) {
                                tracing::warn!(
                                    email_id = %email_id,
                                    error = %e,
                                    "failed to update search index, continuing"
                                );
                            }
                            // Commit after each email (could be batched for perf later).
                            let _ = search_index.commit();
                        }
                    }

                    downloaded += 1;
                }
                Err(e) => {
                    // Non-fatal: log error, emit event, continue with next email.
                    tracing::error!(
                        email_id = %email_id,
                        uid = uid,
                        error = %e,
                        "failed to process body, skipping"
                    );
                    let _ = event_tx
                        .send(SyncEvent::BodyDownloadError {
                            email_id,
                            error: e.to_string(),
                        })
                        .await;
                }
            }
        }

        // Emit progress after each batch.
        let _ = event_tx
            .send(SyncEvent::BodyDownloadProgress {
                account_id: account_id.clone(),
                folder: folder.clone(),
                downloaded,
                total,
            })
            .await;
    }

    // Emit completion event.
    let _ = event_tx
        .send(SyncEvent::BodyDownloadComplete {
            account_id: account_id.clone(),
            folder: folder.clone(),
        })
        .await;

    tracing::info!(
        account_id = %account_id,
        folder = %folder,
        downloaded = downloaded,
        "Phase 2 body download complete"
    );

    Ok(())
}

/// Convert an `EmailRow` to a minimal `EmailMeta` for search index update.
///
/// This bridges the gap between the SQLite row format and the
/// `SearchIndex::update_email()` API which expects `EmailMeta`.
pub(crate) fn row_to_search_meta(
    row: &inboxly_store::EmailRow,
    maildir_path: &str,
) -> Result<inboxly_core::EmailMeta, ImapError> {
    use std::path::PathBuf;
    use chrono::{TimeZone, Utc};
    use inboxly_core::{AccountId, Contact, EmailFlags, EmailId, EmailMeta, ThreadId};

    let account_uuid = uuid::Uuid::parse_str(&row.account_id)
        .map_err(|e| ImapError::DatabaseError(format!("invalid account_id UUID: {e}")))?;

    let thread_uuid = uuid::Uuid::parse_str(&row.thread_id)
        .unwrap_or_else(|_| uuid::Uuid::nil());

    let date = Utc
        .timestamp_opt(row.date, 0)
        .single()
        .unwrap_or_else(Utc::now);

    let flags = EmailFlags {
        read: (row.flags & inboxly_store::flags::READ) != 0,
        starred: (row.flags & inboxly_store::flags::STARRED) != 0,
        answered: (row.flags & inboxly_store::flags::ANSWERED) != 0,
        draft: (row.flags & inboxly_store::flags::DRAFT) != 0,
    };

    // Parse to/cc JSON
    let to: Vec<Contact> = serde_json::from_str(&row.to_json).unwrap_or_default();
    let cc: Vec<Contact> = serde_json::from_str(&row.cc_json).unwrap_or_default();

    Ok(EmailMeta {
        id: EmailId::new(&row.id),
        account_id: AccountId(account_uuid),
        thread_id: ThreadId(thread_uuid),
        from: Contact::new(
            row.from_name.as_deref().unwrap_or(""),
            &row.from_address,
        ),
        to,
        cc,
        subject: row.subject.clone(),
        snippet: row.snippet.clone(),
        date,
        maildir_path: PathBuf::from(maildir_path),
        attachments: vec![],
        flags,
        size_bytes: row.size_bytes as u64,
        imap_uid: row.imap_uid as u32,
        imap_folder: row.imap_folder.clone(),
    })
}
