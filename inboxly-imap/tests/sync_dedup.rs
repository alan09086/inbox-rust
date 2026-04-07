//! Integration test for M35b Phase 4b sync-side Message-ID dedup.
//!
//! Verifies that `inboxly_imap::body_processor::process_body` does NOT
//! write a duplicate `.eml` when the local Maildir already contains a
//! file with the same `Message-ID:` header — the compose path's
//! compose-time write must not be shadowed by a sync-time second write.
//!
//! Both tests below set up an in-memory `Store`, a temp `MaildirStore`,
//! seed an `emails` row that `mark_body_downloaded` can update, and
//! drive `process_body` directly. We do not need a real IMAP session
//! because `process_body` is a pure function over `(raw_bytes, store,
//! maildir)` — the IMAP fetch happens upstream.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use inboxly_core::EmailFlags;
use inboxly_imap::body_processor::process_body;
use inboxly_store::{AccountRow, EmailRow, MaildirStore, StandardFolder, Store};

const ACCOUNT_ID: &str = "test-account";
const THREAD_ID: &str = "test-thread";
const MESSAGE_ID: &str = "<phase4b-dedup@inboxly.local>";

/// Build a minimal RFC 5322 message body with the given Message-ID and
/// subject. The body has a stable shape so the test is deterministic.
fn make_eml(message_id: &str, subject: &str) -> Vec<u8> {
    format!(
        "From: sender@example.com\r\n\
         To: recipient@example.com\r\n\
         Subject: {subject}\r\n\
         Message-ID: {message_id}\r\n\
         Date: Mon, 07 Apr 2026 12:00:00 +0000\r\n\
         Content-Type: text/plain\r\n\
         \r\n\
         Body text for the dedup test.\r\n"
    )
    .into_bytes()
}

/// Insert a minimal account row. `emails.account_id` has a foreign-key
/// constraint on `accounts(id)`, so we must seed this before any email
/// row can be inserted. Field values are placeholders — only `id` is
/// referenced.
fn seed_account(store: &Store) {
    let account = AccountRow {
        id: ACCOUNT_ID.to_string(),
        email: "test@example.com".to_string(),
        display_name: "Test Account".to_string(),
        provider: "imap".to_string(),
        auth_method: "password".to_string(),
        imap_host: "imap.example.com".to_string(),
        imap_port: 993,
        smtp_host: "smtp.example.com".to_string(),
        smtp_port: 465,
    };
    store.insert_account(&account).expect("seed account row");
}

/// Insert a minimal `emails` row so `mark_body_downloaded` (which is an
/// UPDATE) has something to target. The exact field values don't matter
/// for the dedup logic — only that `id` matches what we pass to
/// `process_body`.
fn seed_email_row(store: &Store, id: &str) {
    let row = EmailRow {
        id: id.to_string(),
        account_id: ACCOUNT_ID.to_string(),
        thread_id: THREAD_ID.to_string(),
        from_name: Some("Sender".to_string()),
        from_address: "sender@example.com".to_string(),
        to_json: "[]".to_string(),
        cc_json: "[]".to_string(),
        subject: "Dedup test".to_string(),
        snippet: String::new(),
        date: 0,
        // maildir_path starts empty — process_body will populate it.
        maildir_path: String::new(),
        flags: 0,
        size_bytes: 0,
        imap_uid: 1,
        imap_folder: "Drafts".to_string(),
        has_attachments: false,
        body_downloaded: false,
        message_id_header: Some(MESSAGE_ID.to_string()),
        in_reply_to: None,
        references_json: None,
    };
    store.insert_email(&row).expect("seed emails row");
}

/// Count the .eml files inside the `cur/` and `new/` subdirs of the
/// given Maildir folder. For `StandardFolder::Inbox` the folder directory
/// IS the store root (Maildir++ convention); for every other folder the
/// directory is `<root>/<folder.dirname()>`.
///
/// Used to assert "exactly one file exists, no duplicate was written".
fn count_eml_files(maildir_root: &Path, folder_subdir: Option<&str>) -> usize {
    let folder_dir = match folder_subdir {
        Some(name) => maildir_root.join(name),
        None => maildir_root.to_path_buf(),
    };
    let mut count = 0_usize;
    for sub in ["cur", "new"] {
        let dir = folder_dir.join(sub);
        if !dir.exists() {
            continue;
        }
        let Ok(rd) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd {
            let Ok(entry) = entry else { continue };
            if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                count = count
                    .checked_add(1)
                    .expect("eml file count overflow — impossible in tests");
            }
        }
    }
    count
}

