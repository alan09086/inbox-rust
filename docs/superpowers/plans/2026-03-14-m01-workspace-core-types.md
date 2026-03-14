# M1: Workspace + Core Types — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Scaffold the Inboxly workspace and define all core types in `inboxly-core`.

**Architecture:** 8-crate Cargo workspace. `inboxly-core` is the foundation crate with zero internal dependencies — all other crates depend on it.

**Tech Stack:** Rust, serde, thiserror, chrono, uuid

---

## Task Overview

| # | Task | Est. |
|---|------|------|
| 1 | Create workspace `Cargo.toml` with all 8 members | 3 min |
| 2 | Scaffold all 8 crates with empty `lib.rs`/`main.rs` | 5 min |
| 3 | Set up `inboxly-core/Cargo.toml` with dependencies | 3 min |
| 4 | Define identity types (`AccountId`, `EmailId`, `ThreadId`, `BundleId`) | 5 min |
| 5 | Define `Contact`, `AttachmentMeta`, `Attachment`, `EmailFlags` | 5 min |
| 6 | Define `EmailMeta` and `EmailContent` | 5 min |
| 7 | Define `Thread` type | 3 min |
| 8 | Define `Bundle`, `BundleCategory`, `BundleVisibility`, `BundleThrottle`, `BundleIcon` | 5 min |
| 9 | Define `InboxItem`, `ThreadState`, `SnoozeInfo`, `SnoozeUntil` | 5 min |
| 10 | Define `Highlight` and `TripBundle` | 5 min |
| 11 | Define `InboxlyError` enum | 5 min |
| 12 | Define `Store`, `Bundler`, `Extractor` traits | 5 min |
| 13 | Wire up crate dependencies in each subcrate's `Cargo.toml` | 5 min |
| 14 | Integration test that imports all types | 5 min |

---

### Task 1: Create workspace Cargo.toml
**Files:**
- Create: `Cargo.toml`

- [ ] **Step 1: Create the workspace root `Cargo.toml`**

```toml
[workspace]
members = [
    "inboxly",
    "inboxly-core",
    "inboxly-imap",
    "inboxly-store",
    "inboxly-bundler",
    "inboxly-snooze",
    "inboxly-extract",
    "inboxly-ui",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "GPL-3.0-only"
repository = "https://codeberg.org/alan090/inbox-rust"

[workspace.dependencies]
# Shared across crates — centralized versions
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
tokio = { version = "1", features = ["full"] }

# Internal crates
inboxly-core = { path = "inboxly-core" }
inboxly-imap = { path = "inboxly-imap" }
inboxly-store = { path = "inboxly-store" }
inboxly-bundler = { path = "inboxly-bundler" }
inboxly-snooze = { path = "inboxly-snooze" }
inboxly-extract = { path = "inboxly-extract" }
inboxly-ui = { path = "inboxly-ui" }
```

