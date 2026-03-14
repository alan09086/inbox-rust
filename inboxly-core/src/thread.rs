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
