//! On-demand single-email body fetch.
//!
//! When the user opens an email whose body hasn't been downloaded by
//! Phase 2 yet, this module fetches the single RFC822 body, processes
//! it, and notifies the UI.

use std::fmt::Debug;
use std::sync::Arc;

use async_imap::Session;
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use tokio::io::{AsyncRead, AsyncWrite};

use inboxly_core::EmailFlags;
use inboxly_store::Store;
use inboxly_store::maildir_store::MaildirStore;
use inboxly_store::search::SearchIndex;

use crate::body_fetch::fetch_body_single;
use crate::body_processor::{extract_body_text, process_body};
use crate::channel::SyncEvent;
use crate::error::ImapError;

/// Fetch a single email's body on demand.
///
/// Called when the user opens an email whose body has not been downloaded
/// by Phase 2 yet. This takes priority over the background batch download.
///
/// Returns the raw RFC822 bytes on success (so the caller can parse and
/// display immediately without re-reading from Maildir).
pub async fn fetch_body_on_demand<S>(
    email_id: &str,
    account_id: &str,
    folder: &str,
    imap_uid: u32,
    session: &Arc<AsyncMutex<Session<S>>>,
    store: &Store,
    maildir: &MaildirStore,
    search_index: &SearchIndex,
    event_tx: &mpsc::Sender<SyncEvent>,
) -> Result<Vec<u8>, ImapError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Debug,
{
    // Check if body was already downloaded (race with Phase 2).
    let already_downloaded = store
        .is_body_downloaded(email_id)
        .map_err(|e| ImapError::DatabaseError(e.to_string()))?;

    if already_downloaded {
        // Body already exists — read from Maildir and return.
        let path = store
            .get_maildir_path(email_id)
            .map_err(|e| ImapError::DatabaseError(e.to_string()))?
            .ok_or_else(|| ImapError::EmailNotFound(email_id.to_string()))?;
        let bytes = std::fs::read(&path)
            .map_err(|e| ImapError::MaildirRead(e.to_string()))?;
        return Ok(bytes);
    }

    // Fetch from IMAP.
    let raw_bytes = {
        let mut sess = session.lock().await;
        fetch_body_single(&mut sess, imap_uid)
            .await?
            .ok_or_else(|| ImapError::EmailNotFound(email_id.to_string()))?
    };

    // Process: Maildir write + SQLite update.
    let flags = EmailFlags::default();
    let maildir_path = process_body(
        email_id,
        folder,
        &raw_bytes,
        &flags,
        maildir,
        store,
    )?;

    // Update search index with body text.
    let body_text = extract_body_text(&raw_bytes);
    let uid_i64 = i64::from(imap_uid);
    if let Ok(Some(email_row)) = store.get_email_by_uid(account_id, folder, uid_i64) {
        if let Ok(email_meta) = crate::phase2::row_to_search_meta(&email_row, &maildir_path) {
            if let Err(e) = search_index.update_email(&email_meta, Some(&body_text), None) {
                tracing::warn!(
                    email_id = %email_id,
                    error = %e,
                    "failed to update search index for on-demand fetch"
                );
            }
            let _ = search_index.commit();
        }
    }

    // Notify UI that the body is ready.
    let _ = event_tx
        .send(SyncEvent::BodyFetched {
            email_id: email_id.to_string(),
        })
        .await;

    Ok(raw_bytes)
}