- [ ] **Step 2: Verify with `cargo check --workspace` (will fail until crates exist — that's Task 2)**

---

### Task 2: Scaffold all 8 crates with empty lib.rs/main.rs
**Files:**
- Create: `inboxly-core/Cargo.toml`
- Create: `inboxly-core/src/lib.rs`
- Create: `inboxly-imap/Cargo.toml`
- Create: `inboxly-imap/src/lib.rs`
- Create: `inboxly-store/Cargo.toml`
- Create: `inboxly-store/src/lib.rs`
- Create: `inboxly-bundler/Cargo.toml`
- Create: `inboxly-bundler/src/lib.rs`
- Create: `inboxly-snooze/Cargo.toml`
- Create: `inboxly-snooze/src/lib.rs`
- Create: `inboxly-extract/Cargo.toml`
- Create: `inboxly-extract/src/lib.rs`
- Create: `inboxly-ui/Cargo.toml`
- Create: `inboxly-ui/src/lib.rs`
- Create: `inboxly/Cargo.toml`
- Create: `inboxly/src/main.rs`

- [ ] **Step 1: Create all crate directories**

```bash
mkdir -p inboxly-core/src inboxly-imap/src inboxly-store/src inboxly-bundler/src \
         inboxly-snooze/src inboxly-extract/src inboxly-ui/src inboxly/src
```

- [ ] **Step 2: Create empty lib.rs files for all library crates**

Each `src/lib.rs` (for `inboxly-core`, `inboxly-imap`, `inboxly-store`, `inboxly-bundler`, `inboxly-snooze`, `inboxly-extract`, `inboxly-ui`):

```rust
//! Inboxly — [crate description]
```

Descriptions:
- `inboxly-core`: `Core types, traits, and error definitions for Inboxly`
- `inboxly-imap`: `IMAP sync engine and OAuth2 authentication for Inboxly`
- `inboxly-store`: `Maildir, SQLite, and Tantivy storage layer for Inboxly`
- `inboxly-bundler`: `Email categorisation engine for Inboxly`
- `inboxly-snooze`: `Snooze scheduler and reminder system for Inboxly`
- `inboxly-extract`: `Smart extraction and highlight detection for Inboxly`
- `inboxly-ui`: `Iced-based desktop UI for Inboxly`

- [ ] **Step 3: Create `inboxly/src/main.rs` (binary crate)**

```rust
fn main() {
    println!("Inboxly — starting...");
}
```

- [ ] **Step 4: Create minimal `Cargo.toml` for each non-core crate**

Each subcrate `Cargo.toml` follows this template (shown for `inboxly-imap`):

```toml
[package]
name = "inboxly-imap"
version.workspace = true
edition.workspace = true
license.workspace = true
```

For the binary crate `inboxly/Cargo.toml`:

```toml
[package]
name = "inboxly"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "inboxly"
path = "src/main.rs"
```

- [ ] **Step 5: Run `cargo check --workspace` and confirm all 8 crates compile**

```bash
cargo check --workspace
```

---

### Task 3: Set up inboxly-core Cargo.toml with dependencies
**Files:**
- Modify: `inboxly-core/Cargo.toml`

- [ ] **Step 1: Add dependencies to `inboxly-core/Cargo.toml`**

```toml
[package]
name = "inboxly-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
chrono.workspace = true
uuid.workspace = true
```

- [ ] **Step 2: Run `cargo check -p inboxly-core` to confirm dependencies resolve**

```bash
cargo check -p inboxly-core
```

---

### Task 4: Define identity types
**Files:**
- Create: `inboxly-core/src/id.rs`
- Modify: `inboxly-core/src/lib.rs`

- [ ] **Step 1: Create `inboxly-core/src/id.rs` with identity types**

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::fmt;

/// Unique identifier for an email account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub Uuid);

impl AccountId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AccountId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Email identifier — corresponds to the Message-ID header.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EmailId(pub String);

impl EmailId {
    pub fn new(message_id: impl Into<String>) -> Self {
        Self(message_id.into())
    }
}

impl fmt::Display for EmailId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Locally generated thread identifier — groups emails by References/In-Reply-To.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadId(pub Uuid);

impl ThreadId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ThreadId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ThreadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a bundle (category grouping).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BundleId(pub Uuid);

impl BundleId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for BundleId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for BundleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_id_unique() {
        let a = AccountId::new();
        let b = AccountId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn email_id_from_string() {
        let id = EmailId::new("<abc@example.com>");
        assert_eq!(id.0, "<abc@example.com>");
    }

    #[test]
    fn thread_id_unique() {
        let a = ThreadId::new();
        let b = ThreadId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn bundle_id_unique() {
        let a = BundleId::new();
        let b = BundleId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn id_display() {
        let account = AccountId::new();
        let display = format!("{account}");
        assert!(!display.is_empty());

        let email = EmailId::new("test@example.com");
        assert_eq!(format!("{email}"), "test@example.com");
    }

    #[test]
    fn id_serde_roundtrip() {
        let id = AccountId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: AccountId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);

        let eid = EmailId::new("<test@mail.com>");
        let json = serde_json::to_string(&eid).unwrap();
        let back: EmailId = serde_json::from_str(&json).unwrap();
        assert_eq!(eid, back);
    }
}
```

- [ ] **Step 2: Register the module in `inboxly-core/src/lib.rs`**

```rust
//! Core types, traits, and error definitions for Inboxly.

pub mod id;

pub use id::{AccountId, BundleId, EmailId, ThreadId};
```

- [ ] **Step 3: Run `cargo test -p inboxly-core`**

```bash
cargo test -p inboxly-core
```

---

### Task 5: Define Contact, AttachmentMeta, Attachment, EmailFlags
**Files:**
- Create: `inboxly-core/src/contact.rs`
- Create: `inboxly-core/src/attachment.rs`
- Create: `inboxly-core/src/flags.rs`
- Modify: `inboxly-core/src/lib.rs`

- [ ] **Step 1: Create `inboxly-core/src/contact.rs`**

```rust
use serde::{Deserialize, Serialize};

/// An email address with optional display name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Contact {
    /// Display name (e.g., "Alan Gaudet"). May be empty.
    pub name: String,
    /// Email address (e.g., "alan@example.com").
    pub address: String,
}

impl Contact {
    pub fn new(name: impl Into<String>, address: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            address: address.into(),
        }
    }

    /// Returns the first letter of the display name (for avatar tiles),
    /// falling back to the first letter of the address.
    pub fn avatar_letter(&self) -> char {
        self.name
            .chars()
            .next()
            .or_else(|| self.address.chars().next())
            .unwrap_or('?')
            .to_ascii_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_avatar_letter_from_name() {
        let c = Contact::new("Sarah", "sarah@example.com");
        assert_eq!(c.avatar_letter(), 'S');
    }

    #[test]
    fn contact_avatar_letter_fallback_to_address() {
        let c = Contact::new("", "bob@example.com");
        assert_eq!(c.avatar_letter(), 'B');
    }

    #[test]
    fn contact_serde_roundtrip() {
        let c = Contact::new("Test User", "test@mail.com");
        let json = serde_json::to_string(&c).unwrap();
        let back: Contact = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}
```

- [ ] **Step 2: Create `inboxly-core/src/attachment.rs`**

```rust
use serde::{Deserialize, Serialize};

/// Lightweight attachment metadata — stored in SQLite, no content bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentMeta {
    /// Original filename (e.g., "invoice.pdf").
    pub filename: String,
    /// MIME type (e.g., "application/pdf").
    pub mime_type: String,
    /// Size in bytes.
    pub size_bytes: u64,
}

/// Full attachment including content bytes — loaded on demand from Maildir.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Metadata (name, MIME, size).
    pub meta: AttachmentMeta,
    /// Raw content bytes.
    pub content: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_meta_creation() {
        let meta = AttachmentMeta {
            filename: "report.pdf".into(),
            mime_type: "application/pdf".into(),
            size_bytes: 1024,
        };
        assert_eq!(meta.filename, "report.pdf");
        assert_eq!(meta.size_bytes, 1024);
    }

    #[test]
    fn attachment_with_content() {
        let att = Attachment {
            meta: AttachmentMeta {
                filename: "test.txt".into(),
                mime_type: "text/plain".into(),
                size_bytes: 5,
            },
            content: b"hello".to_vec(),
        };
        assert_eq!(att.content.len(), 5);
    }
}
```

- [ ] **Step 3: Create `inboxly-core/src/flags.rs`**

```rust
use serde::{Deserialize, Serialize};

/// Email status flags, matching IMAP flag semantics.
/// Stored as a bitmask in SQLite for efficient querying.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EmailFlags {
    /// Message has been read (\Seen).
    pub read: bool,
    /// Message is starred/flagged (\Flagged).
    pub starred: bool,
    /// Message has been replied to (\Answered).
    pub answered: bool,
    /// Message is a draft (\Draft).
    pub draft: bool,
}

impl EmailFlags {
    /// All flags unset (new unread message).
    pub fn new() -> Self {
        Self::default()
    }

    /// Convert to bitmask for SQLite storage.
    /// Bit 0 = read, Bit 1 = starred, Bit 2 = answered, Bit 3 = draft.
    pub fn to_bitmask(self) -> u32 {
        let mut mask = 0u32;
        if self.read {
            mask |= 1;
        }
        if self.starred {
            mask |= 2;
        }
        if self.answered {
            mask |= 4;
        }
        if self.draft {
            mask |= 8;
        }
        mask
    }

