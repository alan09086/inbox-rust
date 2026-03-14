//! SQLite storage backend for Inboxly.
//!
//! Owns the metadata database: emails, threads, accounts, bundles,
//! sync state, reminders, highlights, settings, and offline queue.

mod error;
mod migrations;
mod store;

pub mod maildir_store;

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

pub use error::{StoreError, Result};
pub use store::Store;

pub use maildir_store::{
    MaildirStore, StandardFolder, StoredEmail, MaildirEntry, ScanError,
    flags_to_suffix, suffix_to_flags, flags_from_filename,
    parse_email_meta, parse_email_content, rebuild_emails_from_maildir,
};

pub use accounts::AccountRow;
pub use bundles::BundleRow;
pub use bundle_rules::BundleRuleRow;
pub use contacts::ContactRow;
pub use emails::{EmailRow, flags};
pub use highlights::HighlightRow;
pub use offline_queue::OfflineQueueRow;
pub use reminders::ReminderRow;
pub use sender_affinity::SenderAffinityRow;
pub use sync_state::SyncStateRow;
pub use thread_state::ThreadStateRow;
pub use threads::ThreadRow;
