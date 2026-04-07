//! Integration tests for `ThreadReader` (M34 eng review Issue 3.1).
//!
//! Sets up a real on-disk fixture (TempDir for the Maildir, in-memory
//! SQLite for the metadata store) and exercises every branch of
//! `ThreadReader::load_thread()`. These tests are the safety net for
//! the production code path that future consumers (M36 reply,
//! M37 attachments) will inherit.

use std::sync::Arc;

use inboxly_store::thread_reader::ThreadReader;
use inboxly_store::{AccountRow, EmailRow, MaildirStore, Store};
use tempfile::TempDir;

/// Account ID used by every test row. The fixture inserts a matching
/// `accounts` row so the foreign key on `emails.account_id` is satisfied.
const TEST_ACCOUNT_ID: &str = "a1";

/// Build a fixture: in-memory Store, TempDir-backed MaildirStore,
/// and a `ThreadReader` wrapping both. Inserts a sample account row
/// so the FK on `emails.account_id` is satisfied. Returns the TempDir
/// handle (which must outlive the test) so the temp directory isn't
/// cleaned up while the test is still using paths inside it.
fn fixture() -> (TempDir, Arc<Store>, Arc<MaildirStore>, ThreadReader) {
    let temp = TempDir::new().expect("tempdir");
    let store = Arc::new(Store::open_in_memory().expect("store"));
    store
        .insert_account(&AccountRow {
            id: TEST_ACCOUNT_ID.into(),
            email: "alice@example.com".into(),
            display_name: "Alice".into(),
            provider: "generic".into(),
            auth_method: "password".into(),
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            smtp_host: "smtp.example.com".into(),
            smtp_port: 587,
        })
        .expect("insert account");
    let maildir = Arc::new(MaildirStore::new(temp.path()));
    let reader = ThreadReader::new(Arc::clone(&store), Arc::clone(&maildir));
    (temp, store, maildir, reader)
}

/// Build a minimal `EmailRow` for tests. `body_downloaded` and
/// `maildir_path` are the test-relevant fields; the rest are
/// reasonable defaults. The `imap_uid` is derived from `date` so
/// the unique `(account_id, imap_folder, imap_uid)` index isn't
/// violated when a single thread holds multiple emails.
fn make_row(
    id: &str,
    thread_id: &str,
    date: i64,
    body_downloaded: bool,
    maildir_path: &str,
) -> EmailRow {
    EmailRow {
        id: id.into(),
        account_id: TEST_ACCOUNT_ID.into(),
        thread_id: thread_id.into(),
        from_name: Some("Alice".into()),
        from_address: "alice@example.com".into(),
        to_json: "[]".into(),
        cc_json: "[]".into(),
        subject: format!("Subject {id}"),
        snippet: "snip".into(),
        date,
        maildir_path: maildir_path.into(),
        flags: 0,
        size_bytes: 100,
        imap_uid: date,
        imap_folder: "INBOX".into(),
        has_attachments: false,
        body_downloaded,
        message_id_header: None,
        in_reply_to: None,
        references_json: None,
    }
}

/// Write a minimal valid `.eml` file to disk and return the path.
fn write_eml(temp: &TempDir, name: &str, body_text: &str) -> String {
    let path = temp.path().join(name);
    let eml = format!(
        "From: alice@example.com\r\n\
         To: bob@example.com\r\n\
         Subject: Test {name}\r\n\
         Message-ID: <{name}@ex.com>\r\n\
         \r\n\
         {body_text}"
    );
    std::fs::write(&path, eml).expect("write eml");
    path.to_string_lossy().into_owned()
}

// ── Branch 1: empty thread → Err ──────────────────────────────

#[test]
fn load_thread_empty_returns_err() {
    let (_temp, _store, _maildir, reader) = fixture();
    let result = reader.load_thread("nonexistent");
    assert!(result.is_err(), "empty thread should be Err");
}

// ── Branch 2: body_downloaded=true with successful disk read ──

#[test]
fn load_thread_with_downloaded_body_returns_some_content() {
    let (temp, store, _maildir, reader) = fixture();
    let path = write_eml(&temp, "msg1.eml", "Hello world");
    let row = make_row("e1", "t1", 1000, /* downloaded */ true, &path);
    store.insert_email(&row).expect("insert");

    let result = reader.load_thread("t1").expect("ok");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].row.id, "e1");
    let content = result[0].content.as_ref().expect("content present");
    assert_eq!(content.body_text.as_deref(), Some("Hello world"));
}

// ── Branch 3: body_downloaded=false → content: None ───────────

#[test]
fn load_thread_with_undownloaded_body_returns_none_content() {
    let (_temp, store, _maildir, reader) = fixture();
    let row = make_row("e1", "t1", 1000, /* downloaded */ false, "");
    store.insert_email(&row).expect("insert");

    let result = reader.load_thread("t1").expect("ok");
    assert_eq!(result.len(), 1);
    assert!(
        result[0].content.is_none(),
        "undownloaded body must produce None content"
    );
}

// ── Branch 4: body_downloaded=true but disk read fails → None ─

#[test]
fn load_thread_handles_missing_file_gracefully() {
    let (_temp, store, _maildir, reader) = fixture();
    let row = make_row(
        "e1",
        "t1",
        1000,
        /* downloaded */ true,
        "/nonexistent/path/to/missing.eml",
    );
    store.insert_email(&row).expect("insert");

    let result = reader.load_thread("t1").expect("ok despite missing file");
    assert_eq!(result.len(), 1);
    assert!(
        result[0].content.is_none(),
        "missing file should fall through to None, not Err"
    );
}

// ── Branch 5: mixed thread with multiple messages ─────────────

#[test]
fn load_thread_multiple_messages_in_chronological_order() {
    let (temp, store, _maildir, reader) = fixture();
    let p3 = write_eml(&temp, "msg3.eml", "third");
    let p1 = write_eml(&temp, "msg1.eml", "first");
    let p2 = write_eml(&temp, "msg2.eml", "second");
    store
        .insert_email(&make_row("e3", "t1", 3000, true, &p3))
        .unwrap();
    store
        .insert_email(&make_row("e1", "t1", 1000, true, &p1))
        .unwrap();
    store
        .insert_email(&make_row("e2", "t1", 2000, true, &p2))
        .unwrap();

    let result = reader.load_thread("t1").expect("ok");
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].row.id, "e1");
    assert_eq!(result[1].row.id, "e2");
    assert_eq!(result[2].row.id, "e3");
    assert!(result.iter().all(|le| le.content.is_some()));
}

// ── Branch 6: mixed downloaded/undownloaded ───────────────────

#[test]
fn load_thread_mixed_downloaded_state_per_message() {
    let (temp, store, _maildir, reader) = fixture();
    let p1 = write_eml(&temp, "ready.eml", "downloaded body");
    store
        .insert_email(&make_row("e1", "t1", 1000, true, &p1))
        .unwrap();
    store
        .insert_email(&make_row("e2", "t1", 2000, false, ""))
        .unwrap();

    let result = reader.load_thread("t1").expect("ok");
    assert_eq!(result.len(), 2);
    assert!(result[0].content.is_some());
    assert!(result[1].content.is_none());
}
