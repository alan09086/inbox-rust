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

use inboxly_core::{DraftEmail, OfflineAction};
use inboxly_store::Store;

use crate::append::imap_append_sent;
use crate::error::ImapError;
use crate::folders::WellKnownFolders;
use crate::smtp::DraftSender;

/// Replay all queued offline actions against the IMAP server.
///
/// Actions are replayed in FIFO order. Each successfully replayed action
/// is removed from the queue. Failed actions remain in the queue for
/// the next replay attempt.
///
/// `well_known` is used to resolve the archive folder for `MarkDone` actions.
///
/// `draft_sender` is an optional [`DraftSender`] used to replay
/// [`OfflineAction::SendDraftFull`] (and the legacy [`OfflineAction::SendDraft`])
/// actions. Existing callers (sync loop, IDLE reconnect) pass `None` and
/// preserve the legacy "log and skip" behaviour for draft replay; Phase
/// 12's send bridge will pass `Some(&smtp_sender)` so queued drafts
/// actually leave the wire.
///
/// Returns the count of successfully replayed actions.
///
/// # Errors
///
/// Returns [`ImapError::DatabaseError`] if reading or dequeuing the
/// offline queue fails. Per-action failures are logged and left in the
/// queue for the next replay attempt — they do not propagate.
pub async fn replay_offline_queue<S>(
    session: &Arc<AsyncMutex<Session<S>>>,
    store: &Store,
    well_known: &WellKnownFolders,
    draft_sender: Option<&dyn DraftSender>,
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

        match replay_single_action(session, &action, well_known, draft_sender, store).await {
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
    draft_sender: Option<&dyn DraftSender>,
    store: &Store,
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
            // Legacy variant — only the maildir `.eml` path was preserved,
            // and we have no robust way to round-trip a maildir file back
            // into a `DraftEmail` here (the .eml has no embedded
            // attachment metadata, no `mode`, no in-reply-to chain, etc.).
            //
            // Existing queue entries from before M35b are unlikely to
            // exist on user machines (the SendDraft variant was wired in
            // M23 but never enqueued). Phase 12's send bridge enqueues
            // `SendDraftFull` for all new drafts. A future cleanup
            // (post-M40) can remove this variant entirely.
            //
            // For now: log and skip so the queue doesn't get stuck.
            tracing::warn!(
                path = draft_maildir_path,
                "legacy SendDraft replay skipped — use SendDraftFull (M35b)"
            );
        }
        OfflineAction::SendDraftFull { draft } => {
            attempt_send_draft_full(draft_sender, draft).await?;

            // SMTP send succeeded — append the message to the Sent
            // folder so the user's own copy reflects the send. Best-
            // effort: the email already left the wire, so an APPEND
            // failure is logged but does not propagate.
            let Some(account_config) = resolve_account_config(store, &draft.account_id.to_string())
            else {
                tracing::warn!(
                    account_id = %draft.account_id,
                    "SendDraftFull: account row not found, skipping Sent folder APPEND"
                );
                return Ok(());
            };

            if let Err(e) = imap_append_sent(&mut sess, &account_config, draft).await {
                tracing::warn!(
                    error = %e,
                    message_id = %draft.message_id,
                    "SendDraftFull: SMTP send succeeded but Sent folder APPEND failed; continuing"
                );
            }
        }
    }

    Ok(())
}

