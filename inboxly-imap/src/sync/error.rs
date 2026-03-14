use thiserror::Error;

/// Errors specific to the IMAP sync engine.
#[derive(Debug, Error)]
pub enum SyncError {
    #[error("IMAP error: {0}")]
    Imap(#[from] async_imap::error::Error),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("UIDVALIDITY changed: folder={folder}, old={old}, new={new}")]
    UidValidityChanged { folder: String, old: u32, new: u32 },

    #[error("mailbox SELECT returned no UIDVALIDITY for folder: {0}")]
    MissingUidValidity(String),

    #[error("mailbox SELECT returned no UIDNEXT for folder: {0}")]
    MissingUidNext(String),

    #[error("connection lost during sync of folder {folder}: {source}")]
    ConnectionLost {
        folder: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("envelope missing required field: {field} for UID {uid}")]
    MalformedEnvelope { uid: u32, field: String },

    #[error("date parse error for UID {uid}: {raw}")]
    DateParse { uid: u32, raw: String },
}

pub type SyncResult<T> = Result<T, SyncError>;
