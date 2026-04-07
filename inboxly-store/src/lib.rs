//! SQLite storage backend for Inboxly.
//!
//! Owns the metadata database: emails, threads, accounts, bundles,
//! sync state, reminders, highlights, settings, and offline queue.

mod error;
mod migrations;
mod store;
mod trait_impls;

pub mod maildir_store;

mod accounts;
mod bundle_query;
mod bundle_rules;
mod bundles;
mod contacts;
mod emails;
mod highlights;
mod inbox_query;
mod offline_queue;
mod reminders;
mod sender_affinity;
mod settings;
mod sync_state;
mod thread_state;
pub mod thread_reader;
pub mod threading;
mod threads;
mod throttle;

pub use error::{Result, StoreError};
pub use store::Store;

pub use maildir_store::{
    MaildirEntry, MaildirStore, ScanError, StandardFolder, StoredEmail, flags_from_filename,
    flags_to_suffix, parse_email_content, parse_email_meta, parse_email_slim,
    rebuild_emails_from_maildir, suffix_to_flags,
};

pub mod search;

pub use accounts::AccountRow;
pub use bundle_query::{BundleSummary, SenderPreview};
pub use bundle_rules::BundleRuleRow;
pub use bundles::BundleRow;
pub use contacts::ContactRow;
pub use emails::{EmailRow, flags};
pub use highlights::HighlightRow;
pub use inbox_query::InboxThreadSummary;
pub use offline_queue::OfflineQueueRow;
pub use reminders::ReminderRow;
pub use sender_affinity::SenderAffinityRow;
pub use sync_state::SyncStateRow;
pub use thread_state::ThreadStateRow;
pub use threads::ThreadRow;