/// Pure helper that performs the SMTP send portion of a
/// [`OfflineAction::SendDraftFull`] replay.
///
/// Extracted from [`replay_single_action`] so unit tests can verify the
/// success / failure / no-sender paths without standing up a real IMAP
/// session — see the `sendraft_tests` module below.
///
/// Returns `Ok(())` when:
/// - the sender is `None` (logged and skipped, mirroring the legacy
///   behaviour for callers that don't yet pass a `DraftSender`)
/// - the sender accepts the draft.
///
/// # Errors
///
/// Returns [`ImapError::Io`] wrapping the [`crate::smtp::SmtpError`]
/// when the sender rejects the draft. The error is propagated up to
/// `replay_offline_queue` so the entry stays in the queue and is
/// retried on the next replay pass.
pub(crate) async fn attempt_send_draft_full(
    draft_sender: Option<&dyn DraftSender>,
    draft: &DraftEmail,
) -> Result<(), ImapError> {
    let Some(sender) = draft_sender else {
        tracing::warn!(
            message_id = %draft.message_id,
            "SendDraftFull replay skipped — no DraftSender provided to replay_offline_queue"
        );
        return Ok(());
    };

    sender
        .send_draft(draft)
        .await
        .map_err(|e| ImapError::Io(std::io::Error::other(format!("smtp send: {e}"))))?;

    Ok(())
}

