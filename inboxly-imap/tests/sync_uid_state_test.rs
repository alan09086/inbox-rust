use inboxly_imap::sync::uid_state::{FolderSyncState, load_sync_state, save_sync_state};
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sync_state (
            account_id TEXT NOT NULL,
            folder_name TEXT NOT NULL,
            uid_validity INTEGER NOT NULL,
            uid_next INTEGER NOT NULL,
            highest_modseq INTEGER,
            last_sync TEXT NOT NULL,
            last_synced_uid INTEGER,
            PRIMARY KEY (account_id, folder_name)
        );",
    )
    .unwrap();
    conn
}

#[test]
fn save_and_load_sync_state() {
    let conn = setup_db();
    let state = FolderSyncState {
        account_id: "acc-1".to_string(),
        folder_name: "INBOX".to_string(),
        uid_validity: 12345,
        uid_next: 5001,
        highest_modseq: None,
        last_synced_uid: Some(5000),
    };
    save_sync_state(&conn, &state).unwrap();

    let loaded = load_sync_state(&conn, "acc-1", "INBOX").unwrap();
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.uid_validity, 12345);
    assert_eq!(loaded.uid_next, 5001);
    assert_eq!(loaded.last_synced_uid, Some(5000));
}

#[test]
fn load_nonexistent_returns_none() {
    let conn = setup_db();
    let loaded = load_sync_state(&conn, "acc-1", "INBOX").unwrap();
    assert!(loaded.is_none());
}

#[test]
fn save_overwrites_existing() {
    let conn = setup_db();
    let state1 = FolderSyncState {
        account_id: "acc-1".to_string(),
        folder_name: "INBOX".to_string(),
        uid_validity: 100,
        uid_next: 500,
        highest_modseq: None,
        last_synced_uid: Some(499),
    };
    save_sync_state(&conn, &state1).unwrap();

    let state2 = FolderSyncState {
        uid_next: 600,
        last_synced_uid: Some(599),
        ..state1.clone()
    };
    save_sync_state(&conn, &state2).unwrap();

    let loaded = load_sync_state(&conn, "acc-1", "INBOX").unwrap().unwrap();
    assert_eq!(loaded.uid_next, 600);
    assert_eq!(loaded.last_synced_uid, Some(599));
}

#[test]
fn detect_uid_validity_change() {
    use inboxly_imap::sync::uid_state::check_uid_validity;
    let conn = setup_db();

    // No prior state — should return Ok(false) meaning "no reset needed"
    let needs_reset = check_uid_validity(&conn, "acc-1", "INBOX", 100).unwrap();
    assert!(!needs_reset);

    // Save state with uid_validity=100
    let state = FolderSyncState {
        account_id: "acc-1".to_string(),
        folder_name: "INBOX".to_string(),
        uid_validity: 100,
        uid_next: 500,
        highest_modseq: None,
        last_synced_uid: None,
    };
    save_sync_state(&conn, &state).unwrap();

    // Same validity — no reset
    let needs_reset = check_uid_validity(&conn, "acc-1", "INBOX", 100).unwrap();
    assert!(!needs_reset);

    // Different validity — needs reset
    let needs_reset = check_uid_validity(&conn, "acc-1", "INBOX", 200).unwrap();
    assert!(needs_reset);
}
