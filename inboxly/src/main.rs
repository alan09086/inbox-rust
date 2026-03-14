//! Inboxly — main binary entry point.
//!
//! Currently a development stub that demonstrates the throttle scheduler
//! wiring. The full UI launch (via `inboxly-ui`) is planned for M15.

use inboxly_bundler::{ThrottleSchedulerConfig, spawn_throttle_scheduler};
use inboxly_core::BundleId;

fn main() {
    println!("Inboxly v0.14.0 — starting...");

    // Build a tokio runtime for the scheduler demo.
    // M15 will replace this with the Iced application event loop.
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");

    rt.block_on(async {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();

        // Demo query function — in production this will query the Store
        // via spawn_blocking or a DB thread channel.
        let query_fn = || async {
            // No-op: returns empty suppressed list (no bundles suppressed)
            Ok::<Vec<BundleId>, String>(vec![])
        };

        let _scheduler_handle = spawn_throttle_scheduler(
            ThrottleSchedulerConfig::default(),
            query_fn,
            event_tx,
        );

        println!("Throttle scheduler started (checking every 60s)");
        println!("Press Ctrl+C to exit");

        // In production, this select! would also handle UI events, sync
        // events, and other application concerns.
        tokio::select! {
            event = event_rx.recv() => {
                if let Some(event) = event {
                    println!("Received throttle event: {event:?}");
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("Shutting down...");
            }
        }
    });
}