    /// Construct from SQLite bitmask.
    pub fn from_bitmask(mask: u32) -> Self {
        Self {
            read: mask & 1 != 0,
            starred: mask & 2 != 0,
            answered: mask & 4 != 0,
            draft: mask & 8 != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_flags_all_false() {
        let flags = EmailFlags::new();
        assert!(!flags.read);
        assert!(!flags.starred);
        assert!(!flags.answered);
        assert!(!flags.draft);
    }

    #[test]
    fn bitmask_roundtrip() {
        let flags = EmailFlags {
            read: true,
            starred: false,
            answered: true,
            draft: false,
        };
        let mask = flags.to_bitmask();
        assert_eq!(mask, 0b0101); // read=1, answered=4
        let back = EmailFlags::from_bitmask(mask);
        assert_eq!(flags, back);
    }

    #[test]
    fn bitmask_all_set() {
        let flags = EmailFlags {
            read: true,
            starred: true,
            answered: true,
            draft: true,
        };
        assert_eq!(flags.to_bitmask(), 0b1111);
    }

    #[test]
    fn bitmask_none_set() {
        let flags = EmailFlags::new();
        assert_eq!(flags.to_bitmask(), 0);
        assert_eq!(EmailFlags::from_bitmask(0), flags);
    }
}
```

- [ ] **Step 4: Register all three modules in `inboxly-core/src/lib.rs`**

Add to `lib.rs`:

```rust
pub mod contact;
pub mod attachment;
pub mod flags;

pub use contact::Contact;
pub use attachment::{Attachment, AttachmentMeta};
pub use flags::EmailFlags;
```

- [ ] **Step 5: Run `cargo test -p inboxly-core`**

```bash
cargo test -p inboxly-core
```

---

### Task 6: Define EmailMeta and EmailContent
**Files:**
- Create: `inboxly-core/src/email.rs`
- Modify: `inboxly-core/src/lib.rs`

- [ ] **Step 1: Create `inboxly-core/src/email.rs`**

```rust
use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::attachment::{Attachment, AttachmentMeta};
use crate::contact::Contact;
use crate::flags::EmailFlags;
use crate::id::{AccountId, EmailId, ThreadId};

/// Lightweight email metadata — lives in SQLite and memory.
/// Body content is loaded lazily from Maildir on demand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmailMeta {
    /// Message-ID header value.
    pub id: EmailId,
    /// Account this email belongs to.
    pub account_id: AccountId,
    /// Thread grouping (by References/In-Reply-To).
    pub thread_id: ThreadId,
    /// Sender.
    pub from: Contact,
    /// Recipients.
    pub to: Vec<Contact>,
    /// CC recipients.
    pub cc: Vec<Contact>,
    /// Subject line.
    pub subject: String,
    /// First ~200 chars of plaintext body.
    pub snippet: String,
    /// Date sent/received.
    pub date: DateTime<Utc>,
    /// Canonical path on disk (Maildir location).
    pub maildir_path: PathBuf,
    /// Attachment metadata (name, MIME, size — no content).
    pub attachments: Vec<AttachmentMeta>,
    /// IMAP flags (read, starred, answered, draft).
    pub flags: EmailFlags,
    /// Raw message size in bytes.
    pub size_bytes: u64,
    /// IMAP UID for sync tracking (scoped to account_id + folder).
    pub imap_uid: u32,
    /// IMAP folder this UID belongs to (e.g., "INBOX", "Sent").
    pub imap_folder: String,
}

/// Full email content — loaded on demand when user opens a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmailContent {
    /// Message-ID (links to EmailMeta).
    pub id: EmailId,
    /// Plaintext body (if available).
    pub body_text: Option<String>,
    /// HTML body (if available).
    pub body_html: Option<String>,
    /// All email headers.
    pub headers: HashMap<String, String>,
    /// Full attachments including content bytes.
    pub attachments: Vec<Attachment>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_email_meta() -> EmailMeta {
        EmailMeta {
            id: EmailId::new("<test@example.com>"),
            account_id: AccountId::new(),
            thread_id: ThreadId::new(),
            from: Contact::new("Alice", "alice@example.com"),
            to: vec![Contact::new("Bob", "bob@example.com")],
            cc: vec![],
            subject: "Hello World".into(),
            snippet: "This is the beginning of the email...".into(),
            date: Utc::now(),
            maildir_path: PathBuf::from("/home/user/.mail/cur/test:2,S"),
            attachments: vec![],
            flags: EmailFlags::new(),
            size_bytes: 4096,
            imap_uid: 42,
            imap_folder: "INBOX".into(),
        }
    }

    #[test]
    fn email_meta_creation() {
        let meta = sample_email_meta();
        assert_eq!(meta.subject, "Hello World");
        assert_eq!(meta.imap_folder, "INBOX");
        assert!(!meta.flags.read);
    }

    #[test]
    fn email_content_creation() {
        let content = EmailContent {
            id: EmailId::new("<test@example.com>"),
            body_text: Some("Plain text body".into()),
            body_html: Some("<p>HTML body</p>".into()),
            headers: HashMap::from([
                ("From".into(), "alice@example.com".into()),
                ("To".into(), "bob@example.com".into()),
            ]),
            attachments: vec![],
        };
        assert!(content.body_text.is_some());
        assert_eq!(content.headers.len(), 2);
    }

    #[test]
    fn email_meta_serde_roundtrip() {
        let meta = sample_email_meta();
        let json = serde_json::to_string(&meta).unwrap();
        let back: EmailMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(meta.id, back.id);
        assert_eq!(meta.subject, back.subject);
    }
}
```

- [ ] **Step 2: Register the module in `inboxly-core/src/lib.rs`**

Add:

```rust
pub mod email;

pub use email::{EmailContent, EmailMeta};
```

- [ ] **Step 3: Run `cargo test -p inboxly-core`**

```bash
cargo test -p inboxly-core
```

---

### Task 7: Define Thread type
**Files:**
- Create: `inboxly-core/src/thread.rs`
- Modify: `inboxly-core/src/lib.rs`

- [ ] **Step 1: Create `inboxly-core/src/thread.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::contact::Contact;
use crate::id::{AccountId, EmailId, ThreadId};

/// A conversation thread grouping related emails.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Thread {
    /// Locally generated thread ID.
    pub id: ThreadId,
    /// Account this thread belongs to.
    pub account_id: AccountId,
    /// Subject line (from the original/first email).
    pub subject: String,
    /// All participants across all emails in the thread.
    pub participants: Vec<Contact>,
    /// Email IDs in this thread, ordered by date (oldest first).
    pub emails: Vec<EmailId>,
    /// Timestamp of the newest email.
    pub newest_date: DateTime<Utc>,
    /// Timestamp of the oldest email.
    pub oldest_date: DateTime<Utc>,
    /// Count of unread emails in this thread.
    pub unread_count: u32,
    /// Whether any email in the thread has attachments.
    pub has_attachments: bool,
    /// Snippet from the newest email.
    pub snippet: String,
}

impl Thread {
    /// Number of emails in this thread.
    pub fn email_count(&self) -> usize {
        self.emails.len()
    }

