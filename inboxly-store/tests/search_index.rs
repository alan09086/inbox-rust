use inboxly_store::search::schema::SearchSchema;
use inboxly_store::search::SearchIndex;

use inboxly_core::{
    AccountId, AttachmentMeta, BundleCategory, Contact, EmailFlags, EmailId, EmailMeta, ThreadId,
};
use chrono::{TimeZone, Utc};
use std::path::PathBuf;
use tempfile::TempDir;
use uuid::Uuid;

use tantivy::collector::TopDocs;
use tantivy::query::TermQuery;
use tantivy::schema::IndexRecordOption;
use tantivy::Term;

fn make_test_email() -> EmailMeta {
    EmailMeta {
        id: EmailId::new("test-msg-001@example.com"),
        account_id: AccountId(Uuid::nil()),
        thread_id: ThreadId(Uuid::nil()),
        from: Contact {
            name: "Alice Smith".to_string(),
            address: "alice@example.com".to_string(),
        },
        to: vec![Contact {
            name: "Bob Jones".to_string(),
            address: "bob@example.com".to_string(),
        }],
        cc: vec![],
        subject: "Meeting tomorrow at noon".to_string(),
        snippet: "Let's meet at the coffee shop...".to_string(),
        date: Utc.with_ymd_and_hms(2026, 3, 14, 10, 30, 0).unwrap(),
        maildir_path: PathBuf::from("/tmp/maildir/cur/test.eml"),
        attachments: vec![AttachmentMeta {
            filename: "report.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            size_bytes: 1024,
        }],
        flags: EmailFlags::default(),
        size_bytes: 4096,
        imap_uid: 42,
        imap_folder: "INBOX".to_string(),
    }
}

// ── Task 2: Schema tests ────────────────────────────────────────────────────

#[test]
fn schema_has_all_expected_fields() {
    let search_schema = SearchSchema::new();
    let schema = search_schema.schema();

    // All 9 fields must exist
    assert!(schema.get_field("email_id").is_ok());
    assert!(schema.get_field("from").is_ok());
    assert!(schema.get_field("to").is_ok());
    assert!(schema.get_field("subject").is_ok());
    assert!(schema.get_field("body_text").is_ok());
    assert!(schema.get_field("date").is_ok());
    assert!(schema.get_field("account_id").is_ok());
    assert!(schema.get_field("bundle_category").is_ok());
    assert!(schema.get_field("has_attachment").is_ok());
}

// ── Task 3: Document conversion ────────────────────────────────────────────

#[test]
fn build_document_from_email_meta() {
    let search_schema = SearchSchema::new();
    let email = make_test_email();
    let body = Some("Let's meet at the coffee shop to discuss the project timeline.");
    let category = Some(BundleCategory::Updates);

    let doc = search_schema.build_document(&email, body, category.as_ref());

    // Document should be a valid tantivy document (we just verify it doesn't panic
    // and can be added to an index).
    let index = tantivy::Index::create_in_ram(search_schema.schema().clone());
    let mut writer = index.writer(15_000_000).unwrap();
    writer.add_document(doc).unwrap();
    writer.commit().unwrap();

    let reader = index.reader().unwrap();
    let searcher = reader.searcher();
    assert_eq!(searcher.num_docs(), 1);
}

// ── Task 4: SearchIndex lifecycle ──────────────────────────────────────────

#[test]
fn create_new_index_on_disk() {
    let tmp = TempDir::new().unwrap();
    let index_path = tmp.path().join("search_index");

    let _search_index = SearchIndex::create(&index_path).unwrap();

    // Index directory should exist and be non-empty
    assert!(index_path.exists());
    assert!(index_path.join("meta.json").exists());
}

#[test]
fn open_existing_index() {
    let tmp = TempDir::new().unwrap();
    let index_path = tmp.path().join("search_index");

    // Create first
    {
        let _idx = SearchIndex::create(&index_path).unwrap();
    }

    // Reopen
    let search_index = SearchIndex::open(&index_path).unwrap();
    assert_eq!(search_index.num_docs(), 0);
}

#[test]
fn open_or_create_when_missing() {
    let tmp = TempDir::new().unwrap();
    let index_path = tmp.path().join("search_index");

    let search_index = SearchIndex::open_or_create(&index_path).unwrap();
    assert_eq!(search_index.num_docs(), 0);
}

#[test]
fn open_or_create_when_existing() {
    let tmp = TempDir::new().unwrap();
    let index_path = tmp.path().join("search_index");

    // Create and index a doc
    {
        let mut_idx = SearchIndex::create(&index_path).unwrap();
        let email = make_test_email();
        mut_idx.add_email(&email, None, None).unwrap();
        mut_idx.commit().unwrap();
    }

    // Reopen — doc should still be there
    let idx = SearchIndex::open_or_create(&index_path).unwrap();
    assert_eq!(idx.num_docs(), 1);
}

// ── Task 5: Single email indexing ──────────────────────────────────────────

#[test]
fn index_single_email_and_find_by_email_id() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();
    let email = make_test_email();

    idx.add_email(&email, Some("Hello world body text"), None).unwrap();
    idx.commit().unwrap();

    assert_eq!(idx.num_docs(), 1);

    // Search by exact email_id
    let searcher = idx.reader().searcher();
    let term = Term::from_field_text(idx.schema.email_id, "test-msg-001@example.com");
    let query = TermQuery::new(term, IndexRecordOption::Basic);
    let results = searcher.search(&query, &TopDocs::with_limit(10)).unwrap();
    assert_eq!(results.len(), 1);
}

// ── Task 6: Batch indexing ─────────────────────────────────────────────────

#[test]
fn batch_index_multiple_emails() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    let emails: Vec<(EmailMeta, &str)> = (0..100)
        .map(|i| {
            let mut email = make_test_email();
            email.id = EmailId::new(format!("msg-{}@example.com", i));
            email.subject = format!("Test email number {}", i);
            (email, "Batch body text for searching")
        })
        .collect();

    let items: Vec<(&EmailMeta, Option<&str>, Option<&BundleCategory>)> = emails
        .iter()
        .map(|(e, body)| (e, Some(*body), None))
        .collect();

    idx.batch_index(&items).unwrap();

    assert_eq!(idx.num_docs(), 100);
}

// ── Task 7: Remove email ────────────────────────────────────────────────────

#[test]
fn remove_email_from_index() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    let email = make_test_email();
    idx.add_email(&email, Some("body text"), None).unwrap();
    idx.commit().unwrap();
    assert_eq!(idx.num_docs(), 1);

    idx.remove_email(&email.id).unwrap();
    idx.commit().unwrap();
    assert_eq!(idx.num_docs(), 0);
}

#[test]
fn remove_nonexistent_email_is_noop() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    // Removing from empty index should not error
    let fake_id = EmailId::new("nonexistent@example.com");
    idx.remove_email(&fake_id).unwrap();
    idx.commit().unwrap();
    assert_eq!(idx.num_docs(), 0);
}

// ── Task 8: Update email ────────────────────────────────────────────────────

#[test]
fn update_email_in_index() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    let email = make_test_email();

    // First index without body
    idx.add_email(&email, None, None).unwrap();
    idx.commit().unwrap();
    assert_eq!(idx.num_docs(), 1);

    // Update with body and category
    idx.update_email(&email, Some("Now with body text!"), Some(&BundleCategory::Social))
        .unwrap();
    idx.commit().unwrap();

    // Should still be exactly 1 document (not 2)
    assert_eq!(idx.num_docs(), 1);

    // Body text should now be searchable
    let results = idx.search_simple("body", 10).unwrap();
    assert_eq!(results.len(), 1);
}
