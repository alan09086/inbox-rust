//! Trait abstraction for sending drafts.
//!
//! Allows the offline replay handler to call into SMTP without taking a
//! direct dependency on [`crate::smtp::SmtpSender`] — and lets tests
//! inject a mock that records calls without touching the network.
//!
//! Phase 12's send bridge will pass a real
//! [`crate::smtp::SmtpSender`] wrapped as a `&dyn DraftSender` into
//! [`crate::offline_replay::replay_offline_queue`]. Existing callers
//! (sync loop, IDLE reconnect) pass `None` and preserve the legacy
//! "log and skip" behaviour.

use async_trait::async_trait;

use inboxly_core::DraftEmail;

use crate::smtp::error::SmtpError;

/// Anything that can send a draft email.
///
/// Single-attempt — the caller (offline replay handler or Phase 12 send
/// bridge) owns retry decisions via
/// [`crate::smtp::retry::should_retry`].
#[async_trait]
pub trait DraftSender: Send + Sync {
    /// Send the given draft.
    ///
    /// # Errors
    ///
    /// Returns [`SmtpError`] if message construction, transport setup,
    /// or the send call fails. Permanent vs transient classification
    /// is exposed via [`SmtpError::is_permanent`].
    async fn send_draft(&self, draft: &DraftEmail) -> Result<(), SmtpError>;
}