    /// Whether this thread has any unread emails.
    pub fn has_unread(&self) -> bool {
        self.unread_count > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_thread() -> Thread {
        let now = Utc::now();
        Thread {
            id: ThreadId::new(),
            account_id: AccountId::new(),
            subject: "Project Discussion".into(),
            participants: vec![
                Contact::new("Alice", "alice@example.com"),
                Contact::new("Bob", "bob@example.com"),
            ],
            emails: vec![
                EmailId::new("<msg1@example.com>"),
                EmailId::new("<msg2@example.com>"),
                EmailId::new("<msg3@example.com>"),
            ],
            newest_date: now,
            oldest_date: now - chrono::Duration::hours(2),
            unread_count: 1,
            has_attachments: false,
            snippet: "Latest reply in the thread...".into(),
        }
    }

    #[test]
    fn thread_email_count() {
        let t = sample_thread();
        assert_eq!(t.email_count(), 3);
    }

    #[test]
    fn thread_has_unread() {
        let mut t = sample_thread();
        assert!(t.has_unread());
        t.unread_count = 0;
        assert!(!t.has_unread());
    }

    #[test]
    fn thread_serde_roundtrip() {
        let t = sample_thread();
        let json = serde_json::to_string(&t).unwrap();
        let back: Thread = serde_json::from_str(&json).unwrap();
        assert_eq!(t.id, back.id);
        assert_eq!(t.subject, back.subject);
        assert_eq!(t.email_count(), back.email_count());
    }
}
```

- [ ] **Step 2: Register the module in `inboxly-core/src/lib.rs`**

Add:

```rust
pub mod thread;

pub use thread::Thread;
```

- [ ] **Step 3: Run `cargo test -p inboxly-core`**

```bash
cargo test -p inboxly-core
```

---

### Task 8: Define Bundle, BundleCategory, BundleVisibility, BundleThrottle, BundleIcon
**Files:**
- Create: `inboxly-core/src/bundle.rs`
- Modify: `inboxly-core/src/lib.rs`

- [ ] **Step 1: Create `inboxly-core/src/bundle.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::{BundleId, ThreadId};

/// Bundle category — determines default colour, icon, and heuristic rules.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BundleCategory {
    Social,
    Promos,
    Updates,
    Finance,
    Purchases,
    Travel,
    Forums,
    LowPriority,
    /// User-saved items (pinned/kept).
    Saved,
    /// User-defined custom category.
    Custom(String),
}

impl BundleCategory {
    /// Human-readable label for display.
    pub fn label(&self) -> &str {
        match self {
            Self::Social => "Social",
            Self::Promos => "Promos",
            Self::Updates => "Updates",
            Self::Finance => "Finance",
            Self::Purchases => "Purchases",
            Self::Travel => "Travel",
            Self::Forums => "Forums",
            Self::LowPriority => "Low Priority",
            Self::Saved => "Saved",
            Self::Custom(name) => name.as_str(),
        }
    }
}

/// Controls how a bundle appears in the inbox feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BundleVisibility {
    /// Emails grouped and shown as collapsed bundle in inbox.
    Bundled,
    /// Emails shown individually in inbox (not grouped).
    Unbundled,
    /// Emails skip the inbox entirely (only in bundle view).
    SkipInbox,
}

/// Controls delivery frequency for bundle notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BundleThrottle {
    /// Emails appear as they arrive.
    Immediate,
    /// Bundle surfaces once per day at configured time.
    Daily,
    /// Bundle surfaces once per week.
    Weekly,
}

/// Icon identifier for bundle display.
/// Uses named icons rather than embedding actual icon data.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BundleIcon {
    Social,
    Promos,
    Updates,
    Finance,
    Purchases,
    Travel,
    Forums,
    LowPriority,
    Saved,
    /// Custom icon name for user-defined bundles.
    Custom(String),
}

/// RGBA colour representation for bundle title and badge colours.
/// Stored as `[r, g, b, a]` with each component in 0.0..=1.0 range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Create a colour from RGB hex (e.g., 0xd23f31).
    pub fn from_rgb_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as f32 / 255.0,
            g: ((hex >> 8) & 0xFF) as f32 / 255.0,
            b: (hex & 0xFF) as f32 / 255.0,
            a: 1.0,
        }
    }
}

/// A group of related threads displayed as a collapsible unit in the inbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bundle {
    /// Unique bundle identifier.
    pub id: BundleId,
    /// Category (Social, Promos, etc.).
    pub category: BundleCategory,
    /// Display name.
    pub name: String,
    /// Title colour from BigTop palette.
    pub color: Color,
    /// Pastel badge background colour.
    pub badge_color: Color,
    /// Icon for bundle row.
    pub icon: BundleIcon,
    /// Thread IDs contained in this bundle.
    pub threads: Vec<ThreadId>,
    /// Number of unread threads.
    pub unread_count: u32,
    /// Timestamp of the newest thread in the bundle.
    pub newest_date: DateTime<Utc>,
    /// How this bundle appears in the inbox.
    pub visibility: BundleVisibility,
    /// Delivery frequency.
    pub throttle: BundleThrottle,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_category_labels() {
        assert_eq!(BundleCategory::Social.label(), "Social");
        assert_eq!(BundleCategory::LowPriority.label(), "Low Priority");
        assert_eq!(
            BundleCategory::Custom("Work".into()).label(),
            "Work"
        );
    }

    #[test]
    fn color_from_hex() {
        let red = Color::from_rgb_hex(0xFF0000);
        assert!((red.r - 1.0).abs() < f32::EPSILON);
        assert!(red.g.abs() < f32::EPSILON);
        assert!(red.b.abs() < f32::EPSILON);

        let social = Color::from_rgb_hex(0xd23f31);
        assert!((social.r - 0.8235294).abs() < 0.001);
    }

    #[test]
    fn bundle_creation() {
        let bundle = Bundle {
            id: BundleId::new(),
            category: BundleCategory::Social,
            name: "Social".into(),
            color: Color::from_rgb_hex(0xd23f31),
            badge_color: Color::from_rgb_hex(0xfaebea),
            icon: BundleIcon::Social,
            threads: vec![ThreadId::new(), ThreadId::new()],
            unread_count: 2,
            newest_date: Utc::now(),
            visibility: BundleVisibility::Bundled,
            throttle: BundleThrottle::Immediate,
        };
        assert_eq!(bundle.threads.len(), 2);
        assert_eq!(bundle.category.label(), "Social");
    }

    #[test]
    fn bundle_serde_roundtrip() {
        let bundle = Bundle {
            id: BundleId::new(),
            category: BundleCategory::Purchases,
            name: "Purchases".into(),
            color: Color::from_rgb_hex(0x6d4c41),
            badge_color: Color::from_rgb_hex(0xf0edec),
            icon: BundleIcon::Purchases,
            threads: vec![],
            unread_count: 0,
            newest_date: Utc::now(),
            visibility: BundleVisibility::Bundled,
            throttle: BundleThrottle::Daily,
        };
        let json = serde_json::to_string(&bundle).unwrap();
        let back: Bundle = serde_json::from_str(&json).unwrap();
        assert_eq!(bundle.id, back.id);
        assert_eq!(bundle.category, back.category);
    }
}
```

- [ ] **Step 2: Register the module in `inboxly-core/src/lib.rs`**

Add:

```rust
pub mod bundle;

