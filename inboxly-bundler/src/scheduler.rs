//! Background scheduler for throttle window checking.
//!
//! Runs a tokio task that periodically checks whether any bundle's throttle
//! window has opened. When it has, emits a [`ThrottleEvent::WindowOpened`]
//! event so the UI can refresh the inbox feed.
//!
//! ```text
//!   Scheduler loop (every check_interval_secs):
//!       |
//!       v
//!   query_fn() -> currently suppressed bundle IDs
//!       |
//!       v
//!   Compare with previous check
//!       |
//!       v
//!   Any bundle transitioned suppressed -> visible?
//!       |           |
//!      yes         no
//!       |           |
//!       v           v
//!   Emit WindowOpened   sleep until next tick
//! ```
//!
//! ## !Send constraint
//!
//! `rusqlite::Connection` (and thus `Store`) is `!Send`. The scheduler runs
//! as a tokio task which requires `Send`. To bridge this, the scheduler
//! accepts a `query_fn` callback that the caller implements to run the
//! database query on the appropriate thread and return the result.

use inboxly_core::BundleId;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

/// Events emitted by the throttle scheduler.
#[derive(Debug, Clone)]
pub enum ThrottleEvent {
    /// One or more bundle windows have opened. The UI should refresh
    /// the inbox feed. Contains the bundle IDs whose windows just opened.
    WindowOpened(Vec<BundleId>),
}

/// Configuration for the throttle scheduler.
#[derive(Debug, Clone)]
pub struct ThrottleSchedulerConfig {
    /// How often to check throttle windows, in seconds. Default: 60.
    pub check_interval_secs: u64,
}

impl Default for ThrottleSchedulerConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 60,
        }
    }
}

/// Spawn the throttle scheduler as a background tokio task.
///
/// Returns a `JoinHandle` for the task and expects an event sender.
///
/// The scheduler:
/// 1. Every `check_interval_secs`, calls `query_fn` to get currently suppressed bundles.
/// 2. Compares with the previous check's suppressed set.
/// 3. If any bundle transitioned from suppressed to not-suppressed, emits `WindowOpened`.
///
/// The `query_fn` parameter is an async function that returns the currently
/// suppressed bundle IDs. This avoids holding a `!Send` database connection
/// across await points. The caller is responsible for implementing this
/// function to query the database on the appropriate thread (e.g., using
/// `tokio::task::spawn_blocking` or a channel to a DB thread).
///
/// # Errors
///
/// The scheduler handles query errors internally (logs and continues).
/// The task terminates when the event channel is closed.
pub fn spawn_throttle_scheduler<F, Fut>(
    config: ThrottleSchedulerConfig,
    query_fn: F,
    event_tx: mpsc::UnboundedSender<ThrottleEvent>,
) -> tokio::task::JoinHandle<()>
where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = std::result::Result<Vec<BundleId>, String>> + Send,
{
    tokio::spawn(async move {
        // Enforce minimum 1ms interval (tokio panics on zero-duration intervals)
        let interval_duration = Duration::from_secs(config.check_interval_secs).max(Duration::from_millis(1));
        let mut tick = interval(interval_duration);
        let mut previously_suppressed: Vec<BundleId> = Vec::new();

        // Initial population
        match query_fn().await {
            Ok(suppressed) => {
                previously_suppressed = suppressed;
            }
            Err(e) => {
                tracing::warn!("throttle scheduler: initial query failed: {e}");
            }
        }

        loop {
            tick.tick().await;

            let currently_suppressed = match query_fn().await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        "throttle scheduler: failed to query suppressed bundles: {e}"
                    );
                    continue;
                }
            };

            // Find bundles that were suppressed before but are no longer suppressed
            let newly_opened: Vec<BundleId> = previously_suppressed
                .iter()
                .filter(|id| !currently_suppressed.contains(id))
                .copied()
                .collect();

            if !newly_opened.is_empty() {
                tracing::info!(
                    "throttle scheduler: {} bundle window(s) opened",
                    newly_opened.len(),
                );
                if event_tx
                    .send(ThrottleEvent::WindowOpened(newly_opened))
                    .is_err()
                {
                    tracing::debug!(
                        "throttle scheduler: event channel closed, shutting down"
                    );
                    break;
                }
            }

            previously_suppressed = currently_suppressed;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[test]
    fn default_config_is_60_seconds() {
        let config = ThrottleSchedulerConfig::default();
        assert_eq!(config.check_interval_secs, 60);
    }

    #[tokio::test]
    async fn scheduler_detects_window_opening() {

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // State: first call returns [bundle_a], second call returns [] (window opened)
        let bundle_a = BundleId::new();
        let call_count = Arc::new(Mutex::new(0u32));
        let bundle_a_clone = bundle_a;
        let call_count_clone = Arc::clone(&call_count);

        let query_fn = move || {
            let call_count = Arc::clone(&call_count_clone);
            let bundle_a = bundle_a_clone;
            async move {
                let mut count = call_count.lock().await;
                *count = count.saturating_add(1);
                if *count <= 1 {
                    // First call: bundle is suppressed
                    Ok(vec![bundle_a])
                } else {
                    // Second call: bundle window opened
                    Ok(vec![])
                }
            }
        };

        let config = ThrottleSchedulerConfig {
            check_interval_secs: 0, // immediate ticks for testing
        };

        let handle = spawn_throttle_scheduler(config, query_fn, event_tx);

        // Wait for the event
        let event = tokio::time::timeout(Duration::from_secs(2), event_rx.recv())
            .await
            .expect("timed out waiting for event")
            .expect("channel closed");

        match event {
            ThrottleEvent::WindowOpened(ids) => {
                assert_eq!(ids.len(), 1);
                assert_eq!(ids[0], bundle_a);
            }
        }

        handle.abort();
    }

    #[tokio::test]
    async fn scheduler_stops_when_channel_closed() {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Query returns a bundle suppressed first, then empty (triggers a send)
        let bundle = BundleId::new();
        let call_count = Arc::new(Mutex::new(0u32));
        let call_count_clone = Arc::clone(&call_count);

        let query_fn = move || {
            let call_count = Arc::clone(&call_count_clone);
            let b = bundle;
            async move {
                let mut count = call_count.lock().await;
                *count = count.saturating_add(1);
                if *count <= 1 {
                    Ok(vec![b])
                } else {
                    Ok(vec![]) // window opened -- triggers send
                }
            }
        };

        let config = ThrottleSchedulerConfig {
            check_interval_secs: 0,
        };

        // Drop receiver before scheduler tries to send
        drop(event_rx);

        let handle = spawn_throttle_scheduler(config, query_fn, event_tx);

        // Scheduler should stop within a reasonable time when send fails
        let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(result.is_ok(), "scheduler should have stopped");
    }
}
