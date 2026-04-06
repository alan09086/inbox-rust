//! Offline action replay against IMAP on reconnect.
//!
//! Drains the offline queue from SQLite and replays each action
//! against the IMAP server. Successfully replayed actions are
//! removed from the queue. Failed actions remain for the next attempt.

use std::fmt::Debug;
use std::sync::Arc;

use async_imap::Session;
use futures::TryStreamExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex as AsyncMutex;

use inboxly_core::OfflineAction;
use inboxly_store::Store;

use crate::error::ImapError;
use crate::folders::WellKnownFolders;

/// Replay all queued offline actions against the IMAP server.
///
/// Actions are replayed in FIFO order. Each successfully replayed action
/// is removed from the queue. Failed actions remain in the queue for
/// the next replay attempt.
///
/// `well_known` is used to resolve the archive folder for `MarkDone` actions.
///
/// Returns the count of successfully replayed actions.
pub async fn replay_offline_queue<S>(
    session: &Arc<AsyncMutex<Session<S>>>,
    store: &Store,
    well_known: &WellKnownFolders,
) -> Result<u64, ImapError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Debug,
{
    let queue_rows = store
        .get_offline_queue()
        .map_err(|e| ImapError::DatabaseError(e.to_string()))?;

    if queue_rows.is_empty() {
        return Ok(0);
    }

    tracing::info!(count = queue_rows.len(), "replaying offline queue");

    let mut replayed = 0u64;

    for row in &queue_rows {
        let row_id = match row.id {
            Some(id) => id,
            None => continue,
        };

        // Deserialize the action from JSON.
        let action: OfflineAction = match serde_json::from_str(&row.payload_json) {
            Ok(a) => a,
            Err(e) => {
                tracing::error!(
                    action_id = row_id,
                    error = %e,
                    "failed to deserialize offline action, removing from queue"
                );
                // Remove corrupt entries so they don't block the queue forever.
                let _ = store.dequeue_offline_action(row_id);
                continue;
            }
        };

        match replay_single_action(session, &action, well_known).await {
            Ok(()) => {
                store
                    .dequeue_offline_action(row_id)
                    .map_err(|e| ImapError::DatabaseError(e.to_string()))?;
                replayed += 1;
            }
            Err(e) => {
                // Log and continue — action stays in queue for next attempt.
                tracing::error!(
                    action_id = row_id,
                    action = ?action,
                    error = %e,
                    "failed to replay offline action, will retry"
                );
            }
        }
    }

    tracing::info!(
        replayed = replayed,
        remaining = queue_rows.len() as u64 - replayed,
        "offline queue replay complete"
    );

    Ok(replayed)
}

/// Replay a single offline action against IMAP.
async fn replay_single_action<S>(
    session: &Arc<AsyncMutex<Session<S>>>,
    action: &OfflineAction,
    well_known: &WellKnownFolders,
) -> Result<(), ImapError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Debug,
{
    let mut sess = session.lock().await;

    match action {
        OfflineAction::MarkRead {
            folder, imap_uid, ..
        } => {
            sess.select(folder).await?;
            let _ = sess
                .uid_store(imap_uid.to_string(), "+FLAGS (\\Seen)")
                .await?
                .try_collect::<Vec<_>>()
                .await?;
        }
        OfflineAction::MarkUnread {
            folder, imap_uid, ..
        } => {
            sess.select(folder).await?;
            let _ = sess
                .uid_store(imap_uid.to_string(), "-FLAGS (\\Seen)")
                .await?
                .try_collect::<Vec<_>>()
                .await?;
        }
        OfflineAction::Star {
            folder, imap_uid, ..
        } => {
            sess.select(folder).await?;
            let _ = sess
                .uid_store(imap_uid.to_string(), "+FLAGS (\\Flagged)")
                .await?
                .try_collect::<Vec<_>>()
                .await?;
        }
        OfflineAction::Unstar {
            folder, imap_uid, ..
        } => {
            sess.select(folder).await?;
            let _ = sess
                .uid_store(imap_uid.to_string(), "-FLAGS (\\Flagged)")
                .await?
                .try_collect::<Vec<_>>()
                .await?;
        }
        OfflineAction::MarkDone {
            folder, imap_uid, ..
        } => {
            sess.select(folder).await?;
            match resolve_mark_done_strategy(well_known) {
                MarkDoneStrategy::ArchiveThenDelete { ref archive_folder } => {
                    // Mark as read, then move to provider-specific archive folder.
                    // Gmail: [Gmail]/All Mail, Outlook: Archive
                    let _ = sess
                        .uid_store(imap_uid.to_string(), "+FLAGS (\\Seen)")
                        .await?
                        .try_collect::<Vec<_>>()
                        .await?;
                    sess.uid_copy(imap_uid.to_string(), archive_folder).await?;
                    let _ = sess
                        .uid_store(imap_uid.to_string(), "+FLAGS (\\Deleted)")
                        .await?
                        .try_collect::<Vec<_>>()
                        .await?;
                    let _ = sess.expunge().await?.try_collect::<Vec<_>>().await?;
                }
                MarkDoneStrategy::DeleteInPlace => {
                    // No archive folder known — mark read+deleted + expunge.
                    let _ = sess
                        .uid_store(imap_uid.to_string(), "+FLAGS (\\Seen \\Deleted)")
                        .await?
                        .try_collect::<Vec<_>>()
                        .await?;
                    let _ = sess.expunge().await?.try_collect::<Vec<_>>().await?;
                }
            }
        }
        OfflineAction::MoveToTrash {
            folder, imap_uid, ..
        } => {
            sess.select(folder).await?;
            sess.uid_copy(imap_uid.to_string(), "Trash").await?;
            let _ = sess
                .uid_store(imap_uid.to_string(), "+FLAGS (\\Deleted)")
                .await?
                .try_collect::<Vec<_>>()
                .await?;
            let _ = sess.expunge().await?.try_collect::<Vec<_>>().await?;
        }
        OfflineAction::MoveToFolder {
            from_folder,
            to_folder,
            imap_uid,
            ..
        } => {
            sess.select(from_folder).await?;
            sess.uid_copy(imap_uid.to_string(), to_folder).await?;
            let _ = sess
                .uid_store(imap_uid.to_string(), "+FLAGS (\\Deleted)")
                .await?
                .try_collect::<Vec<_>>()
                .await?;
            let _ = sess.expunge().await?.try_collect::<Vec<_>>().await?;
        }
        OfflineAction::MarkAnswered {
            folder, imap_uid, ..
        } => {
            sess.select(folder).await?;
            let _ = sess
                .uid_store(imap_uid.to_string(), "+FLAGS (\\Answered)")
                .await?
                .try_collect::<Vec<_>>()
                .await?;
        }
        OfflineAction::SendDraft {
            draft_maildir_path, ..
        } => {
            // Draft sending is handled by SMTP (M23), not IMAP.
            tracing::warn!(
                path = draft_maildir_path,
                "SendDraft replay deferred to M23 (SMTP integration)"
            );
        }
    }

    Ok(())
}

