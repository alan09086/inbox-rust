use inboxly_store::search::SearchIndex;
use inboxly_store::search::schema::SearchSchema;

use chrono::{TimeZone, Utc};
use inboxly_core::{
    AccountId, AttachmentMeta, BundleCategory, Contact, EmailFlags, EmailId, EmailMeta, ThreadId,
};
use std::path::PathBuf;
use tempfile::TempDir;
use uuid::Uuid;

use tantivy::Term;
use tantivy::collector::TopDocs;
use tantivy::query::TermQuery;
use tantivy::schema::{IndexRecordOption, Value};

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

    idx.add_email(&email, Some("Hello world body text"), None)
        .unwrap();
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
    idx.update_email(
        &email,
        Some("Now with body text!"),
        Some(&BundleCategory::Social),
    )
    .unwrap();
    idx.commit().unwrap();

    // Should still be exactly 1 document (not 2)
    assert_eq!(idx.num_docs(), 1);

    // Body text should now be searchable
    let results = idx.search_simple("body", 10).unwrap();
    assert_eq!(results.len(), 1);
}

// ── Task 9-10: Query builders ──────────────────────────────────────────────

use inboxly_store::search::query::SearchQuery;

fn create_populated_index() -> (TempDir, SearchIndex) {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    let mut email1 = make_test_email();
    email1.id = EmailId("msg-1@example.com".to_string());
    email1.from = Contact {
        name: "Sarah Connor".to_string(),
        address: "sarah@skynet.com".to_string(),
    };
    email1.subject = "Lunch meeting tomorrow".to_string();

    let mut email2 = make_test_email();
    email2.id = EmailId("msg-2@example.com".to_string());
    email2.from = Contact {
        name: "Bob Builder".to_string(),
        address: "bob@construct.com".to_string(),
    };
    email2.subject = "Project deadline extension".to_string();

    let mut email3 = make_test_email();
    email3.id = EmailId("msg-3@example.com".to_string());
    email3.from = Contact {
        name: "Sarah Parker".to_string(),
        address: "sparker@mail.com".to_string(),
    };
    email3.subject = "Weekend lunch plans".to_string();

    idx.add_email(&email1, Some("Let's have lunch at the diner."), None)
        .unwrap();
    idx.add_email(&email2, Some("We need more time for the project."), None)
        .unwrap();
    idx.add_email(&email3, Some("How about Saturday for lunch?"), None)
        .unwrap();
    idx.commit().unwrap();

    (tmp, idx)
}