pub use bundle::{Bundle, BundleCategory, BundleIcon, BundleThrottle, BundleVisibility, Color};
```

- [ ] **Step 3: Run `cargo test -p inboxly-core`**

```bash
cargo test -p inboxly-core
```

---

### Task 9: Define InboxItem, ThreadState, SnoozeInfo, SnoozeUntil
**Files:**
- Create: `inboxly-core/src/inbox.rs`
- Modify: `inboxly-core/src/lib.rs`

- [ ] **Step 1: Create `inboxly-core/src/inbox.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::bundle::Bundle;
use crate::highlight::TripBundle;
use crate::id::{BundleId, ThreadId};
use crate::thread::Thread;

/// A single item in the unified inbox feed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InboxItem {
    /// A conversation thread (possibly with highlight cards).
    Thread(Thread),
    /// A collapsed bundle grouping multiple threads.
    Bundle(Bundle),
    /// A user-created reminder (non-email task).
    Reminder {
        id: Uuid,
        title: String,
        due: DateTime<Utc>,
        done: bool,
    },
    /// Auto-grouped travel itinerary.
    TripBundle(TripBundle),
}

/// Per-thread state that lives in SQLite (local-only, not synced to IMAP).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadState {
    /// The thread this state applies to.
    pub thread_id: ThreadId,
    /// Pinned threads stay at the top and survive sweep.
    pub pinned: bool,
    /// Done (archived) — removed from inbox feed.
    pub done: bool,
    /// Snooze info, if this thread is snoozed.
    pub snoozed: Option<SnoozeInfo>,
    /// Bundle assignment, if categorised.
    pub bundle_id: Option<BundleId>,
    /// Extracted highlights for this thread.
    pub highlights: Vec<crate::highlight::Highlight>,
}

/// Information about a snoozed item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnoozeInfo {
    /// When (or where) to un-snooze.
    pub until: SnoozeUntil,
    /// Original inbox date (for restoring position context).
    pub original_date: DateTime<Utc>,
}

/// Snooze trigger — time-based or location-based.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SnoozeUntil {
    /// Un-snooze at a specific time.
    Time(DateTime<Utc>),
    /// Un-snooze when device enters a geofence.
    Location {
        lat: f64,
        lng: f64,
        radius_m: f64,
        label: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::AccountId;

    #[test]
    fn snooze_until_time() {
        let snooze = SnoozeUntil::Time(Utc::now() + chrono::Duration::hours(4));
        match &snooze {
            SnoozeUntil::Time(t) => assert!(*t > Utc::now()),
            _ => panic!("expected Time variant"),
        }
    }

    #[test]
    fn snooze_until_location() {
        let snooze = SnoozeUntil::Location {
            lat: 43.6532,
            lng: -79.3832,
            radius_m: 500.0,
            label: "Office".into(),
        };
        match &snooze {
            SnoozeUntil::Location { label, .. } => assert_eq!(label, "Office"),
            _ => panic!("expected Location variant"),
        }
    }

    #[test]
    fn thread_state_default_values() {
        let state = ThreadState {
            thread_id: ThreadId::new(),
            pinned: false,
            done: false,
            snoozed: None,
            bundle_id: None,
            highlights: vec![],
        };
        assert!(!state.pinned);
        assert!(!state.done);
        assert!(state.snoozed.is_none());
    }

    #[test]
    fn inbox_item_reminder() {
        let item = InboxItem::Reminder {
            id: Uuid::new_v4(),
            title: "Buy groceries".into(),
            due: Utc::now() + chrono::Duration::hours(2),
            done: false,
        };
        match item {
            InboxItem::Reminder { title, done, .. } => {
                assert_eq!(title, "Buy groceries");
                assert!(!done);
            }
            _ => panic!("expected Reminder variant"),
        }
    }

    #[test]
    fn snooze_info_serde_roundtrip() {
        let info = SnoozeInfo {
            until: SnoozeUntil::Time(Utc::now()),
            original_date: Utc::now(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: SnoozeInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, back);
    }
}
```

- [ ] **Step 2: Register the module in `inboxly-core/src/lib.rs`**

Add:

```rust
pub mod inbox;

pub use inbox::{InboxItem, SnoozeInfo, SnoozeUntil, ThreadState};
```

**Note:** This module depends on `highlight::Highlight` and `highlight::TripBundle` — Task 10 must be completed first, OR the modules can be created in the same step. The recommended approach is to implement Task 10 immediately before this task's `cargo check`, or stub `pub mod highlight;` with empty types first and fill in Task 10.

- [ ] **Step 3: Run `cargo test -p inboxly-core` (after Task 10)**

```bash
cargo test -p inboxly-core
```

---

### Task 10: Define Highlight and TripBundle
**Files:**
- Create: `inboxly-core/src/highlight.rs`
- Modify: `inboxly-core/src/lib.rs`

**Important:** This task MUST be implemented before or alongside Task 9, as `inbox.rs` references `Highlight` and `TripBundle`.

- [ ] **Step 1: Create `inboxly-core/src/highlight.rs`**

```rust
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ThreadId;

/// Smart extraction results from email body analysis.
/// Each variant represents a different type of actionable information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Highlight {
    /// Package tracking number with carrier info.
    TrackingNumber {
        carrier: String,
        number: String,
        url: Option<String>,
    },
    /// Flight reservation details.
    Flight {
        airline: String,
        number: String,
        depart: DateTime<Utc>,
        arrive: DateTime<Utc>,
        gate: Option<String>,
    },
    /// Hotel reservation details.
    Hotel {
        name: String,
        checkin: NaiveDate,
        checkout: NaiveDate,
        confirmation: Option<String>,
    },
    /// Calendar event.
    Event {
        title: String,
        datetime: DateTime<Utc>,
        location: Option<String>,
    },
    /// Payment or financial transaction.
    Payment {
        amount: String,
        currency: String,
        from_or_to: String,
    },
}

impl Highlight {
    /// Returns the highlight type as a string for storage/display.
    pub fn highlight_type(&self) -> &'static str {
        match self {
            Self::TrackingNumber { .. } => "tracking",
            Self::Flight { .. } => "flight",
            Self::Hotel { .. } => "hotel",
            Self::Event { .. } => "event",
            Self::Payment { .. } => "payment",
        }
    }
}

