//! # IMAP Sync Engine — Phase 1 & 2
//!
//! ## Phase 1 (Header Sync)
//!
//! 1. SELECT the target folder to discover UIDVALIDITY and UIDNEXT
//! 2. Split the UID range into batches of 500, newest-first
//! 3. For each batch, issue `UID FETCH (ENVELOPE FLAGS RFC822.SIZE)`
//! 4. Parse IMAP ENVELOPE responses into `EnvelopeData` structs
//! 5. Batch-insert into the SQLite `emails` table
//! 6. Assign basic thread IDs using `In-Reply-To` headers
//! 7. Emit progress events to the UI via `tokio::sync::mpsc`
//! 8. Fire a first-batch-ready signal so the inbox is usable immediately
//! 9. Persist UIDVALIDITY + last-synced-UID for crash recovery
//!
//! ## Phase 2 (Body Download — M8)
//!
//! After Phase 1 completes, Phase 2 runs as a background tokio task:
//! 1. Query SQLite for `body_downloaded = false` UIDs (newest-first)
//! 2. Fetch RFC822 bodies in batches of 500
//! 3. Write each body to Maildir, extract text for search, update SQLite
//! 4. Emit progress events to the UI
//! 5. Resume capability: on restart, picks up where it left off
//!
//! Phase 2 entry point: [`crate::phase2::phase2_download`]
//! On-demand fetch: [`crate::on_demand::fetch_body_on_demand`]
//! Offline replay: [`crate::offline_replay::replay_offline_queue`]
//!
//! ## Usage
//!
//! ```rust,ignore
//! use inboxly_imap::sync::{SyncConfig, run_phase1_sync, sync_event_channel};
//! use std::sync::Arc;
//! use tokio::sync::Mutex;
//!
//! // After authenticating via M6:
//! let config = SyncConfig::new("account-uuid", "INBOX");
//! let (event_tx, mut event_rx) = sync_event_channel(256);
//! let conn = rusqlite::Connection::open_in_memory().unwrap();
//! let db = Arc::new(Mutex::new(conn));
//!
//! // Spawn progress consumer
//! tokio::spawn(async move {
//!     while let Some(event) = event_rx.recv().await {
//!         println!("{event:?}");
//!     }
//! });
//!
//! // Run sync (requires &mut Session from M6)
//! // let result = run_phase1_sync(&mut session, db, &config, event_tx).await?;
//! ```
//!
//! ## Crash Recovery
//!
//! After each batch, `last_synced_uid` is persisted to `sync_state`. On restart,
//! the engine reads this value and resumes from where it left off, skipping
//! already-fetched UID ranges.
//!
//! ## Threading
//!
//! Phase 1 uses a simplified threading algorithm based only on `In-Reply-To`
//! (since `References` is not available in ENVELOPE). Full threading with
//! `References` header parsing and placeholder resolution is implemented in M10.

pub mod batch;
pub mod engine;
pub mod envelope;
pub mod error;
pub mod progress;
pub mod store;
pub mod threading;
pub mod uid_state;

pub use engine::{SyncConfig, SyncPhase1Result, run_phase1_sync};
pub use error::{SyncError, SyncResult};
pub use progress::{
    SyncEvent, SyncEventReceiver, SyncEventSender, SyncProgress, sync_event_channel,
};
