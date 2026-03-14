use inboxly_core::{AccountId, EmailFlags};
use inboxly_store::{
    MaildirStore, StandardFolder,
    parse_email_meta,
};
use std::path::PathBuf;
use tempfile::TempDir;
use uuid::Uuid;

// ===== Task 13 Step 1: Initialization tests =====

#[test]
fn test_init_creates_all_directories() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    // INBOX directories
    assert!(tmp.path().join("new").is_dir());
    assert!(tmp.path().join("cur").is_dir());
    assert!(tmp.path().join("tmp").is_dir());

    // Subfolder directories
    for folder in &[".Sent", ".Drafts", ".Trash", ".Spam"] {
        assert!(
            tmp.path().join(folder).join("new").is_dir(),
            "missing {folder}/new"
        );
        assert!(
            tmp.path().join(folder).join("cur").is_dir(),
            "missing {folder}/cur"
        );
        assert!(
            tmp.path().join(folder).join("tmp").is_dir(),
            "missing {folder}/tmp"
        );
    }
}

#[test]
fn test_init_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();
    store.init().unwrap(); // second call must not error
}

// ===== Task 13 Step 2: Store and list =====

#[test]
fn test_store_new_and_list() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    let eml = include_bytes!("fixtures/simple.eml");
    let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();

    assert!(!stored.id.is_empty());

    let messages = store.list_messages(&StandardFolder::Inbox).unwrap();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].is_new);
    assert_eq!(messages[0].id, stored.id);
}

// ===== Task 13 Step 3: Deliver and flag operations =====

#[test]
fn test_deliver_moves_to_cur() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    let eml = include_bytes!("fixtures/simple.eml");
    let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();

    // Deliver with read flag
    let flags = EmailFlags {
        read: true,
        ..Default::default()
    };
    store
        .deliver(&StandardFolder::Inbox, &stored.id, &flags)
        .unwrap();

    let messages = store.list_messages(&StandardFolder::Inbox).unwrap();
    assert_eq!(messages.len(), 1);
    assert!(!messages[0].is_new); // now in cur/
    assert!(messages[0].flags.read);
}

#[test]
fn test_set_flags() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    let eml = include_bytes!("fixtures/simple.eml");
    let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();
    store
        .deliver_unread(&StandardFolder::Inbox, &stored.id)
        .unwrap();

    // Set starred + read
    let flags = EmailFlags {
        read: true,
        starred: true,
        ..Default::default()
    };
    store
        .set_flags(&StandardFolder::Inbox, &stored.id, &flags)
        .unwrap();

    let messages = store.list_messages(&StandardFolder::Inbox).unwrap();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].flags.read);
    assert!(messages[0].flags.starred);
}

// ===== Task 13 Step 4: parse_email_meta =====

#[test]
fn test_parse_simple_email_meta() {
    let data = include_bytes!("fixtures/simple.eml");
    let meta = parse_email_meta(
        data,
        AccountId(Uuid::nil()),
        PathBuf::from("/tmp/test.eml"),
        EmailFlags::default(),
        42,
        "INBOX".to_string(),
    )
    .unwrap();

    assert_eq!(meta.id.0, "msg001@example.com");
    assert_eq!(meta.from.name, "Alice Smith");
    assert_eq!(meta.from.address, "alice@example.com");
    assert_eq!(meta.to.len(), 1);
    assert_eq!(meta.to[0].address, "bob@example.com");
    assert_eq!(meta.subject, "Hello from Inboxly");
    assert!(meta.snippet.contains("test email"));
    assert_eq!(meta.imap_uid, 42);
    assert!(meta.attachments.is_empty()); // no attachments
}

#[test]
fn test_parse_multipart_email_meta() {
    let data = include_bytes!("fixtures/multipart.eml");
    let meta = parse_email_meta(
        data,
        AccountId(Uuid::nil()),
        PathBuf::from("/tmp/test.eml"),
        EmailFlags::default(),
        0,
        "INBOX".to_string(),
    )
    .unwrap();

    assert_eq!(meta.id.0, "msg002@example.com");
    assert!(meta.snippet.contains("Weekly digest"));
    assert!(meta.attachments.is_empty()); // no attachments
}

#[test]
fn test_parse_email_with_attachment() {
    let data = include_bytes!("fixtures/with_attachment.eml");
    let meta = parse_email_meta(
        data,
        AccountId(Uuid::nil()),
        PathBuf::from("/tmp/test.eml"),
        EmailFlags::default(),
        0,
        "INBOX".to_string(),
    )
    .unwrap();

    assert_eq!(meta.id.0, "msg003@example.com");
    assert!(!meta.attachments.is_empty());
    assert_eq!(meta.attachments.len(), 1);
    // AttachmentMeta uses `filename` field
    assert_eq!(meta.attachments[0].filename, "report.pdf");
    assert_eq!(meta.attachments[0].mime_type, "application/pdf");
}