#[test]
fn process_body_skips_duplicate_draft_write_and_points_at_existing_file() {
    let tmp = TempDir::new().expect("tmpdir");
    let maildir = MaildirStore::new(tmp.path().to_path_buf());
    maildir.init().expect("init maildir");
    let store = Store::open_in_memory().expect("in-memory store");

    // === Compose path: write the local draft FIRST ===
    //
    // This simulates what the compose-time save bridge does: drop the
    // .eml directly into .Drafts/ via store_cur. The IMAP server-side
    // APPEND would happen in parallel; we don't need to model it
    // because the test only cares about the sync return path.
    let raw = make_eml(MESSAGE_ID, "Draft from compose");
    let flags = EmailFlags {
        draft: true,
        ..Default::default()
    };
    let pre_existing = maildir
        .store_cur(&StandardFolder::Drafts, &raw, &flags)
        .expect("compose-time write to .Drafts succeeds");
    let pre_existing_path = pre_existing.path.to_string_lossy().into_owned();

    // Sanity check: exactly one .eml file in .Drafts after the compose
    // write.
    assert_eq!(
        count_eml_files(tmp.path(), Some(".Drafts")),
        1,
        "compose-time write must produce exactly one .eml"
    );

    // Seed the SQLite emails row that the sync path will UPDATE.
    let email_id = "sync-fetched-email-id";
    seed_account(&store);
    seed_email_row(&store, email_id);

    // === Sync path: process_body sees the same Message-ID coming back ===
    //
    // The IMAP downloader has just fetched RFC822 bytes for what looks
    // like a new draft. Without dedup, process_body would store_cur it
    // into .Drafts/ a second time. With dedup, it must:
    //   1. Detect the duplicate
    //   2. Skip the second write
    //   3. Return the path of the pre-existing file
    //   4. Mark the SQLite row as downloaded with that path
    let returned_path = process_body(email_id, "Drafts", &raw, &flags, &maildir, &store)
        .expect("process_body must succeed when dedup hits");

    // Assertion 1: process_body returned the EXISTING file's path, not
    // a freshly minted one.
    assert_eq!(
        returned_path, pre_existing_path,
        "process_body must return the pre-existing file path on dedup hit"
    );

    // Assertion 2: NO duplicate file was written. Still exactly one
    // .eml in .Drafts.
    assert_eq!(
        count_eml_files(tmp.path(), Some(".Drafts")),
        1,
        "dedup must NOT write a second .eml — found duplicate"
    );

    // Assertion 3: the SQLite row was updated. body_downloaded=true and
    // maildir_path matches the pre-existing path.
    let updated = store.get_email(email_id).expect("re-read seeded row");
    assert!(
        updated.body_downloaded,
        "process_body must set body_downloaded=true even on dedup hit"
    );
    assert_eq!(
        updated.maildir_path, pre_existing_path,
        "maildir_path must point at the pre-existing compose-time file"
    );
}

#[test]
fn process_body_writes_normally_for_inbox_even_with_matching_message_id() {
    // Counter-test: the dedup short-circuit must NOT trigger for
    // non-Drafts/Sent folders. Even if the same Message-ID happens to
    // be present in the Drafts folder (pathological case), an Inbox
    // sync fetch must still write a fresh .eml to the Inbox maildir —
    // Inbox is the destination of record and skipping the write would
    // lose the message.
    //
    // In the Maildir++ layout used by `MaildirStore`, the Inbox folder
    // IS the store root: files land in `<root>/cur/` and `<root>/new/`,
    // not `<root>/.INBOX/...`.
    let tmp = TempDir::new().expect("tmpdir");
    let maildir = MaildirStore::new(tmp.path().to_path_buf());
    maildir.init().expect("init maildir");
    let store = Store::open_in_memory().expect("in-memory store");

    let raw = make_eml(MESSAGE_ID, "Inbox copy");
    let flags = EmailFlags::default();

    // Pre-seed a file with the matching Message-ID in .Drafts — the
    // worst case for the dedup logic: the needle exists in the sibling
    // folder we would normally short-circuit. Inbox writes must still
    // happen because the folder is neither Drafts nor Sent.
    maildir
        .store_cur(
            &StandardFolder::Drafts,
            &raw,
            &EmailFlags {
                draft: true,
                ..Default::default()
            },
        )
        .expect("seed .Drafts file");
    assert_eq!(count_eml_files(tmp.path(), Some(".Drafts")), 1);
    assert_eq!(
        count_eml_files(tmp.path(), None),
        0,
        "Inbox must be empty before process_body runs"
    );

    // Seed the SQLite row and run the sync path against the Inbox
    // folder name.
    let email_id = "inbox-email-id";
    seed_account(&store);
    seed_email_row(&store, email_id);

    let returned_path = process_body(email_id, "INBOX", &raw, &flags, &maildir, &store)
        .expect("process_body must succeed for Inbox");

    // Inbox must have gained exactly one .eml — dedup did NOT run.
    assert_eq!(
        count_eml_files(tmp.path(), None),
        1,
        "Inbox must have received exactly one new .eml"
    );
    // Drafts must remain at one file — Inbox processing must not touch it.
    assert_eq!(
        count_eml_files(tmp.path(), Some(".Drafts")),
        1,
        "Drafts file must remain untouched during Inbox processing"
    );
    // The returned path must live inside the Inbox root, not inside .Drafts.
    assert!(
        !returned_path.contains("/.Drafts/"),
        "Inbox write path must not resolve to the Drafts folder, got: {returned_path}"
    );

    // Confirm the SQLite row was updated with the new path.
    let updated = store.get_email(email_id).expect("re-read seeded row");
    assert!(
        updated.body_downloaded,
        "body_downloaded must be set after Inbox write"
    );
    assert_eq!(updated.maildir_path, returned_path);
}
