//! Per-account sync loop orchestrator.
//!
//! Manages the lifecycle of incremental sync + IDLE for a single account:
//! 1. On start: incremental catch-up for all synced folders
//! 2. Enter IDLE on INBOX (primary notification target)
//! 3. On IDLE wakeup: incremental catch-up, re-enter IDLE
//! 4. Periodic full catch-up for non-INBOX folders (every 5 min)
//! 5. On cancellation: exit IDLE, stop syncing, task exits

use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

use crate::channel::SyncEvent;
use crate::error::ImapError;
use crate::folders::WellKnownFolders;
use crate::idle::{IdleEvent, IdleLoopConfig, IdleWakeup, convert_idle_response};
use crate::incremental::incremental_sync_folder;

/// Periodic sync interval for non-INBOX folders (5 minutes).
const PERIODIC_SYNC_INTERVAL_SECS: u64 = 300;

/// Configuration for a per-account sync loop.
pub struct AccountSyncConfig {
    /// SQLite connection (wrapped in Arc<Mutex> for async safety).
    pub db: Arc<Mutex<Connection>>,
    /// Account identifier.
    pub account_id: String,
    /// Whether the server supports CONDSTORE.
    pub has_condstore: bool,
    /// Whether the server supports IDLE.
    pub has_idle: bool,
    /// Resolved well-known folder names for this account.
    pub well_known: WellKnownFolders,
    /// Channel for sending sync events to the UI.
    pub event_tx: mpsc::Sender<SyncEvent>,
    /// Cancellation token for clean shutdown.
    pub cancel: CancellationToken,
}

/// Per-account sync loop.
///
/// This is spawned as a tokio task by the `SyncManager`. It runs until
/// the cancellation token is triggered.
///
/// The `session_factory` closure is called to create new IMAP sessions
/// when needed (initial connection, reconnect after IDLE failure, etc.).
pub async fn account_sync_loop<S, F, Fut>(
    config: AccountSyncConfig,
    session_factory: Arc<F>,
) -> Result<(), ImapError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + std::fmt::Debug + 'static,
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<async_imap::Session<S>, ImapError>> + Send,
{
    let AccountSyncConfig {
        db,
        account_id,
        has_condstore,
        has_idle,
        well_known,
        event_tx,
        cancel,
    } = config;

    tracing::info!(account_id = %account_id, "sync loop started");

    // Phase 1: Initial incremental catch-up for all folders
    {
        let mut session = session_factory().await?;
        let conn = db.lock().await;

        let folder_names = resolve_folder_list(&well_known);

        for folder in &folder_names {
            if cancel.is_cancelled() {
                break;
            }

            match incremental_sync_folder(
                &mut session,
                &conn,
                &account_id,
                folder,
                has_condstore,
                &event_tx,
            )
            .await
            {
                Ok(result) => {
                    tracing::info!(
                        account_id = %account_id,
                        folder,
                        new = result.new_uids.len(),
                        deleted = result.deleted_uids.len(),
                        flags = result.flag_change_count,
                        "incremental sync complete"
                    );
                }
                Err(ImapError::UidValidityChanged { ref folder, .. }) => {
                    tracing::warn!(
                        account_id = %account_id,
                        folder,
                        "UIDVALIDITY changed — full re-sync needed (skipping in M9)"
                    );
                    continue;
                }
                Err(ImapError::NoSyncState { ref folder, .. }) => {
                    tracing::debug!(
                        account_id = %account_id,
                        folder,
                        "no sync state — folder needs initial sync first, skipping"
                    );
                    continue;
                }
                Err(e) => {
                    tracing::error!(
                        account_id = %account_id,
                        folder,
                        error = %e,
                        "incremental sync failed"
                    );
                    let _ = event_tx
                        .send(SyncEvent::Error {
                            account_id: account_id.clone(),
                            message: e.to_string(),
                        })
                        .await;
                }
            }
        }
        // Drop session — IDLE needs its own connection
    }

    if cancel.is_cancelled() {
        tracing::info!(account_id = %account_id, "sync loop cancelled during initial catch-up");
        return Ok(());
    }

    // Phase 2: IDLE loop on INBOX + periodic catch-up for other folders
    if has_idle {
        run_idle_phase(
            db,
            &account_id,
            has_condstore,
            &well_known,
            &event_tx,
            cancel,
            session_factory,
        )
        .await
    } else {
        // No IDLE support — fall back to periodic polling
        run_poll_phase(
            db,
            &account_id,
            has_condstore,
            &well_known,
            &event_tx,
            cancel,
            session_factory,
        )
        .await
    }
}