/// The IMAP operation sequence to use when replaying a `MarkDone` action.
///
/// Used as the return type of [`resolve_mark_done_strategy`] so the
/// branching logic can be tested without a live IMAP connection.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MarkDoneStrategy {
    /// Mark read, copy to archive, mark deleted, expunge.
    ArchiveThenDelete { archive_folder: String },
    /// Mark read+deleted in-place, expunge (no archive available).
    DeleteInPlace,
}

/// Resolve which `MarkDone` strategy to use for a given set of well-known folders.
pub(crate) fn resolve_mark_done_strategy(well_known: &WellKnownFolders) -> MarkDoneStrategy {
    match &well_known.archive {
        Some(folder) => MarkDoneStrategy::ArchiveThenDelete {
            archive_folder: folder.clone(),
        },
        None => MarkDoneStrategy::DeleteInPlace,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// When the archive folder is known, `MarkDone` should use the archive strategy.
    #[test]
    fn mark_done_uses_archive_when_available() {
        let wk = WellKnownFolders {
            inbox: Some("INBOX".to_string()),
            sent: Some("[Gmail]/Sent Mail".to_string()),
            drafts: Some("[Gmail]/Drafts".to_string()),
            trash: Some("[Gmail]/Trash".to_string()),
            spam: Some("[Gmail]/Spam".to_string()),
            archive: Some("[Gmail]/All Mail".to_string()),
        };

        let strategy = resolve_mark_done_strategy(&wk);
        assert_eq!(
            strategy,
            MarkDoneStrategy::ArchiveThenDelete {
                archive_folder: "[Gmail]/All Mail".to_string(),
            }
        );
    }

    /// When no archive folder is available, `MarkDone` should delete in place.
    #[test]
    fn mark_done_falls_back_to_delete() {
        let wk = WellKnownFolders {
            inbox: Some("INBOX".to_string()),
            sent: None,
            drafts: None,
            trash: None,
            spam: None,
            archive: None,
        };

        let strategy = resolve_mark_done_strategy(&wk);
        assert_eq!(strategy, MarkDoneStrategy::DeleteInPlace);
    }

    /// Outlook's `Archive` folder should produce an archive strategy.
    #[test]
    fn mark_done_uses_outlook_archive() {
        let wk = WellKnownFolders {
            inbox: Some("INBOX".to_string()),
            sent: Some("Sent Items".to_string()),
            drafts: Some("Drafts".to_string()),
            trash: Some("Deleted Items".to_string()),
            spam: Some("Junk Email".to_string()),
            archive: Some("Archive".to_string()),
        };

        let strategy = resolve_mark_done_strategy(&wk);
        assert_eq!(
            strategy,
            MarkDoneStrategy::ArchiveThenDelete {
                archive_folder: "Archive".to_string(),
            }
        );
    }
}