#[test]
fn test_parse_reply_email_meta() {
    let data = include_bytes!("fixtures/reply_in_thread.eml");
    let meta = parse_email_meta(
        data,
        AccountId(Uuid::nil()),
        PathBuf::from("/tmp/test.eml"),
        EmailFlags::default(),
        0,
        "INBOX".to_string(),
    )
    .unwrap();

    // Verify the reply is parsed correctly; In-Reply-To/References
    // are stored in EmailRow (not EmailMeta) — just check identity here.
    assert_eq!(meta.id.0, "msg004@example.com");
    assert_eq!(meta.from.address, "bob@example.com");
    assert_eq!(meta.to[0].address, "alice@example.com");
    assert!(meta.snippet.contains("Thanks"));
}

// ===== Task 13 Step 5: read_email_content (lazy load) =====

#[test]
fn test_read_email_content() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    let eml = include_bytes!("fixtures/with_attachment.eml");
    let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();
    store
        .deliver_unread(&StandardFolder::Inbox, &stored.id)
        .unwrap();

    // Get the path from listing
    let messages = store.list_messages(&StandardFolder::Inbox).unwrap();
    assert_eq!(messages.len(), 1);
    let content = store.read_email_content(&messages[0].path).unwrap();

    assert!(content
        .body_text
        .as_deref()
        .unwrap_or("")
        .contains("document attached"));
    assert_eq!(content.attachments.len(), 1);
    // Attachment uses nested meta struct
    assert_eq!(content.attachments[0].meta.filename, "report.pdf");
    assert!(!content.attachments[0].content.is_empty());
}

// ===== Task 13 Step 6: Delete and move =====

#[test]
fn test_delete_message() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    let eml = include_bytes!("fixtures/simple.eml");
    let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();
    store
        .deliver_unread(&StandardFolder::Inbox, &stored.id)
        .unwrap();

    assert_eq!(store.count_messages(&StandardFolder::Inbox), 1);

    store
        .delete_message(&StandardFolder::Inbox, &stored.id)
        .unwrap();

    assert_eq!(store.count_messages(&StandardFolder::Inbox), 0);
}

#[test]
fn test_move_between_folders() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    let eml = include_bytes!("fixtures/simple.eml");
    let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();
    store
        .deliver_unread(&StandardFolder::Inbox, &stored.id)
        .unwrap();

    // Move from Inbox to Trash
    store
        .move_message(&StandardFolder::Inbox, &StandardFolder::Trash, &stored.id)
        .unwrap();

    assert_eq!(store.count_messages(&StandardFolder::Inbox), 0);
    assert_eq!(store.count_messages(&StandardFolder::Trash), 1);
}

// ===== Task 13 Step 7: scan_folder (rebuild) =====

#[test]
fn test_scan_folder() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    // Store several emails (note: same Message-ID in multiple emails is fine for
    // Maildir — they get unique filenames; parse_email_meta uses Message-ID as
    // the logical ID).
    let fixtures: &[&[u8]] = &[
        include_bytes!("fixtures/simple.eml"),
        include_bytes!("fixtures/multipart.eml"),
        include_bytes!("fixtures/with_attachment.eml"),
        include_bytes!("fixtures/reply_in_thread.eml"),
    ];

    for eml in fixtures {
        let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();
        store
            .deliver_unread(&StandardFolder::Inbox, &stored.id)
            .unwrap();
    }

    let (metas, errors) = store.scan_folder(&StandardFolder::Inbox, AccountId(Uuid::nil()));

    assert!(errors.is_empty(), "scan errors: {:?}", errors);
    assert_eq!(metas.len(), 4);

    // Verify all unique Message-IDs were parsed
    let ids: Vec<&str> = metas.iter().map(|m| m.id.0.as_str()).collect();
    assert!(ids.contains(&"msg001@example.com"));
    assert!(ids.contains(&"msg002@example.com"));
    assert!(ids.contains(&"msg003@example.com"));
    assert!(ids.contains(&"msg004@example.com"));
}

// ===== Additional: copy_message =====

#[test]
fn test_copy_between_folders() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    let eml = include_bytes!("fixtures/simple.eml");
    let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();
    store
        .deliver_unread(&StandardFolder::Inbox, &stored.id)
        .unwrap();

    // Copy from Inbox to Sent — original remains
    store
        .copy_message(&StandardFolder::Inbox, &StandardFolder::Sent, &stored.id)
        .unwrap();

    assert_eq!(store.count_messages(&StandardFolder::Inbox), 1);
    assert_eq!(store.count_messages(&StandardFolder::Sent), 1);
}

// ===== Additional: store_cur (direct to cur with flags) =====

#[test]
fn test_store_cur_with_flags() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    let eml = include_bytes!("fixtures/simple.eml");
    let flags = EmailFlags {
        read: true,
        starred: true,
        ..Default::default()
    };
    let stored = store.store_cur(&StandardFolder::Sent, eml, &flags).unwrap();
    assert!(!stored.id.is_empty());

    let messages = store.list_messages(&StandardFolder::Sent).unwrap();
    assert_eq!(messages.len(), 1);
    assert!(!messages[0].is_new); // stored directly in cur/
    assert!(messages[0].flags.read);
    assert!(messages[0].flags.starred);
}
