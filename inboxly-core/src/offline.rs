//! Offline action types for the action queue.
//!
//! When the user performs actions while disconnected (or during sync),
//! they are serialized and stored in SQLite. On reconnect, the queue
//! is drained and each action is replayed against IMAP.

use serde::{Deserialize, Serialize};

/// An action taken by the user while offline (or during sync).
///
/// Queued in SQLite's `offline_queue` table as JSON and replayed
/// against IMAP when connectivity is restored.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OfflineAction {
    /// Mark email as read (set \Seen flag on IMAP).
    MarkRead {
        account_id: String,
        folder: String,
        imap_uid: u32,
    },
    /// Mark email as unread (clear \Seen flag).
    MarkUnread {
        account_id: String,
        folder: String,
        imap_uid: u32,
    },
    /// Star/flag an email (set \Flagged).
    Star {
        account_id: String,
        folder: String,
        imap_uid: u32,
    },
    /// Unstar an email (clear \Flagged).
    Unstar {
        account_id: String,
        folder: String,
        imap_uid: u32,
    },
    /// Archive / "Done" — mark as read + deleted, then expunge.
    MarkDone {
        account_id: String,
        folder: String,
        imap_uid: u32,
    },
    /// Move email to trash (COPY to Trash, then delete from source).
    MoveToTrash {
        account_id: String,
        folder: String,
        imap_uid: u32,
    },
    /// Move email to a different IMAP folder.
    MoveToFolder {
        account_id: String,
        from_folder: String,
        to_folder: String,
        imap_uid: u32,
    },
    /// Mark as answered (set \Answered flag).
    MarkAnswered {
        account_id: String,
        folder: String,
        imap_uid: u32,
    },
    /// Send a queued draft (composed offline). Handled by SMTP (M23).
    SendDraft {
        account_id: String,
        draft_maildir_path: String,
    },
}

impl OfflineAction {
    /// Return a short name for the action variant (for the `action` column).
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::MarkRead { .. } => "mark_read",
            Self::MarkUnread { .. } => "mark_unread",
            Self::Star { .. } => "star",
            Self::Unstar { .. } => "unstar",
            Self::MarkDone { .. } => "mark_done",
            Self::MoveToTrash { .. } => "move_to_trash",
            Self::MoveToFolder { .. } => "move_to_folder",
            Self::MarkAnswered { .. } => "mark_answered",
            Self::SendDraft { .. } => "send_draft",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_names() {
        let action = OfflineAction::MarkRead {
            account_id: "acc-1".into(),
            folder: "INBOX".into(),
            imap_uid: 42,
        };
        assert_eq!(action.variant_name(), "mark_read");
    }

    #[test]
    fn test_serde_roundtrip() {
        let action = OfflineAction::MoveToFolder {
            account_id: "acc-1".into(),
            from_folder: "INBOX".into(),
            to_folder: "Archive".into(),
            imap_uid: 99,
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: OfflineAction = serde_json::from_str(&json).unwrap();
        assert_eq!(back.variant_name(), "move_to_folder");
    }

    #[test]
    fn test_all_variants_serialize() {
        let variants = vec![
            OfflineAction::MarkRead { account_id: "a".into(), folder: "I".into(), imap_uid: 1 },
            OfflineAction::MarkUnread { account_id: "a".into(), folder: "I".into(), imap_uid: 2 },
            OfflineAction::Star { account_id: "a".into(), folder: "I".into(), imap_uid: 3 },
            OfflineAction::Unstar { account_id: "a".into(), folder: "I".into(), imap_uid: 4 },
            OfflineAction::MarkDone { account_id: "a".into(), folder: "I".into(), imap_uid: 5 },
            OfflineAction::MoveToTrash { account_id: "a".into(), folder: "I".into(), imap_uid: 6 },
            OfflineAction::MoveToFolder {
                account_id: "a".into(), from_folder: "I".into(), to_folder: "A".into(), imap_uid: 7,
            },
            OfflineAction::MarkAnswered { account_id: "a".into(), folder: "I".into(), imap_uid: 8 },
            OfflineAction::SendDraft {
                account_id: "a".into(), draft_maildir_path: "/tmp/d.eml".into(),
            },
        ];

        let expected_names = [
            "mark_read", "mark_unread", "star", "unstar",
            "mark_done", "move_to_trash", "move_to_folder",
            "mark_answered", "send_draft",
        ];

        for (action, expected) in variants.iter().zip(expected_names.iter()) {
            assert_eq!(action.variant_name(), *expected);
            // Verify roundtrip
            let json = serde_json::to_string(action).unwrap();
            let back: OfflineAction = serde_json::from_str(&json).unwrap();
            assert_eq!(back.variant_name(), *expected);
        }
    }
}
