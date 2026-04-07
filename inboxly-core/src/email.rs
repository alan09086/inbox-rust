use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::attachment::{Attachment, AttachmentDraft, AttachmentMeta};
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

impl EmailMeta {
    /// Create a default `EmailMeta` for use in test fixtures.
    ///
    /// All fields are set to sensible defaults. Tests should override the
    /// fields they care about (e.g., `from`, `subject`).
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn test_default() -> Self {
        Self {
            id: EmailId::new(format!("<test-{}@example.com>", uuid::Uuid::new_v4())),
            account_id: AccountId::new(),
            thread_id: ThreadId::new(),
            from: Contact::new("", "test@example.com"),
            to: vec![],
            cc: vec![],
            subject: String::new(),
            snippet: String::new(),
            date: Utc::now(),
            maildir_path: PathBuf::from("/tmp/test"),
            attachments: vec![],
            flags: EmailFlags::new(),
            size_bytes: 0,
            imap_uid: 1,
            imap_folder: "INBOX".into(),
        }
    }
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

/// Slim view of email content — body text/HTML + attachment metadata only.
/// Used by the thread detail view where full headers and attachment byte
/// content aren't needed. Eng review Issue 2.6: avoids carrying 5-20 KB
/// of headers and potentially MB of attachment bytes through the loader
/// just to drop them at the rendering step.
///
/// When M37 adds attachment download, it will call a separate method
/// that loads the actual bytes for a single attachment on demand —
/// this type will NOT be extended to carry byte content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlimEmailContent {
    /// Message-ID (links to EmailMeta).
    pub id: EmailId,
    /// Plaintext body (if available).
    pub body_text: Option<String>,
    /// HTML body (if available).
    pub body_html: Option<String>,
    /// Attachment metadata only — no byte content.
    pub attachments: Vec<AttachmentMeta>,
}

/// Mode of an in-progress compose operation.
///
/// Set when the user opens compose. The variants tell the message builder
/// which RFC 5322 reply headers to populate (`In-Reply-To`, `References`).
///
/// `Reply`, `ReplyAll`, and `Forward` are placeholder variants for M36 — they
/// exist in the enum so the storage layer doesn't need a migration when M36
/// implements them, but dispatching them in M35 is a runtime error.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComposeMode {
    /// Brand-new compose, no reply context.
    #[default]
    New,
    /// Reply to one message in a thread (M36 — placeholder).
    Reply {
        /// Thread the reply belongs to.
        thread_id: ThreadId,
        /// Email being replied to.
        original_email_id: EmailId,
    },
    /// Reply-all to one message in a thread (M36 — placeholder).
    ReplyAll {
        /// Thread the reply belongs to.
        thread_id: ThreadId,
        /// Email being replied to.
        original_email_id: EmailId,
    },
    /// Forward one message (M36 — placeholder).
    Forward {
        /// Thread the forwarded email belongs to.
        thread_id: ThreadId,
        /// Email being forwarded.
        original_email_id: EmailId,
    },
}

/// An in-progress draft email being composed by the user.
///
/// Lives in the `drafts` SQLite table (Phase 2 migration), the local Maildir
/// `.Drafts/` folder, AND the IMAP server's `Drafts` folder. The three layers
/// are reconciled by `Message-ID` — the field below is the canonical identifier
/// across all three storage tiers.
///
/// Distinct from `EmailMeta`/`EmailContent`: those describe a fully-sent
/// message that arrived via IMAP. `DraftEmail` describes something still
/// being composed and not yet on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftEmail {
    /// Unique draft id (UUID v4 string).
    pub id: String,
    /// Account this draft will be sent FROM.
    pub account_id: AccountId,
    /// RFC 5322 Message-ID. Generated at draft creation and never changes.
    /// Used as the dedup key across SQLite, Maildir, and IMAP Drafts folder.
    pub message_id: String,
    /// Subject line.
    pub subject: String,
    /// Markdown body source. Rendered to HTML + plaintext at send time.
    pub body_markdown: String,
    /// Recipients.
    pub to: Vec<Contact>,
    /// CC recipients.
    pub cc: Vec<Contact>,
    /// BCC recipients (envelope-only when sent).
    pub bcc: Vec<Contact>,
    /// Compose-time attachments. Files live in
    /// `~/.local/share/inboxly/drafts/<id>/`.
    pub attachments: Vec<AttachmentDraft>,
    /// New compose vs reply/replyall/forward (M36 placeholders).
    pub mode: ComposeMode,
    /// `In-Reply-To` header value (only set when `mode` is Reply/ReplyAll).
    pub in_reply_to: Option<String>,
    /// `References` header value (chain of ancestors).
    pub references: Option<String>,
    /// Path to the local Maildir `.Drafts/` file (set on explicit save).
    pub maildir_path: Option<PathBuf>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Last updated at.
    pub updated_at: DateTime<Utc>,
}

impl DraftEmail {
    /// Create a new empty draft for the given account.
    ///
    /// Generates a fresh UUID for `id` and a Message-ID of the form
    /// `<{uuid}@inboxly.local>`. The created/updated timestamps are set to now.
    #[must_use]
    pub fn new_empty(account_id: AccountId) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let message_id = format!("<{id}@inboxly.local>");
        let now = Utc::now();
        Self {
            id,
            account_id,
            message_id,
            subject: String::new(),
            body_markdown: String::new(),
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            attachments: Vec::new(),
            mode: ComposeMode::New,
            in_reply_to: None,
            references: None,
            maildir_path: None,
            created_at: now,
            updated_at: now,
        }
    }
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
