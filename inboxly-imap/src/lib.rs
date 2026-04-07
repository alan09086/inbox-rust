//! # inboxly-imap
//!
//! IMAP sync engine for Inboxly. Handles:
//! - TLS connections (implicit TLS + STARTTLS)
//! - Authentication (password LOGIN, OAuth2 XOAUTH2 SASL)
//! - Capability detection (CONDSTORE, IDLE, SPECIAL-USE, etc.)
//! - Folder listing and well-known folder mapping
//! - Connection pooling with reconnect
//! - Channel types for UI communication
//! - Phase 1: Header sync (ENVELOPE + FLAGS)
//! - Phase 2: Background body download (RFC822 fetch + Maildir + tantivy)
//! - On-demand body fetch for immediate display
//! - Offline action queue replay on reconnect
//! - Incremental sync: UIDNEXT comparison, CONDSTORE flags, deletion detection
//! - Push sync: IDLE command with timeout, reconnect loop
//! - Sync lifecycle: per-account sync loops, multi-account management

pub mod auth;
pub mod body_fetch;
pub mod body_processor;
pub mod channel;
pub mod connection;
pub mod error;
pub mod folders;
pub mod idle;
pub mod incremental;
pub mod offline_replay;
pub mod on_demand;
pub mod phase2;
pub mod pool;
pub mod smtp;
pub mod sync;
pub mod sync_loop;
pub mod sync_manager;
pub mod tls;

// Convenience re-exports
pub use auth::{GmailOAuth2Config, OAuth2Token, PasswordCredentials, XOAuth2Credentials};
pub use auth::{SharedOAuth2, SharedOAuth2State};
pub use channel::{SyncEvent, UiCommand, create_sync_channels};
pub use connection::{ImapCapabilities, ImapConnection};
pub use error::{ImapError, Result};
pub use folders::{FolderRole, ImapFolder, WellKnownFolders};
pub use idle::{IdleEvent, IdleLoopConfig, IdleWakeup};
pub use incremental::{IncrementalSyncResult, NewUidCheckResult};
pub use pool::{ConnectionPool, PoolConfig};
pub use smtp::{SmtpError, SmtpSender};
pub use sync_manager::SyncManager;
pub use tls::build_tls_config;
