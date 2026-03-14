//! Phase 2 background body download orchestrator.
//!
//! Spawned as a tokio task after Phase 1 header sync completes.
//! Iterates through all emails with `body_downloaded = false` in batches,
//! fetching RFC822 bodies, writing to Maildir, indexing in tantivy,
//! and updating SQLite.
