//! Integration tests for the drafts Store API.
//!
//! All tests use an in-memory SQLite DB — no temp files. Each test runs the
//! v4 -> v5 migration automatically via `Store::open_in_memory()`.

use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use tempfile::TempDir;
use uuid::Uuid;

use inboxly_core::{
    AccountId, AttachmentDraft, AttachmentSource, ComposeMode, Contact, DraftEmail,
};
use inboxly_store::{MaildirStore, StandardFolder, Store};

/// Build a sample draft. `id` and `message_id` are caller-supplied so each
/// test can place multiple drafts in the same DB without UNIQUE conflicts.
fn sample_draft(id: &str, message_id: &str, account_id: AccountId) -> DraftEmail {
    let now = Utc::now();
    DraftEmail {
        id: id.to_string(),
        account_id,
        message_id: message_id.to_string(),
        subject: "Test subject".to_string(),
        body_markdown: "Hello **world**".to_string(),
        to: vec![Contact::new("Alice", "alice@example.com")],
        cc: vec![],
        bcc: vec![Contact::new("", "secret@example.com")],
        attachments: vec![],
        mode: ComposeMode::New,
        in_reply_to: None,
        references: None,
        maildir_path: None,
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn insert_and_get_round_trip() {
    let store = Store::open_in_memory().expect("open in-memory store");
    let account = AccountId(Uuid::new_v4());
    let draft = sample_draft("d1", "<d1@inboxly.local>", account);
    store.insert_draft(&draft).expect("insert");

    let row = store
        .get_draft("d1")
        .expect("get_draft")
        .expect("draft present");
    assert_eq!(row.id, "d1");
    assert_eq!(row.message_id, "<d1@inboxly.local>");
    assert_eq!(row.subject, "Test subject");
    assert_eq!(row.body_markdown, "Hello **world**");
    assert_eq!(row.account_id, account.to_string());
    assert!(row.to_json.contains("alice@example.com"));
    assert!(row.bcc_json.contains("secret@example.com"));

    // Round-trip via into_draft.
    let round = row.into_draft().expect("into_draft");
    assert_eq!(round.account_id, account);
    assert_eq!(round.to.len(), 1);
    assert_eq!(round.to[0].address, "alice@example.com");
    assert_eq!(round.bcc.len(), 1);
    assert_eq!(round.bcc[0].address, "secret@example.com");
    assert_eq!(round.mode, ComposeMode::New);
    assert_eq!(round.in_reply_to, None);
    assert_eq!(round.references, None);
    assert_eq!(round.maildir_path, None);
}

#[test]
fn update_draft_preserves_id_and_changes_fields() {
    let store = Store::open_in_memory().expect("open in-memory store");
    let account = AccountId(Uuid::new_v4());
    let mut draft = sample_draft("d1", "<d1@inboxly.local>", account);
    let original_created = draft.created_at;
    store.insert_draft(&draft).expect("insert");

    // Modify subject + body, keep id stable.
    draft.subject = "Edited subject".to_string();
    draft.body_markdown = "Edited body".to_string();
    draft.updated_at = Utc::now();
    store.update_draft(&draft).expect("update");

    let row = store
        .get_draft("d1")
        .expect("get_draft")
        .expect("draft present");
    assert_eq!(row.id, "d1", "id unchanged");
    assert_eq!(row.subject, "Edited subject");
    assert_eq!(row.body_markdown, "Edited body");
    // created_at must NOT have moved — it's immutable after first insert.
    assert_eq!(
        row.created_at,
        original_created.timestamp(),
        "created_at must be preserved across updates"
    );
}

#[test]
fn update_missing_draft_returns_not_found() {
    let store = Store::open_in_memory().expect("open in-memory store");
    let account = AccountId(Uuid::new_v4());
    let draft = sample_draft("ghost", "<ghost@inboxly.local>", account);
    let err = store
        .update_draft(&draft)
        .expect_err("update of nonexistent draft must fail");
    let msg = format!("{err}");
    assert!(
        msg.contains("ghost"),
        "expected NotFound containing 'ghost', got: {msg}"
    );
}

#[test]
fn list_drafts_filters_by_account_and_orders_newest_first() {
    let store = Store::open_in_memory().expect("open in-memory store");
    let account_a = AccountId(Uuid::new_v4());
    let account_b = AccountId(Uuid::new_v4());

    // d1 is older, d2 is newer (both on account_a).
    let mut d1 = sample_draft("d1", "<d1@inboxly.local>", account_a);
    let now = Utc::now();
    d1.updated_at = now - chrono::Duration::seconds(60);
    let mut d2 = sample_draft("d2", "<d2@inboxly.local>", account_a);
    d2.updated_at = now;
    let d3 = sample_draft("d3", "<d3@inboxly.local>", account_b);

    store.insert_draft(&d1).expect("insert d1");
    store.insert_draft(&d2).expect("insert d2");
    store.insert_draft(&d3).expect("insert d3");

    let account_a_drafts = store
        .list_drafts(&account_a.to_string())
        .expect("list account_a");
    assert_eq!(account_a_drafts.len(), 2);
    // Newest first: d2 then d1.
    assert_eq!(account_a_drafts[0].id, "d2");
    assert_eq!(account_a_drafts[1].id, "d1");

    let account_b_drafts = store
        .list_drafts(&account_b.to_string())
        .expect("list account_b");
    assert_eq!(account_b_drafts.len(), 1);
    assert_eq!(account_b_drafts[0].id, "d3");

    // Unknown account → empty.
    let unknown = AccountId(Uuid::new_v4());
    let empty = store
        .list_drafts(&unknown.to_string())
        .expect("list unknown");
    assert!(empty.is_empty());
}

#[test]
fn delete_draft_removes_row_and_second_delete_is_not_found() {
    let store = Store::open_in_memory().expect("open in-memory store");
    let account = AccountId(Uuid::new_v4());
    store
        .insert_draft(&sample_draft("d1", "<d1@inboxly.local>", account))
        .expect("insert");

    store.delete_draft("d1").expect("delete");
    let row = store.get_draft("d1").expect("get_draft after delete");
    assert!(row.is_none(), "draft should be gone after delete");

    // Second delete returns NotFound.
    let err = store
        .delete_draft("d1")
        .expect_err("re-delete must fail with NotFound");
    assert!(format!("{err}").contains("d1"));
}

#[test]
fn draft_with_attachments_round_trips() {
    let store = Store::open_in_memory().expect("open in-memory store");
    let account = AccountId(Uuid::new_v4());
    let mut draft = sample_draft("d1", "<d1@inboxly.local>", account);
    draft.attachments = vec![
        AttachmentDraft {
            filename: "invoice.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            size_bytes: 4096,
            source: AttachmentSource::Disk(PathBuf::from(
                "/home/alan/.local/share/inboxly/drafts/d1/invoice-abcdef.pdf",
            )),
        },
        AttachmentDraft {
            filename: "photo.jpg".to_string(),
            mime_type: "image/jpeg".to_string(),
            size_bytes: 8192,
            source: AttachmentSource::Disk(PathBuf::from(
                "/home/alan/.local/share/inboxly/drafts/d1/photo-123abc.jpg",
            )),
        },
    ];
    draft.in_reply_to = Some("<parent@example.com>".to_string());
    draft.references = Some("<root@example.com> <parent@example.com>".to_string());
    draft.maildir_path = Some(PathBuf::from("/home/alan/Mail/.Drafts/cur/d1:2,DS"));
    store.insert_draft(&draft).expect("insert");

    let row = store
        .get_draft("d1")
        .expect("get_draft")
        .expect("draft present");
    let round = row.into_draft().expect("into_draft");
    assert_eq!(round.attachments.len(), 2);
    assert_eq!(round.attachments[0].filename, "invoice.pdf");
    assert_eq!(round.attachments[0].size_bytes, 4096);
    assert_eq!(round.attachments[0].mime_type, "application/pdf");
    assert_eq!(round.attachments[1].filename, "photo.jpg");
    assert_eq!(round.attachments[1].mime_type, "image/jpeg");
    let AttachmentSource::Disk(path) = &round.attachments[0].source;
    assert!(path.to_string_lossy().contains("invoice-abcdef.pdf"));
    assert_eq!(round.in_reply_to.as_deref(), Some("<parent@example.com>"));
    assert_eq!(
        round.references.as_deref(),
        Some("<root@example.com> <parent@example.com>")
    );
    assert_eq!(
        round.maildir_path.as_deref(),
        Some(std::path::Path::new("/home/alan/Mail/.Drafts/cur/d1:2,DS"))
    );
}

// ===== M35b Phase 4: MaildirStore::has_message_id =====

/// Build a minimal RFC 5322 message body containing the given Message-ID.
fn make_eml(message_id: &str) -> Vec<u8> {
    format!(
        "From: sender@example.com\r\n\
         To: recipient@example.com\r\n\
         Subject: Test\r\n\
         Message-ID: {message_id}\r\n\
         Date: Mon, 07 Apr 2026 12:00:00 +0000\r\n\
         \r\n\
         Body text.\r\n"
    )
    .into_bytes()
}

#[test]
fn has_message_id_finds_existing_and_skips_unrelated() {
    let tmp = TempDir::new().expect("tmpdir");
    let store = MaildirStore::new(tmp.path().to_path_buf());
    store.init().expect("init maildir");

    // Drop two drafts into .Drafts/cur/ via the public store_cur API.
    let target_eml = make_eml("<target@inboxly.local>");
    let other_eml = make_eml("<other@example.com>");
    store
        .store_cur(
            &StandardFolder::Drafts,
            &target_eml,
            &inboxly_core::EmailFlags {
                draft: true,
                ..Default::default()
            },
        )
        .expect("store target draft");
    store
        .store_cur(
            &StandardFolder::Drafts,
            &other_eml,
            &inboxly_core::EmailFlags {
                draft: true,
                ..Default::default()
            },
        )
        .expect("store other draft");

    // Exact bracketed match.
    assert!(
        store
            .has_message_id(StandardFolder::Drafts, "<target@inboxly.local>")
            .expect("has_message_id ok"),
        "expected exact bracketed Message-ID to match"
    );

    // Without brackets — the helper must normalise both sides.
    assert!(
        store
            .has_message_id(StandardFolder::Drafts, "target@inboxly.local")
            .expect("has_message_id ok"),
        "expected unbracketed Message-ID to normalise and match"
    );

    // A different Message-ID must not match.
    assert!(
        !store
            .has_message_id(StandardFolder::Drafts, "<nonexistent@example.com>")
            .expect("has_message_id ok"),
        "non-existent Message-ID must not match"
    );

    // The Sent folder is initialised but empty — must return Ok(false), not Err.
    assert!(
        !store
            .has_message_id(StandardFolder::Sent, "<target@inboxly.local>")
            .expect("has_message_id on empty folder must succeed"),
        "empty folder lookup must return Ok(false)"
    );

    // A folder whose directory has been removed entirely must still
    // return Ok(false) (mirrors the fresh-account case where init() has
    // not yet run for that subfolder).
    let trash_dir = tmp.path().join(".Trash");
    fs::remove_dir_all(&trash_dir).expect("remove .Trash dir");
    assert!(
        !store
            .has_message_id(StandardFolder::Trash, "<target@inboxly.local>")
            .expect("has_message_id on missing folder must succeed"),
        "missing folder lookup must return Ok(false)"
    );

    // Empty needle must return Ok(false) — never matches (would be an
    // accidental wildcard against headerless messages).
    assert!(
        !store
            .has_message_id(StandardFolder::Drafts, "")
            .expect("has_message_id ok"),
        "empty Message-ID must not match anything"
    );
    assert!(
        !store
            .has_message_id(StandardFolder::Drafts, "<>")
            .expect("has_message_id ok"),
        "bracket-only Message-ID must not match anything"
    );
}
