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
