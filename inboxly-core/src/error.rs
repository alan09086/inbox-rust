use std::path::PathBuf;

use thiserror::Error;

use crate::id::{AccountId, BundleId, EmailId, ThreadId};

/// Top-level error type for the Inboxly application.
/// Each crate maps its internal errors into these variants.
#[derive(Debug, Error)]
pub enum InboxlyError {
    // === Storage errors ===
    #[error("database error: {0}")]
    Database(String),

    #[error("maildir error at {path}: {message}")]
    Maildir { path: PathBuf, message: String },

    #[error("search index error: {0}")]
    SearchIndex(String),

    // === IMAP errors ===
    #[error("IMAP connection failed for account {account_id}: {message}")]
    ImapConnection {
        account_id: AccountId,
        message: String,
    },

    #[error("IMAP authentication failed for account {account_id}: {message}")]
    ImapAuth {
        account_id: AccountId,
        message: String,
    },

    #[error("IMAP sync error for account {account_id}: {message}")]
    ImapSync {
        account_id: AccountId,
        message: String,
    },

    #[error("SMTP error: {0}")]
    Smtp(String),

    #[error("OAuth2 error: {0}")]
    OAuth2(String),

    // === Entity not found ===
    #[error("email not found: {0}")]
    EmailNotFound(EmailId),

    #[error("thread not found: {0}")]
    ThreadNotFound(ThreadId),

    #[error("bundle not found: {0}")]
    BundleNotFound(BundleId),

    #[error("account not found: {0}")]
    AccountNotFound(AccountId),

    // === Bundler errors ===
    #[error("bundler rule error: {0}")]
    BundlerRule(String),

    // === Extract errors ===
    #[error("extraction error: {0}")]
    Extraction(String),

    #[error("email parse error: {0}")]
    EmailParse(String),

    // === Snooze errors ===
    #[error("snooze error: {0}")]
    Snooze(String),

    #[error("geolocation unavailable: {0}")]
    GeoLocation(String),

    // === Config errors ===
    #[error("configuration error: {0}")]
    Config(String),

    // === Generic ===
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

/// Convenience type alias for Inboxly results.
pub type Result<T> = std::result::Result<T, InboxlyError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        let err = InboxlyError::Database("connection pool exhausted".into());
        assert_eq!(err.to_string(), "database error: connection pool exhausted");

        let err = InboxlyError::EmailNotFound(EmailId::new("<missing@mail.com>"));
        assert_eq!(err.to_string(), "email not found: <missing@mail.com>");
    }

    #[test]
    fn error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: InboxlyError = io_err.into();
        assert!(err.to_string().contains("file missing"));
    }

    #[test]
    fn result_type_alias() {
        fn example() -> Result<u32> {
            Ok(42)
        }
        assert_eq!(example().unwrap(), 42);
    }

    #[test]
    fn imap_error_includes_account() {
        let err = InboxlyError::ImapConnection {
            account_id: AccountId::new(),
            message: "timeout".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("IMAP connection failed"));
        assert!(msg.contains("timeout"));
    }

    #[test]
    fn maildir_error_includes_path() {
        let err = InboxlyError::Maildir {
            path: PathBuf::from("/home/user/.mail/cur"),
            message: "permission denied".into(),
        };
        assert!(err.to_string().contains("/home/user/.mail/cur"));
        assert!(err.to_string().contains("permission denied"));
    }
}