/// Run the IDLE-based sync phase.
async fn run_idle_phase<S, F, Fut>(
    db: Arc<Mutex<Connection>>,
    account_id: &str,
    has_condstore: bool,
    well_known: &WellKnownFolders,
    event_tx: &mpsc::Sender<SyncEvent>,
    cancel: CancellationToken,
    session_factory: Arc<F>,
) -> Result<(), ImapError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + std::fmt::Debug + 'static,
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<async_imap::Session<S>, ImapError>> + Send,
{
    let (wakeup_tx, mut wakeup_rx) = mpsc::channel::<IdleWakeup>(32);
    let idle_config = IdleLoopConfig::default();

    // Spawn IDLE task for INBOX
    let idle_cancel = cancel.child_token();
    let idle_account_id = account_id.to_string();
    let inbox_folder = well_known.inbox.as_deref().unwrap_or("INBOX").to_string();

    let idle_handle = {
        let idle_cancel = idle_cancel.clone();
        let wakeup_tx = wakeup_tx.clone();
        let idle_config = idle_config.clone();
        let session_factory = Arc::clone(&session_factory);

        tokio::spawn(async move {
            run_idle_task(
                &idle_account_id,
                &inbox_folder,
                &wakeup_tx,
                idle_cancel,
                &idle_config,
                session_factory,
            )
            .await
        })
    };

    // Periodic sync interval for non-INBOX folders
    let mut periodic_interval =
        tokio::time::interval(Duration::from_secs(PERIODIC_SYNC_INTERVAL_SECS));
    periodic_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Skip the first tick (we just did initial catch-up)
    periodic_interval.tick().await;

    let folder_names = resolve_folder_list(well_known);

    loop {
        tokio::select! {
            // IDLE wakeup — changes detected on INBOX
            Some(wakeup) = wakeup_rx.recv() => {
                tracing::debug!(account_id, ?wakeup, "IDLE wakeup received");

                let folder = match &wakeup {
                    IdleWakeup::NewMail { folder, .. }
                    | IdleWakeup::Expunge { folder, .. }
                    | IdleWakeup::FlagsChanged { folder, .. }
                    | IdleWakeup::TimeoutCatchup { folder, .. } => folder.as_str(),
                };

                // Quick incremental catch-up
                let mut session = match session_factory().await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(account_id, error = %e, "post-IDLE catch-up connect failed");
                        continue;
                    }
                };

                let conn = db.lock().await;
                if let Err(e) = incremental_sync_folder(
                    &mut session, &conn, account_id, folder, has_condstore, event_tx,
                ).await {
                    tracing::warn!(account_id, folder, error = %e, "post-IDLE catch-up failed");
                }
            }

            // Periodic catch-up for non-INBOX folders
            _ = periodic_interval.tick() => {
                let mut session = match session_factory().await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(account_id, error = %e, "periodic sync connect failed");
                        continue;
                    }
                };

                let conn = db.lock().await;
                for folder in &folder_names {
                    if *folder == "INBOX" {
                        continue; // Handled by IDLE
                    }
                    if cancel.is_cancelled() {
                        break;
                    }
                    if let Err(e) = incremental_sync_folder(
                        &mut session, &conn, account_id, folder, has_condstore, event_tx,
                    ).await {
                        tracing::warn!(account_id, folder, error = %e, "periodic sync failed");
                    }
                }
            }

            // Cancellation
            _ = cancel.cancelled() => {
                tracing::info!(account_id, "sync loop cancelled");
                idle_cancel.cancel();
                break;
            }
        }
    }

    // Wait for IDLE task to finish
    let _ = idle_handle.await;
    tracing::info!(account_id, "sync loop stopped");
    Ok(())
}

