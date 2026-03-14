pub mod query;
pub mod rebuild;
pub mod schema;
pub mod scoring;

use std::path::Path;
use std::sync::Mutex;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::{DocAddress, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

use self::schema::SearchSchema;
use inboxly_core::{BundleCategory, EmailId, EmailMeta};

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

    /// Get the IndexReader for executing searches.
    pub fn reader(&self) -> &IndexReader {
        &self.reader
    }

    /// Return the total number of documents in the index.
    pub fn num_docs(&self) -> u64 {
        let searcher = self.reader.searcher();
        searcher.num_docs()
    }

    /// Commit all pending changes and make them searchable.
    pub fn commit(&self) -> Result<(), SearchError> {
        let mut writer = self.writer.lock().map_err(|_| SearchError::WriterLock)?;
        writer.commit()?;
        drop(writer); // Release lock before reload
        self.reader.reload()?;
        Ok(())
    }

    /// Index a single email. Call `commit()` afterwards to make it searchable.
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
        let writer = self.writer.lock().map_err(|_| SearchError::WriterLock)?;

        // Delete the old document
        let term = Term::from_field_text(self.schema.email_id, &email.id.0);
        writer.delete_term(term);

        // Add the new document
        let doc = self.schema.build_document(email, body_text, bundle_category);
        writer.add_document(doc)?;

        Ok(())
    }

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

    #[error("query parse error: {0}")]
    QueryParse(String),
}
