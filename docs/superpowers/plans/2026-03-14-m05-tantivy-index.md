# M5: Tantivy Search Index — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Implement full-text search indexing with tantivy in `inboxly-store`, enabling search across email metadata and bodies with BM25 relevance ranking and recency boost.

**Architecture:** Search index lives in `inboxly-store` alongside SQLite and Maildir. Tantivy index stored on disk in `data_dir/search_index/`. The `SearchIndex` struct owns both a tantivy `IndexWriter` (behind a `Mutex` for single-writer semantics) and an `IndexReader` (cloneable, lock-free reads). Documents are keyed by `email_id` (the Message-ID header, stored as a STRING field for exact-match deletion). The index is fully rebuildable from Maildir + SQLite at any time. Facets use tantivy's hierarchical facet fields for `account_id` and `bundle_category` filtering.

**Tech Stack:** Rust, tantivy 0.22, inboxly-core

**Prerequisite:** M1 complete (core types: `AccountId`, `EmailId`, `EmailMeta`, `EmailContent`, `BundleCategory` exist in `inboxly-core`), M3 complete (`inboxly-store` crate exists with SQLite schema and `Cargo.toml`), M4 complete (Maildir read/write operations exist in `inboxly-store`).

---

## File Structure

| File | Responsibility |
|------|---------------|
| `inboxly-store/Cargo.toml` | Add `tantivy` dependency |
| `inboxly-store/src/search/mod.rs` | `SearchIndex` struct — public API, owns reader + writer |
| `inboxly-store/src/search/schema.rs` | Schema definition, field accessors, document conversion |
| `inboxly-store/src/search/query.rs` | Query builders — term, phrase, multi-field, faceted, date range |
| `inboxly-store/src/search/scoring.rs` | BM25 + recency boost scorer |
| `inboxly-store/src/search/rebuild.rs` | Full index rebuild from Maildir + SQLite |
| `inboxly-store/src/lib.rs` | Add `pub mod search;` |
| `inboxly-store/tests/search_index.rs` | Integration tests |

---

## Task 1: Add tantivy dependency to inboxly-store

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/Cargo.toml`

- [ ] **Step 1: Add tantivy to dependencies**

Add `tantivy` to the `[dependencies]` section:

```toml
tantivy = "0.22"
```

- [ ] **Step 2: Verify compilation**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

Expected: compiles successfully with tantivy downloaded.

- [ ] **Step 3: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/Cargo.toml Cargo.lock
git commit -m "feat(store): add tantivy dependency for full-text search"
```

---

## Task 2: Define tantivy schema with all indexed fields

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/schema.rs` (new file)

The schema defines all fields from the spec's Search & Highlights section: `email_id`, `from`, `to`, `subject`, `body_text`, `date`, `account_id`, `bundle_category`, `has_attachment`.

- [ ] **Step 1: Write the failing test**

Create `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
use inboxly_store::search::schema::SearchSchema;

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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index schema_has_all_expected_fields
```

Expected: FAIL — module `search` not found.

- [ ] **Step 3: Create the search module structure**

Create `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`:

```rust
pub mod schema;
```

Add to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/lib.rs`:

```rust
pub mod search;
```

- [ ] **Step 4: Implement SearchSchema**

Create `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/schema.rs`:

```rust
use tantivy::schema::{
    DateOptions, FacetOptions, Field, Schema, SchemaBuilder, TextFieldIndexing, TextOptions,
    FAST, INDEXED, STORED, STRING, TEXT,
};
use tantivy::tokenizer::TextAnalyzer;

/// Holds the tantivy schema and named field handles for fast access.
///
/// Field mapping (from spec Search & Highlights section):
/// - `email_id`: STRING + STORED — Message-ID header, used as document key for deletion
/// - `from`: TEXT + STORED — sender name + address, tokenized for search
/// - `to`: TEXT + STORED — recipient(s), tokenized for search
/// - `subject`: TEXT + STORED — email subject line, tokenized for search
/// - `body_text`: TEXT — plain text body, tokenized for full-text search (not stored — load from Maildir)
/// - `date`: Date + FAST + STORED — email date as UTC timestamp, FAST for recency boost scoring
/// - `account_id`: Facet — account UUID as facet for filtering (e.g., `/account/<uuid>`)
/// - `bundle_category`: Facet — bundle category as facet for filtering (e.g., `/bundle/social`)
/// - `has_attachment`: u64 + INDEXED + FAST — 1 or 0, indexed for `has:attachment` queries
#[derive(Clone, Debug)]
pub struct SearchSchema {
    schema: Schema,
    pub email_id: Field,
    pub from: Field,
    pub to: Field,
    pub subject: Field,
    pub body_text: Field,
    pub date: Field,
    pub account_id: Field,
    pub bundle_category: Field,
    pub has_attachment: Field,
}

impl SearchSchema {
    /// Build the tantivy schema with all indexed fields.
    pub fn new() -> Self {
        let mut builder = Schema::builder();

        // email_id: exact-match key for deletion/lookup. STRING = untokenized + indexed.
        let email_id = builder.add_text_field("email_id", STRING | STORED);

        // from: tokenized for search ("from:sarah"), stored for result display.
        let from = builder.add_text_field("from", TEXT | STORED);

        // to: tokenized for search ("to:bob@example.com"), stored for result display.
        let to = builder.add_text_field("to", TEXT | STORED);

        // subject: tokenized for full-text search, stored for snippet display.
        let subject = builder.add_text_field("subject", TEXT | STORED);

        // body_text: tokenized for full-text search. NOT stored — bodies are large
        // and should be loaded from Maildir on demand.
        let body_text = builder.add_text_field("body_text", TEXT);

        // date: indexed + fast field for range queries and recency boost scoring.
        // Stored so we can display dates from search results without a SQLite join.
        let date = builder.add_date_field(
            "date",
            DateOptions::default()
                .set_indexed()
                .set_fast()
                .set_stored()
                .set_precision(tantivy::schema::DateTimePrecision::Seconds),
        );

        // account_id: hierarchical facet for filtering by account.
        // Values stored as "/account/<uuid>".
        let account_id = builder.add_facet_field("account_id", FacetOptions::default());

        // bundle_category: hierarchical facet for filtering by bundle.
        // Values stored as "/bundle/<category>" (e.g., "/bundle/social").
        let bundle_category =
            builder.add_facet_field("bundle_category", FacetOptions::default());

        // has_attachment: boolean-like u64 (0 or 1). INDEXED for `has:attachment` filter.
        // FAST for efficient access during scoring if needed.
        let has_attachment = builder.add_u64_field(
            "has_attachment",
            INDEXED | FAST | STORED,
        );

        let schema = builder.build();

        Self {
            schema,
            email_id,
            from,
            to,
            subject,
            body_text,
            date,
            account_id,
            bundle_category,
            has_attachment,
        }
    }

    /// Return a reference to the underlying tantivy Schema.
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index schema_has_all_expected_fields
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/ inboxly-store/src/lib.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): define tantivy schema with all indexed fields"
```

