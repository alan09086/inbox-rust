//! Offline action types for the action queue.
//!
//! When the user performs actions while disconnected (or during sync),
//! they are serialized and stored in SQLite. On reconnect, the queue
//! is drained and each action is replayed against IMAP.

use serde::{Deserialize, Serialize};

use crate::email::DraftEmail;

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
    ///
    /// Legacy form: only the maildir `.eml` path is preserved, so the
    /// replay handler must reparse the file back into a `DraftEmail`.
    /// Kept for backwards compatibility with queue entries written
    /// before M35b — new entries should use [`Self::SendDraftFull`].
    SendDraft {
        account_id: String,
        draft_maildir_path: String,
    },

    /// **M35b path**: Send a draft whose full [`DraftEmail`] is embedded
    /// in the action payload. Replaces the legacy [`Self::SendDraft`]
    /// for new drafts queued by Phase 12's send bridge. The legacy
    /// variant is preserved so existing queue entries from before M35b
    /// still replay correctly.
    ///
    /// `draft` is boxed so this variant doesn't bloat the size of the
    /// other (small) `OfflineAction` variants — `DraftEmail` is ~350
    /// bytes due to its `Vec<Contact>` / `Vec<AttachmentDraft>` /
    /// timestamp fields.
    SendDraftFull {
        /// Fully-hydrated draft, including markdown body and recipients.
        draft: Box<DraftEmail>,
    },

    /// **M35b Gemini G6**: SMTP send succeeded but the IMAP Sent folder
    /// `APPEND` failed (or could not be attempted because the bridge
    /// did not own a session). The next sync's offline replay loop will
    /// retry the `APPEND` so the user's Sent folder eventually catches
    /// up. The user sees the standard "Sent" overlay regardless — the
    /// email already left the wire.
    ///
    /// The replay handler is expected to look the message up in the
    /// local Maildir Sent folder by `Message-ID` and replay the IMAP
    /// `APPEND` from those bytes. For Phase 12 the handler logs and
    /// skips because the `MaildirStore` + active `Session` are not yet
    /// plumbed through `replay_offline_queue`'s signature; the variant
    /// exists so the queue model is correct now and Phase 13 / M36 can
    /// fill in the body without a schema migration.
    AppendSent {
        /// Account that owns the Sent folder copy. Stored as the
        /// account's email address (matches the bridge's accessor).
        account_id: String,
        /// `Message-ID` of the sent message. The replay handler uses
        /// this to look up the local Maildir copy.
        draft_message_id: String,
    },

    /// **M36 Phase 5**: A draft was saved locally (SQLite + Maildir
    /// `.Drafts/`) by the explicit-save bridge or the Navigate guard,
    /// but the IMAP `APPEND` to the server's Drafts folder could not
    /// be performed because the UI bridge does not own an IMAP
    /// session. The next sync's offline replay loop will look up the
    /// locally-stored Draft copy by `Message-ID` and replay the
    /// `APPEND` so the server eventually catches up.
    ///
    /// Phase 5 ships the variant + a warn-and-skip replay stub. A
    /// post-M36 phase will fill in the real `APPEND` handler (the
    /// Phase 4 `AppendSent` arm is the template — both variants
    /// resolve a Maildir copy by `Message-ID` and `APPEND` it to a
    /// well-known folder).
    AppendDraft {
        /// Account that owns the Drafts folder copy. Stored as the
        /// account's email address (matches the bridge's accessor).
        account_id: String,
        /// `Message-ID` of the draft. The replay handler uses this
        /// to look up the local Maildir copy in `.Drafts/`.
        draft_message_id: String,
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
            Self::SendDraftFull { .. } => "send_draft_full",
            Self::AppendSent { .. } => "append_sent",
            Self::AppendDraft { .. } => "append_draft",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::AccountId;

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
            OfflineAction::MarkRead {
                account_id: "a".into(),
                folder: "I".into(),
                imap_uid: 1,
            },
            OfflineAction::MarkUnread {
                account_id: "a".into(),
                folder: "I".into(),
                imap_uid: 2,
            },
            OfflineAction::Star {
                account_id: "a".into(),
                folder: "I".into(),
                imap_uid: 3,
            },
            OfflineAction::Unstar {
                account_id: "a".into(),
                folder: "I".into(),
                imap_uid: 4,
            },
            OfflineAction::MarkDone {
                account_id: "a".into(),
                folder: "I".into(),
                imap_uid: 5,
            },
            OfflineAction::MoveToTrash {
                account_id: "a".into(),
                folder: "I".into(),
                imap_uid: 6,
            },
            OfflineAction::MoveToFolder {
                account_id: "a".into(),
                from_folder: "I".into(),
                to_folder: "A".into(),
                imap_uid: 7,
            },
            OfflineAction::MarkAnswered {
                account_id: "a".into(),
                folder: "I".into(),
                imap_uid: 8,
            },
            OfflineAction::SendDraft {
                account_id: "a".into(),
                draft_maildir_path: "/tmp/d.eml".into(),
            },
            OfflineAction::SendDraftFull {
                draft: Box::new(DraftEmail::new_empty(AccountId::new())),
            },
            OfflineAction::AppendSent {
                account_id: "alice@example.com".into(),
                draft_message_id: "<msg-1@inboxly.local>".into(),
            },
            OfflineAction::AppendDraft {
                account_id: "alice@example.com".into(),
                draft_message_id: "<draft-1@inboxly.local>".into(),
            },
        ];

        let expected_names = [
            "mark_read",
            "mark_unread",
            "star",
            "unstar",
            "mark_done",
            "move_to_trash",
            "move_to_folder",
            "mark_answered",
            "send_draft",
            "send_draft_full",
            "append_sent",
            "append_draft",
        ];

        for (action, expected) in variants.iter().zip(expected_names.iter()) {
            assert_eq!(action.variant_name(), *expected);
            // Verify roundtrip
            let json = serde_json::to_string(action).unwrap();
            let back: OfflineAction = serde_json::from_str(&json).unwrap();
            assert_eq!(back.variant_name(), *expected);
        }
    }

    /// **M35b Phase 12 — Gemini G6**: focused round-trip test for the
    /// new `AppendSent` variant. Verifies that the variant serialises
    /// to a JSON-tagged form whose `account_id` and `draft_message_id`
    /// fields survive a round trip, that `variant_name()` reports
    /// `"append_sent"`, and that the deserialised value still matches
    /// the original.
    ///
    /// `test_all_variants_serialize` above already covers `AppendSent`
    /// inside its loop, but the loop only checks `variant_name()` —
    /// this test asserts the *field-level* round trip so a future
    /// rename (or accidental `#[serde(skip)]`) would fail loudly.
    #[test]
    fn append_sent_serialize_round_trip() {
        let action = OfflineAction::AppendSent {
            account_id: "alice@example.com".into(),
            draft_message_id: "<msg-roundtrip@inboxly.local>".into(),
        };
        assert_eq!(action.variant_name(), "append_sent");

        let json = serde_json::to_string(&action).expect("AppendSent must serialise");
        let back: OfflineAction = serde_json::from_str(&json).expect("AppendSent must round-trip");
        assert_eq!(back.variant_name(), "append_sent");

        match back {
            OfflineAction::AppendSent {
                account_id,
                draft_message_id,
            } => {
                assert_eq!(account_id, "alice@example.com");
                assert_eq!(draft_message_id, "<msg-roundtrip@inboxly.local>");
            }
            other => panic!("expected AppendSent variant, got {other:?}"),
        }
    }

    /// **M36 Phase 5**: focused round-trip test for the new
    /// `AppendDraft` variant. Mirrors the `append_sent_serialize_round_trip`
    /// test above so a future rename of either `account_id` or
    /// `draft_message_id` (or an accidental `#[serde(skip)]`) fails
    /// loudly at test time rather than silently dropping queue entries.
    #[test]
    fn append_draft_serialize_round_trip() {
        let action = OfflineAction::AppendDraft {
            account_id: "alice@example.com".into(),
            draft_message_id: "<draft-roundtrip@inboxly.local>".into(),
        };
        assert_eq!(action.variant_name(), "append_draft");

        let json = serde_json::to_string(&action).expect("AppendDraft must serialise");
        let back: OfflineAction = serde_json::from_str(&json).expect("AppendDraft must round-trip");
        assert_eq!(back.variant_name(), "append_draft");

        match back {
            OfflineAction::AppendDraft {
                account_id,
                draft_message_id,
            } => {
                assert_eq!(account_id, "alice@example.com");
                assert_eq!(draft_message_id, "<draft-roundtrip@inboxly.local>");
            }
            other => panic!("expected AppendDraft variant, got {other:?}"),
        }
    }
}
