use inboxly_imap::sync::threading::assign_thread_ids;
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
            message_id_header TEXT,
            in_reply_to TEXT,
            references_json TEXT,
            UNIQUE(account_id, imap_folder, imap_uid)
        );
        CREATE TABLE IF NOT EXISTS threads (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            subject TEXT NOT NULL,
            newest_date INTEGER NOT NULL,
            oldest_date INTEGER NOT NULL,
            email_count INTEGER NOT NULL DEFAULT 0,
            unread_count INTEGER NOT NULL DEFAULT 0,
            has_attachments INTEGER NOT NULL DEFAULT 0,
            snippet TEXT NOT NULL DEFAULT ''
        );",
    )
    .unwrap();
    conn
}

fn insert_email(conn: &Connection, msg_id: &str, in_reply_to: Option<&str>, uid: u32) {
    conn.execute(
        "INSERT INTO emails (id, account_id, thread_id, from_name, from_address, to_json, cc_json,
         subject, date, imap_uid, imap_folder, message_id_header, in_reply_to)
         VALUES (?1, 'acc-1', NULL, 'Test', 'test@x.com', '[]', '[]',
         'Subject', 1773338200, ?2, 'INBOX', ?1, ?3)",
        rusqlite::params![msg_id, uid, in_reply_to],
    )
    .unwrap();
}

#[test]
fn standalone_email_gets_new_thread() {
    let conn = setup_db();
    insert_email(&conn, "<msg-1@x.com>", None, 1);

    let assigned = assign_thread_ids(&conn, "acc-1").unwrap();
    assert_eq!(assigned, 1);

    let thread_id: String = conn
        .query_row(
            "SELECT thread_id FROM emails WHERE id = '<msg-1@x.com>'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!thread_id.is_empty());
}

#[test]
fn reply_joins_parent_thread() {
    let conn = setup_db();
    insert_email(&conn, "<msg-1@x.com>", None, 1);
    insert_email(&conn, "<msg-2@x.com>", Some("<msg-1@x.com>"), 2);

    assign_thread_ids(&conn, "acc-1").unwrap();

    let tid1: String = conn
        .query_row(
            "SELECT thread_id FROM emails WHERE id = '<msg-1@x.com>'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let tid2: String = conn
        .query_row(
            "SELECT thread_id FROM emails WHERE id = '<msg-2@x.com>'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tid1, tid2);
}

#[test]
fn reply_to_unknown_parent_gets_own_thread() {
    let conn = setup_db();
    // Parent not in DB — reply gets its own thread (M10 will unify when parent arrives)
    insert_email(&conn, "<msg-2@x.com>", Some("<missing@x.com>"), 2);

    let assigned = assign_thread_ids(&conn, "acc-1").unwrap();
    assert_eq!(assigned, 1);

    let thread_id: String = conn
        .query_row(
            "SELECT thread_id FROM emails WHERE id = '<msg-2@x.com>'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!thread_id.is_empty());
}

#[test]
fn already_threaded_emails_skipped() {
    let conn = setup_db();
    insert_email(&conn, "<msg-1@x.com>", None, 1);
    assign_thread_ids(&conn, "acc-1").unwrap();

    // Run again — should not re-assign
    let assigned = assign_thread_ids(&conn, "acc-1").unwrap();
    assert_eq!(assigned, 0);
}

#[test]
fn thread_row_created() {
    let conn = setup_db();
    insert_email(&conn, "<msg-1@x.com>", None, 1);
    assign_thread_ids(&conn, "acc-1").unwrap();

    let thread_count: u32 = conn
        .query_row("SELECT COUNT(*) FROM threads", [], |r| r.get(0))
        .unwrap();
    assert_eq!(thread_count, 1);
}