/// Auto-grouped travel itinerary combining multiple travel highlights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TripBundle {
    /// Destination label (from flight arrival city or hotel location).
    pub destination: String,
    /// Trip start date.
    pub start_date: NaiveDate,
    /// Trip end date.
    pub end_date: NaiveDate,
    /// Thread IDs containing the travel-related emails.
    pub threads: Vec<ThreadId>,
    /// Individual highlights that make up this trip.
    pub highlights: Vec<Highlight>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_type_names() {
        let h = Highlight::TrackingNumber {
            carrier: "UPS".into(),
            number: "1Z999AA10123456784".into(),
            url: Some("https://ups.com/track".into()),
        };
        assert_eq!(h.highlight_type(), "tracking");

        let h = Highlight::Flight {
            airline: "Air Canada".into(),
            number: "AC 123".into(),
            depart: Utc::now(),
            arrive: Utc::now() + chrono::Duration::hours(5),
            gate: Some("B42".into()),
        };
        assert_eq!(h.highlight_type(), "flight");
    }

    #[test]
    fn highlight_serde_roundtrip() {
        let h = Highlight::Hotel {
            name: "Marriott Downtown".into(),
            checkin: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
            checkout: NaiveDate::from_ymd_opt(2026, 6, 18).unwrap(),
            confirmation: Some("ABC123".into()),
        };
        let json = serde_json::to_string(&h).unwrap();
        let back: Highlight = serde_json::from_str(&json).unwrap();
        assert_eq!(h, back);
    }

    #[test]
    fn trip_bundle_creation() {
        let trip = TripBundle {
            destination: "Toronto".into(),
            start_date: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 6, 20).unwrap(),
            threads: vec![ThreadId::new(), ThreadId::new()],
            highlights: vec![
                Highlight::Flight {
                    airline: "WestJet".into(),
                    number: "WS 456".into(),
                    depart: Utc::now(),
                    arrive: Utc::now() + chrono::Duration::hours(4),
                    gate: None,
                },
                Highlight::Hotel {
                    name: "Hilton".into(),
                    checkin: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
                    checkout: NaiveDate::from_ymd_opt(2026, 6, 20).unwrap(),
                    confirmation: Some("XYZ789".into()),
                },
            ],
        };
        assert_eq!(trip.destination, "Toronto");
        assert_eq!(trip.highlights.len(), 2);
        assert_eq!(trip.threads.len(), 2);
    }

    #[test]
    fn payment_highlight() {
        let h = Highlight::Payment {
            amount: "42.99".into(),
            currency: "CAD".into(),
            from_or_to: "Amazon.ca".into(),
        };
        assert_eq!(h.highlight_type(), "payment");
    }

    #[test]
    fn event_highlight() {
        let h = Highlight::Event {
            title: "Team Standup".into(),
            datetime: Utc::now(),
            location: Some("Room 301".into()),
        };
        assert_eq!(h.highlight_type(), "event");
    }
}
```

- [ ] **Step 2: Register the module in `inboxly-core/src/lib.rs`**

Add:

```rust
pub mod highlight;

pub use highlight::{Highlight, TripBundle};
```

- [ ] **Step 3: Run `cargo test -p inboxly-core`**

```bash
cargo test -p inboxly-core
```

---

### Task 11: Define InboxlyError enum
**Files:**
- Create: `inboxly-core/src/error.rs`
- Modify: `inboxly-core/src/lib.rs`

- [ ] **Step 1: Create `inboxly-core/src/error.rs`**

```rust
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
        assert_eq!(
            err.to_string(),
            "database error: connection pool exhausted"
        );

        let err = InboxlyError::EmailNotFound(EmailId::new("<missing@mail.com>"));
        assert_eq!(
            err.to_string(),
            "email not found: <missing@mail.com>"
        );
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
```

- [ ] **Step 2: Register the module in `inboxly-core/src/lib.rs`**

Add:

```rust
pub mod error;

pub use error::{InboxlyError, Result};
```

- [ ] **Step 3: Run `cargo test -p inboxly-core`**

```bash
cargo test -p inboxly-core
```

---

### Task 12: Define Store, Bundler, Extractor traits
**Files:**
- Create: `inboxly-core/src/traits.rs`
- Modify: `inboxly-core/src/lib.rs`

- [ ] **Step 1: Create `inboxly-core/src/traits.rs`**

```rust
use crate::bundle::{Bundle, BundleCategory};
use crate::email::{EmailContent, EmailMeta};
use crate::error::Result;
use crate::highlight::Highlight;
use crate::id::{AccountId, BundleId, EmailId, ThreadId};
use crate::inbox::ThreadState;
use crate::thread::Thread;

/// Storage interface — abstracts over SQLite + Maildir + Tantivy.
/// Implemented by `inboxly-store`. All methods are async for database I/O.
pub trait Store: Send + Sync {
    // --- Email operations ---

