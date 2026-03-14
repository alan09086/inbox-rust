//! # inboxly-imap
//!
//! IMAP sync engine for Inboxly. Handles:
//! - TLS connections (implicit TLS + STARTTLS)
//! - Authentication (password LOGIN, OAuth2 XOAUTH2 SASL)
//! - Capability detection (CONDSTORE, IDLE, SPECIAL-USE, etc.)
//! - Folder listing and well-known folder mapping
//! - Connection pooling with reconnect
//! - Channel types for UI communication

pub mod auth;
pub mod channel;
pub mod connection;
pub mod error;
pub mod folders;
pub mod pool;
pub mod tls;

// Convenience re-exports
pub use auth::{GmailOAuth2Config, OAuth2Token, PasswordCredentials, XOAuth2Credentials};
pub use channel::{create_sync_channels, SyncEvent, UiCommand};
pub use connection::{ImapCapabilities, ImapConnection};
pub use error::{ImapError, Result};
pub use folders::{FolderRole, ImapFolder, WellKnownFolders};
pub use pool::{ConnectionPool, PoolConfig};
pub use tls::build_tls_config;
