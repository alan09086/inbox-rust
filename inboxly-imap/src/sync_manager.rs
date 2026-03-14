//! Multi-account sync lifecycle management.
//!
//! `SyncManager` owns per-account sync tasks and exposes lifecycle control:
//! - `start(account_id)` — spawn a sync loop for the account
//! - `stop(account_id)` — cancel and clean up the sync loop
//! - `stop_all()` — stop all accounts (application shutdown)
//! - `is_running(account_id)` — check if an account is syncing
//! - `running_accounts()` — list all syncing accounts

use std::collections::HashMap;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::error::ImapError;

/// Handle for a running account sync task.
struct AccountSyncHandle {
    cancel: CancellationToken,
    join_handle: JoinHandle<Result<(), ImapError>>,
}

/// Manages sync lifecycle for all accounts.
///
/// Thread-safe: all methods take `&self` and use interior mutability.
pub struct SyncManager {
    accounts: Mutex<HashMap<String, AccountSyncHandle>>,
    /// Master cancellation token — cancelling this stops ALL accounts.
    master_cancel: CancellationToken,
}

impl SyncManager {
    /// Create a new sync manager.
    pub fn new() -> Self {
        Self {
            accounts: Mutex::new(HashMap::new()),
            master_cancel: CancellationToken::new(),
        }
    }

    /// Start syncing for an account by providing a pre-spawned task handle
    /// and its cancellation token.
    ///
    /// If already running, this is a no-op.
    pub async fn register(
        &self,
        account_id: String,
        cancel: CancellationToken,
        join_handle: JoinHandle<Result<(), ImapError>>,
    ) {
        let mut accounts = self.accounts.lock().await;

        if accounts.contains_key(&account_id) {
            tracing::warn!(account_id = %account_id, "sync already running");
            return;
        }

        accounts.insert(
            account_id.clone(),
            AccountSyncHandle {
                cancel,
                join_handle,
            },
        );

        tracing::info!(account_id = %account_id, "sync registered");
    }

    /// Get a child token of the master cancellation token.
    ///
    /// Use this when spawning account sync tasks so that `stop_all()`
    /// propagates to all accounts.
    pub fn child_token(&self) -> CancellationToken {
        self.master_cancel.child_token()
    }

    /// Stop syncing for an account. Task is cancelled and cleaned up.
    pub async fn stop(&self, account_id: &str) -> Result<(), ImapError> {
        let mut accounts = self.accounts.lock().await;

        if let Some(handle) = accounts.remove(account_id) {
            handle.cancel.cancel();
            match handle.join_handle.await {
                Ok(Ok(())) => {
                    tracing::info!(account_id = %account_id, "sync stopped cleanly");
                }
                Ok(Err(e)) => {
                    tracing::warn!(account_id = %account_id, error = %e, "sync stopped with error");
                }
                Err(e) => {
                    tracing::error!(account_id = %account_id, error = %e, "sync task panicked");
                }
            }
            Ok(())
        } else {
            Err(ImapError::SyncNotRunning(account_id.to_string()))
        }
    }

    /// Stop all account syncs. Used on application shutdown.
    pub async fn stop_all(&self) {
        self.master_cancel.cancel();

        let mut accounts = self.accounts.lock().await;
        let ids: Vec<String> = accounts.keys().cloned().collect();

        for id in ids {
            if let Some(handle) = accounts.remove(&id) {
                let _ = handle.join_handle.await;
            }
        }

        tracing::info!("all sync tasks stopped");
    }

    /// Check if an account's sync is currently running.
    pub async fn is_running(&self, account_id: &str) -> bool {
        let accounts = self.accounts.lock().await;
        accounts.contains_key(account_id)
    }

    /// Get IDs of all accounts currently syncing.
    pub async fn running_accounts(&self) -> Vec<String> {
        let accounts = self.accounts.lock().await;
        accounts.keys().cloned().collect()
    }
}

impl Default for SyncManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sync_manager_register_and_check() {
        let manager = SyncManager::new();
        let cancel = manager.child_token();

        // Spawn a dummy task
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move {
            cancel_clone.cancelled().await;
            Ok(())
        });

        manager
            .register("account1".to_string(), cancel, handle)
            .await;

        assert!(manager.is_running("account1").await);
        assert!(!manager.is_running("account2").await);
    }

    #[tokio::test]
    async fn test_sync_manager_stop() {
        let manager = SyncManager::new();
        let cancel = manager.child_token();

        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move {
            cancel_clone.cancelled().await;
            Ok(())
        });

        manager
            .register("account1".to_string(), cancel, handle)
            .await;

        assert!(manager.is_running("account1").await);

        manager.stop("account1").await.expect("stop should succeed");
        assert!(!manager.is_running("account1").await);
    }

    #[tokio::test]
    async fn test_sync_manager_stop_nonexistent() {
        let manager = SyncManager::new();
        let result = manager.stop("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sync_manager_stop_all() {
        let manager = SyncManager::new();

        for i in 0..3 {
            let cancel = manager.child_token();
            let cancel_clone = cancel.clone();
            let handle = tokio::spawn(async move {
                cancel_clone.cancelled().await;
                Ok(())
            });
            manager
                .register(format!("account{i}"), cancel, handle)
                .await;
        }

        assert_eq!(manager.running_accounts().await.len(), 3);

        manager.stop_all().await;
        assert!(manager.running_accounts().await.is_empty());
    }

    #[tokio::test]
    async fn test_sync_manager_running_accounts() {
        let manager = SyncManager::new();

        for name in &["alice", "bob", "charlie"] {
            let cancel = manager.child_token();
            let cancel_clone = cancel.clone();
            let handle = tokio::spawn(async move {
                cancel_clone.cancelled().await;
                Ok(())
            });
            manager.register(name.to_string(), cancel, handle).await;
        }

        let mut running = manager.running_accounts().await;
        running.sort();
        assert_eq!(running, vec!["alice", "bob", "charlie"]);

        // Cleanup
        manager.stop_all().await;
    }

    #[tokio::test]
    async fn test_sync_manager_register_idempotent() {
        let manager = SyncManager::new();

        let cancel1 = manager.child_token();
        let cancel1_clone = cancel1.clone();
        let handle1 = tokio::spawn(async move {
            cancel1_clone.cancelled().await;
            Ok(())
        });
        manager
            .register("account1".to_string(), cancel1, handle1)
            .await;

        // Register again — should be a no-op
        let cancel2 = manager.child_token();
        let cancel2_clone = cancel2.clone();
        let handle2 = tokio::spawn(async move {
            cancel2_clone.cancelled().await;
            Ok(())
        });
        manager
            .register("account1".to_string(), cancel2, handle2)
            .await;

        assert_eq!(manager.running_accounts().await.len(), 1);

        // Cleanup
        manager.stop_all().await;
    }
}
