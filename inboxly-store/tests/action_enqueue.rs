//! Integration tests for the offline action enqueue/dequeue flow.

use inboxly_core::offline::OfflineAction;
use inboxly_store::Store;

fn make_store() -> Store {
    Store::open_in_memory().expect("failed to open in-memory store")
}

// ---------------------------------------------------------------------------

#[test]
fn enqueue_and_dequeue_mark_read() {
    let store = make_store();

    let action = OfflineAction::MarkRead {
        account_id: "acct-001".into(),
        folder: "INBOX".into(),
        imap_uid: 42,
    };
    let payload = serde_json::to_string(&action).expect("serialize");
    let row_id = store
        .enqueue_offline_action(action.variant_name(), &payload)
        .expect("enqueue failed");

    // Verify one item is in the queue with correct fields.
    let queue = store.get_offline_queue().expect("get_offline_queue failed");
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].id, Some(row_id));
    assert_eq!(queue[0].action, "mark_read");
    assert_eq!(queue[0].payload_json, payload);

    // Dequeue the item and verify the queue is now empty.
    store
        .dequeue_offline_action(row_id)
        .expect("dequeue failed");

    let count = store
        .count_offline_queue()
        .expect("count_offline_queue failed");
    assert_eq!(count, 0, "queue should be empty after dequeue");
}

#[test]
fn enqueue_mark_done_and_move_to_folder() {
    let store = make_store();

    let done = OfflineAction::MarkDone {
        account_id: "acct-002".into(),
        folder: "INBOX".into(),
        imap_uid: 100,
    };
    let move_action = OfflineAction::MoveToFolder {
        account_id: "acct-002".into(),
        from_folder: "INBOX".into(),
        to_folder: "Archive".into(),
        imap_uid: 200,
    };

    let payload_done = serde_json::to_string(&done).expect("serialize done");
    let payload_move = serde_json::to_string(&move_action).expect("serialize move");

    store
        .enqueue_offline_action(done.variant_name(), &payload_done)
        .expect("enqueue done failed");
    store
        .enqueue_offline_action(move_action.variant_name(), &payload_move)
        .expect("enqueue move failed");

    // Verify FIFO order: MarkDone first, then MoveToFolder.
    let queue = store.get_offline_queue().expect("get_offline_queue failed");
    assert_eq!(queue.len(), 2);
    assert_eq!(queue[0].action, "mark_done");
    assert_eq!(queue[1].action, "move_to_folder");

    // IDs should be ascending (FIFO).
    assert!(
        queue[0].id < queue[1].id,
        "queue should be in insertion order"
    );
}

#[test]
fn all_offline_action_variants_serialize_roundtrip() {
    let store = make_store();

    let variants: Vec<OfflineAction> = vec![
        OfflineAction::MarkRead {
            account_id: "acct-1".into(),
            folder: "INBOX".into(),
            imap_uid: 1,
        },
        OfflineAction::MarkUnread {
            account_id: "acct-1".into(),
            folder: "INBOX".into(),
            imap_uid: 2,
        },
        OfflineAction::Star {
            account_id: "acct-1".into(),
            folder: "INBOX".into(),
            imap_uid: 3,
        },
        OfflineAction::Unstar {
            account_id: "acct-1".into(),
            folder: "INBOX".into(),
            imap_uid: 4,
        },
        OfflineAction::MarkDone {
            account_id: "acct-1".into(),
            folder: "INBOX".into(),
            imap_uid: 5,
        },
        OfflineAction::MoveToTrash {
            account_id: "acct-1".into(),
            folder: "INBOX".into(),
            imap_uid: 6,
        },
        OfflineAction::MoveToFolder {
            account_id: "acct-1".into(),
            from_folder: "INBOX".into(),
            to_folder: "Archive".into(),
            imap_uid: 7,
        },
        OfflineAction::MarkAnswered {
            account_id: "acct-1".into(),
            folder: "INBOX".into(),
            imap_uid: 8,
        },
        OfflineAction::SendDraft {
            account_id: "acct-1".into(),
            draft_maildir_path: "/home/user/Maildir/drafts/msg.eml".into(),
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
    ];

    // Enqueue all variants.
    for (action, &expected_name) in variants.iter().zip(expected_names.iter()) {
        let payload = serde_json::to_string(action).expect("serialize");
        store
            .enqueue_offline_action(expected_name, &payload)
            .expect("enqueue failed");
    }

    // Verify total count.
    let count = store
        .count_offline_queue()
        .expect("count_offline_queue failed");
    assert_eq!(count, 9, "all 9 variants should be in the queue");

    // Dequeue all and verify each deserializes back correctly.
    let queue = store.get_offline_queue().expect("get_offline_queue failed");
    assert_eq!(queue.len(), 9);

    for (row, &expected_name) in queue.iter().zip(expected_names.iter()) {
        assert_eq!(
            row.action, expected_name,
            "action column mismatch for {expected_name}"
        );

        let deserialized: OfflineAction =
            serde_json::from_str(&row.payload_json).expect("deserialize failed");
        assert_eq!(
            deserialized.variant_name(),
            expected_name,
            "roundtrip variant_name mismatch for {expected_name}"
        );

        store
            .dequeue_offline_action(row.id.unwrap())
            .expect("dequeue failed");
    }

    // Queue should be empty after draining.
    let final_count = store
        .count_offline_queue()
        .expect("count_offline_queue failed");
    assert_eq!(final_count, 0, "queue should be empty after draining all");
}
