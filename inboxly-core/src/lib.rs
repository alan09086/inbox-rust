//! Core types, traits, and error definitions for Inboxly.

pub mod id;
pub mod contact;
pub mod attachment;
pub mod flags;
pub mod email;
pub mod thread;
pub mod bundle;
pub mod highlight;
pub mod inbox;
pub mod error;
pub mod traits;
pub mod config;

// Re-exports for convenience
pub use id::{AccountId, BundleId, EmailId, ThreadId};
pub use contact::Contact;
pub use attachment::{Attachment, AttachmentMeta};
pub use flags::EmailFlags;
pub use email::{EmailContent, EmailMeta};
pub use thread::Thread;
pub use bundle::{Bundle, BundleCategory, BundleIcon, BundleThrottle, BundleVisibility, Color};
pub use highlight::{Highlight, TripBundle};
pub use inbox::{InboxItem, SnoozeInfo, SnoozeUntil, ThreadState};
pub use error::{InboxlyError, Result};
pub use traits::{Bundler, Extractor, Store};
pub use config::{AccountConfig, AppConfig, AuthMethod, ConfigError, Paths, SnoozePresets, ThemePreference};
