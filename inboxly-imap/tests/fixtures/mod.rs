use rusqlite::Connection;

/// Create an in-memory SQLite database with the full Inboxly schema
/// (enough tables for sync engine testing).
pub fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "
        CREATE TABLE emails (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            thread_id TEXT,
            from_name TEXT NOT NULL,
            from_address TEXT NOT NULL,
            to_json TEXT NOT NULL,
            cc_json TEXT NOT NULL,
            subject TEXT NOT NULL,
            snippet TEXT NOT NULL DEFAULT '',
            date INTEGER NOT NULL,
            maildir_path TEXT NOT NULL DEFAULT '',
            flags INTEGER NOT NULL DEFAULT 0,
            size_bytes INTEGER NOT NULL DEFAULT 0,
            imap_uid INTEGER NOT NULL,
            imap_folder TEXT NOT NULL,
            has_attachments INTEGER NOT NULL DEFAULT 0,
            message_id_header TEXT,
            in_reply_to TEXT,
            references_json TEXT,
            UNIQUE(account_id, imap_folder, imap_uid)
        );

        CREATE TABLE threads (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            subject TEXT NOT NULL,
            newest_date INTEGER NOT NULL,
            oldest_date INTEGER NOT NULL,
            email_count INTEGER NOT NULL DEFAULT 0,
            unread_count INTEGER NOT NULL DEFAULT 0,
            has_attachments INTEGER NOT NULL DEFAULT 0,
            snippet TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE sync_state (
            account_id TEXT NOT NULL,
            folder_name TEXT NOT NULL,
            uid_validity INTEGER NOT NULL,
            uid_next INTEGER NOT NULL,
            highest_modseq INTEGER,
            last_sync TEXT NOT NULL,
            last_synced_uid INTEGER,
            PRIMARY KEY (account_id, folder_name)
        );
        ",
    )
    .unwrap();
    conn
}

/// Build a Vec of EnvelopeData for testing, with UIDs from `start` to `end` inclusive.
pub fn make_envelopes(
    start: u32,
    end: u32,
    account_id: &str,
    folder: &str,
) -> Vec<inboxly_imap::sync::envelope::EnvelopeData> {
    (start..=end)
        .map(|uid| inboxly_imap::sync::envelope::EnvelopeData {
            message_id: format!("<msg-{uid}@test.inboxly>"),
            account_id: account_id.to_string(),
            imap_uid: uid,
            imap_folder: folder.to_string(),
            from_name: format!("Sender {uid}"),
            from_address: format!("sender{uid}@example.com"),
            to_json: r#"[{"name":"Me","address":"me@example.com"}]"#.to_string(),
            cc_json: "[]".to_string(),
            subject: format!("Test email #{uid}"),
            date_unix: 1773338200 + uid as i64,
            size_bytes: 1024 + uid as u64,
            flags: if uid % 3 == 0 { 1 } else { 0 }, // every 3rd is "seen"
            in_reply_to: None,
            references_json: None,
        })
        .collect()
}
