//! Core types, traits, and error definitions for Inboxly.

pub mod attachment;
pub mod bundle;
pub mod config;
pub mod contact;
pub mod email;
pub mod error;
pub mod flags;
pub mod highlight;
pub mod id;
pub mod inbox;
pub mod offline;
pub mod thread;
pub mod traits;

// Re-exports for convenience
pub use attachment::{Attachment, AttachmentMeta};
pub use bundle::{Bundle, BundleCategory, BundleIcon, BundleThrottle, BundleVisibility, Color};
pub use config::{
    AccountConfig, AppConfig, AuthMethod, ConfigError, Paths, SnoozePresets, ThemePreference,
};
pub use contact::{
    AVATAR_COLOR_DEFAULT, AVATAR_PALETTE, AvatarColor, Contact, ParsedAddress,
    avatar_color_for_letter, avatar_color_index, parse_address, parse_address_list,
};
pub use email::{EmailContent, EmailMeta};
pub use error::{InboxlyError, Result};
pub use flags::EmailFlags;
pub use highlight::{Highlight, TripBundle};
pub use id::{AccountId, BundleId, EmailId, ThreadId};
pub use inbox::{InboxItem, SnoozeInfo, SnoozeUntil, ThreadState};
pub use offline::OfflineAction;
pub use thread::Thread;
pub use traits::{Bundler, Extractor, Store};
