use inboxly_imap::sync::envelope::EnvelopeData;
use inboxly_imap::sync::store::{batch_insert_envelopes, count_emails_in_folder};
use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS emails (
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
            body_downloaded INTEGER NOT NULL DEFAULT 0,
            message_id_header TEXT,
            in_reply_to TEXT,
            references_json TEXT,
            UNIQUE(account_id, imap_folder, imap_uid)
        );"
    ).unwrap();
    conn
}

fn make_envelope(uid: u32) -> EnvelopeData {
    EnvelopeData {
        message_id: format!("<msg-{uid}@example.com>"),
        account_id: "acc-1".to_string(),
        imap_uid: uid,
        imap_folder: "INBOX".to_string(),
        from_name: "Test Sender".to_string(),
        from_address: "test@example.com".to_string(),
        to_json: r#"[{"name":"Me","address":"me@example.com"}]"#.to_string(),
        cc_json: "[]".to_string(),
        subject: format!("Subject {uid}"),
        date_unix: 1773338200 + uid as i64,
        size_bytes: 1024,
        flags: 0,
        in_reply_to: None,
        references_json: None,
    }
}

#[test]
fn insert_single_batch() {
    let conn = setup_db();
    let envelopes: Vec<_> = (1..=5).map(make_envelope).collect();
    let inserted = batch_insert_envelopes(&conn, &envelopes).unwrap();
    assert_eq!(inserted, 5);
}

#[test]
fn insert_ignores_duplicates() {
    let conn = setup_db();
    let envelopes: Vec<_> = (1..=5).map(make_envelope).collect();

    batch_insert_envelopes(&conn, &envelopes).unwrap();
    // Insert same batch again — duplicates should be ignored (ON CONFLICT IGNORE)
    let inserted = batch_insert_envelopes(&conn, &envelopes).unwrap();
    assert_eq!(inserted, 0);
}

#[test]
fn insert_large_batch() {
    let conn = setup_db();
    let envelopes: Vec<_> = (1..=1000).map(make_envelope).collect();
    let inserted = batch_insert_envelopes(&conn, &envelopes).unwrap();
    assert_eq!(inserted, 1000);
}

#[test]
fn count_emails() {
    let conn = setup_db();
    assert_eq!(count_emails_in_folder(&conn, "acc-1", "INBOX").unwrap(), 0);

    let envelopes: Vec<_> = (1..=3).map(make_envelope).collect();
    batch_insert_envelopes(&conn, &envelopes).unwrap();
    assert_eq!(count_emails_in_folder(&conn, "acc-1", "INBOX").unwrap(), 3);
}

#[test]
fn insert_empty_batch() {
    let conn = setup_db();
    let inserted = batch_insert_envelopes(&conn, &[]).unwrap();
    assert_eq!(inserted, 0);
}