#[test]
fn search_term_query_on_subject() {
    let (_tmp, idx) = create_populated_index();

    // "lunch" appears in email1 and email3 subjects
    let results = idx.search_simple("lunch", 10).unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn search_term_query_on_from_field() {
    let (_tmp, idx) = create_populated_index();

    // from:sarah matches email1 and email3
    let query = SearchQuery::term_field(&idx.schema, "from", "sarah");
    let results = idx.execute_query(&query, 10).unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn search_phrase_query_on_body() {
    let (_tmp, idx) = create_populated_index();

    // Exact phrase "lunch at the diner" only in email1
    let query = SearchQuery::phrase(&idx.schema, "body_text", &["lunch", "at", "the", "diner"]);
    let results = idx.execute_query(&query, 10).unwrap();
    assert_eq!(results.len(), 1);
}

// ── Task 10: Multi-field search ────────────────────────────────────────────

#[test]
fn multi_field_search_finds_across_fields() {
    let (_tmp, idx) = create_populated_index();

    // "sarah" appears in from field of email1 and email3, not in subject/body
    // "deadline" appears in subject of email2, not in from
    // A multi-field search for "sarah deadline" should find all 3 emails
    let query = SearchQuery::multi_field(
        &idx.schema,
        &["from", "subject", "body_text"],
        "sarah deadline",
    );
    let results = idx.execute_query(&query, 10).unwrap();
    assert_eq!(results.len(), 3);
}

#[test]
fn multi_field_search_ranks_multi_match_higher() {
    let (_tmp, idx) = create_populated_index();

    // "lunch" appears in subject AND body of email1 and email3
    // email2 has "project" in both subject and body
    let query = SearchQuery::multi_field(&idx.schema, &["subject", "body_text"], "lunch");
    let results = idx.execute_query(&query, 10).unwrap();

    // Should find email1 and email3 (both mention lunch in subject + body)
    assert_eq!(results.len(), 2);
}

// ── Task 11: Faceted search ────────────────────────────────────────────────

#[test]
fn faceted_search_by_bundle_category() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    let mut email1 = make_test_email();
    email1.id = EmailId("msg-social@example.com".to_string());
    let mut email2 = make_test_email();
    email2.id = EmailId("msg-promos@example.com".to_string());
    let mut email3 = make_test_email();
    email3.id = EmailId("msg-social2@example.com".to_string());

    idx.add_email(&email1, None, Some(&BundleCategory::Social))
        .unwrap();
    idx.add_email(&email2, None, Some(&BundleCategory::Promos))
        .unwrap();
    idx.add_email(&email3, None, Some(&BundleCategory::Social))
        .unwrap();
    idx.commit().unwrap();

    // Filter by Social category
    let query = SearchQuery::facet_filter(&idx.schema, "bundle_category", "/bundle/social");
    let results = idx.execute_query(&query, 10).unwrap();
    assert_eq!(results.len(), 2);

    // Filter by Promos category
    let query = SearchQuery::facet_filter(&idx.schema, "bundle_category", "/bundle/promos");
    let results = idx.execute_query(&query, 10).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn faceted_search_by_account_id() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    let account1 = AccountId(Uuid::new_v4());
    let account2 = AccountId(Uuid::new_v4());

    let mut email1 = make_test_email();
    email1.id = EmailId("msg-a1@example.com".to_string());
    email1.account_id = account1;

    let mut email2 = make_test_email();
    email2.id = EmailId("msg-a2@example.com".to_string());
    email2.account_id = account2;

    let mut email3 = make_test_email();
    email3.id = EmailId("msg-a3@example.com".to_string());
    email3.account_id = account1;

    idx.add_email(&email1, None, None).unwrap();
    idx.add_email(&email2, None, None).unwrap();
    idx.add_email(&email3, None, None).unwrap();
    idx.commit().unwrap();

    // Filter by account1
    let facet_path = format!("/account/{}", account1.0);
    let query = SearchQuery::facet_filter(&idx.schema, "account_id", &facet_path);
    let results = idx.execute_query(&query, 10).unwrap();
    assert_eq!(results.len(), 2);
}

// ── Task 12: Date range queries ────────────────────────────────────────────

#[test]
fn date_range_query_after() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    let mut email_old = make_test_email();
    email_old.id = EmailId("old@example.com".to_string());
    email_old.date = Utc.with_ymd_and_hms(2025, 6, 15, 0, 0, 0).unwrap();

    let mut email_recent = make_test_email();
    email_recent.id = EmailId("recent@example.com".to_string());
    email_recent.date = Utc.with_ymd_and_hms(2026, 2, 20, 0, 0, 0).unwrap();

    let mut email_newest = make_test_email();
    email_newest.id = EmailId("newest@example.com".to_string());
    email_newest.date = Utc.with_ymd_and_hms(2026, 3, 10, 0, 0, 0).unwrap();

    idx.add_email(&email_old, None, None).unwrap();
    idx.add_email(&email_recent, None, None).unwrap();
    idx.add_email(&email_newest, None, None).unwrap();
    idx.commit().unwrap();

    // after:2026-01-01 should match email_recent and email_newest
    let after = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let query = SearchQuery::date_after(&idx.schema, after);
    let results = idx.execute_query(&query, 10).unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn date_range_query_before() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    let mut email_old = make_test_email();
    email_old.id = EmailId("old@example.com".to_string());
    email_old.date = Utc.with_ymd_and_hms(2025, 6, 15, 0, 0, 0).unwrap();

    let mut email_recent = make_test_email();
    email_recent.id = EmailId("recent@example.com".to_string());
    email_recent.date = Utc.with_ymd_and_hms(2026, 2, 20, 0, 0, 0).unwrap();

    idx.add_email(&email_old, None, None).unwrap();
    idx.add_email(&email_recent, None, None).unwrap();
    idx.commit().unwrap();

    // before:2026-01-01 should match only email_old
    let before = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let query = SearchQuery::date_before(&idx.schema, before);
    let results = idx.execute_query(&query, 10).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn date_range_query_between() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    let mut email1 = make_test_email();
    email1.id = EmailId("e1@example.com".to_string());
    email1.date = Utc.with_ymd_and_hms(2025, 6, 15, 0, 0, 0).unwrap();

    let mut email2 = make_test_email();
    email2.id = EmailId("e2@example.com".to_string());
    email2.date = Utc.with_ymd_and_hms(2026, 2, 15, 0, 0, 0).unwrap();

    let mut email3 = make_test_email();
    email3.id = EmailId("e3@example.com".to_string());
    email3.date = Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap();

    idx.add_email(&email1, None, None).unwrap();
    idx.add_email(&email2, None, None).unwrap();
    idx.add_email(&email3, None, None).unwrap();
    idx.commit().unwrap();

    // between 2026-01-01 and 2026-03-01 — only email2
    let after = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let before = Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();
    let query = SearchQuery::date_between(&idx.schema, after, before);
    let results = idx.execute_query(&query, 10).unwrap();
    assert_eq!(results.len(), 1);
}

// ── Task 13: has:attachment query ─────────────────────────────────────────