/// The IDLE task: creates sessions, enters IDLE, handles responses, reconnects.
///
/// This runs in its own spawned task because the async-imap IDLE Handle
/// takes ownership of the Session.
async fn run_idle_task<S, F, Fut>(
    account_id: &str,
    folder: &str,
    wakeup_tx: &mpsc::Sender<IdleWakeup>,
    cancel: CancellationToken,
    config: &IdleLoopConfig,
    session_factory: Arc<F>,
) -> Result<(), ImapError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + std::fmt::Debug,
    F: Fn() -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<async_imap::Session<S>, ImapError>> + Send,
{
    let mut consecutive_failures = 0u32;
    let mut backoff = config.initial_backoff;
    let timeout = Duration::from_secs(config.idle_timeout_secs);

    loop {
        if cancel.is_cancelled() {
            return Ok(());
        }

        // TODO(M19): On reconnect, drain offline_queue via
        // offline_replay::replay_offline_actions()

        // Create a new session for IDLE
        let mut session = match session_factory().await {
            Ok(s) => s,
            Err(e) => {
                consecutive_failures += 1;
                tracing::warn!(
                    account_id,
                    error = %e,
                    consecutive_failures,
                    "IDLE: connection failed"
                );
                if consecutive_failures >= config.max_consecutive_failures {
                    return Err(ImapError::IdleInterrupted(format!(
                        "exceeded {} consecutive IDLE failures",
                        config.max_consecutive_failures
                    )));
                }
                tokio::select! {
                    () = tokio::time::sleep(backoff) => {}
                    () = cancel.cancelled() => return Ok(()),
                }
                backoff = Duration::from_secs_f64(
                    (backoff.as_secs_f64() * config.backoff_multiplier)
                        .min(config.max_backoff.as_secs_f64()),
                );
                continue;
            }
        };

        // SELECT the folder for IDLE
        if let Err(e) = session.select(folder).await {
            tracing::warn!(account_id, error = %e, "IDLE: SELECT failed");
            consecutive_failures += 1;
            continue;
        }

        // Enter IDLE mode — async-imap's idle() takes ownership of the session
        let mut idle_handle = session.idle();
        if let Err(e) = idle_handle.init().await {
            tracing::warn!(account_id, error = %e, "IDLE: init failed");
            consecutive_failures += 1;
            continue;
        }

        // Wait for server data or timeout
        let (idle_future, _stop_source) = idle_handle.wait_with_timeout(timeout);

        let idle_result = tokio::select! {
            result = idle_future => result,
            () = cancel.cancelled() => {
                // Cancelled — can't send DONE easily since we'd need the handle back
                // The connection will be dropped, which is fine for cancellation
                return Ok(());
            }
        };

        match idle_result {
            Ok(response) => {
                let event = convert_idle_response(&response);

                // Send DONE to exit IDLE — done() consumes the handle and returns the session
                // We need the handle back. But wait_with_timeout borrows &mut self...
                // Actually after wait resolves, we need to call done() on the handle.
                // The handle was consumed by wait_with_timeout's borrow. Let me re-check...
                //
                // Actually: wait_with_timeout takes &mut self and returns a future.
                // After the future resolves, we still have the &mut borrow on idle_handle.
                // We can then call idle_handle.done().await to get the session back.
                //
                // But done(self) takes ownership... and we have idle_handle as a local.
                // Let's restructure.
                drop(_stop_source);

                // We need to reconstruct — done() consumes self
                let _session = match idle_handle.done().await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(account_id, error = %e, "IDLE: DONE failed");
                        consecutive_failures += 1;
                        continue;
                    }
                };

                // Reset backoff on success
                consecutive_failures = 0;
                backoff = config.initial_backoff;

                match event {
                    IdleEvent::NewMessages { exists } => {
                        tracing::debug!(account_id, folder, exists, "IDLE: new messages");
                        let _ = wakeup_tx
                            .send(IdleWakeup::NewMail {
                                account_id: account_id.to_string(),
                                folder: folder.to_string(),
                            })
                            .await;
                    }
                    IdleEvent::Expunge { seq } => {
                        tracing::debug!(account_id, folder, seq, "IDLE: message expunged");
                        let _ = wakeup_tx
                            .send(IdleWakeup::Expunge {
                                account_id: account_id.to_string(),
                                folder: folder.to_string(),
                            })
                            .await;
                    }
                    IdleEvent::FlagsChanged => {
                        tracing::debug!(account_id, folder, "IDLE: flags changed");
                        let _ = wakeup_tx
                            .send(IdleWakeup::FlagsChanged {
                                account_id: account_id.to_string(),
                                folder: folder.to_string(),
                            })
                            .await;
                    }
                    IdleEvent::Timeout => {
                        tracing::trace!(account_id, folder, "IDLE: timeout, re-entering");
                        let _ = wakeup_tx
                            .send(IdleWakeup::TimeoutCatchup {
                                account_id: account_id.to_string(),
                                folder: folder.to_string(),
                            })
                            .await;
                    }
                    IdleEvent::Cancelled => {
                        tracing::info!(account_id, folder, "IDLE: cancelled");
                        return Ok(());
                    }
                }

                // Loop continues — will create a new session and re-enter IDLE
            }
            Err(e) => {
                consecutive_failures += 1;
                tracing::warn!(
                    account_id,
                    error = %e,
                    consecutive_failures,
                    "IDLE: session error, will reconnect"
                );

                if consecutive_failures >= config.max_consecutive_failures {
                    return Err(ImapError::IdleInterrupted(format!(
                        "exceeded {} consecutive IDLE failures",
                        config.max_consecutive_failures
                    )));
                }

                tokio::select! {
                    () = tokio::time::sleep(backoff) => {}
                    () = cancel.cancelled() => return Ok(()),
                }
                backoff = Duration::from_secs_f64(
                    (backoff.as_secs_f64() * config.backoff_multiplier)
                        .min(config.max_backoff.as_secs_f64()),
                );
            }
        }
    }
}