---

## Task 3: Document conversion — EmailMeta + body to tantivy Document

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/schema.rs` (append)

Add a method that converts an `EmailMeta` + optional body text + optional `BundleCategory` into a tantivy `TantivyDocument`. This is the bridge between `inboxly-core` types and the search index.

- [ ] **Step 1: Write the failing test**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
use inboxly_core::types::{
    AccountId, AttachmentMeta, BundleCategory, Contact, EmailFlags, EmailId, EmailMeta,
};
use chrono::{TimeZone, Utc};
use std::path::PathBuf;
use uuid::Uuid;

fn make_test_email() -> EmailMeta {
    EmailMeta {
        id: EmailId("test-msg-001@example.com".to_string()),
        account_id: AccountId(Uuid::nil()),
        thread_id: inboxly_core::types::ThreadId(Uuid::nil()),
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
            name: "report.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            size_bytes: 1024,
        }],
        flags: EmailFlags::default(),
        size_bytes: 4096,
        imap_uid: 42,
        imap_folder: "INBOX".to_string(),
    }
}

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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index build_document_from_email_meta
```

Expected: FAIL — `build_document` method not found.

- [ ] **Step 3: Implement build_document**

Add to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/schema.rs`:

```rust
use inboxly_core::types::{BundleCategory, EmailMeta};
use tantivy::doc;
use tantivy::schema::Facet;
use tantivy::{DateTime as TantivyDateTime, TantivyDocument};

