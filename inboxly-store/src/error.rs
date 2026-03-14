use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Database migration failed: expected version {expected}, found {found}")]
    MigrationFailed { expected: u32, found: u32 },

    #[error("Record not found: {0}")]
    NotFound(String),

    #[error("Constraint violation: {0}")]
    Constraint(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Maildir operation failed: {0}")]
    Maildir(String),

    #[error("Email parse error: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;