    /// Insert or update email metadata.
    fn upsert_email_meta(
        &self,
        meta: &EmailMeta,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Retrieve email metadata by ID.
    fn get_email_meta(
        &self,
        id: &EmailId,
    ) -> impl std::future::Future<Output = Result<Option<EmailMeta>>> + Send;

    /// Load full email content from Maildir.
    fn get_email_content(
        &self,
        id: &EmailId,
    ) -> impl std::future::Future<Output = Result<Option<EmailContent>>> + Send;

    /// List email metadata for a thread, ordered by date.
    fn list_emails_for_thread(
        &self,
        thread_id: &ThreadId,
    ) -> impl std::future::Future<Output = Result<Vec<EmailMeta>>> + Send;

    // --- Thread operations ---

    /// Insert or update a thread.
    fn upsert_thread(
        &self,
        thread: &Thread,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Retrieve a thread by ID.
    fn get_thread(
        &self,
        id: &ThreadId,
    ) -> impl std::future::Future<Output = Result<Option<Thread>>> + Send;

    /// List threads for an account, ordered by newest_date descending.
    fn list_threads(
        &self,
        account_id: &AccountId,
        limit: u32,
        offset: u32,
    ) -> impl std::future::Future<Output = Result<Vec<Thread>>> + Send;

    // --- Thread state operations ---

    /// Get or create thread state.
    fn get_thread_state(
        &self,
        thread_id: &ThreadId,
    ) -> impl std::future::Future<Output = Result<ThreadState>> + Send;

    /// Update thread state (pin, done, snooze, bundle assignment).
    fn update_thread_state(
        &self,
        state: &ThreadState,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    // --- Bundle operations ---

    /// List all bundles.
    fn list_bundles(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<Bundle>>> + Send;

    /// Get a bundle by ID.
    fn get_bundle(
        &self,
        id: &BundleId,
    ) -> impl std::future::Future<Output = Result<Option<Bundle>>> + Send;

    /// Insert or update a bundle.
    fn upsert_bundle(
        &self,
        bundle: &Bundle,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// Email categorisation interface — assigns emails to bundles.
/// Implemented by `inboxly-bundler`.
pub trait Bundler: Send + Sync {
    /// Categorise an email and return the bundle category it belongs to.
    /// Returns `None` if the email doesn't match any rules (stays in primary inbox).
    fn categorise(
        &self,
        meta: &EmailMeta,
        content: Option<&EmailContent>,
    ) -> impl std::future::Future<Output = Result<Option<BundleCategory>>> + Send;

    /// Record a user's manual bundle assignment for sender learning.
    fn record_user_assignment(
        &self,
        meta: &EmailMeta,
        category: &BundleCategory,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// Smart extraction interface — detects highlights in email content.
/// Implemented by `inboxly-extract`.
pub trait Extractor: Send + Sync {
    /// Extract highlights from an email's content.
    /// Returns an empty vec if no actionable information is found.
    fn extract(
        &self,
        meta: &EmailMeta,
        content: &EmailContent,
    ) -> impl std::future::Future<Output = Result<Vec<Highlight>>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verify traits are object-safe enough to be used as bounds.
    // (They use RPITIT so they're not dyn-compatible, but they work as generic bounds.)

    fn _assert_store_bound<T: Store>(_t: &T) {}
    fn _assert_bundler_bound<T: Bundler>(_t: &T) {}
    fn _assert_extractor_bound<T: Extractor>(_t: &T) {}

    #[test]
    fn traits_module_compiles() {
        // This test verifies the traits module compiles successfully.
        // Actual implementations are in their respective crates.
        assert!(true);
    }
}
```

- [ ] **Step 2: Register the module in `inboxly-core/src/lib.rs`**

Add:

```rust
pub mod traits;

pub use traits::{Bundler, Extractor, Store};
```

- [ ] **Step 3: Run `cargo test -p inboxly-core` and `cargo check -p inboxly-core`**

```bash
cargo test -p inboxly-core && cargo check -p inboxly-core
```

---

### Task 13: Wire up crate dependencies in each subcrate's Cargo.toml
**Files:**
- Modify: `inboxly-imap/Cargo.toml`
- Modify: `inboxly-store/Cargo.toml`
- Modify: `inboxly-bundler/Cargo.toml`
- Modify: `inboxly-snooze/Cargo.toml`
- Modify: `inboxly-extract/Cargo.toml`
- Modify: `inboxly-ui/Cargo.toml`
- Modify: `inboxly/Cargo.toml`

- [ ] **Step 1: Update `inboxly-imap/Cargo.toml`**

```toml
[package]
name = "inboxly-imap"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
inboxly-core.workspace = true
```

- [ ] **Step 2: Update `inboxly-store/Cargo.toml`**

```toml
[package]
name = "inboxly-store"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
inboxly-core.workspace = true
```

- [ ] **Step 3: Update `inboxly-extract/Cargo.toml`**

```toml
[package]
name = "inboxly-extract"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
inboxly-core.workspace = true
```

- [ ] **Step 4: Update `inboxly-bundler/Cargo.toml`**

```toml
[package]
name = "inboxly-bundler"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
inboxly-core.workspace = true
inboxly-store.workspace = true
```

- [ ] **Step 5: Update `inboxly-snooze/Cargo.toml`**

```toml
[package]
name = "inboxly-snooze"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
inboxly-core.workspace = true
inboxly-store.workspace = true
```

- [ ] **Step 6: Update `inboxly-ui/Cargo.toml`**

```toml
[package]
name = "inboxly-ui"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
inboxly-core.workspace = true
inboxly-imap.workspace = true
inboxly-store.workspace = true
inboxly-bundler.workspace = true
inboxly-snooze.workspace = true
inboxly-extract.workspace = true
```

- [ ] **Step 7: Update `inboxly/Cargo.toml` (binary)**

```toml
[package]
name = "inboxly"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "inboxly"
path = "src/main.rs"

[dependencies]
inboxly-ui.workspace = true
```

- [ ] **Step 8: Run `cargo check --workspace` to verify all dependency wiring**

```bash
cargo check --workspace
```

---

### Task 14: Integration test that imports all types
**Files:**
- Create: `inboxly-core/tests/all_types.rs`

- [ ] **Step 1: Create the integration test file**

```rust
//! Integration test: verify all public types from inboxly-core compile
//! and can be instantiated together.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{NaiveDate, Utc};
use uuid::Uuid;

use inboxly_core::{
    // Identity types
    AccountId, BundleId, EmailId, ThreadId,
    // Contact and attachment
    Attachment, AttachmentMeta, Contact, EmailFlags,
    // Email types
    EmailContent, EmailMeta,
    // Thread
    Thread,
    // Bundle types
    Bundle, BundleCategory, BundleIcon, BundleThrottle, BundleVisibility, Color,
    // Inbox types
    InboxItem, SnoozeInfo, SnoozeUntil, ThreadState,
    // Highlight types
    Highlight, TripBundle,
    // Error types
    InboxlyError, Result,
    // Trait types (verify they're importable)
    Bundler, Extractor, Store,
};

#[test]
fn all_identity_types_constructable() {
    let _account = AccountId::new();
    let _email = EmailId::new("<test@example.com>");
    let _thread = ThreadId::new();
    let _bundle = BundleId::new();
}

#[test]
fn full_email_lifecycle() {
    let account_id = AccountId::new();
    let thread_id = ThreadId::new();
    let email_id = EmailId::new("<lifecycle@example.com>");

    // Create contact
    let sender = Contact::new("Alice", "alice@example.com");
    let recipient = Contact::new("Bob", "bob@example.com");

    // Create email metadata
    let meta = EmailMeta {
        id: email_id.clone(),
        account_id,
        thread_id,
        from: sender.clone(),
        to: vec![recipient.clone()],
        cc: vec![],
        subject: "Integration test email".into(),
        snippet: "This is a test...".into(),
        date: Utc::now(),
        maildir_path: PathBuf::from("/tmp/mail/cur/test:2,S"),
        attachments: vec![AttachmentMeta {
            filename: "doc.pdf".into(),
            mime_type: "application/pdf".into(),
            size_bytes: 2048,
        }],
        flags: EmailFlags {
            read: true,
            starred: false,
            answered: false,
            draft: false,
        },
        size_bytes: 8192,
        imap_uid: 100,
        imap_folder: "INBOX".into(),
    };

    assert_eq!(meta.subject, "Integration test email");
    assert!(meta.flags.read);

    // Create email content
    let content = EmailContent {
        id: email_id,
        body_text: Some("Plain text body".into()),
        body_html: Some("<p>HTML body</p>".into()),
        headers: HashMap::from([
            ("From".into(), "alice@example.com".into()),
            ("Subject".into(), "Integration test email".into()),
        ]),
        attachments: vec![Attachment {
            meta: meta.attachments[0].clone(),
            content: vec![0u8; 2048],
        }],
    };

    assert!(content.body_html.is_some());

    // Create thread
    let thread = Thread {
        id: thread_id,
        account_id,
        subject: meta.subject.clone(),
        participants: vec![sender, recipient],
        emails: vec![meta.id.clone()],
        newest_date: meta.date,
        oldest_date: meta.date,
        unread_count: 0,
        has_attachments: true,
        snippet: meta.snippet.clone(),
    };

    assert_eq!(thread.email_count(), 1);
    assert!(!thread.has_unread());
}

#[test]
fn full_bundle_lifecycle() {
    let bundle = Bundle {
        id: BundleId::new(),
        category: BundleCategory::Social,
        name: "Social".into(),
        color: Color::from_rgb_hex(0xd23f31),
        badge_color: Color::from_rgb_hex(0xfaebea),
        icon: BundleIcon::Social,
        threads: vec![ThreadId::new()],
        unread_count: 1,
        newest_date: Utc::now(),
        visibility: BundleVisibility::Bundled,
        throttle: BundleThrottle::Immediate,
    };

    assert_eq!(bundle.category.label(), "Social");

    // Wrap in InboxItem
    let item = InboxItem::Bundle(bundle);
    match &item {
        InboxItem::Bundle(b) => assert_eq!(b.name, "Social"),
        _ => panic!("expected Bundle"),
    }
}

#[test]
fn full_snooze_lifecycle() {
    let state = ThreadState {
        thread_id: ThreadId::new(),
        pinned: true,
        done: false,
        snoozed: Some(SnoozeInfo {
            until: SnoozeUntil::Time(Utc::now() + chrono::Duration::hours(4)),
            original_date: Utc::now(),
        }),
        bundle_id: Some(BundleId::new()),
        highlights: vec![
            Highlight::TrackingNumber {
                carrier: "UPS".into(),
                number: "1Z999".into(),
                url: None,
            },
        ],
    };

    assert!(state.pinned);
    assert!(state.snoozed.is_some());
    assert_eq!(state.highlights.len(), 1);
}

#[test]
fn trip_bundle_assembly() {
    let trip = TripBundle {
        destination: "Vancouver".into(),
        start_date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2026, 7, 5).unwrap(),
        threads: vec![ThreadId::new()],
        highlights: vec![
            Highlight::Flight {
                airline: "WestJet".into(),
                number: "WS 100".into(),
                depart: Utc::now(),
                arrive: Utc::now() + chrono::Duration::hours(5),
                gate: Some("C12".into()),
            },
            Highlight::Hotel {
                name: "Fairmont Pacific Rim".into(),
                checkin: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
                checkout: NaiveDate::from_ymd_opt(2026, 7, 5).unwrap(),
                confirmation: Some("FAIR-001".into()),
            },
        ],
    };

    let item = InboxItem::TripBundle(trip);
    match &item {
        InboxItem::TripBundle(t) => {
            assert_eq!(t.destination, "Vancouver");
            assert_eq!(t.highlights.len(), 2);
        }
        _ => panic!("expected TripBundle"),
    }
}

#[test]
fn error_types_usable() {
    fn failing_operation() -> Result<()> {
        Err(InboxlyError::EmailNotFound(EmailId::new("<missing@mail.com>")))
    }

    let err = failing_operation().unwrap_err();
    assert!(err.to_string().contains("email not found"));
}

#[test]
fn custom_bundle_category() {
    let custom = BundleCategory::Custom("Work Projects".into());
    assert_eq!(custom.label(), "Work Projects");
}

#[test]
fn email_flags_bitmask_storage() {
    let flags = EmailFlags {
        read: true,
        starred: true,
        answered: false,
        draft: false,
    };
    let mask = flags.to_bitmask();
    let restored = EmailFlags::from_bitmask(mask);
    assert_eq!(flags, restored);
}

#[test]
fn location_snooze() {
    let snooze = SnoozeUntil::Location {
        lat: 43.6532,
        lng: -79.3832,
        radius_m: 200.0,
        label: "Home".into(),
    };
    match &snooze {
        SnoozeUntil::Location { label, radius_m, .. } => {
            assert_eq!(label, "Home");
            assert!(*radius_m > 0.0);
        }
        _ => panic!("expected Location"),
    }
}
```

- [ ] **Step 2: Run the integration test**

```bash
cargo test -p inboxly-core --test all_types
```

- [ ] **Step 3: Run full workspace check to verify everything compiles**

```bash
cargo check --workspace && cargo test --workspace
```

- [ ] **Step 4: Run clippy across the workspace**

```bash
cargo clippy --workspace -- -D warnings
```

---

## Final lib.rs State

After all tasks, `inboxly-core/src/lib.rs` should look like this:

```rust
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
```

---

## Implementation Order

Tasks 1-3 must be sequential (workspace setup). Tasks 4-12 build on each other within `inboxly-core`:

```
Task 1 (workspace Cargo.toml)
  → Task 2 (scaffold crates)
    → Task 3 (core dependencies)
      → Task 4 (identity types)
        → Task 5 (contact, attachment, flags)
          → Task 6 (email types)
            → Task 7 (thread)
              → Task 8 (bundle types)
                → Task 10 (highlight, trip bundle)  ← BEFORE Task 9
                  → Task 9 (inbox item, thread state, snooze)
                    → Task 11 (error types)
                      → Task 12 (traits)
                        → Task 13 (wire dependencies)
                          → Task 14 (integration test)
```

**Critical ordering note:** Task 10 (Highlight, TripBundle) must come before Task 9 (InboxItem, ThreadState) because `inbox.rs` imports from `highlight.rs`.

---

## Commit Strategy

One commit per logical unit. Suggested commits:

1. **`feat: scaffold 8-crate workspace`** (Tasks 1-2)
2. **`feat(core): add identity types`** (Tasks 3-4)
3. **`feat(core): add contact, attachment, and email flag types`** (Task 5)
4. **`feat(core): add email metadata and content types`** (Task 6)
5. **`feat(core): add thread type`** (Task 7)
6. **`feat(core): add bundle and category types`** (Task 8)
7. **`feat(core): add highlight and trip bundle types`** (Task 10)
8. **`feat(core): add inbox item, thread state, and snooze types`** (Task 9)
9. **`feat(core): add error types`** (Task 11)
10. **`feat(core): add Store, Bundler, Extractor trait definitions`** (Task 12)
11. **`feat: wire inter-crate dependencies`** (Task 13)
12. **`test(core): add integration test importing all types`** (Task 14)

All commits must include `Co-Authored-By: Claude <noreply@anthropic.com>`.

---

## Verification Checklist

Before declaring M1 complete:

- [ ] `cargo check --workspace` passes with zero errors
- [ ] `cargo test --workspace` passes with zero failures
- [ ] `cargo clippy --workspace -- -D warnings` passes with zero warnings
- [ ] All 8 crates exist with correct `Cargo.toml` files
- [ ] All core types are defined and publicly exported from `inboxly-core`
- [ ] All identity types implement `Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize`
- [ ] All data types implement `Debug, Clone, PartialEq, Serialize, Deserialize`
- [ ] `InboxlyError` uses `thiserror` with meaningful messages
- [ ] `Store`, `Bundler`, `Extractor` traits compile with async methods
- [ ] Integration test imports and instantiates every public type
- [ ] No `inboxly-core` dependencies on other internal crates (zero internal deps)
- [ ] Dependency graph matches the design spec
