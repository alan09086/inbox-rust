use std::io;

/// All errors that can occur in the IMAP crate.
#[derive(Debug, thiserror::Error)]
pub enum ImapError {
    #[error("TLS error: {0}")]
    Tls(#[from] rustls::Error),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("IMAP protocol error: {0}")]
    Imap(#[from] async_imap::error::Error),

    #[error("Authentication failed: {reason}")]
    AuthFailed { reason: String },

    #[error("OAuth2 error: {reason}")]
    OAuth2 { reason: String },

    #[error("OAuth2 token expired, refresh required")]
    TokenExpired,

    #[error("Connection lost: {reason}")]
    ConnectionLost { reason: String },

    #[error("STARTTLS not supported by server")]
    StarttlsUnsupported,

    #[error("Invalid server name: {0}")]
    InvalidServerName(String),

    #[error("Capability not supported: {0}")]
    CapabilityNotSupported(String),

    #[error("Folder not found: {0}")]
    FolderNotFound(String),

    #[error("Connection pool exhausted")]
    PoolExhausted,

    #[error("Channel closed")]
    ChannelClosed,

    #[error("Timeout after {0:?}")]
    Timeout(std::time::Duration),

    // -- Phase 2 (M8) error variants --
    #[error("Maildir write failed: {0}")]
    MaildirWrite(String),

    #[error("search index error: {0}")]
    IndexError(String),

    #[error("database error: {0}")]
    DatabaseError(String),

    #[error("email not found: {0}")]
    EmailNotFound(String),

    #[error("Maildir read failed: {0}")]
    MaildirRead(String),
}

pub type Result<T> = std::result::Result<T, ImapError>;
