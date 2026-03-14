use std::ops::Bound;

use chrono::{DateTime, Utc};
use tantivy::query::{BooleanQuery, Occur, PhraseQuery, Query, RangeQuery, TermQuery};
use tantivy::schema::{Facet, IndexRecordOption};
use tantivy::{DateTime as TantivyDateTime, Index, Term};

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
        let parser = tantivy::query::QueryParser::for_index(&tmp_index, fields);

        parser
            .parse_query(query_str)
            .expect("failed to parse multi-field query")
    }

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

    /// Date range query: emails after the given date (inclusive).
    ///
    /// Maps to spec syntax `after:2026-01-01`.
    pub fn date_after(schema: &SearchSchema, after: DateTime<Utc>) -> Box<dyn Query> {
        let _ = schema; // field name is known
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
        let _ = schema;
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
        let _ = schema;
        let after_tantivy = TantivyDateTime::from_timestamp_secs(after.timestamp());
        let before_tantivy = TantivyDateTime::from_timestamp_secs(before.timestamp());
        Box::new(RangeQuery::new_date_bounds(
            "date".to_string(),
            Bound::Included(after_tantivy),
            Bound::Included(before_tantivy),
        ))
    }

    /// Query for emails with attachments.
    ///
    /// Maps to spec syntax `has:attachment`.
    /// The `has_attachment` field stores 1 (has) or 0 (doesn't have).
    pub fn has_attachment(schema: &SearchSchema) -> Box<dyn Query> {
        let term = Term::from_field_u64(schema.has_attachment, 1);
        Box::new(TermQuery::new(term, IndexRecordOption::Basic))
    }
}
