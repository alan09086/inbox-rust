//! SMTP-specific error type.
//!
//! Kept separate from [`crate::error::ImapError`] because SMTP has its own
//! protocol concerns (response codes, DSN formatting, rejection classification)
//! that don't map cleanly onto IMAP error variants.

use thiserror::Error;

/// Errors that can occur during SMTP message sending.
#[derive(Debug, Error)]
pub enum SmtpError {
    /// Failed to establish the SMTP connection (DNS, TCP, TLS handshake).
    #[error("SMTP connection failed: {reason}")]
    ConnectionFailed {
        /// Human-readable reason for the failure.
        reason: String,
    },

    /// Authentication failed (bad credentials, expired token, unsupported mechanism).
    ///
    /// Transient — the retry loop may refresh the token and try again.
    #[error("SMTP authentication failed: {reason}")]
    AuthFailed {
        /// Human-readable reason for the failure.
        reason: String,
    },

    /// Server rejected the message (5xx response).
    ///
    /// Permanent — do NOT retry. Common codes:
    /// - 550: mailbox unavailable / invalid recipient
    /// - 552: message too large
    /// - 553: mailbox name not allowed
    /// - 554: transaction failed
    #[error("SMTP message rejected: code {code}, message: {message}")]
    Rejected {
        /// Numeric SMTP response code (5xx range).
        code: u16,
        /// Server-provided rejection message.
        message: String,
    },

    /// Failed to build the RFC 5322 message before sending.
    ///
    /// Permanent — retrying can't fix a malformed message.
    #[error("message build error: {reason}")]
    MessageBuildError {
        /// Human-readable reason for the failure.
        reason: String,
    },

    /// Network I/O error (connection reset, timeout mid-transaction).
    ///
    /// Transient — may succeed on retry.
    #[error("network error: {reason}")]
    NetworkError {
        /// Human-readable reason for the failure.
        reason: String,
    },
}

impl SmtpError {
    /// `true` if this error is permanent (no benefit to retrying).
    ///
    /// Used by [`crate::smtp::retry::should_retry`] — see `retry.rs`.
    #[must_use]
    pub fn is_permanent(&self) -> bool {
        matches!(
            self,
            SmtpError::Rejected { .. } | SmtpError::MessageBuildError { .. }
        )
    }
}