impl SearchSchema {
    /// Convert an EmailMeta + optional body text + optional category into a tantivy document.
    ///
    /// - `email`: the email metadata from SQLite
    /// - `body_text`: optional plain text body (loaded from Maildir on demand)
    /// - `bundle_category`: optional bundle assignment (from bundler)
    pub fn build_document(
        &self,
        email: &EmailMeta,
        body_text: Option<&str>,
        bundle_category: Option<&BundleCategory>,
    ) -> TantivyDocument {
        let mut doc = TantivyDocument::default();

        // email_id — exact key
        doc.add_text(self.email_id, &email.id.0);

        // from — "Name <address>" for better tokenization of both parts
        let from_str = if email.from.name.is_empty() {
            email.from.address.clone()
        } else {
            format!("{} <{}>", email.from.name, email.from.address)
        };
        doc.add_text(self.from, &from_str);

        // to — concatenate all recipients
        let to_str: String = email
            .to
            .iter()
            .map(|c| {
                if c.name.is_empty() {
                    c.address.clone()
                } else {
                    format!("{} <{}>", c.name, c.address)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        doc.add_text(self.to, &to_str);

        // subject
        doc.add_text(self.subject, &email.subject);

        // body_text — only if available (Phase 2 sync or on-demand load)
        if let Some(body) = body_text {
            doc.add_text(self.body_text, body);
        }

        // date — convert chrono DateTime<Utc> to tantivy DateTime
        let timestamp_secs = email.date.timestamp();
        let tantivy_dt = TantivyDateTime::from_timestamp_secs(timestamp_secs);
        doc.add_date(self.date, tantivy_dt);

        // account_id facet — "/account/<uuid>"
        let account_facet = Facet::from(&format!("/account/{}", email.account_id.0));
        doc.add_facet(self.account_id, account_facet);

        // bundle_category facet — "/bundle/<category>"
        if let Some(cat) = bundle_category {
            let cat_str = category_to_facet_str(cat);
            let bundle_facet = Facet::from(&format!("/bundle/{}", cat_str));
            doc.add_facet(self.bundle_category, bundle_facet);
        } else {
            // Uncategorised emails get a root facet so they're still filterable
            let bundle_facet = Facet::from("/bundle/uncategorised");
            doc.add_facet(self.bundle_category, bundle_facet);
        }

        // has_attachment — 1 if any attachments, 0 otherwise
        let has_att: u64 = if email.attachments.is_empty() { 0 } else { 1 };
        doc.add_u64(self.has_attachment, has_att);

        doc
    }
}

/// Convert a BundleCategory enum variant to its lowercase string for faceting.
fn category_to_facet_str(cat: &BundleCategory) -> String {
    match cat {
        BundleCategory::Social => "social".to_string(),
        BundleCategory::Promos => "promos".to_string(),
        BundleCategory::Updates => "updates".to_string(),
        BundleCategory::Finance => "finance".to_string(),
        BundleCategory::Purchases => "purchases".to_string(),
        BundleCategory::Travel => "travel".to_string(),
        BundleCategory::Forums => "forums".to_string(),
        BundleCategory::LowPriority => "low_priority".to_string(),
        BundleCategory::Saved => "saved".to_string(),
        BundleCategory::Custom(name) => name.to_lowercase().replace(' ', "_"),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index build_document_from_email_meta
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/schema.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): convert EmailMeta to tantivy document"
```

---

## Task 4: SearchIndex struct — create/open index on disk

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs` (rewrite)

The `SearchIndex` struct wraps the tantivy `Index`, `IndexWriter`, and `IndexReader`. It manages index lifecycle: creation, opening, and committing.

- [ ] **Step 1: Write the failing test**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
use inboxly_store::search::SearchIndex;
use tempfile::TempDir;

#[test]
fn create_new_index_on_disk() {
    let tmp = TempDir::new().unwrap();
    let index_path = tmp.path().join("search_index");

    let search_index = SearchIndex::create(&index_path).unwrap();

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
        let mut idx = SearchIndex::create(&index_path).unwrap();
        let email = make_test_email();
        idx.add_email(&email, None, None).unwrap();
        idx.commit().unwrap();
    }

    // Reopen — doc should still be there
    let idx = SearchIndex::open_or_create(&index_path).unwrap();
    assert_eq!(idx.num_docs(), 1);
}
```

- [ ] **Step 2: Add `tempfile` dev-dependency**

Add to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/Cargo.toml` under `[dev-dependencies]`:

```toml
tempfile = "3"
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index create_new_index
```

Expected: FAIL — `SearchIndex` not found.

- [ ] **Step 4: Implement SearchIndex**

Rewrite `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`:

```rust
pub mod query;
pub mod rebuild;
pub mod schema;
pub mod scoring;

use std::path::Path;
use std::sync::Mutex;

use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};

use self::schema::SearchSchema;
use inboxly_core::types::{BundleCategory, EmailMeta};

/// Memory budget for the IndexWriter (50 MB).
/// tantivy uses this to size its in-memory buffer before flushing segments.
const WRITER_MEMORY_BUDGET: usize = 50_000_000;

/// Full-text search index backed by tantivy.
///
/// Wraps the tantivy Index, writer, and reader. The writer is behind a Mutex
/// because tantivy requires single-writer semantics. The reader is Clone +
/// Send + Sync and hands out Searcher instances on demand.
pub struct SearchIndex {
    index: Index,
    writer: Mutex<IndexWriter<TantivyDocument>>,
    reader: IndexReader,
    pub schema: SearchSchema,
}

impl SearchIndex {
    /// Create a new search index at the given directory path.
    ///
    /// The directory is created if it does not exist. Fails if an index
    /// already exists at this path.
    pub fn create(path: &Path) -> Result<Self, SearchError> {
        std::fs::create_dir_all(path)?;
        let search_schema = SearchSchema::new();
        let index = Index::create_in_dir(path, search_schema.schema().clone())?;
        Self::from_index(index, search_schema)
    }

    /// Open an existing search index at the given directory path.
    pub fn open(path: &Path) -> Result<Self, SearchError> {
        let index = Index::open_in_dir(path)?;
        let search_schema = SearchSchema::from_existing(index.schema());
        Self::from_index(index, search_schema)
    }

    /// Open the index if it exists, or create a new one.
    pub fn open_or_create(path: &Path) -> Result<Self, SearchError> {
        if path.join("meta.json").exists() {
            Self::open(path)
        } else {
            Self::create(path)
        }
    }

    /// Internal: build a SearchIndex from a tantivy Index + schema.
    fn from_index(index: Index, search_schema: SearchSchema) -> Result<Self, SearchError> {
        let writer = index.writer(WRITER_MEMORY_BUDGET)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        Ok(Self {
            index,
            writer: Mutex::new(writer),
            reader,
            schema: search_schema,
        })
    }

    /// Index a single email. Call `commit()` afterwards to make it searchable.
    pub fn add_email(
        &self,
        email: &EmailMeta,
        body_text: Option<&str>,
        bundle_category: Option<&BundleCategory>,
    ) -> Result<(), SearchError> {
        let doc = self.schema.build_document(email, body_text, bundle_category.as_ref().copied());
        let writer = self.writer.lock().map_err(|_| SearchError::WriterLock)?;
        writer.add_document(doc)?;
        Ok(())
    }

    /// Commit all pending changes and make them searchable.
    pub fn commit(&self) -> Result<(), SearchError> {
        let mut writer = self.writer.lock().map_err(|_| SearchError::WriterLock)?;
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Return the total number of documents in the index.
    pub fn num_docs(&self) -> u64 {
        let searcher = self.reader.searcher();
        searcher.num_docs()
    }
}

/// Errors from search index operations.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("tantivy error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to acquire writer lock")]
    WriterLock,

    #[error("field not found in schema: {0}")]
    FieldNotFound(String),
}
```

- [ ] **Step 5: Add `SearchSchema::from_existing` for reopening an index**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/schema.rs`:

```rust
impl SearchSchema {
    /// Reconstruct field handles from an existing tantivy Schema.
    ///
    /// Used when reopening an index that was created previously.
    pub fn from_existing(schema: Schema) -> Self {
        let email_id = schema.get_field("email_id").expect("email_id field missing");
        let from = schema.get_field("from").expect("from field missing");
        let to = schema.get_field("to").expect("to field missing");
        let subject = schema.get_field("subject").expect("subject field missing");
        let body_text = schema.get_field("body_text").expect("body_text field missing");
        let date = schema.get_field("date").expect("date field missing");
        let account_id = schema.get_field("account_id").expect("account_id field missing");
        let bundle_category = schema
            .get_field("bundle_category")
            .expect("bundle_category field missing");
        let has_attachment = schema
            .get_field("has_attachment")
            .expect("has_attachment field missing");

        Self {
            schema,
            email_id,
            from,
            to,
            subject,
            body_text,
            date,
            account_id,
            bundle_category,
            has_attachment,
        }
    }
}
```

- [ ] **Step 6: Create placeholder modules for `query`, `scoring`, `rebuild`**

Create `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/query.rs`:

```rust
// Query builders — implemented in Task 7-10.
```

Create `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/scoring.rs`:

```rust
// BM25 + recency boost — implemented in Task 11.
```

Create `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/rebuild.rs`:

```rust
// Full index rebuild — implemented in Task 12.
```

- [ ] **Step 7: Run all tests**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index
```

Expected: all 5 tests pass (schema_has_all_expected_fields, build_document_from_email_meta, create_new_index_on_disk, open_existing_index, open_or_create_when_missing, open_or_create_when_existing).

- [ ] **Step 8: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/ inboxly-store/tests/search_index.rs inboxly-store/Cargo.toml
git commit -m "feat(store): SearchIndex struct with create/open/commit lifecycle"
```

---

## Task 5: Index a single email and verify it's searchable

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs` (the `add_email` method already exists from Task 4)

This task validates the full round-trip: add email -> commit -> search -> find it.

- [ ] **Step 1: Write the failing test**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
use tantivy::collector::TopDocs;
use tantivy::query::TermQuery;
use tantivy::schema::IndexRecordOption;
use tantivy::Term;

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
```

- [ ] **Step 2: Expose the reader via a public method**

Add to `SearchIndex` in `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`:

```rust
impl SearchIndex {
    /// Get the IndexReader for executing searches.
    pub fn reader(&self) -> &IndexReader {
        &self.reader
    }
}
```

- [ ] **Step 3: Fix the `add_email` signature**

Note: the `build_document` call in `add_email` needs to accept `Option<&BundleCategory>` directly (not `Option<&BundleCategory>.as_ref().copied()`). Adjust the method:

```rust
pub fn add_email(
    &self,
    email: &EmailMeta,
    body_text: Option<&str>,
    bundle_category: Option<&BundleCategory>,
) -> Result<(), SearchError> {
    let doc = self.schema.build_document(email, body_text, bundle_category);
    let writer = self.writer.lock().map_err(|_| SearchError::WriterLock)?;
    writer.add_document(doc)?;
    Ok(())
}
```

And update `build_document` to take `Option<&BundleCategory>` directly:

```rust
pub fn build_document(
    &self,
    email: &EmailMeta,
    body_text: Option<&str>,
    bundle_category: Option<&BundleCategory>,
) -> TantivyDocument {
    // ... (implementation unchanged, already uses Option<&BundleCategory>)
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index index_single_email
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/mod.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): index and retrieve single email by email_id"
```

---

## Task 6: Batch indexing with IndexWriter

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`

Batch indexing is used during initial sync (Phase 2) and full rebuilds. Adds multiple documents then commits once.

- [ ] **Step 1: Write the failing test**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
#[test]
fn batch_index_multiple_emails() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    let emails: Vec<(EmailMeta, &str)> = (0..100)
        .map(|i| {
            let mut email = make_test_email();
            email.id = EmailId(format!("msg-{}@example.com", i));
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index batch_index
```

Expected: FAIL — `batch_index` method not found.

- [ ] **Step 3: Implement batch_index**

Add to `SearchIndex` in `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`:

```rust
impl SearchIndex {
    /// Index a batch of emails and commit once at the end.
    ///
    /// More efficient than individual `add_email` + `commit` calls because
    /// it acquires the writer lock once and commits once.
    pub fn batch_index(
        &self,
        emails: &[(&EmailMeta, Option<&str>, Option<&BundleCategory>)],
    ) -> Result<(), SearchError> {
        let mut writer = self.writer.lock().map_err(|_| SearchError::WriterLock)?;

        for (email, body_text, bundle_category) in emails {
            let doc = self.schema.build_document(email, *body_text, *bundle_category);
            writer.add_document(doc)?;
        }

        writer.commit()?;
        drop(writer); // Release lock before reload
        self.reader.reload()?;
        Ok(())
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index batch_index
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/mod.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): batch indexing for initial sync and rebuilds"
```

---

## Task 7: Remove document from index by EmailId

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`

Uses tantivy's `delete_term` on the `email_id` STRING field. Since `email_id` is a STRING (untokenized), the exact Message-ID value matches.

- [ ] **Step 1: Write the failing test**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
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
    let fake_id = EmailId("nonexistent@example.com".to_string());
    idx.remove_email(&fake_id).unwrap();
    idx.commit().unwrap();
    assert_eq!(idx.num_docs(), 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index remove_email
```

Expected: FAIL — `remove_email` method not found.

- [ ] **Step 3: Implement remove_email**

Add to `SearchIndex` in `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`:

```rust
use inboxly_core::types::EmailId;
use tantivy::Term;

impl SearchIndex {
    /// Delete all documents matching the given EmailId from the index.
    ///
    /// The deletion is staged — call `commit()` to apply it.
    /// Deleting a non-existent email is a no-op (tantivy ignores unknown terms).
    pub fn remove_email(&self, email_id: &EmailId) -> Result<(), SearchError> {
        let writer = self.writer.lock().map_err(|_| SearchError::WriterLock)?;
        let term = Term::from_field_text(self.schema.email_id, &email_id.0);
        writer.delete_term(term);
        Ok(())
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index remove_email
```

Expected: PASS (both tests)

- [ ] **Step 5: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/mod.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): remove document from search index by EmailId"
```

---

## Task 8: Incremental update — re-index an existing email

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`

When an email's bundle category changes or its body becomes available (Phase 2), we need to update the index. Tantivy doesn't support in-place updates, so this is delete + re-add.

- [ ] **Step 1: Write the failing test**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index update_email
```

Expected: FAIL — `update_email` and `search_simple` not found.

- [ ] **Step 3: Implement update_email**

Add to `SearchIndex` in `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`:

```rust
impl SearchIndex {
    /// Update an existing email in the index (delete + re-add).
    ///
    /// Use this when:
    /// - Body text becomes available (Phase 2 sync)
    /// - Bundle category changes (user moves email to different bundle)
    /// - Email flags change that affect indexed fields
    ///
    /// The update is staged — call `commit()` to apply it.
    pub fn update_email(
        &self,
        email: &EmailMeta,
        body_text: Option<&str>,
        bundle_category: Option<&BundleCategory>,
    ) -> Result<(), SearchError> {
        let mut writer = self.writer.lock().map_err(|_| SearchError::WriterLock)?;

        // Delete the old document
        let term = Term::from_field_text(self.schema.email_id, &email.id.0);
        writer.delete_term(term);

        // Add the new document
        let doc = self.schema.build_document(email, body_text, bundle_category);
        writer.add_document(doc)?;

        Ok(())
    }
}
```

- [ ] **Step 4: Stub `search_simple` (needed for the test — full impl in Task 9)**

Add a minimal `search_simple` to `SearchIndex`:

```rust
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::DocAddress;

impl SearchIndex {
    /// Simple full-text search across subject + body_text fields.
    ///
    /// Returns up to `limit` results as (score, doc_address) pairs,
    /// ranked by BM25 relevance.
    pub fn search_simple(
        &self,
        query_str: &str,
        limit: usize,
    ) -> Result<Vec<(f32, DocAddress)>, SearchError> {
        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(
            &self.index,
            vec![self.schema.subject, self.schema.body_text],
        );
        let query = query_parser
            .parse_query(query_str)
            .map_err(|e| SearchError::QueryParse(e.to_string()))?;
        let results = searcher.search(&query, &TopDocs::with_limit(limit))?;
        Ok(results)
    }
}
```

Add the `QueryParse` variant to `SearchError`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    // ... existing variants ...

    #[error("query parse error: {0}")]
    QueryParse(String),
}
```

- [ ] **Step 5: Run test to verify it passes**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index update_email
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/mod.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): incremental update (delete + re-add) for email documents"
```

---

## Task 9: Basic search — single term and phrase queries

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/query.rs`

Implements the query module with typed query builders matching the spec's query syntax table.

- [ ] **Step 1: Write the failing tests**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
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

    idx.add_email(&email1, Some("Let's have lunch at the diner."), None).unwrap();
    idx.add_email(&email2, Some("We need more time for the project."), None).unwrap();
    idx.add_email(&email3, Some("How about Saturday for lunch?"), None).unwrap();
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index search_term
```

Expected: FAIL — `SearchQuery` not found, `execute_query` not found.

- [ ] **Step 3: Implement SearchQuery with term and phrase builders**

Write `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/query.rs`:

```rust
use tantivy::query::{BooleanQuery, Box as TantivyBox, Occur, PhraseQuery, Query, TermQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::Term;

use super::schema::SearchSchema;

/// Typed query builders matching the spec's query syntax table.
pub struct SearchQuery;

impl SearchQuery {
    /// Term query on a specific text field.
    ///
    /// Maps to spec syntax like `from:sarah`, `to:bob@example.com`, `subject:lunch`.
    pub fn term_field(schema: &SearchSchema, field_name: &str, value: &str) -> Box<dyn Query> {
        let field = schema
            .schema()
            .get_field(field_name)
            .expect("unknown field");
        let term = Term::from_field_text(field, value);
        Box::new(TermQuery::new(term, IndexRecordOption::WithFreqs))
    }

    /// Phrase query on a specific text field.
    ///
    /// Matches an exact ordered sequence of terms in the field.
    pub fn phrase(schema: &SearchSchema, field_name: &str, terms: &[&str]) -> Box<dyn Query> {
        let field = schema
            .schema()
            .get_field(field_name)
            .expect("unknown field");
        let term_vec: Vec<Term> = terms
            .iter()
            .map(|t| Term::from_field_text(field, &t.to_lowercase()))
            .collect();
        Box::new(PhraseQuery::new(term_vec))
    }
}
```

- [ ] **Step 4: Add `execute_query` to SearchIndex**

Add to `SearchIndex` in `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`:

```rust
use tantivy::query::Query;

impl SearchIndex {
    /// Execute an arbitrary tantivy query and return scored results.
    pub fn execute_query(
        &self,
        query: &dyn Query,
        limit: usize,
    ) -> Result<Vec<(f32, DocAddress)>, SearchError> {
        let searcher = self.reader.searcher();
        let results = searcher.search(query, &TopDocs::with_limit(limit))?;
        Ok(results)
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index search_term search_phrase
```

Expected: PASS (all 3 tests)

- [ ] **Step 6: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/query.rs inboxly-store/src/search/mod.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): term and phrase query builders"
```

---

## Task 10: Multi-field search (across from + subject + body)

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/query.rs`

The spec says bare words search across `subject` + `body_text`. This task adds a multi-field query builder using BooleanQuery with `Occur::Should` across multiple fields.

- [ ] **Step 1: Write the failing test**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
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
    let query = SearchQuery::multi_field(
        &idx.schema,
        &["subject", "body_text"],
        "lunch",
    );
    let results = idx.execute_query(&query, 10).unwrap();

    // Should find email1 and email3 (both mention lunch in subject + body)
    assert_eq!(results.len(), 2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index multi_field
```

Expected: FAIL — `multi_field` not found.

- [ ] **Step 3: Implement multi_field query builder**

Add to `SearchQuery` in `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/query.rs`:

```rust
use tantivy::query::QueryParser;
use tantivy::Index;

impl SearchQuery {
    /// Multi-field search — the spec's "bare words" query.
    ///
    /// Searches across multiple text fields using tantivy's QueryParser,
    /// which creates a disjunction (OR) across the fields.
    /// BM25 naturally ranks documents matching in multiple fields higher.
    pub fn multi_field(
        schema: &SearchSchema,
        field_names: &[&str],
        query_str: &str,
    ) -> Box<dyn Query> {
        let fields: Vec<tantivy::schema::Field> = field_names
            .iter()
            .map(|name| {
                schema
                    .schema()
                    .get_field(name)
                    .unwrap_or_else(|_| panic!("unknown field: {}", name))
            })
            .collect();

        // Build an in-memory index just for the QueryParser (it needs an Index reference
        // to resolve tokenizers). This is lightweight — no data is written.
        let tmp_index = Index::create_in_ram(schema.schema().clone());
        let parser = QueryParser::for_index(&tmp_index, fields);

        parser
            .parse_query(query_str)
            .expect("failed to parse multi-field query")
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index multi_field
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/query.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): multi-field search across from + subject + body"
```

---

## Task 11: Faceted search — filter by account_id and bundle_category

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/query.rs`

The spec defines `in:purchases` as a facet query on `bundle_category`. This task adds facet-filtered queries using tantivy's `TermQuery` on facet fields.

- [ ] **Step 1: Write the failing tests**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
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

    idx.add_email(&email1, None, Some(&BundleCategory::Social)).unwrap();
    idx.add_email(&email2, None, Some(&BundleCategory::Promos)).unwrap();
    idx.add_email(&email3, None, Some(&BundleCategory::Social)).unwrap();
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index faceted_search
```

Expected: FAIL — `facet_filter` not found.

- [ ] **Step 3: Implement facet_filter**

Add to `SearchQuery` in `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/query.rs`:

```rust
use tantivy::schema::Facet;

impl SearchQuery {
    /// Facet filter query — matches all documents with the given facet value.
    ///
    /// Maps to spec syntax `in:purchases` → facet query on `bundle_category` field
    /// with value `/bundle/purchases`.
    ///
    /// Also used for account filtering: `account_id` field with `/account/<uuid>`.
    pub fn facet_filter(
        schema: &SearchSchema,
        field_name: &str,
        facet_path: &str,
    ) -> Box<dyn Query> {
        let field = schema
            .schema()
            .get_field(field_name)
            .expect("unknown field");
        let facet = Facet::from(facet_path);
        let term = Term::from_facet(field, &facet);
        Box::new(TermQuery::new(term, IndexRecordOption::Basic))
    }

    /// Combine a text query with a facet filter using BooleanQuery.
    ///
    /// Example: search for "lunch" within the "social" bundle.
    pub fn filtered_search(
        text_query: Box<dyn Query>,
        filter_query: Box<dyn Query>,
    ) -> Box<dyn Query> {
        Box::new(BooleanQuery::new(vec![
            (Occur::Must, text_query),
            (Occur::Must, filter_query),
        ]))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index faceted_search
```

Expected: PASS (both tests)

- [ ] **Step 5: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/query.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): faceted search for account_id and bundle_category"
```

---

## Task 12: Date range queries

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/query.rs`

The spec defines `after:2026-01-01` and `before:2026-03-01` as range queries on the `date` field.

- [ ] **Step 1: Write the failing test**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
use chrono::NaiveDate;

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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index date_range
```

Expected: FAIL — `date_after`, `date_before`, `date_between` not found.

- [ ] **Step 3: Implement date range query builders**

Add to `SearchQuery` in `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/query.rs`:

```rust
use chrono::{DateTime, Utc};
use std::ops::Bound;
use tantivy::query::RangeQuery;
use tantivy::DateTime as TantivyDateTime;

impl SearchQuery {
    /// Date range query: emails after the given date (inclusive).
    ///
    /// Maps to spec syntax `after:2026-01-01`.
    pub fn date_after(schema: &SearchSchema, after: DateTime<Utc>) -> Box<dyn Query> {
        let after_tantivy = TantivyDateTime::from_timestamp_secs(after.timestamp());
        Box::new(RangeQuery::new_date_bounds(
            "date".to_string(),
            Bound::Included(after_tantivy),
            Bound::Unbounded,
        ))
    }

    /// Date range query: emails before the given date (inclusive).
    ///
    /// Maps to spec syntax `before:2026-03-01`.
    pub fn date_before(schema: &SearchSchema, before: DateTime<Utc>) -> Box<dyn Query> {
        let before_tantivy = TantivyDateTime::from_timestamp_secs(before.timestamp());
        Box::new(RangeQuery::new_date_bounds(
            "date".to_string(),
            Bound::Unbounded,
            Bound::Included(before_tantivy),
        ))
    }

    /// Date range query: emails between two dates (both inclusive).
    ///
    /// Combines `after` and `before` into a single bounded range.
    pub fn date_between(
        schema: &SearchSchema,
        after: DateTime<Utc>,
        before: DateTime<Utc>,
    ) -> Box<dyn Query> {
        let after_tantivy = TantivyDateTime::from_timestamp_secs(after.timestamp());
        let before_tantivy = TantivyDateTime::from_timestamp_secs(before.timestamp());
        Box::new(RangeQuery::new_date_bounds(
            "date".to_string(),
            Bound::Included(after_tantivy),
            Bound::Included(before_tantivy),
        ))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index date_range
```

Expected: PASS (all 3 tests)

- [ ] **Step 5: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/query.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): date range queries (after, before, between)"
```

---

## Task 13: has:attachment query

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/query.rs`

The spec defines `has:attachment` as a bool query on `has_attachment`. Since we use u64 (0/1), this is a term query on the u64 field.

- [ ] **Step 1: Write the failing test**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index has_attachment
```

Expected: FAIL — `has_attachment` method not found on `SearchQuery`.

- [ ] **Step 3: Implement has_attachment query**

Add to `SearchQuery` in `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/query.rs`:

```rust
impl SearchQuery {
    /// Query for emails with attachments.
    ///
    /// Maps to spec syntax `has:attachment`.
    /// The `has_attachment` field stores 1 (has) or 0 (doesn't have).
    pub fn has_attachment(schema: &SearchSchema) -> Box<dyn Query> {
        let term = Term::from_field_u64(schema.has_attachment, 1);
        Box::new(TermQuery::new(term, IndexRecordOption::Basic))
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index has_attachment
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/query.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): has:attachment query filter"
```

---

## Task 14: BM25 with recency boost scorer

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/scoring.rs`

The spec says results are "ranked by BM25 relevance with recency boost". Tantivy's default scoring is BM25. We add a recency boost via `TopDocs::tweak_score` that reads the `date` fast field and applies an exponential decay: emails from today get a ~2x boost, 30 days old get ~1.3x, 365 days old get ~1.0x (no boost).

- [ ] **Step 1: Write the failing test**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
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
    let first_doc = searcher.doc(results[0].1).unwrap();
    let email_id_field = idx.schema.email_id;
    let first_id: &str = first_doc
        .get_first(email_id_field)
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(first_id, "new-lunch@example.com");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index recency_boost
```

Expected: FAIL — `search_with_recency_boost` not found.

- [ ] **Step 3: Implement recency boost scorer**

Write `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/scoring.rs`:

```rust
use tantivy::collector::TopDocs;
use tantivy::fastfield::Column;
use tantivy::query::QueryParser;
use tantivy::{DocAddress, DocId, Index, Score, SegmentReader};

use super::schema::SearchSchema;

/// Half-life for recency decay in days.
///
/// An email `RECENCY_HALF_LIFE_DAYS` old gets a boost of ~1.5x (halfway between
/// max boost and no boost). Newer emails get up to 2x; much older emails approach 1x.
const RECENCY_HALF_LIFE_DAYS: f64 = 60.0;

/// Maximum recency boost multiplier for the newest emails.
const MAX_RECENCY_BOOST: f32 = 2.0;

/// Minimum recency boost multiplier (floor — very old emails).
const MIN_RECENCY_BOOST: f32 = 1.0;

/// Build a TopDocs collector with BM25 + recency boost.
///
/// Uses `TopDocs::tweak_score` to read the `date` fast field and apply
/// an exponential decay boost: recent emails score higher.
///
/// Formula: `boosted_score = bm25_score * (MIN + (MAX - MIN) * exp(-age_days / half_life))`
pub fn top_docs_with_recency_boost(limit: usize) -> TopDocs {
    // The tweak_score closure will be applied per-segment.
    // We return the base TopDocs here; the actual scoring is applied in SearchIndex.
    TopDocs::with_limit(limit)
}

/// Compute the recency boost factor for a given timestamp.
///
/// Returns a multiplier between MIN_RECENCY_BOOST and MAX_RECENCY_BOOST.
pub fn recency_boost_factor(email_timestamp_secs: i64) -> f32 {
    let now_secs = chrono::Utc::now().timestamp();
    let age_secs = (now_secs - email_timestamp_secs).max(0) as f64;
    let age_days = age_secs / 86400.0;

    let decay = (-age_days / RECENCY_HALF_LIFE_DAYS).exp() as f32;
    MIN_RECENCY_BOOST + (MAX_RECENCY_BOOST - MIN_RECENCY_BOOST) * decay
}
```

- [ ] **Step 4: Add `search_with_recency_boost` to SearchIndex**

Add to `SearchIndex` in `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`:

```rust
use tantivy::SegmentReader;

impl SearchIndex {
    /// Search with BM25 relevance + recency boost.
    ///
    /// This is the primary search method for the UI. Bare text queries
    /// search across subject + body_text, with newer emails boosted.
    pub fn search_with_recency_boost(
        &self,
        query_str: &str,
        limit: usize,
    ) -> Result<Vec<(f32, DocAddress)>, SearchError> {
        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(
            &self.index,
            vec![self.schema.subject, self.schema.body_text],
        );
        let query = query_parser
            .parse_query(query_str)
            .map_err(|e| SearchError::QueryParse(e.to_string()))?;

        let date_field = self.schema.date;

        let collector = TopDocs::with_limit(limit).tweak_score(
            move |segment_reader: &SegmentReader| {
                // Get the date fast field column for this segment.
                let date_column = segment_reader
                    .fast_fields()
                    .date(date_field)
                    .expect("date field must be FAST");

                move |doc: DocId, original_score: Score| {
                    let tantivy_dt = date_column.first(doc);
                    if let Some(dt) = tantivy_dt {
                        let timestamp_secs = dt.into_timestamp_secs();
                        let boost = scoring::recency_boost_factor(timestamp_secs);
                        original_score * boost
                    } else {
                        original_score
                    }
                }
            },
        );

        let results = searcher.search(&query, &collector)?;
        Ok(results)
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index recency_boost
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/scoring.rs inboxly-store/src/search/mod.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): BM25 + recency boost scoring for search results"
```

---

## Task 15: Full index rebuild from Maildir + SQLite

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/rebuild.rs`

The spec says the tantivy index is "rebuildable from Maildir + SQLite at any time". This task implements a full rebuild that:
1. Deletes and recreates the index
2. Reads all EmailMeta from SQLite
3. Loads body text from Maildir for each email
4. Batch-indexes everything

This task defines the trait/callback interface. The actual SQLite/Maildir integration depends on the store's existing read APIs from M3/M4.

- [ ] **Step 1: Write the failing test**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
use inboxly_store::search::rebuild::RebuildSource;

/// Test implementation of RebuildSource that returns in-memory emails.
struct MockRebuildSource {
    emails: Vec<(EmailMeta, Option<String>, Option<BundleCategory>)>,
}

impl RebuildSource for MockRebuildSource {
    fn all_emails(&self) -> Box<dyn Iterator<Item = (EmailMeta, Option<String>, Option<BundleCategory>)> + '_> {
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index full_rebuild
```

Expected: FAIL — `RebuildSource` trait and `SearchIndex::rebuild` not found.

- [ ] **Step 3: Implement RebuildSource trait and rebuild method**

Write `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/rebuild.rs`:

```rust
use inboxly_core::types::{BundleCategory, EmailMeta};

/// Trait abstracting the data source for a full index rebuild.
///
/// Implemented by the Store (which combines SQLite metadata + Maildir body reads).
/// Using a trait allows the rebuild logic to be tested with mock data.
pub trait RebuildSource {
    /// Iterate over all emails with their body text and bundle category.
    ///
    /// The body text is loaded from Maildir. The bundle category comes from SQLite.
    /// Returns (EmailMeta, Option<body_text>, Option<BundleCategory>) for each email.
    fn all_emails(
        &self,
    ) -> Box<dyn Iterator<Item = (EmailMeta, Option<String>, Option<BundleCategory>)> + '_>;

    /// Total number of emails (for progress reporting).
    fn email_count(&self) -> u64;
}
```

Add `rebuild` method to `SearchIndex` in `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`:

```rust
use self::rebuild::RebuildSource;

/// Batch size for rebuild operations — commit every N documents
/// to bound memory usage during large rebuilds.
const REBUILD_BATCH_SIZE: usize = 5000;

impl SearchIndex {
    /// Destroy and rebuild the search index from scratch.
    ///
    /// 1. Deletes the existing index directory
    /// 2. Creates a new empty index
    /// 3. Iterates over all emails from the source
    /// 4. Batch-indexes them with periodic commits
    ///
    /// Returns the new SearchIndex, ready for queries.
    pub fn rebuild(path: &Path, source: &dyn RebuildSource) -> Result<Self, SearchError> {
        // Remove old index if it exists
        if path.exists() {
            std::fs::remove_dir_all(path)?;
        }

        let idx = Self::create(path)?;

        let mut count = 0;
        {
            let mut writer = idx.writer.lock().map_err(|_| SearchError::WriterLock)?;

            for (email, body_text, category) in source.all_emails() {
                let doc = idx.schema.build_document(
                    &email,
                    body_text.as_deref(),
                    category.as_ref(),
                );
                writer.add_document(doc)?;
                count += 1;

                // Periodic commit to bound memory
                if count % REBUILD_BATCH_SIZE == 0 {
                    writer.commit()?;
                }
            }

            // Final commit for remaining documents
            if count % REBUILD_BATCH_SIZE != 0 {
                writer.commit()?;
            }
        }

        idx.reader.reload()?;
        Ok(idx)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index full_rebuild
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/rebuild.rs inboxly-store/src/search/mod.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): full index rebuild from Maildir + SQLite source"
```

---

## Task 16: Retrieve search results as SearchHit structs

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`

Raw `(f32, DocAddress)` results aren't useful to callers. This task adds a `SearchHit` struct that extracts stored fields from result documents, providing the caller with email_id, subject, from, date, and score.

- [ ] **Step 1: Write the failing test**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index search_returns_structured
```

Expected: FAIL — `SearchHit` and `search` method not found.

- [ ] **Step 3: Implement SearchHit and the `search` method**

Add to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`:

```rust
use tantivy::TantivyDocument;
use tantivy::schema::Value;

/// A structured search result with extracted stored fields.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Message-ID of the matching email.
    pub email_id: String,
    /// Email subject line.
    pub subject: String,
    /// Sender (formatted as "Name <address>").
    pub from: String,
    /// Recipient(s).
    pub to: String,
    /// Email date as Unix timestamp in seconds.
    pub date_timestamp: i64,
    /// Whether the email has attachments.
    pub has_attachment: bool,
    /// BM25 relevance score (with recency boost applied).
    pub score: f32,
}

impl SearchIndex {
    /// High-level search: parse query, execute with recency boost, return structured hits.
    ///
    /// This is the primary API for the UI search bar.
    pub fn search(
        &self,
        query_str: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, SearchError> {
        let raw_results = self.search_with_recency_boost(query_str, limit)?;
        let searcher = self.reader.searcher();
        let mut hits = Vec::with_capacity(raw_results.len());

        for (score, doc_address) in raw_results {
            let doc: TantivyDocument = searcher.doc(doc_address)?;

            let email_id = doc
                .get_first(self.schema.email_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let subject = doc
                .get_first(self.schema.subject)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let from = doc
                .get_first(self.schema.from)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let to = doc
                .get_first(self.schema.to)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let date_timestamp = doc
                .get_first(self.schema.date)
                .and_then(|v| v.as_datetime())
                .map(|dt| dt.into_timestamp_secs())
                .unwrap_or(0);

            let has_attachment = doc
                .get_first(self.schema.has_attachment)
                .and_then(|v| v.as_u64())
                .map(|v| v == 1)
                .unwrap_or(false);

            hits.push(SearchHit {
                email_id,
                subject,
                from,
                to,
                date_timestamp,
                has_attachment,
                score,
            });
        }

        Ok(hits)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index search_returns_structured
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/mod.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): SearchHit struct for structured search results"
```

---

## Task 17: Destroy index (for rebuilds and testing)

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`

- [ ] **Step 1: Write the failing test**

Append to `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index clear_index
```

Expected: FAIL — `clear` method not found.

- [ ] **Step 3: Implement clear**

Add to `SearchIndex` in `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/search/mod.rs`:

```rust
use tantivy::query::AllQuery;

impl SearchIndex {
    /// Remove all documents from the index.
    ///
    /// Useful for full rebuilds where we want to start fresh without
    /// deleting the index directory.
    pub fn clear(&self) -> Result<(), SearchError> {
        let mut writer = self.writer.lock().map_err(|_| SearchError::WriterLock)?;
        writer.delete_all_documents()?;
        writer.commit()?;
        drop(writer);
        self.reader.reload()?;
        Ok(())
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index clear_index
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/src/search/mod.rs inboxly-store/tests/search_index.rs
git commit -m "feat(store): clear all documents from search index"
```

---

## Task 18: Integration tests — end-to-end search scenarios

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/search_index.rs` (append)

These tests exercise realistic search workflows: combined text + facet queries, multi-account filtering, and the full lifecycle of index -> search -> update -> search.

- [ ] **Step 1: Write combined filter + text search test**

```rust
#[test]
fn combined_text_and_facet_search() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    let mut email1 = make_test_email();
    email1.id = EmailId("promo-lunch@example.com".to_string());
    email1.subject = "Lunch special 50% off".to_string();

    let mut email2 = make_test_email();
    email2.id = EmailId("social-lunch@example.com".to_string());
    email2.subject = "Lunch with friends this weekend".to_string();

    let mut email3 = make_test_email();
    email3.id = EmailId("promo-shoes@example.com".to_string());
    email3.subject = "New shoes on sale".to_string();

    idx.add_email(&email1, None, Some(&BundleCategory::Promos)).unwrap();
    idx.add_email(&email2, None, Some(&BundleCategory::Social)).unwrap();
    idx.add_email(&email3, None, Some(&BundleCategory::Promos)).unwrap();
    idx.commit().unwrap();

    // "lunch" in Promos bundle — should match only email1
    let text_query = SearchQuery::multi_field(&idx.schema, &["subject"], "lunch");
    let facet_query = SearchQuery::facet_filter(&idx.schema, "bundle_category", "/bundle/promos");
    let combined = SearchQuery::filtered_search(text_query, facet_query);
    let results = idx.execute_query(&combined, 10).unwrap();
    assert_eq!(results.len(), 1);
}
```

- [ ] **Step 2: Write lifecycle test — index, update, delete, search**

```rust
#[test]
fn full_lifecycle_index_update_delete_search() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    // 1. Index an email without body
    let email = make_test_email();
    idx.add_email(&email, None, None).unwrap();
    idx.commit().unwrap();

    // Body text search should find nothing
    let results = idx.search_simple("coffee shop", 10).unwrap();
    assert_eq!(results.len(), 0);

    // 2. Update with body text (simulates Phase 2 sync)
    idx.update_email(
        &email,
        Some("Meet at the coffee shop at 3pm"),
        Some(&BundleCategory::Social),
    ).unwrap();
    idx.commit().unwrap();

    // Now body search works
    let results = idx.search_simple("coffee shop", 10).unwrap();
    assert_eq!(results.len(), 1);

    // And facet filter works
    let query = SearchQuery::facet_filter(&idx.schema, "bundle_category", "/bundle/social");
    let results = idx.execute_query(&query, 10).unwrap();
    assert_eq!(results.len(), 1);

    // 3. Delete the email
    idx.remove_email(&email.id).unwrap();
    idx.commit().unwrap();

    let results = idx.search_simple("coffee", 10).unwrap();
    assert_eq!(results.len(), 0);
    assert_eq!(idx.num_docs(), 0);
}
```

- [ ] **Step 3: Write multi-account search test**

```rust
#[test]
fn multi_account_search_with_account_filter() {
    let tmp = TempDir::new().unwrap();
    let idx = SearchIndex::create(&tmp.path().join("idx")).unwrap();

    let account_work = AccountId(Uuid::new_v4());
    let account_personal = AccountId(Uuid::new_v4());

    let mut work_email = make_test_email();
    work_email.id = EmailId("work-invoice@example.com".to_string());
    work_email.account_id = account_work;
    work_email.subject = "Q1 Invoice attached".to_string();

    let mut personal_email = make_test_email();
    personal_email.id = EmailId("personal-invoice@example.com".to_string());
    personal_email.account_id = account_personal;
    personal_email.subject = "Your personal invoice from Amazon".to_string();

    idx.add_email(&work_email, None, None).unwrap();
    idx.add_email(&personal_email, None, None).unwrap();
    idx.commit().unwrap();

    // "invoice" across all accounts = 2 results
    let results = idx.search_simple("invoice", 10).unwrap();
    assert_eq!(results.len(), 2);

    // "invoice" filtered to work account = 1 result
    let text_q = SearchQuery::multi_field(&idx.schema, &["subject"], "invoice");
    let acct_q = SearchQuery::facet_filter(
        &idx.schema,
        "account_id",
        &format!("/account/{}", account_work.0),
    );
    let combined = SearchQuery::filtered_search(text_q, acct_q);
    let results = idx.execute_query(&combined, 10).unwrap();
    assert_eq!(results.len(), 1);
}
```

- [ ] **Step 4: Write batch index + rebuild test**

```rust
#[test]
fn rebuild_replaces_existing_index_completely() {
    let tmp = TempDir::new().unwrap();
    let index_path = tmp.path().join("idx");

    // Create index with "old" emails
    {
        let idx = SearchIndex::create(&index_path).unwrap();
        for i in 0..5 {
            let mut email = make_test_email();
            email.id = EmailId(format!("old-{}@example.com", i));
            email.subject = format!("Old email {}", i);
            idx.add_email(&email, Some("old body"), None).unwrap();
        }
        idx.commit().unwrap();
        assert_eq!(idx.num_docs(), 5);
    }

    // Rebuild with "new" emails (only 2)
    let source = MockRebuildSource {
        emails: (0..2)
            .map(|i| {
                let mut email = make_test_email();
                email.id = EmailId(format!("new-{}@example.com", i));
                email.subject = format!("New email {}", i);
                (email, Some("new body".to_string()), None)
            })
            .collect(),
    };

    let idx = SearchIndex::rebuild(&index_path, &source).unwrap();
    assert_eq!(idx.num_docs(), 2);

    // Old emails should be gone
    let results = idx.search_simple("Old", 10).unwrap();
    assert_eq!(results.len(), 0);

    // New emails should be present
    let results = idx.search_simple("New", 10).unwrap();
    assert_eq!(results.len(), 2);
}
```

- [ ] **Step 5: Run all integration tests**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store --test search_index -- --test-threads=1
```

Note: `--test-threads=1` avoids potential filesystem contention with multiple tmpdir-based tests. Expected: all tests pass.

- [ ] **Step 6: Run clippy**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo clippy -p inboxly-store -- -D warnings
```

Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
cd /mnt/TempNVME/projects/inbox-rust
git add inboxly-store/tests/search_index.rs
git commit -m "test(store): integration tests for search index lifecycle"
```

---

## Summary

| Task | What It Delivers | Commit Message |
|------|-----------------|----------------|
| 1 | tantivy dependency | `feat(store): add tantivy dependency for full-text search` |
| 2 | Schema with 9 fields | `feat(store): define tantivy schema with all indexed fields` |
| 3 | EmailMeta -> Document conversion | `feat(store): convert EmailMeta to tantivy document` |
| 4 | SearchIndex struct (create/open/commit) | `feat(store): SearchIndex struct with create/open/commit lifecycle` |
| 5 | Single email index + retrieve | `feat(store): index and retrieve single email by email_id` |
| 6 | Batch indexing | `feat(store): batch indexing for initial sync and rebuilds` |
| 7 | Delete by EmailId | `feat(store): remove document from search index by EmailId` |
| 8 | Incremental update (delete + re-add) | `feat(store): incremental update (delete + re-add) for email documents` |
| 9 | Term + phrase queries | `feat(store): term and phrase query builders` |
| 10 | Multi-field search | `feat(store): multi-field search across from + subject + body` |
| 11 | Faceted search (account + category) | `feat(store): faceted search for account_id and bundle_category` |
| 12 | Date range queries | `feat(store): date range queries (after, before, between)` |
| 13 | has:attachment query | `feat(store): has:attachment query filter` |
| 14 | BM25 + recency boost scorer | `feat(store): BM25 + recency boost scoring for search results` |
| 15 | Full rebuild from source | `feat(store): full index rebuild from Maildir + SQLite source` |
| 16 | SearchHit struct | `feat(store): SearchHit struct for structured search results` |
| 17 | Clear/destroy index | `feat(store): clear all documents from search index` |
| 18 | Integration tests | `test(store): integration tests for search index lifecycle` |

**After M5**: The storage engine is complete (SQLite + Maildir + tantivy). Emails can be stored, read, searched, and the search index can be fully rebuilt. The search API supports all query types from the spec: term, phrase, multi-field, faceted, date range, and boolean attachment filter. Results are ranked by BM25 with recency boost. Ready for M6 (IMAP sync) to start populating real data.
