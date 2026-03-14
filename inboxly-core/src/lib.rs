//! Core types, traits, and error definitions for Inboxly.

pub mod id;
pub mod contact;
pub mod attachment;
pub mod flags;

pub use id::{AccountId, BundleId, EmailId, ThreadId};
pub use contact::Contact;
pub use attachment::{Attachment, AttachmentMeta};
pub use flags::EmailFlags;

pub mod email;

pub use email::{EmailContent, EmailMeta};

pub mod thread;

pub use thread::Thread;

pub mod bundle;

pub use bundle::{Bundle, BundleCategory, BundleIcon, BundleThrottle, BundleVisibility, Color};

pub mod highlight;

pub use highlight::{Highlight, TripBundle};
