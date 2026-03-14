//! SQLite storage backend for Inboxly.
//!
//! Owns the metadata database: emails, threads, accounts, bundles,
//! sync state, reminders, highlights, settings, and offline queue.

mod error;
mod migrations;
mod store;

// Table-specific CRUD modules
mod accounts;
mod bundles;
mod bundle_rules;
mod contacts;
mod emails;
mod highlights;
mod offline_queue;
mod reminders;
mod sender_affinity;
mod settings;
mod sync_state;
mod thread_state;
mod threads;

pub use error::StoreError;
pub use store::Store;
pub use accounts::AccountRow;