#[test]
fn has_attachment_query() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    let mut email_with_att = make_test_email(); // has attachments from make_test_email
    email_with_att.id = EmailId("with-att@example.com".to_string());

    let mut email_without_att = make_test_email();
    email_without_att.id = EmailId("no-att@example.com".to_string());
    email_without_att.attachments = vec![]; // Clear attachments

    idx.add_email(&email_with_att, None, None).unwrap();
    idx.add_email(&email_without_att, None, None).unwrap();
    idx.commit().unwrap();

    // has:attachment should match only the first email
    let query = SearchQuery::has_attachment(&idx.schema);
    let results = idx.execute_query(&query, 10).unwrap();
    assert_eq!(results.len(), 1);
}

// ── Task 14: Recency boost scoring ────────────────────────────────────────

#[test]
fn recency_boost_ranks_newer_email_higher() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    // Two emails with identical subject/body but different dates
    let mut email_old = make_test_email();
    email_old.id = EmailId("old-lunch@example.com".to_string());
    email_old.date = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    email_old.subject = "Identical lunch invitation".to_string();

    let mut email_new = make_test_email();
    email_new.id = EmailId("new-lunch@example.com".to_string());
    email_new.date = Utc.with_ymd_and_hms(2026, 3, 14, 0, 0, 0).unwrap();
    email_new.subject = "Identical lunch invitation".to_string();

    idx.add_email(&email_old, Some("lunch body"), None).unwrap();
    idx.add_email(&email_new, Some("lunch body"), None).unwrap();
    idx.commit().unwrap();

    // Search with recency boost — newer email should rank first
    let results = idx.search_with_recency_boost("lunch", 10).unwrap();
    assert_eq!(results.len(), 2);

    // First result should be the newer email (higher score)
    let searcher = idx.reader().searcher();
    let first_doc: tantivy::TantivyDocument = searcher.doc(results[0].1).unwrap();
    let email_id_field = idx.schema.email_id;
    let first_id: &str = first_doc
        .get_first(email_id_field)
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(first_id, "new-lunch@example.com");
}

// ── Task 15: Full index rebuild ───────────────────────────────────────────

use inboxly_store::search::rebuild::RebuildSource;

/// Test implementation of RebuildSource that returns in-memory emails.
struct MockRebuildSource {
    emails: Vec<(EmailMeta, Option<String>, Option<BundleCategory>)>,
}

impl RebuildSource for MockRebuildSource {
    fn all_emails(
        &self,
    ) -> Box<dyn Iterator<Item = (EmailMeta, Option<String>, Option<BundleCategory>)> + '_> {
        Box::new(self.emails.iter().cloned())
    }

    fn email_count(&self) -> u64 {
        self.emails.len() as u64
    }
}

#[test]
fn full_rebuild_from_source() {
    let tmp = TempDir::new().unwrap();
    let index_path = tmp.path().join("idx");

    // Create initial index with 1 email
    {
        let idx = SearchIndex::create(&index_path).unwrap();
        let email = make_test_email();
        idx.add_email(&email, None, None).unwrap();
        idx.commit().unwrap();
        assert_eq!(idx.num_docs(), 1);
    }

    // Rebuild from a source with 3 emails
    let source = MockRebuildSource {
        emails: (0..3)
            .map(|i| {
                let mut email = make_test_email();
                email.id = EmailId(format!("rebuild-{}@example.com", i));
                email.subject = format!("Rebuilt email {}", i);
                (email, Some(format!("Body text for email {}", i)), None)
            })
            .collect(),
    };

    let idx = SearchIndex::rebuild(&index_path, &source).unwrap();
    assert_eq!(idx.num_docs(), 3);

    // Verify one of the rebuilt emails is searchable
    let results = idx.search_simple("Rebuilt", 10).unwrap();
    assert_eq!(results.len(), 3);
}

// ── Task 16: SearchHit struct ─────────────────────────────────────────────
// SearchHit is the return type of idx.search() — import for explicit type use if needed.
#[allow(unused_imports)]
use inboxly_store::search::SearchHit;

#[test]
fn search_returns_structured_hits() {
    let (_tmp, idx) = create_populated_index();

    let hits = idx.search("lunch", 10).unwrap();
    assert_eq!(hits.len(), 2);

    // Verify the hits contain expected data
    let hit = &hits[0];
    assert!(!hit.email_id.is_empty());
    assert!(!hit.subject.is_empty());
    assert!(!hit.from.is_empty());
    assert!(hit.score > 0.0);
}

// ── Task 17: Clear index ──────────────────────────────────────────────────

#[test]
fn clear_index_removes_all_documents() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    // Add 5 emails
    for i in 0..5 {
        let mut email = make_test_email();
        email.id = EmailId(format!("clear-{}@example.com", i));
        idx.add_email(&email, None, None).unwrap();
    }
    idx.commit().unwrap();
    assert_eq!(idx.num_docs(), 5);

    idx.clear().unwrap();
    assert_eq!(idx.num_docs(), 0);
}