/// Run the polling-based sync phase (for servers without IDLE).
async fn run_poll_phase<S, F, Fut>(
    db: Arc<Mutex<Connection>>,
    account_id: &str,
    has_condstore: bool,
    well_known: &WellKnownFolders,
    event_tx: &mpsc::Sender<SyncEvent>,
    cancel: CancellationToken,
    session_factory: Arc<F>,
) -> Result<(), ImapError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + std::fmt::Debug,
    F: Fn() -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<async_imap::Session<S>, ImapError>> + Send,
{
    // Poll every 60 seconds for all folders
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Skip the first tick (we just did initial catch-up)
    interval.tick().await;

    let folder_names = resolve_folder_list(well_known);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let mut session = match session_factory().await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(account_id, error = %e, "poll sync connect failed");
                        continue;
                    }
                };

                let conn = db.lock().await;
                for folder in &folder_names {
                    if cancel.is_cancelled() {
                        break;
                    }
                    if let Err(e) = incremental_sync_folder(
                        &mut session, &conn, account_id, folder, has_condstore, event_tx,
                    ).await {
                        tracing::warn!(account_id, folder, error = %e, "poll sync failed");
                    }
                }
            }

            _ = cancel.cancelled() => {
                tracing::info!(account_id, "poll sync loop cancelled");
                break;
            }
        }
    }

    Ok(())
}

/// Resolve the list of folder names to sync from well-known folders.
fn resolve_folder_list(well_known: &WellKnownFolders) -> Vec<String> {
    let mut folders = Vec::with_capacity(5);

    if let Some(ref inbox) = well_known.inbox {
        folders.push(inbox.clone());
    } else {
        folders.push("INBOX".to_string());
    }

    if let Some(ref sent) = well_known.sent {
        folders.push(sent.clone());
    }
    if let Some(ref drafts) = well_known.drafts {
        folders.push(drafts.clone());
    }
    if let Some(ref trash) = well_known.trash {
        folders.push(trash.clone());
    }
    if let Some(ref spam) = well_known.spam {
        folders.push(spam.clone());
    }

    folders
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_folder_list_complete() {
        let wk = WellKnownFolders {
            inbox: Some("INBOX".to_string()),
            sent: Some("[Gmail]/Sent Mail".to_string()),
            drafts: Some("[Gmail]/Drafts".to_string()),
            trash: Some("[Gmail]/Trash".to_string()),
            spam: Some("[Gmail]/Spam".to_string()),
        };
        let folders = resolve_folder_list(&wk);
        assert_eq!(folders.len(), 5);
        assert_eq!(folders[0], "INBOX");
        assert_eq!(folders[1], "[Gmail]/Sent Mail");
    }

    #[test]
    fn test_resolve_folder_list_partial() {
        let wk = WellKnownFolders {
            inbox: None,
            sent: Some("Sent".to_string()),
            drafts: None,
            trash: None,
            spam: None,
        };
        let folders = resolve_folder_list(&wk);
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0], "INBOX"); // Fallback
        assert_eq!(folders[1], "Sent");
    }
}