/// Resolve an [`inboxly_core::AccountConfig`] for the given account id.
///
/// Used by [`OfflineAction::SendDraftFull`] replay to look up the
/// `From:` mailbox + IMAP folder context for the Sent folder APPEND.
/// Returns `None` if the account row has been deleted (e.g. while a
/// queued draft was waiting to send) — caller logs and skips.
fn resolve_account_config(store: &Store, account_id: &str) -> Option<inboxly_core::AccountConfig> {
    let row = store.get_account(account_id).ok()?;

    let auth_method = match row.auth_method.as_str() {
        "oauth2" => inboxly_core::AuthMethod::OAuth2,
        "app_password" => inboxly_core::AuthMethod::AppPassword,
        _ => inboxly_core::AuthMethod::Password,
    };

    Some(inboxly_core::AccountConfig {
        email: row.email,
        display_name: row.display_name,
        provider: row.provider,
        auth_method,
        imap_host: row.imap_host,
        imap_port: row.imap_port,
        smtp_host: row.smtp_host,
        smtp_port: row.smtp_port,
    })
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

#[cfg(test)]
mod sendraft_tests {
    //! Unit tests for the [`OfflineAction::SendDraftFull`] replay path.
    //!
    //! These target the pure helper [`attempt_send_draft_full`] so they
    //! can verify success / failure / no-sender semantics without
    //! standing up a real IMAP `Session`. The IMAP session integration
    //! (Sent folder APPEND on success) is exercised end-to-end in
    //! Phase 13's manual verification.

    use super::*;
    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;
    use inboxly_core::{AccountId, Contact, DraftEmail};

    use crate::smtp::error::SmtpError;

    /// Mock that records each `send_draft` call and returns a configurable result.
    ///
    /// Each call pops one outcome from `outcomes` (LIFO) so callers can
    /// queue up a sequence of expected results. An empty queue returns
    /// `Ok(())` to keep the always-succeed path concise.
    struct MockDraftSender {
        outcomes: StdMutex<Vec<Result<(), SmtpError>>>,
        sent_drafts: StdMutex<Vec<String>>,
    }

    impl MockDraftSender {
        fn always_succeed() -> Self {
            Self {
                outcomes: StdMutex::new(Vec::new()),
                sent_drafts: StdMutex::new(Vec::new()),
            }
        }

        fn with_outcomes(outcomes: Vec<Result<(), SmtpError>>) -> Self {
            Self {
                outcomes: StdMutex::new(outcomes),
                sent_drafts: StdMutex::new(Vec::new()),
            }
        }

        fn sent_message_ids(&self) -> Vec<String> {
            self.sent_drafts
                .lock()
                .expect("mock sent_drafts mutex poisoned")
                .clone()
        }
    }

    #[async_trait]
    impl DraftSender for MockDraftSender {
        async fn send_draft(&self, draft: &DraftEmail) -> Result<(), SmtpError> {
            self.sent_drafts
                .lock()
                .expect("mock sent_drafts mutex poisoned")
                .push(draft.message_id.clone());
            self.outcomes
                .lock()
                .expect("mock outcomes mutex poisoned")
                .pop()
                .unwrap_or(Ok(()))
        }
    }

    fn sample_draft(message_id: &str) -> DraftEmail {
        let mut d = DraftEmail::new_empty(AccountId::new());
        d.message_id = message_id.to_string();
        d.subject = "Test".to_string();
        d.body_markdown = "Hello".to_string();
        d.to = vec![Contact::new("Recipient", "recipient@example.com")];
        d
    }

    /// Success path: a sender that accepts the draft returns `Ok(())`
    /// and the mock records the message_id of the call. The replay
    /// loop's caller (`replay_offline_queue`) will then dequeue.
    #[tokio::test]
    async fn send_draft_full_success_calls_sender_and_returns_ok() {
        let sender = MockDraftSender::always_succeed();
        let draft = sample_draft("<msg-success@inboxly.local>");

        let result = attempt_send_draft_full(Some(&sender), &draft).await;

        assert!(result.is_ok(), "expected Ok, got {result:?}");
        assert_eq!(
            sender.sent_message_ids(),
            vec!["<msg-success@inboxly.local>".to_string()],
            "mock should have recorded exactly one send call"
        );
    }

    /// Failure path: a sender that returns a transient error propagates
    /// as `Err`, so `replay_offline_queue` leaves the entry in the
    /// queue for retry on the next pass.
    #[tokio::test]
    async fn send_draft_full_failure_propagates_error() {
        let sender = MockDraftSender::with_outcomes(vec![Err(SmtpError::NetworkError {
            reason: "tcp reset".to_string(),
        })]);
        let draft = sample_draft("<msg-failure@inboxly.local>");

        let result = attempt_send_draft_full(Some(&sender), &draft).await;

        assert!(result.is_err(), "expected Err, got {result:?}");
        let err = result.expect_err("checked above");
        let display = err.to_string();
        assert!(
            display.contains("smtp send"),
            "error should mention smtp send context, got: {display}"
        );
        assert!(
            display.contains("tcp reset"),
            "error should preserve underlying SmtpError reason, got: {display}"
        );
        assert_eq!(
            sender.sent_message_ids(),
            vec!["<msg-failure@inboxly.local>".to_string()],
            "mock should still have recorded the attempt"
        );
    }

    /// No-sender path: when the caller passes `None` for `draft_sender`,
    /// the helper logs a warning and returns `Ok(())` so the queue
    /// entry stays in place (preserving pre-M35b behaviour for the
    /// sync_loop / IDLE callers that don't yet pass a `DraftSender`).
    #[tokio::test]
    async fn send_draft_full_skips_when_no_sender_provided() {
        let draft = sample_draft("<msg-noop@inboxly.local>");

        let result = attempt_send_draft_full(None, &draft).await;

        assert!(
            result.is_ok(),
            "no-sender path must return Ok so the queue entry stays put: {result:?}"
        );
    }

    /// Permanent rejection (5xx) propagates as Err exactly the same way
    /// as a transient failure — the caller (`replay_offline_queue`)
    /// will leave the entry in the queue. Phase 12's send bridge owns
    /// the permanent-vs-transient retry policy via
    /// [`crate::smtp::retry::should_retry`]; the offline replay path
    /// is intentionally simple.
    #[tokio::test]
    async fn send_draft_full_permanent_rejection_propagates_error() {
        let sender = MockDraftSender::with_outcomes(vec![Err(SmtpError::Rejected {
            code: 550,
            message: "mailbox unavailable".to_string(),
        })]);
        let draft = sample_draft("<msg-rejected@inboxly.local>");

        let result = attempt_send_draft_full(Some(&sender), &draft).await;

        assert!(result.is_err(), "expected Err for permanent rejection");
        let err = result.expect_err("checked above");
        assert!(
            err.to_string().contains("mailbox unavailable"),
            "error should preserve rejection reason"
        );
    }
}
