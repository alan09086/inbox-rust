# M24: Search + Query Parser — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Implement full-text search with a custom query syntax parser, tantivy query construction, BM25 + recency ranking, and a search results UI with snippet highlighting and debounced search-as-you-type.

**Architecture:** Query parsing and tantivy query construction live in `inboxly-store` (no UI types). The search bar widget and results view live in `inboxly-ui`. Mixed queries (tantivy + SQLite state filters like `is:pinned`) execute tantivy first, then filter via SQLite join.

**Tech Stack:** Rust, tantivy, rusqlite, iced

**Prerequisites:**
- M5 (tantivy index — schema, index writer, incremental add/delete, reader)
- M15 (Iced shell with toolbar and search bar placeholder)
- M3 (SQLite schema — `thread_state.pinned`, `emails.flags` for unread detection)

**Spec ref:** Design spec section "Search & Highlights" (lines 438-461), SearchBar widget (line 529), UI Communication (line 562)

---

## Task Overview

| # | Task | Crate | Est. |
|---|------|-------|------|
| 1 | Define search types (query AST, results, snippets) | `store` | 10 min |
| 2 | Query tokeniser (split raw input into typed tokens) | `store` | 20 min |
| 3 | Query parser (tokens to structured `SearchQuery` AST) | `store` | 25 min |
| 4 | Tantivy query builder (AST to tantivy `Query`) | `store` | 25 min |
| 5 | BM25 recency boost collector | `store` | 15 min |
| 6 | Snippet generator (extract matching context from body) | `store` | 20 min |
| 7 | Search executor (orchestrate tantivy + SQLite filtering) | `store` | 25 min |
| 8 | Public search API on Store | `store` | 10 min |
| 9 | SearchBar widget (Iced) | `ui` | 25 min |
| 10 | Search results view | `ui` | 25 min |
| 11 | Search-as-you-type debounce | `ui` | 15 min |
| 12 | Unit tests for parser + query builder | `store` | 20 min |
| 13 | Integration tests (end-to-end search) | `store` | 20 min |
| 14 | UI integration (wire search into app state) | `ui` | 15 min |

---

### Task 1 — Define search types (query AST, results, snippets)

**File:** `inboxly-store/src/search/types.rs` (new)

Define the types that represent parsed queries, search results, and highlighted snippets. These are pure data types with no tantivy or Iced dependencies.

**Types:**

```rust
use chrono::{DateTime, Utc, NaiveDate};
use inboxly_core::{ThreadId, BundleCategory};

/// A fully parsed search query, ready for execution.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchQuery {
    /// Full-text terms to match across subject + body_text.
    pub text_terms: Vec<TextTerm>,
    /// Field-specific filters.
    pub field_filters: Vec<FieldFilter>,
    /// State filters that require SQLite join (not in tantivy).
    pub state_filters: Vec<StateFilter>,
}

/// A bare text term or quoted phrase for full-text search.
#[derive(Debug, Clone, PartialEq)]
pub enum TextTerm {
    /// Single word: matched against subject + body_text.
    Word(String),
    /// Quoted phrase: "meeting tomorrow" — matched as exact phrase.
    Phrase(String),
}

/// A field-specific filter from query syntax (e.g., from:sarah).
#[derive(Debug, Clone, PartialEq)]
pub enum FieldFilter {
    From(String),
    To(String),
    Subject(String),
    HasAttachment,
    InBundle(String),          // bundle category name
    After(NaiveDate),
    Before(NaiveDate),
}

/// A state filter that requires SQLite lookup, not tantivy.
#[derive(Debug, Clone, PartialEq)]
pub enum StateFilter {
    Pinned,
    Unread,
}

/// A single search result with relevance score and snippet.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub thread_id: ThreadId,
    pub score: f32,
    /// Subject of the thread.
    pub subject: String,
    /// From address of the newest email in the thread.
    pub from: String,
    /// Date of the newest email in the thread.
    pub date: DateTime<Utc>,
    /// Highlighted snippet showing matching context.
    pub snippet: HighlightedSnippet,
    /// Whether the thread has unread emails.
    pub has_unread: bool,
    /// Whether the thread has attachments.
    pub has_attachments: bool,
}

/// A snippet with highlighted match regions.
#[derive(Debug, Clone)]
pub struct HighlightedSnippet {
    /// Segments of the snippet, alternating between plain and highlighted.
    pub segments: Vec<SnippetSegment>,
}

/// One segment of a highlighted snippet.
#[derive(Debug, Clone)]
pub enum SnippetSegment {
    Plain(String),
    Highlight(String),
}

impl HighlightedSnippet {
    /// Construct from plain text (no highlighting).
    pub fn plain(text: &str) -> Self {
        Self {
            segments: vec![SnippetSegment::Plain(text.to_string())],
        }
    }

    /// Get the full text without highlighting markers.
    pub fn to_plain_text(&self) -> String {
        self.segments
            .iter()
            .map(|s| match s {
                SnippetSegment::Plain(t) | SnippetSegment::Highlight(t) => t.as_str(),
            })
            .collect()
    }
}

/// Container for search results with pagination info.
#[derive(Debug, Clone)]
pub struct SearchResults {
    pub results: Vec<SearchResult>,
    pub total_hits: usize,
    pub query_text: String,
    pub elapsed_ms: u64,
}

impl SearchQuery {
    /// Returns true if this query has any tantivy-searchable components
    /// (text terms or field filters). State-only queries (just is:pinned)
    /// don't need tantivy at all.
    pub fn has_tantivy_components(&self) -> bool {
        !self.text_terms.is_empty() || !self.field_filters.is_empty()
    }

    /// Returns true if this query has state filters that need SQLite.
    pub fn has_state_filters(&self) -> bool {
        !self.state_filters.is_empty()
    }

    /// Returns true if the query is empty (no terms, no filters).
    pub fn is_empty(&self) -> bool {
        self.text_terms.is_empty()
            && self.field_filters.is_empty()
            && self.state_filters.is_empty()
    }
}
```

**Tests** (inline `#[cfg(test)]`):

1. `SearchQuery::is_empty` returns true for default/empty query
2. `SearchQuery::has_tantivy_components` returns true when text terms present
3. `SearchQuery::has_tantivy_components` returns false for state-only query
4. `HighlightedSnippet::plain` produces single Plain segment
5. `HighlightedSnippet::to_plain_text` concatenates all segments

**Commit:** `feat(store): define search query AST, result, and snippet types`

---

### Task 2 — Query tokeniser

**File:** `inboxly-store/src/search/tokeniser.rs` (new)

Tokenise raw user input into typed tokens. This stage handles splitting on whitespace while respecting quoted phrases, and identifies field prefixes (`from:`, `to:`, etc.).

**Types:**

```rust
/// A raw token from the user's search input.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A field:value pair, e.g., from:sarah, has:attachment, is:pinned
    FieldValue { field: String, value: String },
    /// A quoted phrase, e.g., "hello world"
    Phrase(String),
    /// A bare word, e.g., meeting
    Word(String),
}
```

**Function:**

```rust
/// Tokenise a raw search query string into structured tokens.
///
/// Rules:
/// - Splits on whitespace (spaces, tabs)
/// - Quoted strings ("...") are preserved as Phrase tokens, including internal spaces
/// - field:value patterns are detected for known fields:
///   from, to, subject, has, in, after, before, is
/// - field:value where value is quoted ("field:\"multi word\"") extracts the quoted content
/// - Unknown field prefixes (e.g., "foo:bar") are treated as bare words
/// - Empty input returns empty vec
///
/// Known fields (case-insensitive):
///   from, to, subject, has, in, after, before, is
pub fn tokenise(input: &str) -> Vec<Token>;
```

**Implementation notes:**
- Iterate character by character to handle quote state
- Track whether we're inside a quoted string
- When encountering `"`, toggle quote mode
- When encountering `:` outside quotes, check if the prefix is a known field name
- Handle edge cases: unclosed quotes (treat rest of input as the phrase), empty value after colon (treat as bare word), colon at end of word (treat as bare word)
- Field names are case-insensitive: `From:`, `FROM:`, `from:` all match

**Algorithm:**

```
fn tokenise(input):
    tokens = []
    i = 0
    chars = input.chars().collect()

    while i < chars.len():
        skip whitespace

        if chars[i] == '"':
            // Read until closing quote or end of input
            phrase = read_until('"' or EOF)
            tokens.push(Phrase(phrase))
            continue

        // Read a non-whitespace chunk
        chunk = read_until(whitespace or EOF)

        // Check for field:value pattern
        if chunk contains ':':
            (field, value) = split_first(':')
            field_lower = field.to_lowercase()

            if field_lower in KNOWN_FIELDS:
                // Handle quoted value: from:"John Smith"
                if value.starts_with('"'):
                    // The value starts with quote — read until closing quote
                    // (the closing quote may be in a subsequent chunk)
                    value = strip quotes, possibly read more chars
                tokens.push(FieldValue { field: field_lower, value })
            else:
                // Unknown field prefix — treat entire chunk as bare word
                tokens.push(Word(chunk))
        else:
            tokens.push(Word(chunk))

    return tokens
```

**Edge cases to handle:**
- `from:"John Smith"` — field with quoted multi-word value
- `"hello` — unclosed quote, treat rest of input as phrase
- `:value` — colon at start, treat as bare word
- `field:` — empty value, treat as bare word
- `from:` — known field but empty value, treat as bare word
- Multiple colons: `from:user@example.com` — value is `user@example.com` (only split on first colon)
- `foo:bar` where `foo` is not a known field — treat as `Word("foo:bar")`

**Tests:**

1. Empty string → empty vec
2. Single bare word → `[Word("meeting")]`
3. Multiple bare words → `[Word("hello"), Word("world")]`
4. Quoted phrase → `[Phrase("hello world")]`
5. `from:sarah` → `[FieldValue { field: "from", value: "sarah" }]`
6. `FROM:Sarah` → `[FieldValue { field: "from", value: "Sarah" }]` (case-insensitive field)
7. `from:"John Smith"` → `[FieldValue { field: "from", value: "John Smith" }]`
8. `has:attachment` → `[FieldValue { field: "has", value: "attachment" }]`
9. `is:pinned` → `[FieldValue { field: "is", value: "pinned" }]`
10. `after:2026-01-01` → `[FieldValue { field: "after", value: "2026-01-01" }]`
11. Mixed: `from:sarah meeting "lunch plans"` → 3 tokens
12. `from:user@example.com` → value is `user@example.com` (colon in value)
13. Unknown field: `foo:bar` → `[Word("foo:bar")]`
14. Unclosed quote: `"hello world` → `[Phrase("hello world")]`
15. Empty value: `from:` → `[Word("from:")]`
16. `in:purchases subject:receipt` → 2 FieldValue tokens

**Commit:** `feat(store): implement search query tokeniser`

---

### Task 3 — Query parser (tokens to SearchQuery AST)

**File:** `inboxly-store/src/search/parser.rs` (new)

Transform tokens from the tokeniser into the structured `SearchQuery` AST. This stage validates field values (e.g., dates parse correctly, `has:` only accepts `attachment`, `is:` only accepts `pinned`/`unread`).

**Function:**

```rust
use super::types::*;
use super::tokeniser::Token;

/// Parse tokenised input into a structured SearchQuery.
///
/// - Word tokens → TextTerm::Word
/// - Phrase tokens → TextTerm::Phrase
/// - FieldValue tokens → mapped to FieldFilter or StateFilter
///
/// Invalid field values (e.g., after:not-a-date, has:foo) are treated
/// as plain text search terms rather than causing an error. This follows
/// the principle of least surprise — the user's intent was to search,
/// even if the syntax is wrong.
///
/// Returns a SearchQuery. Never fails — malformed input degrades to
/// plain text search, never to an error.
pub fn parse(tokens: &[Token]) -> SearchQuery;

/// Parse a date string in YYYY-MM-DD format.
/// Returns None if the format doesn't match.
fn parse_date(s: &str) -> Option<NaiveDate>;
```

**Field mapping rules:**

| Token | Maps To |
|-------|---------|
| `FieldValue { field: "from", value }` | `FieldFilter::From(value)` |
| `FieldValue { field: "to", value }` | `FieldFilter::To(value)` |
| `FieldValue { field: "subject", value }` | `FieldFilter::Subject(value)` |
| `FieldValue { field: "has", value: "attachment" }` | `FieldFilter::HasAttachment` |
| `FieldValue { field: "has", value: other }` | `TextTerm::Word("has:other")` (invalid, degrade) |
| `FieldValue { field: "in", value }` | `FieldFilter::InBundle(value)` |
| `FieldValue { field: "after", value }` | `FieldFilter::After(parse_date(value))` or degrade if invalid date |
| `FieldValue { field: "before", value }` | `FieldFilter::Before(parse_date(value))` or degrade if invalid date |
| `FieldValue { field: "is", value: "pinned" }` | `StateFilter::Pinned` |
| `FieldValue { field: "is", value: "unread" }` | `StateFilter::Unread` |
| `FieldValue { field: "is", value: other }` | `TextTerm::Word("is:other")` (invalid, degrade) |

**Date parsing:**
- Accept `YYYY-MM-DD` format (e.g., `2026-01-15`)
- Use `NaiveDate::parse_from_str(value, "%Y-%m-%d")`
- If parse fails, degrade the entire `FieldValue` to a `TextTerm::Word` of the original text

**Implementation:**

```rust
pub fn parse(tokens: &[Token]) -> SearchQuery {
    let mut query = SearchQuery {
        text_terms: vec![],
        field_filters: vec![],
        state_filters: vec![],
    };

    for token in tokens {
        match token {
            Token::Word(w) => {
                query.text_terms.push(TextTerm::Word(w.to_lowercase()));
            }
            Token::Phrase(p) => {
                query.text_terms.push(TextTerm::Phrase(p.clone()));
            }
            Token::FieldValue { field, value } => {
                match field.as_str() {
                    "from" => query.field_filters.push(FieldFilter::From(value.to_lowercase())),
                    "to" => query.field_filters.push(FieldFilter::To(value.to_lowercase())),
                    "subject" => query.field_filters.push(FieldFilter::Subject(value.to_lowercase())),
                    "has" if value.eq_ignore_ascii_case("attachment") => {
                        query.field_filters.push(FieldFilter::HasAttachment);
                    }
                    "in" => {
                        query.field_filters.push(FieldFilter::InBundle(value.to_lowercase()));
                    }
                    "after" => {
                        match parse_date(value) {
                            Some(d) => query.field_filters.push(FieldFilter::After(d)),
                            None => query.text_terms.push(TextTerm::Word(
                                format!("{}:{}", field, value),
                            )),
                        }
                    }
                    "before" => {
                        match parse_date(value) {
                            Some(d) => query.field_filters.push(FieldFilter::Before(d)),
                            None => query.text_terms.push(TextTerm::Word(
                                format!("{}:{}", field, value),
                            )),
                        }
                    }
                    "is" if value.eq_ignore_ascii_case("pinned") => {
                        query.state_filters.push(StateFilter::Pinned);
                    }
                    "is" if value.eq_ignore_ascii_case("unread") => {
                        query.state_filters.push(StateFilter::Unread);
                    }
                    _ => {
                        // Unknown or invalid — degrade to text search
                        query.text_terms.push(TextTerm::Word(
                            format!("{}:{}", field, value),
                        ));
                    }
                }
            }
        }
    }

    query
}
```

**Tests:**

1. Empty token list → empty SearchQuery
2. Single Word → one TextTerm::Word
3. Single Phrase → one TextTerm::Phrase
4. `from:sarah` token → FieldFilter::From("sarah")
5. `to:bob@example.com` → FieldFilter::To("bob@example.com")
6. `subject:lunch` → FieldFilter::Subject("lunch")
7. `has:attachment` → FieldFilter::HasAttachment
8. `has:something` → TextTerm::Word("has:something") (degraded)
9. `in:purchases` → FieldFilter::InBundle("purchases")
10. `after:2026-01-01` → FieldFilter::After(NaiveDate(2026, 1, 1))
11. `after:not-a-date` → TextTerm::Word("after:not-a-date") (degraded)
12. `before:2026-03-01` → FieldFilter::Before(NaiveDate(2026, 3, 1))
13. `is:pinned` → StateFilter::Pinned
14. `is:unread` → StateFilter::Unread
15. `is:starred` → TextTerm::Word("is:starred") (degraded)
16. Mixed query: `from:sarah meeting is:pinned` → 1 field filter, 1 text term, 1 state filter
17. All values lowercased: `FROM:Sarah` token → FieldFilter::From("sarah")

**Commit:** `feat(store): implement search query parser (tokens to AST)`

---

### Task 4 — Tantivy query builder (AST to tantivy Query)

**File:** `inboxly-store/src/search/query_builder.rs` (new)

Transform a `SearchQuery` AST into a tantivy `Box<dyn Query>`. Only the tantivy-searchable components are translated — state filters are handled separately in the executor (Task 7).

**Dependencies:** This task depends on the tantivy index schema from M5. The M5 schema defines these indexed fields:
- `from` (TEXT, tokenised)
- `to` (TEXT, tokenised)
- `subject` (TEXT, tokenised)
- `body_text` (TEXT, tokenised, full-text)
- `date` (DATE or I64, unix timestamp)
- `account_id` (FACET)
- `bundle_category` (FACET)
- `has_attachment` (BOOL or U64 where 1=true)
- `thread_id` (STRING, stored, not tokenised — for grouping results)
- `email_id` (STRING, stored, not tokenised — for snippet retrieval)

**Function:**

```rust
use tantivy::query::*;
use tantivy::schema::*;
use tantivy::{Index, Term};
use super::types::*;

/// Build a tantivy query from the tantivy-searchable parts of a SearchQuery.
///
/// Returns None if there are no tantivy-searchable components (e.g., query
/// is purely state filters like `is:pinned`).
///
/// Query construction:
/// - Multiple components are combined with BooleanQuery (all MUST match)
/// - Text terms: full-text query across subject + body_text
/// - Field filters: term queries, range queries, or bool queries per field
pub fn build_tantivy_query(
    query: &SearchQuery,
    schema: &Schema,
) -> Option<Box<dyn Query>>;
```

**Sub-functions:**

```rust
/// Build a full-text query for bare words/phrases across subject + body_text.
///
/// For Word terms:
///   Create a BooleanQuery with SHOULD clauses for subject and body_text.
///   This means matching in either field returns results.
///
/// For Phrase terms:
///   Create PhraseQuery on each full-text field, combined with SHOULD.
///
/// Multiple text terms are combined with MUST (all terms must match in
/// at least one field).
fn build_text_query(
    terms: &[TextTerm],
    schema: &Schema,
) -> Option<Box<dyn Query>>;

/// Build a query for a single FieldFilter.
fn build_field_filter_query(
    filter: &FieldFilter,
    schema: &Schema,
) -> Box<dyn Query>;
```

**Query mapping detail:**

| FieldFilter | Tantivy Query |
|-------------|---------------|
| `From(value)` | `TermQuery` on `from` field with the value as a term. If value contains `@`, match as full term; otherwise match as tokenised term (partial name match). |
| `To(value)` | `TermQuery` on `to` field, same logic as From. |
| `Subject(value)` | `TermQuery` on `subject` field. |
| `HasAttachment` | `TermQuery` on `has_attachment` field, matching `true` (or U64 value 1, depending on M5 schema). |
| `InBundle(category)` | `TermQuery` on `bundle_category` facet field. Use `Facet::from_path(&[category])`. |
| `After(date)` | `RangeQuery` on `date` field, `>=` the start of the given day (midnight UTC epoch seconds). |
| `Before(date)` | `RangeQuery` on `date` field, `<=` the end of the given day (23:59:59 UTC epoch seconds). |

**Text term query construction:**

```rust
fn build_text_query(terms: &[TextTerm], schema: &Schema) -> Option<Box<dyn Query>> {
    if terms.is_empty() {
        return None;
    }

    let subject_field = schema.get_field("subject").unwrap();
    let body_field = schema.get_field("body_text").unwrap();

    let mut must_clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

    for term in terms {
        match term {
            TextTerm::Word(word) => {
                // Match this word in EITHER subject or body_text
                let subject_term = Term::from_field_text(subject_field, word);
                let body_term = Term::from_field_text(body_field, word);

                let either_field = BooleanQuery::new(vec![
                    (Occur::Should, Box::new(TermQuery::new(subject_term, IndexRecordOption::WithFreqsAndPositions))),
                    (Occur::Should, Box::new(TermQuery::new(body_term, IndexRecordOption::WithFreqsAndPositions))),
                ]);

                must_clauses.push((Occur::Must, Box::new(either_field)));
            }
            TextTerm::Phrase(phrase) => {
                // Split phrase into words, create PhraseQuery on each field
                let words: Vec<&str> = phrase.split_whitespace().collect();
                if words.is_empty() {
                    continue;
                }

                let subject_terms: Vec<Term> = words.iter()
                    .map(|w| Term::from_field_text(subject_field, &w.to_lowercase()))
                    .collect();
                let body_terms: Vec<Term> = words.iter()
                    .map(|w| Term::from_field_text(body_field, &w.to_lowercase()))
                    .collect();

                let either_field = BooleanQuery::new(vec![
                    (Occur::Should, Box::new(PhraseQuery::new(subject_terms))),
                    (Occur::Should, Box::new(PhraseQuery::new(body_terms))),
                ]);

                must_clauses.push((Occur::Must, Box::new(either_field)));
            }
        }
    }

    if must_clauses.is_empty() {
        return None;
    }

    Some(Box::new(BooleanQuery::new(must_clauses)))
}
```

**Top-level query construction:**

```rust
pub fn build_tantivy_query(
    query: &SearchQuery,
    schema: &Schema,
) -> Option<Box<dyn Query>> {
    if !query.has_tantivy_components() {
        return None;
    }

    let mut must_clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

    // Add text terms query
    if let Some(text_q) = build_text_query(&query.text_terms, schema) {
        must_clauses.push((Occur::Must, text_q));
    }

    // Add field filter queries
    for filter in &query.field_filters {
        must_clauses.push((Occur::Must, build_field_filter_query(filter, schema)));
    }

    match must_clauses.len() {
        0 => None,
        1 => Some(must_clauses.into_iter().next().unwrap().1),
        _ => Some(Box::new(BooleanQuery::new(must_clauses))),
    }
}
```

**Implementation notes:**
- The tantivy schema field types must match what M5 created. If M5 uses `FAST` fields for date (common for range queries), the RangeQuery will work. If M5 uses a DateTimeField, convert `NaiveDate` to tantivy's DateTime format.
- For date range queries: `After(date)` means `>= date at 00:00:00 UTC`. `Before(date)` means `<= date at 23:59:59 UTC`. Convert using `date.and_hms(0,0,0).timestamp()` and `date.and_hms(23,59,59).timestamp()`.
- The `has_attachment` field type in M5 may be a `BOOL` field (tantivy 0.22+) or a `U64` field with 0/1 values. Check M5's schema definition and adapt. If U64: `Term::from_field_u64(field, 1)`.
- For `InBundle`, the bundle_category facet stores categories as facet paths. Use `Facet::from_path(&[category])` and wrap in `TermQuery` on the facet field.

**Tests:**

1. Empty query → returns None
2. Single word → BooleanQuery with SHOULD on subject + body_text
3. Single phrase → PhraseQuery on subject and body_text with SHOULD
4. `from:sarah` → TermQuery on from field
5. `has:attachment` → TermQuery on has_attachment field
6. `after:2026-01-01` → RangeQuery on date field (>= epoch)
7. `before:2026-03-01` → RangeQuery on date field (<= epoch)
8. `in:purchases` → TermQuery on bundle_category facet
9. Mixed: `from:sarah meeting` → BooleanQuery with MUST for from + text query
10. State-only query (`is:pinned`) → returns None (no tantivy component)
11. Multiple text terms: `hello world` → each word is a MUST clause
12. Multiple field filters: `from:sarah to:bob` → both in MUST clauses

**Commit:** `feat(store): build tantivy queries from parsed search AST`

---

### Task 5 — BM25 recency boost collector

**File:** `inboxly-store/src/search/scoring.rs` (new)

Implement a custom score modifier that boosts recent emails in search results. Tantivy uses BM25 by default; we multiply the BM25 score by a recency factor so that recent matches rank higher than old ones with the same text relevance.

**Approach:** Use tantivy's `TopDocs` collector with a custom `ScoreModifier` (or `TweakedScoreTopCollector` pattern). The recency boost is a multiplicative factor based on the document's date relative to the current time.

**Design:**

```rust
use tantivy::collector::TopDocs;
use tantivy::fastfield::FastFieldReader;
use tantivy::schema::Schema;
use tantivy::{DocId, Score, SegmentReader};
use std::time::{SystemTime, UNIX_EPOCH};

/// Maximum number of search results to return.
pub const MAX_SEARCH_RESULTS: usize = 50;

/// Recency boost parameters.
/// The boost follows an exponential decay: boost = 1.0 + max_boost * exp(-age_days / half_life_days)
/// - Emails from today get boost ~1.0 + max_boost (e.g., 1.5x)
/// - Emails from half_life_days ago get boost ~1.0 + max_boost/2 (e.g., 1.25x)
/// - Very old emails get boost ~1.0 (no boost, pure BM25)
const RECENCY_MAX_BOOST: f32 = 0.5;
const RECENCY_HALF_LIFE_DAYS: f32 = 30.0;

/// Create a TopDocs collector with recency-boosted scoring.
///
/// Uses tantivy's `TopDocs::with_limit().tweak_score()` to modify
/// BM25 scores based on the `date` fast field.
///
/// The date field must be indexed as a fast field (FAST flag) for this
/// to work efficiently. If the date field is not a fast field, fall back
/// to standard BM25 without recency boost.
pub fn recency_boosted_collector(
    schema: &Schema,
    limit: usize,
) -> impl tantivy::collector::Collector<Fruit = Vec<(f32, tantivy::DocAddress)>>;
```

**Implementation:**

```rust
pub fn recency_boosted_collector(
    schema: &Schema,
    limit: usize,
) -> impl tantivy::collector::Collector<Fruit = Vec<(f32, tantivy::DocAddress)>> {
    let date_field = schema.get_field("date").expect("date field must exist in schema");

    let now_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    TopDocs::with_limit(limit).tweak_score(move |segment_reader: &SegmentReader| {
        // Attempt to get the date fast field reader.
        // If the field isn't FAST, return identity scorer.
        let date_reader = segment_reader
            .fast_fields()
            .i64("date")
            .ok();

        move |doc_id: DocId, original_score: Score| -> Score {
            let Some(ref reader) = date_reader else {
                return original_score;
            };

            let doc_epoch = reader.get_val(doc_id);
            let age_seconds = (now_epoch - doc_epoch).max(0) as f32;
            let age_days = age_seconds / 86400.0;

            let recency_boost = 1.0
                + RECENCY_MAX_BOOST * (-age_days / RECENCY_HALF_LIFE_DAYS).exp();

            original_score * recency_boost
        }
    })
}
```

**Implementation notes:**
- The tantivy `tweak_score` API takes a closure that receives a `SegmentReader` and returns a scoring closure. This is called per-segment.
- The fast field API varies by tantivy version. For tantivy 0.22+, use `segment_reader.fast_fields().i64("date")`. For older versions, use `segment_reader.fast_fields().i64(date_field)`. Check M5's tantivy version and adapt.
- If the date field is stored as `DateTimeField` in tantivy (tantivy 0.22+), use the appropriate fast field accessor for DateTime. The value will be in microseconds since epoch — adjust the age calculation accordingly.
- The exponential decay curve:
  - Age 0 days: boost = 1.5x
  - Age 30 days: boost = ~1.18x
  - Age 90 days: boost = ~1.02x
  - Age 365 days: boost = ~1.0x (essentially no boost)

**Tests:**

1. Collector returns results in score-descending order
2. Two documents with identical text relevance: the newer one ranks higher
3. Very old document (1 year): recency boost is negligible (~1.0)
4. Document from today: recency boost is significant (~1.5x)
5. Collector respects the `limit` parameter
6. Graceful fallback when date fast field is unavailable (returns standard BM25 scores)

**Commit:** `feat(store): implement BM25 + recency boost search scoring`

---

### Task 6 — Snippet generator

**File:** `inboxly-store/src/search/snippets.rs` (new)

Generate highlighted text snippets showing where search terms match in the email body. These snippets appear in search result rows.

**Functions:**

```rust
use super::types::*;

/// Maximum length of a snippet in characters.
const SNIPPET_MAX_CHARS: usize = 200;

/// Context characters to show before and after a match.
const SNIPPET_CONTEXT_CHARS: usize = 40;

/// Generate a highlighted snippet from email body text and search terms.
///
/// Algorithm:
/// 1. Find the first occurrence of any search term in the body text.
/// 2. Extract a window of text around that match (SNIPPET_CONTEXT_CHARS before and after).
/// 3. Mark the matching term as Highlight, surrounding text as Plain.
/// 4. If multiple terms match within the window, highlight all of them.
/// 5. If no terms match, return the first SNIPPET_MAX_CHARS of the body as plain text.
/// 6. Add "..." at boundaries if the snippet doesn't start at the beginning
///    or end at the end of the body.
///
/// Search terms are matched case-insensitively.
pub fn generate_snippet(body: &str, terms: &[TextTerm]) -> HighlightedSnippet;

/// Find all match positions (byte offset, length) for a term in the text.
/// Returns matches sorted by position.
fn find_matches(text: &str, term: &TextTerm) -> Vec<(usize, usize)>;

/// Build a HighlightedSnippet from text and sorted, non-overlapping match ranges.
/// Ranges are (byte_offset, byte_length).
fn build_highlighted(
    text: &str,
    matches: &[(usize, usize)],
    window_start: usize,
    window_end: usize,
    prepend_ellipsis: bool,
    append_ellipsis: bool,
) -> HighlightedSnippet;
```

**Implementation detail:**

```rust
pub fn generate_snippet(body: &str, terms: &[TextTerm]) -> HighlightedSnippet {
    if body.is_empty() || terms.is_empty() {
        let truncated = if body.len() > SNIPPET_MAX_CHARS {
            format!("{}...", &body[..SNIPPET_MAX_CHARS])
        } else {
            body.to_string()
        };
        return HighlightedSnippet::plain(&truncated);
    }

    // Collect all matches across all terms
    let mut all_matches: Vec<(usize, usize)> = Vec::new();
    for term in terms {
        all_matches.extend(find_matches(body, term));
    }

    if all_matches.is_empty() {
        // No matches — return start of body
        let end = body.len().min(SNIPPET_MAX_CHARS);
        let end = snap_to_char_boundary(body, end);
        let append_ellipsis = end < body.len();
        return build_highlighted(body, &[], 0, end, false, append_ellipsis);
    }

    // Sort by position, merge overlapping ranges
    all_matches.sort_by_key(|&(pos, _)| pos);
    let merged = merge_overlapping(&all_matches);

    // Center the window around the first match
    let first_match_start = merged[0].0;
    let window_start = first_match_start.saturating_sub(SNIPPET_CONTEXT_CHARS);
    let window_start = snap_to_char_boundary(body, window_start);
    let window_end = (first_match_start + SNIPPET_MAX_CHARS).min(body.len());
    let window_end = snap_to_char_boundary(body, window_end);

    // Filter matches to those within the window
    let window_matches: Vec<(usize, usize)> = merged
        .into_iter()
        .filter(|&(pos, len)| pos >= window_start && pos + len <= window_end)
        .collect();

    let prepend_ellipsis = window_start > 0;
    let append_ellipsis = window_end < body.len();

    build_highlighted(
        body,
        &window_matches,
        window_start,
        window_end,
        prepend_ellipsis,
        append_ellipsis,
    )
}

fn find_matches(text: &str, term: &TextTerm) -> Vec<(usize, usize)> {
    let lower_text = text.to_lowercase();
    let mut matches = Vec::new();

    let search_str = match term {
        TextTerm::Word(w) => w.to_lowercase(),
        TextTerm::Phrase(p) => p.to_lowercase(),
    };

    let mut start = 0;
    while let Some(pos) = lower_text[start..].find(&search_str) {
        let absolute_pos = start + pos;
        matches.push((absolute_pos, search_str.len()));
        start = absolute_pos + 1;
    }

    matches
}

/// Snap a byte offset to a valid UTF-8 char boundary (round down).
fn snap_to_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut p = pos;
    while !s.is_char_boundary(p) && p > 0 {
        p -= 1;
    }
    p
}
```

**Tests:**

1. Empty body → plain empty snippet
2. No search terms → first 200 chars of body
3. Single word match at start of body → highlighted, no leading ellipsis
4. Single word match in middle of body → context around match, leading + trailing ellipsis
5. Multiple matches of same word → all highlighted within window
6. Phrase match → entire phrase highlighted as one segment
7. No match found → first 200 chars as plain text
8. Case-insensitive match: searching "Hello" matches "hello" in body
9. Body shorter than snippet max → no ellipsis
10. Overlapping match ranges merged correctly
11. UTF-8 content: snippet boundaries don't split multi-byte chars
12. Match at very end of body → trailing context, no trailing ellipsis

**Commit:** `feat(store): implement search result snippet generator with highlighting`

---

### Task 7 — Search executor (tantivy + SQLite filtering)

**File:** `inboxly-store/src/search/executor.rs` (new)

The executor orchestrates search across both tantivy and SQLite. For queries with tantivy components, it runs the tantivy query first, then optionally filters results through SQLite for state-based filters (`is:pinned`, `is:unread`). For state-only queries, it queries SQLite directly and returns results ordered by date.

**Functions:**

```rust
use tantivy::{Index, IndexReader, Searcher};
use rusqlite::Connection;
use super::types::*;
use super::scoring::recency_boosted_collector;

/// Execute a search query and return ranked results.
///
/// Execution strategy:
/// 1. If query has tantivy components:
///    a. Build tantivy query (Task 4)
///    b. Search with recency-boosted collector (Task 5)
///    c. Extract thread_ids from tantivy results
///    d. If query also has state filters, filter thread_ids via SQLite
///    e. Load thread metadata from SQLite for surviving results
///    f. Generate snippets for each result
///
/// 2. If query has ONLY state filters (no tantivy components):
///    a. Query SQLite directly for matching threads
///    b. Return results ordered by newest_date DESC
///
/// Returns results capped at MAX_SEARCH_RESULTS (50).
pub fn execute_search(
    query: &SearchQuery,
    reader: &IndexReader,
    index: &Index,
    db: &Connection,
    account_id: &str,
) -> Result<SearchResults, StoreError>;
```

**Sub-functions:**

```rust
/// Execute tantivy search and return (thread_id, score) pairs.
/// Deduplicates by thread_id, keeping the highest score per thread
/// (since multiple emails in a thread may match).
fn search_tantivy(
    query: &SearchQuery,
    reader: &IndexReader,
    index: &Index,
) -> Result<Vec<(String, f32)>, StoreError>;

/// Filter a list of thread_ids by state filters using SQLite.
///
/// is:pinned → SELECT thread_id FROM thread_state WHERE pinned = 1 AND thread_id IN (...)
/// is:unread → SELECT DISTINCT thread_id FROM emails WHERE (flags & 1) = 0 AND thread_id IN (...)
///
/// Returns the intersection (thread_ids that pass ALL state filters).
fn filter_by_state(
    thread_ids: &[(String, f32)],
    state_filters: &[StateFilter],
    db: &Connection,
) -> Result<Vec<(String, f32)>, StoreError>;

/// Query SQLite for threads matching state-only filters.
/// Used when there are no tantivy components.
///
/// is:pinned → threads joined with thread_state WHERE pinned = 1
/// is:unread → threads with at least one unread email
///
/// Returns thread_ids ordered by newest_date DESC, capped at limit.
fn query_state_only(
    state_filters: &[StateFilter],
    db: &Connection,
    account_id: &str,
    limit: usize,
) -> Result<Vec<String>, StoreError>;

/// Load full SearchResult metadata for a list of thread_ids from SQLite.
/// Preserves the ordering of the input thread_ids (which are ranked by score).
fn load_result_metadata(
    thread_ids_with_scores: &[(String, f32)],
    db: &Connection,
) -> Result<Vec<SearchResult>, StoreError>;

/// Load the body text for an email (from Maildir via the email's maildir_path).
/// Used for snippet generation.
fn load_body_for_snippet(
    db: &Connection,
    thread_id: &str,
) -> Result<Option<String>, StoreError>;
```

**Tantivy search detail (search_tantivy):**

```rust
fn search_tantivy(
    query: &SearchQuery,
    reader: &IndexReader,
    index: &Index,
) -> Result<Vec<(String, f32)>, StoreError> {
    let schema = index.schema();
    let tantivy_query = build_tantivy_query(query, &schema)
        .ok_or_else(|| StoreError::Search("No tantivy-searchable components".into()))?;

    let searcher = reader.searcher();
    let collector = recency_boosted_collector(&schema, MAX_SEARCH_RESULTS * 3);
    // Over-fetch 3x to account for deduplication by thread_id

    let top_docs = searcher.search(&*tantivy_query, &collector)?;

    let thread_id_field = schema.get_field("thread_id").unwrap();

    // Deduplicate by thread_id, keeping highest score per thread
    let mut thread_scores: HashMap<String, f32> = HashMap::new();

    for (score, doc_address) in top_docs {
        let doc = searcher.doc(doc_address)?;
        if let Some(thread_id_val) = doc.get_first(thread_id_field) {
            let thread_id = thread_id_val.as_str()
                .unwrap_or_default()
                .to_string();
            let entry = thread_scores.entry(thread_id).or_insert(0.0);
            *entry = entry.max(score);
        }
    }

    // Sort by score descending
    let mut results: Vec<(String, f32)> = thread_scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(MAX_SEARCH_RESULTS);

    Ok(results)
}
```

**SQLite state filtering:**

```rust
fn filter_by_state(
    thread_ids: &[(String, f32)],
    state_filters: &[StateFilter],
    db: &Connection,
) -> Result<Vec<(String, f32)>, StoreError> {
    if state_filters.is_empty() || thread_ids.is_empty() {
        return Ok(thread_ids.to_vec());
    }

    let mut surviving_ids: HashSet<String> = thread_ids
        .iter()
        .map(|(id, _)| id.clone())
        .collect();

    for filter in state_filters {
        match filter {
            StateFilter::Pinned => {
                // Get pinned thread_ids from thread_state
                let pinned: HashSet<String> = {
                    let mut stmt = db.prepare(
                        "SELECT thread_id FROM thread_state WHERE pinned = 1"
                    )?;
                    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                    rows.filter_map(|r| r.ok()).collect()
                };
                surviving_ids.retain(|id| pinned.contains(id));
            }
            StateFilter::Unread => {
                // Get thread_ids that have at least one unread email
                let unread_threads: HashSet<String> = {
                    let mut stmt = db.prepare(
                        "SELECT DISTINCT thread_id FROM emails WHERE (flags & 1) = 0"
                    )?;
                    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                    rows.filter_map(|r| r.ok()).collect()
                };
                surviving_ids.retain(|id| unread_threads.contains(id));
            }
        }
    }

    // Preserve original ordering, filter to surviving
    Ok(thread_ids
        .iter()
        .filter(|(id, _)| surviving_ids.contains(id))
        .cloned()
        .collect())
}
```

**Implementation notes:**
- For large result sets, the SQLite filtering queries could be optimized with `WHERE thread_id IN (?, ?, ...)` parameterised placeholders. For v1, loading all pinned/unread thread_ids and intersecting in memory is simpler and fast enough for typical mailbox sizes (<100k threads).
- The body text for snippet generation should be loaded lazily — only load bodies for the final result set (after filtering), not for all tantivy hits.
- Thread metadata (subject, from, date, has_unread, has_attachments) is loaded from the `threads` table (populated by M10's metadata aggregation).

**Tests:**

1. Tantivy-only query: returns ranked results with scores
2. State-only query (`is:pinned`): returns pinned threads from SQLite
3. Mixed query (`from:sarah is:pinned`): tantivy results filtered by SQLite
4. No results: returns empty SearchResults
5. Deduplication: two emails in same thread match → one result
6. Score ordering: higher-scored thread appears first
7. Result cap: more than 50 matching threads → only 50 returned
8. State filter removes all tantivy results → empty results
9. `is:unread` filter: only threads with at least one unread email survive
10. `is:pinned` + `is:unread` combined: both filters applied (intersection)

**Commit:** `feat(store): implement search executor with tantivy + SQLite hybrid filtering`

---

### Task 8 — Public search API on Store

**File:** `inboxly-store/src/search/mod.rs` (new module root) + modify `inboxly-store/src/lib.rs`

Wire up the search module and expose a clean public API. The API takes raw query text and returns `SearchResults` — all parsing, query building, and execution are internal.

**Module structure:**

```
inboxly-store/src/search/
├── mod.rs              ← public API, module declarations
├── types.rs            ← Task 1
├── tokeniser.rs        ← Task 2
├── parser.rs           ← Task 3
├── query_builder.rs    ← Task 4
├── scoring.rs          ← Task 5
├── snippets.rs         ← Task 6
└── executor.rs         ← Task 7
```

**Public API** (`mod.rs`):

```rust
mod types;
mod tokeniser;
mod parser;
mod query_builder;
mod scoring;
mod snippets;
mod executor;

pub use types::*;

use tantivy::IndexReader;
use tantivy::Index;
use rusqlite::Connection;

/// Parse a raw search query string and execute it.
///
/// This is the main entry point for search. It:
/// 1. Tokenises the raw input
/// 2. Parses tokens into a SearchQuery AST
/// 3. Builds a tantivy query (if applicable)
/// 4. Executes against tantivy + SQLite
/// 5. Generates snippets for matching results
/// 6. Returns ranked SearchResults
///
/// Returns an empty SearchResults for empty or whitespace-only input.
pub fn search(
    raw_query: &str,
    reader: &IndexReader,
    index: &Index,
    db: &Connection,
    account_id: &str,
) -> Result<SearchResults, StoreError> {
    let start = std::time::Instant::now();

    let trimmed = raw_query.trim();
    if trimmed.is_empty() {
        return Ok(SearchResults {
            results: vec![],
            total_hits: 0,
            query_text: String::new(),
            elapsed_ms: 0,
        });
    }

    let tokens = tokeniser::tokenise(trimmed);
    let query = parser::parse(&tokens);

    if query.is_empty() {
        return Ok(SearchResults {
            results: vec![],
            total_hits: 0,
            query_text: raw_query.to_string(),
            elapsed_ms: 0,
        });
    }

    let results = executor::execute_search(&query, reader, index, db, account_id)?;

    Ok(SearchResults {
        total_hits: results.results.len(),
        query_text: raw_query.to_string(),
        elapsed_ms: start.elapsed().as_millis() as u64,
        results: results.results,
    })
}

/// Parse a raw query string without executing — for UI query validation
/// and syntax highlighting.
pub fn parse_query(raw_query: &str) -> SearchQuery {
    let tokens = tokeniser::tokenise(raw_query.trim());
    parser::parse(&tokens)
}
```

**Modify `inboxly-store/src/lib.rs`** to declare the search module:

```rust
pub mod search;
```

**Also add tantivy to `inboxly-store/Cargo.toml`** if not already present from M5:

```toml
# Should already be present from M5; verify:
tantivy = "0.22"
```

**Tests:**

1. `search("")` → empty results
2. `search("  ")` → empty results
3. `parse_query("from:sarah meeting")` → correct AST
4. `parse_query("")` → empty SearchQuery

**Commit:** `feat(store): expose public search API with parse + execute pipeline`

---

### Task 9 — SearchBar widget (Iced)

**File:** `inboxly-ui/src/widgets/search_bar.rs` (new)

Build the SearchBar widget per the spec: lives in the toolbar, expands on click/focus, text input with query syntax, closes on Escape or clear button.

**Dependencies:** This task depends on M15 (Iced shell with toolbar). The toolbar has a search icon/placeholder that this widget replaces.

**Types:**

```rust
use iced::widget::{text_input, button, row, container, text};
use iced::{Element, Length, Padding};

/// Messages emitted by the SearchBar widget.
#[derive(Debug, Clone)]
pub enum SearchBarMessage {
    /// User clicked the search icon or focused the search bar.
    Activate,
    /// User typed in the search input.
    InputChanged(String),
    /// User pressed Enter to submit the search.
    Submit,
    /// User clicked the clear/close button or pressed Escape.
    Clear,
}

/// State for the SearchBar widget.
#[derive(Debug, Clone)]
pub struct SearchBarState {
    /// Whether the search bar is expanded (active) or collapsed (icon only).
    pub active: bool,
    /// Current text in the search input.
    pub query_text: String,
    /// ID for the text input widget (for focus management).
    pub input_id: text_input::Id,
}
```

**Implementation:**

```rust
impl SearchBarState {
    pub fn new() -> Self {
        Self {
            active: false,
            query_text: String::new(),
            input_id: text_input::Id::unique(),
        }
    }

    /// Update state in response to a SearchBarMessage.
    /// Returns an optional Command (e.g., focus the text input on activate).
    pub fn update(&mut self, message: SearchBarMessage) -> iced::Command<SearchBarMessage> {
        match message {
            SearchBarMessage::Activate => {
                self.active = true;
                text_input::focus(self.input_id.clone())
            }
            SearchBarMessage::InputChanged(text) => {
                self.query_text = text;
                iced::Command::none()
            }
            SearchBarMessage::Submit => {
                // No state change — the parent handles executing the search
                iced::Command::none()
            }
            SearchBarMessage::Clear => {
                self.active = false;
                self.query_text.clear();
                iced::Command::none()
            }
        }
    }

    /// Render the search bar.
    ///
    /// When inactive: shows a search icon button in the toolbar.
    /// When active: shows a full-width text input with a clear button.
    pub fn view(&self) -> Element<'_, SearchBarMessage> {
        if !self.active {
            // Collapsed: search icon button
            button(text("\u{1F50D}").size(20))  // magnifying glass (or use an icon font)
                .on_press(SearchBarMessage::Activate)
                .padding(8)
                .into()
        } else {
            // Expanded: text input + clear button
            let input = text_input("Search mail...", &self.query_text)
                .id(self.input_id.clone())
                .on_input(SearchBarMessage::InputChanged)
                .on_submit(SearchBarMessage::Submit)
                .padding(Padding::from([8, 12]))
                .size(16)
                .width(Length::Fill);

            let clear_btn = button(text("X").size(14))
                .on_press(SearchBarMessage::Clear)
                .padding(8);

            row![input, clear_btn]
                .spacing(4)
                .align_y(iced::Alignment::Center)
                .width(Length::Fill)
                .into()
        }
    }
}
```

**Keyboard handling:**
- `Escape` key when search is active → emit `Clear` message
- This needs to be handled in the parent application's `subscription` or keyboard event handler (Task 14), not in the widget itself, since Iced's `text_input` doesn't expose raw key events directly. The parent intercepts `Escape` when `search_bar.active == true` and dispatches `SearchBarMessage::Clear`.

**Layout notes:**
- When inactive, the search icon sits at the right end of the toolbar (per spec: toolbar height 56dp)
- When active, the text input expands to fill available toolbar width
- The transition from icon to expanded input can be animated (future polish), but for this milestone, an instant switch is sufficient
- Use the theme's toolbar text colour for the input text, and a slightly transparent version for placeholder text

**Tests:** Widget state tests (not rendering — Iced widgets are tested by state transitions):

1. `new()` → inactive, empty query
2. `Activate` → active = true
3. `InputChanged("hello")` → query_text = "hello"
4. `Clear` → active = false, query_text = ""
5. `Submit` → no state change (parent handles)
6. `Activate` returns focus command

**Commit:** `feat(ui): implement SearchBar widget with expand/collapse and text input`

---

### Task 10 — Search results view

**File:** `inboxly-ui/src/views/search_results.rs` (new)

Build the view that replaces the inbox feed when a search is active. Shows matching threads with highlighted snippets, scores, and standard metadata (from, subject, date).

**Types:**

```rust
use iced::widget::{column, container, row, scrollable, text, Space};
use iced::{Element, Length, Padding, Color};
use inboxly_store::search::{SearchResults, SearchResult, HighlightedSnippet, SnippetSegment};

/// Messages emitted by the search results view.
#[derive(Debug, Clone)]
pub enum SearchResultsMessage {
    /// User clicked on a search result to open the thread.
    OpenThread(String),  // thread_id
    /// User wants to return to the inbox (via back button or clear).
    ReturnToInbox,
}

/// State for the search results view.
#[derive(Debug, Clone)]
pub struct SearchResultsState {
    /// Current search results to display.
    pub results: Option<SearchResults>,
    /// Whether a search is in progress (loading spinner).
    pub loading: bool,
}
```

**Implementation:**

```rust
impl SearchResultsState {
    pub fn new() -> Self {
        Self {
            results: None,
            loading: false,
        }
    }

    /// Render the search results view.
    ///
    /// Shows:
    /// - Header: "N results for 'query'" with elapsed time
    /// - Scrollable list of result rows
    /// - Each row: from, subject, date, highlighted snippet
    /// - "No results found" message if empty
    pub fn view(&self) -> Element<'_, SearchResultsMessage> {
        let content = if self.loading {
            column![text("Searching...").size(16)].into()
        } else if let Some(ref results) = self.results {
            self.render_results(results)
        } else {
            column![text("Type to search").size(16)].into()
        };

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(16)
            .into()
    }

    fn render_results(&self, results: &SearchResults) -> Element<'_, SearchResultsMessage> {
        let header = text(format!(
            "{} result{} for '{}' ({} ms)",
            results.total_hits,
            if results.total_hits == 1 { "" } else { "s" },
            results.query_text,
            results.elapsed_ms,
        ))
        .size(14);

        if results.results.is_empty() {
            return column![
                header,
                Space::with_height(24),
                text("No results found.").size(16),
                Space::with_height(8),
                text("Try different search terms or check your spelling.").size(14),
            ]
            .spacing(4)
            .into();
        }

        let result_rows: Vec<Element<'_, SearchResultsMessage>> = results
            .results
            .iter()
            .map(|r| self.render_result_row(r))
            .collect();

        let list = column(result_rows).spacing(2);

        let scrollable_list = scrollable(list)
            .height(Length::Fill);

        column![header, Space::with_height(12), scrollable_list]
            .spacing(4)
            .width(Length::Fill)
            .into()
    }

    fn render_result_row(&self, result: &SearchResult) -> Element<'_, SearchResultsMessage> {
        // From + date row
        let from_text = text(&result.from)
            .size(14);
        let date_text = text(format_date(&result.date))
            .size(12);
        let top_row = row![from_text, Space::with_width(Length::Fill), date_text]
            .align_y(iced::Alignment::Center);

        // Subject
        let subject_weight = if result.has_unread {
            iced::font::Weight::Bold
        } else {
            iced::font::Weight::Normal
        };
        let subject_text = text(&result.subject)
            .size(16)
            .font(iced::Font {
                weight: subject_weight,
                ..Default::default()
            });

        // Snippet with highlighting
        let snippet_element = render_snippet(&result.snippet);

        // Attachment indicator
        let attachment_row = if result.has_attachments {
            row![text("\u{1F4CE}").size(12), Space::with_width(4)] // paperclip emoji or icon
        } else {
            row![]
        };

        let card = container(
            column![top_row, subject_text, snippet_element, attachment_row]
                .spacing(4)
                .padding(Padding::from([12, 16]))
        )
        .width(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(Color::WHITE)),
            border: iced::Border {
                radius: 0.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            ..Default::default()
        });

        // Make the entire card clickable
        iced::widget::mouse_area(card)
            .on_press(SearchResultsMessage::OpenThread(result.thread_id.to_string()))
            .into()
    }
}

/// Render a HighlightedSnippet as an Iced Element with rich text.
///
/// Plain segments use secondary text colour.
/// Highlight segments use primary text colour with bold weight.
fn render_snippet<'a>(snippet: &HighlightedSnippet) -> Element<'a, SearchResultsMessage> {
    // Use iced::widget::rich_text if available in the Iced version,
    // otherwise fall back to a row of text widgets.
    //
    // Approach: Use rich_text with spans for highlighting.
    // Each segment maps to a Span with appropriate styling.
    use iced::widget::rich_text;

    let spans: Vec<iced::widget::text::Span<'a>> = snippet
        .segments
        .iter()
        .map(|seg| match seg {
            SnippetSegment::Plain(t) => {
                iced::widget::text::Span::new(t.clone())
                    .size(14)
                    .color(Color::from_rgb(0.46, 0.46, 0.46)) // secondary text
            }
            SnippetSegment::Highlight(t) => {
                iced::widget::text::Span::new(t.clone())
                    .size(14)
                    .color(Color::from_rgb(0.13, 0.13, 0.13)) // primary text
                    .font(iced::Font {
                        weight: iced::font::Weight::Bold,
                        ..Default::default()
                    })
            }
        })
        .collect();

    rich_text(spans).into()
}

/// Format a DateTime for display in search results.
/// Today's dates show time only, older dates show month + day.
fn format_date(date: &DateTime<Utc>) -> String {
    let now = Utc::now();
    if date.date_naive() == now.date_naive() {
        date.format("%l:%M %p").to_string().trim().to_string()
    } else if date.year() == now.year() {
        date.format("%b %e").to_string().trim().to_string()
    } else {
        date.format("%b %e, %Y").to_string().trim().to_string()
    }
}
```

**Implementation notes:**
- The `rich_text` widget is available in Iced 0.13+. If the project uses an older version, fall back to concatenating text into a single `text()` widget (losing inline highlighting) or use a `Row` of `text()` widgets.
- The result row styling should use theme tokens from M16 (BigTop theme). For now, use hardcoded colours that match the spec's light theme values. These can be swapped for theme references when M16 is integrated.
- The `mouse_area` widget makes the entire card clickable. An alternative is to use `button` with custom styling, but `mouse_area` avoids the button's default padding/border.
- The card uses 0dp corner radius per the spec's "List item corner radius: 0dp (flat cards)".

**Tests:** State tests:

1. `new()` → results = None, loading = false
2. Setting results with 0 hits → "No results found" rendered
3. Setting results with 3 hits → 3 result rows rendered

**Commit:** `feat(ui): implement search results view with snippet highlighting`

---

### Task 11 — Search-as-you-type debounce

**File:** `inboxly-ui/src/search/debounce.rs` (new)

Implement a 300ms debounce mechanism for search-as-you-type. When the user types, we wait 300ms after the last keystroke before executing the search. This prevents firing a search on every character.

**Approach:** Use Iced's `subscription` with a timer-based approach. When input changes, record the timestamp and start a delayed command. If another input change arrives before the delay expires, cancel the previous one.

**Types:**

```rust
use std::time::{Duration, Instant};

/// Debounce timer for search-as-you-type.
///
/// Usage:
/// 1. On every InputChanged, call `debounce.trigger(query_text)`
/// 2. On every tick (or via subscription), call `debounce.check()`
/// 3. If check() returns Some(query), execute that search
#[derive(Debug, Clone)]
pub struct SearchDebounce {
    /// The pending query text (most recent input).
    pending_query: Option<String>,
    /// When the last input change occurred.
    last_input_at: Option<Instant>,
    /// Debounce delay.
    delay: Duration,
    /// The last query that was actually executed (to avoid re-executing identical queries).
    last_executed: Option<String>,
}

/// Messages for the debounce system.
#[derive(Debug, Clone)]
pub enum DebounceMessage {
    /// A debounce timer tick occurred — check if we should fire.
    Tick,
}
```

**Implementation:**

```rust
impl SearchDebounce {
    pub fn new() -> Self {
        Self {
            pending_query: None,
            last_input_at: None,
            delay: Duration::from_millis(300),
            last_executed: None,
        }
    }

    /// Record a new input change. Resets the debounce timer.
    pub fn trigger(&mut self, query: String) {
        self.pending_query = Some(query);
        self.last_input_at = Some(Instant::now());
    }

    /// Check if the debounce delay has elapsed since the last input.
    /// Returns Some(query) if a search should be executed, None otherwise.
    ///
    /// Also returns None if the pending query is identical to the last
    /// executed query (no-op detection).
    pub fn check(&mut self) -> Option<String> {
        let pending = self.pending_query.as_ref()?;
        let last_input = self.last_input_at?;

        if last_input.elapsed() < self.delay {
            return None; // Still within debounce window
        }

        // Debounce expired — check if query changed
        if self.last_executed.as_ref() == Some(pending) {
            // Same query already executed
            self.pending_query = None;
            self.last_input_at = None;
            return None;
        }

        let query = pending.clone();
        self.last_executed = Some(query.clone());
        self.pending_query = None;
        self.last_input_at = None;
        Some(query)
    }

    /// Reset the debounce state (e.g., when search is cleared).
    pub fn reset(&mut self) {
        self.pending_query = None;
        self.last_input_at = None;
        self.last_executed = None;
    }

    /// Whether there is a pending query waiting for debounce.
    pub fn is_pending(&self) -> bool {
        self.pending_query.is_some()
    }
}
```

**Iced subscription for debounce ticks:**

```rust
/// Create an Iced subscription that ticks every 100ms while search is active.
/// This drives the debounce check.
///
/// The subscription only runs while the search bar is active (to avoid
/// unnecessary ticking when not searching).
pub fn debounce_subscription(active: bool) -> iced::Subscription<DebounceMessage> {
    if active {
        iced::time::every(Duration::from_millis(100))
            .map(|_| DebounceMessage::Tick)
    } else {
        iced::Subscription::none()
    }
}
```

**Implementation notes:**
- The 100ms tick rate means the actual debounce resolution is 100ms (we might fire up to 100ms after the 300ms window expires). This is fine for UX — the user won't notice the difference between 300ms and 400ms.
- The `last_executed` check prevents re-executing the same query when the user types, pauses, then types the same character again. This is important because tantivy searches are not free.
- The debounce is bypassed when the user presses Enter (Submit) — that executes the search immediately regardless of the timer.

**Tests:**

1. `trigger("hello")` then immediate `check()` → None (within delay)
2. `trigger("hello")`, wait 300ms, `check()` → Some("hello")
3. `trigger("hel")`, `trigger("hello")`, wait 300ms, `check()` → Some("hello") (only latest)
4. Execute "hello", `trigger("hello")`, wait, `check()` → None (duplicate)
5. Execute "hello", `trigger("world")`, wait, `check()` → Some("world")
6. `reset()` clears all state
7. `is_pending()` returns true after trigger, false after check

**Commit:** `feat(ui): implement 300ms search debounce for search-as-you-type`

---

### Task 12 — Unit tests for parser + query builder

**File:** `inboxly-store/src/search/tests.rs` (new)

Comprehensive unit tests for the full parsing pipeline: raw text → tokens → AST → tantivy query. These tests validate the end-to-end parsing without executing against a real tantivy index.

**Test organisation:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use super::tokeniser::tokenise;
    use super::parser::parse;
    use super::types::*;
    use chrono::NaiveDate;

    // === Tokeniser round-trip tests ===

    #[test]
    fn test_parse_simple_text_query() {
        let results = parse(&tokenise("hello world"));
        assert_eq!(results.text_terms.len(), 2);
        assert_eq!(results.text_terms[0], TextTerm::Word("hello".into()));
        assert_eq!(results.text_terms[1], TextTerm::Word("world".into()));
        assert!(results.field_filters.is_empty());
        assert!(results.state_filters.is_empty());
    }

    #[test]
    fn test_parse_quoted_phrase() {
        let results = parse(&tokenise("\"hello world\""));
        assert_eq!(results.text_terms.len(), 1);
        assert_eq!(results.text_terms[0], TextTerm::Phrase("hello world".into()));
    }

    #[test]
    fn test_parse_from_filter() {
        let results = parse(&tokenise("from:sarah"));
        assert_eq!(results.field_filters.len(), 1);
        assert_eq!(results.field_filters[0], FieldFilter::From("sarah".into()));
    }

    #[test]
    fn test_parse_from_with_email() {
        let results = parse(&tokenise("from:sarah@example.com"));
        assert_eq!(results.field_filters[0], FieldFilter::From("sarah@example.com".into()));
    }

    #[test]
    fn test_parse_to_filter() {
        let results = parse(&tokenise("to:bob@example.com"));
        assert_eq!(results.field_filters[0], FieldFilter::To("bob@example.com".into()));
    }

    #[test]
    fn test_parse_subject_filter() {
        let results = parse(&tokenise("subject:lunch"));
        assert_eq!(results.field_filters[0], FieldFilter::Subject("lunch".into()));
    }

    #[test]
    fn test_parse_has_attachment() {
        let results = parse(&tokenise("has:attachment"));
        assert_eq!(results.field_filters[0], FieldFilter::HasAttachment);
    }

    #[test]
    fn test_parse_has_invalid() {
        let results = parse(&tokenise("has:something"));
        assert!(results.field_filters.is_empty());
        assert_eq!(results.text_terms.len(), 1); // degraded to text
    }

    #[test]
    fn test_parse_in_bundle() {
        let results = parse(&tokenise("in:purchases"));
        assert_eq!(results.field_filters[0], FieldFilter::InBundle("purchases".into()));
    }

    #[test]
    fn test_parse_date_after() {
        let results = parse(&tokenise("after:2026-01-15"));
        assert_eq!(
            results.field_filters[0],
            FieldFilter::After(NaiveDate::from_ymd_opt(2026, 1, 15).unwrap())
        );
    }

    #[test]
    fn test_parse_date_before() {
        let results = parse(&tokenise("before:2026-03-01"));
        assert_eq!(
            results.field_filters[0],
            FieldFilter::Before(NaiveDate::from_ymd_opt(2026, 3, 1).unwrap())
        );
    }

    #[test]
    fn test_parse_invalid_date() {
        let results = parse(&tokenise("after:not-a-date"));
        assert!(results.field_filters.is_empty());
        assert_eq!(results.text_terms.len(), 1); // degraded
    }

    #[test]
    fn test_parse_is_pinned() {
        let results = parse(&tokenise("is:pinned"));
        assert_eq!(results.state_filters.len(), 1);
        assert_eq!(results.state_filters[0], StateFilter::Pinned);
    }

    #[test]
    fn test_parse_is_unread() {
        let results = parse(&tokenise("is:unread"));
        assert_eq!(results.state_filters.len(), 1);
        assert_eq!(results.state_filters[0], StateFilter::Unread);
    }

    #[test]
    fn test_parse_is_invalid() {
        let results = parse(&tokenise("is:starred"));
        assert!(results.state_filters.is_empty());
        assert_eq!(results.text_terms.len(), 1); // degraded
    }

    #[test]
    fn test_parse_complex_mixed_query() {
        // from:sarah meeting "lunch plans" is:unread after:2026-01-01 has:attachment
        let input = "from:sarah meeting \"lunch plans\" is:unread after:2026-01-01 has:attachment";
        let results = parse(&tokenise(input));

        assert_eq!(results.text_terms.len(), 2); // "meeting" + "lunch plans"
        assert_eq!(results.field_filters.len(), 3); // from + after + has
        assert_eq!(results.state_filters.len(), 1); // is:unread

        assert!(matches!(results.text_terms[0], TextTerm::Word(ref w) if w == "meeting"));
        assert!(matches!(results.text_terms[1], TextTerm::Phrase(ref p) if p == "lunch plans"));
        assert!(matches!(results.field_filters[0], FieldFilter::From(ref f) if f == "sarah"));
        assert_eq!(results.field_filters[1], FieldFilter::After(
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
        ));
        assert_eq!(results.field_filters[2], FieldFilter::HasAttachment);
        assert_eq!(results.state_filters[0], StateFilter::Unread);
    }

    #[test]
    fn test_parse_preserves_has_tantivy_components() {
        // State-only query
        let results = parse(&tokenise("is:pinned"));
        assert!(!results.has_tantivy_components());
        assert!(results.has_state_filters());

        // Text-only query
        let results2 = parse(&tokenise("hello"));
        assert!(results2.has_tantivy_components());
        assert!(!results2.has_state_filters());

        // Mixed
        let results3 = parse(&tokenise("hello is:pinned"));
        assert!(results3.has_tantivy_components());
        assert!(results3.has_state_filters());
    }

    #[test]
    fn test_parse_empty_input() {
        let results = parse(&tokenise(""));
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_whitespace_only() {
        let results = parse(&tokenise("   \t  "));
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_case_insensitive_field_names() {
        let r1 = parse(&tokenise("FROM:sarah"));
        let r2 = parse(&tokenise("from:sarah"));
        assert_eq!(r1.field_filters, r2.field_filters);
    }

    #[test]
    fn test_parse_case_insensitive_is_values() {
        let r1 = parse(&tokenise("is:Pinned"));
        let r2 = parse(&tokenise("is:pinned"));
        assert_eq!(r1.state_filters, r2.state_filters);
    }

    #[test]
    fn test_parse_multiple_same_field() {
        // from:alice from:bob — both should be present
        let results = parse(&tokenise("from:alice from:bob"));
        assert_eq!(results.field_filters.len(), 2);
    }

    #[test]
    fn test_parse_from_with_quoted_name() {
        let results = parse(&tokenise("from:\"John Smith\""));
        assert_eq!(results.field_filters[0], FieldFilter::From("john smith".into()));
    }

    // === Query builder tests (require tantivy schema) ===

    fn test_schema() -> tantivy::schema::Schema {
        let mut builder = tantivy::schema::Schema::builder();
        builder.add_text_field("from", tantivy::schema::TEXT | tantivy::schema::STORED);
        builder.add_text_field("to", tantivy::schema::TEXT | tantivy::schema::STORED);
        builder.add_text_field("subject", tantivy::schema::TEXT | tantivy::schema::STORED);
        builder.add_text_field("body_text", tantivy::schema::TEXT);
        builder.add_i64_field("date", tantivy::schema::INDEXED | tantivy::schema::FAST);
        builder.add_text_field("thread_id", tantivy::schema::STRING | tantivy::schema::STORED);
        builder.add_text_field("email_id", tantivy::schema::STRING | tantivy::schema::STORED);
        builder.add_u64_field("has_attachment", tantivy::schema::INDEXED);
        builder.add_facet_field("bundle_category", tantivy::schema::FacetOptions::default());
        builder.build()
    }

    #[test]
    fn test_build_empty_query() {
        let schema = test_schema();
        let query = SearchQuery {
            text_terms: vec![],
            field_filters: vec![],
            state_filters: vec![StateFilter::Pinned], // state only
        };
        assert!(build_tantivy_query(&query, &schema).is_none());
    }

    #[test]
    fn test_build_single_word_query() {
        let schema = test_schema();
        let query = SearchQuery {
            text_terms: vec![TextTerm::Word("meeting".into())],
            field_filters: vec![],
            state_filters: vec![],
        };
        let tantivy_q = build_tantivy_query(&query, &schema);
        assert!(tantivy_q.is_some());
    }

    #[test]
    fn test_build_field_plus_text_query() {
        let schema = test_schema();
        let query = SearchQuery {
            text_terms: vec![TextTerm::Word("urgent".into())],
            field_filters: vec![FieldFilter::From("sarah".into())],
            state_filters: vec![],
        };
        let tantivy_q = build_tantivy_query(&query, &schema);
        assert!(tantivy_q.is_some());
    }
}
```

**Commit:** `test(store): add comprehensive unit tests for search parser and query builder`

---

### Task 13 — Integration tests (end-to-end search)

**File:** `inboxly-store/tests/search_integration.rs` (new, integration test)

End-to-end tests that create a real tantivy index and SQLite database, index emails, and verify search results.

**Setup helper:**

```rust
use tempfile::TempDir;
use tantivy::{Index, IndexWriter, Document};
use rusqlite::Connection;

/// Create a test environment with tantivy index and SQLite database.
struct TestSearchEnv {
    index: Index,
    reader: tantivy::IndexReader,
    db: Connection,
    _dir: TempDir,
}

impl TestSearchEnv {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();

        // Create tantivy index with the schema from M5
        let schema = build_test_schema();
        let index = Index::create_in_dir(dir.path().join("index"), schema.clone()).unwrap();
        let reader = index.reader().unwrap();

        // Create SQLite database with schema from M3
        let db = Connection::open_in_memory().unwrap();
        init_test_schema(&db);

        Self { index, reader, db, _dir: dir }
    }

    /// Index an email in tantivy and insert metadata in SQLite.
    fn add_email(&mut self, email: TestEmail) {
        // Add to tantivy
        let schema = self.index.schema();
        let mut writer = self.index.writer(50_000_000).unwrap();
        let mut doc = Document::new();
        doc.add_text(schema.get_field("from").unwrap(), &email.from);
        doc.add_text(schema.get_field("to").unwrap(), &email.to);
        doc.add_text(schema.get_field("subject").unwrap(), &email.subject);
        doc.add_text(schema.get_field("body_text").unwrap(), &email.body);
        doc.add_i64(schema.get_field("date").unwrap(), email.date_epoch);
        doc.add_text(schema.get_field("thread_id").unwrap(), &email.thread_id);
        doc.add_text(schema.get_field("email_id").unwrap(), &email.id);
        doc.add_u64(schema.get_field("has_attachment").unwrap(), if email.has_attachment { 1 } else { 0 });
        if let Some(ref cat) = email.bundle_category {
            doc.add_facet(schema.get_field("bundle_category").unwrap(), tantivy::schema::Facet::from_path(&[cat]));
        }
        writer.add_document(doc).unwrap();
        writer.commit().unwrap();
        self.reader.reload().unwrap();

        // Add to SQLite
        self.db.execute(
            "INSERT INTO emails (id, account_id, thread_id, from_address, to_json, subject, snippet, date, flags, has_attachments, message_id_header)
             VALUES (?1, 'acct1', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                email.id, email.thread_id, email.from, email.to,
                email.subject, &email.body[..email.body.len().min(200)],
                email.date_epoch, email.flags, email.has_attachment, email.id,
            ],
        ).unwrap();

        // Add thread if not exists
        self.db.execute(
            "INSERT OR IGNORE INTO threads (id, account_id, subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet)
             VALUES (?1, 'acct1', ?2, ?3, ?3, 1, ?4, ?5, ?6)",
            rusqlite::params![
                email.thread_id, email.subject, email.date_epoch,
                if email.flags & 1 == 0 { 1 } else { 0 },
                email.has_attachment,
                &email.body[..email.body.len().min(200)],
            ],
        ).unwrap();
    }

    fn pin_thread(&self, thread_id: &str) {
        self.db.execute(
            "INSERT OR REPLACE INTO thread_state (thread_id, pinned, done) VALUES (?1, 1, 0)",
            [thread_id],
        ).unwrap();
    }

    fn search(&self, query: &str) -> SearchResults {
        inboxly_store::search::search(query, &self.reader, &self.index, &self.db, "acct1").unwrap()
    }
}
```

**Test cases:**

```rust
#[test]
fn test_search_bare_word() {
    let mut env = TestSearchEnv::new();
    env.add_email(TestEmail {
        id: "1", thread_id: "t1", from: "sarah@example.com", to: "me@example.com",
        subject: "Lunch meeting tomorrow", body: "Let's meet for lunch at noon.",
        date_epoch: 1709200000, flags: 0, has_attachment: false, bundle_category: None,
    });
    env.add_email(TestEmail {
        id: "2", thread_id: "t2", from: "bob@example.com", to: "me@example.com",
        subject: "Project update", body: "Here is the latest project status.",
        date_epoch: 1709100000, flags: 1, has_attachment: false, bundle_category: None,
    });

    let results = env.search("lunch");
    assert_eq!(results.total_hits, 1);
    assert_eq!(results.results[0].thread_id.to_string(), "t1");
}

#[test]
fn test_search_from_filter() {
    let mut env = TestSearchEnv::new();
    env.add_email(TestEmail {
        id: "1", thread_id: "t1", from: "sarah@example.com", ..default_email()
    });
    env.add_email(TestEmail {
        id: "2", thread_id: "t2", from: "bob@example.com", ..default_email()
    });

    let results = env.search("from:sarah");
    assert_eq!(results.total_hits, 1);
}

#[test]
fn test_search_has_attachment() {
    let mut env = TestSearchEnv::new();
    env.add_email(TestEmail {
        id: "1", thread_id: "t1", has_attachment: true, ..default_email()
    });
    env.add_email(TestEmail {
        id: "2", thread_id: "t2", has_attachment: false, ..default_email()
    });

    let results = env.search("has:attachment");
    assert_eq!(results.total_hits, 1);
}

#[test]
fn test_search_date_range() {
    let mut env = TestSearchEnv::new();
    env.add_email(TestEmail {
        id: "1", thread_id: "t1", date_epoch: 1704067200, // 2024-01-01
        ..default_email()
    });
    env.add_email(TestEmail {
        id: "2", thread_id: "t2", date_epoch: 1709200000, // 2024-02-29
        ..default_email()
    });

    let results = env.search("after:2024-02-01");
    assert_eq!(results.total_hits, 1);
}

#[test]
fn test_search_is_pinned() {
    let mut env = TestSearchEnv::new();
    env.add_email(TestEmail {
        id: "1", thread_id: "t1", subject: "Important", body: "Very important email",
        ..default_email()
    });
    env.add_email(TestEmail {
        id: "2", thread_id: "t2", subject: "Regular", body: "Normal email",
        ..default_email()
    });
    env.pin_thread("t1");

    // State-only query
    let results = env.search("is:pinned");
    assert_eq!(results.total_hits, 1);
    assert_eq!(results.results[0].thread_id.to_string(), "t1");
}

#[test]
fn test_search_mixed_tantivy_and_state() {
    let mut env = TestSearchEnv::new();
    env.add_email(TestEmail {
        id: "1", thread_id: "t1", subject: "Important meeting",
        body: "Discuss budget", ..default_email()
    });
    env.add_email(TestEmail {
        id: "2", thread_id: "t2", subject: "Casual meeting",
        body: "Coffee chat", ..default_email()
    });
    env.pin_thread("t1");

    // Mixed: tantivy text search + SQLite state filter
    let results = env.search("meeting is:pinned");
    assert_eq!(results.total_hits, 1);
    assert_eq!(results.results[0].thread_id.to_string(), "t1");
}

#[test]
fn test_search_in_bundle() {
    let mut env = TestSearchEnv::new();
    env.add_email(TestEmail {
        id: "1", thread_id: "t1", bundle_category: Some("purchases"),
        subject: "Order confirmation", ..default_email()
    });
    env.add_email(TestEmail {
        id: "2", thread_id: "t2", bundle_category: Some("social"),
        subject: "New follower", ..default_email()
    });

    let results = env.search("in:purchases");
    assert_eq!(results.total_hits, 1);
}

#[test]
fn test_search_no_results() {
    let mut env = TestSearchEnv::new();
    env.add_email(TestEmail {
        id: "1", thread_id: "t1", subject: "Hello", body: "World",
        ..default_email()
    });

    let results = env.search("nonexistent");
    assert_eq!(results.total_hits, 0);
    assert!(results.results.is_empty());
}

#[test]
fn test_search_empty_query() {
    let env = TestSearchEnv::new();
    let results = env.search("");
    assert_eq!(results.total_hits, 0);
}

#[test]
fn test_search_recency_boost() {
    let mut env = TestSearchEnv::new();
    // Old email with "meeting" in body
    env.add_email(TestEmail {
        id: "1", thread_id: "t1", subject: "Old meeting",
        body: "meeting discussion from long ago",
        date_epoch: 1609459200, // 2021-01-01
        ..default_email()
    });
    // Recent email with "meeting" in body
    env.add_email(TestEmail {
        id: "2", thread_id: "t2", subject: "New meeting",
        body: "meeting discussion today",
        date_epoch: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64,
        ..default_email()
    });

    let results = env.search("meeting");
    assert_eq!(results.total_hits, 2);
    // Recent email should rank higher due to recency boost
    assert_eq!(results.results[0].thread_id.to_string(), "t2");
}

#[test]
fn test_search_phrase_query() {
    let mut env = TestSearchEnv::new();
    env.add_email(TestEmail {
        id: "1", thread_id: "t1", subject: "Lunch meeting",
        body: "Let's have a lunch meeting tomorrow at noon.",
        ..default_email()
    });
    env.add_email(TestEmail {
        id: "2", thread_id: "t2", subject: "Lunch and meeting",
        body: "I had lunch. Then I had a meeting.",
        ..default_email()
    });

    let results = env.search("\"lunch meeting\"");
    // Only the first email has "lunch meeting" as a contiguous phrase
    assert_eq!(results.total_hits, 1);
    assert_eq!(results.results[0].thread_id.to_string(), "t1");
}

#[test]
fn test_search_thread_deduplication() {
    let mut env = TestSearchEnv::new();
    // Two emails in the same thread, both matching "budget"
    env.add_email(TestEmail {
        id: "1", thread_id: "t1", subject: "Budget review",
        body: "Let's review the budget.", date_epoch: 1709100000,
        ..default_email()
    });
    env.add_email(TestEmail {
        id: "2", thread_id: "t1", subject: "Re: Budget review",
        body: "Budget looks good.", date_epoch: 1709200000,
        ..default_email()
    });

    let results = env.search("budget");
    // Should return 1 result (deduplicated by thread)
    assert_eq!(results.total_hits, 1);
    assert_eq!(results.results[0].thread_id.to_string(), "t1");
}

#[test]
fn test_search_complex_query() {
    // from:sarah subject:budget after:2024-01-01 has:attachment is:unread
    let mut env = TestSearchEnv::new();
    env.add_email(TestEmail {
        id: "1", thread_id: "t1", from: "sarah@example.com",
        subject: "Budget report Q1", body: "Attached is the budget report.",
        date_epoch: 1709200000, flags: 0, // unread
        has_attachment: true, bundle_category: None,
        ..default_email()
    });
    env.add_email(TestEmail {
        id: "2", thread_id: "t2", from: "sarah@example.com",
        subject: "Budget report Q2", body: "Budget info here.",
        date_epoch: 1709200000, flags: 1, // read
        has_attachment: true, bundle_category: None,
        ..default_email()
    });

    let results = env.search("from:sarah subject:budget has:attachment is:unread");
    assert_eq!(results.total_hits, 1);
    assert_eq!(results.results[0].thread_id.to_string(), "t1");
}
```

**Commit:** `test(store): add end-to-end search integration tests`

---

### Task 14 — UI integration (wire search into app state)

**File:** Modify `inboxly-ui/src/app.rs` (or wherever M15 placed the main application state)

Wire the SearchBar widget, debounce, and search results view into the main application. This task connects all the pieces: the search bar in the toolbar triggers searches via the debounce mechanism, results are displayed in the search results view (replacing the inbox feed), and clearing the search returns to the inbox.

**Changes to main app state:**

```rust
// Add to the main App struct:
use crate::widgets::search_bar::{SearchBarState, SearchBarMessage};
use crate::views::search_results::{SearchResultsState, SearchResultsMessage};
use crate::search::debounce::{SearchDebounce, DebounceMessage, debounce_subscription};

pub struct InboxlyApp {
    // ... existing fields from M15 ...

    /// Search bar state (toolbar widget).
    search_bar: SearchBarState,
    /// Search results view state.
    search_results: SearchResultsState,
    /// Search debounce timer.
    search_debounce: SearchDebounce,
}
```

**Add to the main Message enum:**

```rust
pub enum Message {
    // ... existing variants from M15 ...

    /// Search bar events.
    SearchBar(SearchBarMessage),
    /// Search results events.
    SearchResults(SearchResultsMessage),
    /// Debounce timer tick.
    SearchDebounce(DebounceMessage),
    /// Search completed (received results from background task).
    SearchCompleted(Result<SearchResults, String>),
}
```

**Update function additions:**

```rust
fn update(&mut self, message: Message) -> Command<Message> {
    match message {
        // ... existing handlers ...

        Message::SearchBar(msg) => {
            match &msg {
                SearchBarMessage::InputChanged(text) => {
                    // Trigger debounce
                    self.search_debounce.trigger(text.clone());
                }
                SearchBarMessage::Submit => {
                    // Immediate search (bypass debounce)
                    return self.execute_search(self.search_bar.query_text.clone());
                }
                SearchBarMessage::Clear => {
                    // Return to inbox
                    self.search_debounce.reset();
                    self.search_results = SearchResultsState::new();
                }
                _ => {}
            }
            self.search_bar.update(msg).map(Message::SearchBar)
        }

        Message::SearchResults(msg) => {
            match msg {
                SearchResultsMessage::OpenThread(thread_id) => {
                    // Navigate to conversation view (M17+ handles this)
                    Command::none()
                }
                SearchResultsMessage::ReturnToInbox => {
                    // Clear search and return to inbox
                    self.search_bar.update(SearchBarMessage::Clear);
                    self.search_debounce.reset();
                    self.search_results = SearchResultsState::new();
                    Command::none()
                }
            }
        }

        Message::SearchDebounce(DebounceMessage::Tick) => {
            if let Some(query) = self.search_debounce.check() {
                return self.execute_search(query);
            }
            Command::none()
        }

        Message::SearchCompleted(result) => {
            self.search_results.loading = false;
            match result {
                Ok(results) => {
                    self.search_results.results = Some(results);
                }
                Err(err) => {
                    // Log error, show empty results with error message
                    eprintln!("Search error: {}", err);
                }
            }
            Command::none()
        }
    }
}
```

**Search execution helper:**

```rust
impl InboxlyApp {
    /// Execute a search query in the background.
    fn execute_search(&mut self, query: String) -> Command<Message> {
        if query.trim().is_empty() {
            self.search_results = SearchResultsState::new();
            return Command::none();
        }

        self.search_results.loading = true;

        // Clone the reader/index/db handles for the background task.
        // These are all designed for concurrent read access:
        // - IndexReader is cloneable and thread-safe
        // - SQLite Connection needs to be from a shared pool or cloned path
        let reader = self.index_reader.clone();
        let index = self.index.clone();
        let db_path = self.db_path.clone();
        let account_id = self.current_account_id.clone();

        Command::perform(
            async move {
                // Open a read-only SQLite connection for the search
                let db = rusqlite::Connection::open_with_flags(
                    &db_path,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                ).map_err(|e| e.to_string())?;

                inboxly_store::search::search(&query, &reader, &index, &db, &account_id)
                    .map_err(|e| e.to_string())
            },
            Message::SearchCompleted,
        )
    }
}
```

**View integration:**

```rust
fn view(&self) -> Element<'_, Message> {
    // Toolbar with search bar
    let toolbar = row![
        // ... existing toolbar content ...
        self.search_bar.view().map(Message::SearchBar),
    ]
    .height(56)
    .padding(Padding::from([0, 16]));

    // Main content area — either inbox feed or search results
    let content = if self.search_bar.active && self.search_results.results.is_some() {
        self.search_results.view().map(Message::SearchResults)
    } else {
        self.render_inbox_feed() // existing inbox feed from M17
    };

    column![toolbar, content].into()
}
```

**Subscription integration:**

```rust
fn subscription(&self) -> Subscription<Message> {
    let mut subs = vec![
        // ... existing subscriptions ...
    ];

    // Add search debounce subscription
    subs.push(
        debounce_subscription(self.search_bar.active)
            .map(Message::SearchDebounce)
    );

    // Handle Escape key when search is active
    if self.search_bar.active {
        subs.push(
            iced::keyboard::on_key_press(|key, _modifiers| {
                if key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) {
                    Some(Message::SearchBar(SearchBarMessage::Clear))
                } else {
                    None
                }
            })
        );
    }

    Subscription::batch(subs)
}
```

**Implementation notes:**
- The search runs on a background task via `Command::perform` to avoid blocking the UI thread. Tantivy searches are fast (sub-millisecond for typical mailboxes) but SQLite queries and snippet generation can take a few milliseconds.
- The `IndexReader` from tantivy is designed to be cloned and shared across threads. Each `searcher()` call gets a consistent snapshot.
- SQLite concurrent reads require either WAL mode (recommended, likely set in M3) or opening a separate read-only connection. The approach above opens a read-only connection per search, which is safe and simple. For higher performance, a connection pool could be used (future optimization).
- The Escape key handler is registered only when the search bar is active, to avoid interfering with other keyboard shortcuts.

**Tests:** Application state tests:

1. Initial state: search bar inactive, no results
2. Activate search bar → search bar active, debounce subscription starts
3. Input change → debounce triggered
4. Clear → search bar inactive, results cleared, debounce reset
5. Search completed → results populated, loading = false
6. Submit → immediate search executed (loading = true)
7. Escape key when active → search cleared

**Commit:** `feat(ui): wire search bar, debounce, and results into main application`

---

## Module Structure

After all tasks, the search-related files:

```
inboxly-store/src/
├── search/
│   ├── mod.rs              ← Task 8: public API (search(), parse_query())
│   ├── types.rs            ← Task 1: SearchQuery, SearchResult, HighlightedSnippet
│   ├── tokeniser.rs        ← Task 2: raw text → Token vec
│   ├── parser.rs           ← Task 3: Token vec → SearchQuery AST
│   ├── query_builder.rs    ← Task 4: SearchQuery → tantivy Query
│   ├── scoring.rs          ← Task 5: BM25 + recency boost collector
│   ├── snippets.rs         ← Task 6: snippet generation with highlighting
│   ├── executor.rs         ← Task 7: orchestrate tantivy + SQLite
│   └── tests.rs            ← Task 12: unit tests
├── lib.rs                  ← modified: `pub mod search;`
└── ...

inboxly-store/tests/
└── search_integration.rs   ← Task 13: end-to-end tests

inboxly-ui/src/
├── widgets/
│   └── search_bar.rs       ← Task 9: SearchBar widget
├── views/
│   └── search_results.rs   ← Task 10: search results view
├── search/
│   └── debounce.rs         ← Task 11: 300ms debounce
└── app.rs                  ← Task 14: modified for search integration
```

---

## Build & Verify

```bash
# From workspace root
cd /mnt/TempNVME/projects/inbox-rust

# Store crate tests (parser, query builder, snippets, executor)
cargo test -p inboxly-store -- search

# Store integration tests
cargo test -p inboxly-store --test search_integration

# UI crate compilation check (widget tests are limited without a display)
cargo check -p inboxly-ui

# Full workspace lint
cargo clippy --workspace -- -D warnings

# Full workspace tests
cargo test --workspace
```

---

## Commit Sequence

| # | Message |
|---|---------|
| 1 | `feat(store): define search query AST, result, and snippet types` |
| 2 | `feat(store): implement search query tokeniser` |
| 3 | `feat(store): implement search query parser (tokens to AST)` |
| 4 | `feat(store): build tantivy queries from parsed search AST` |
| 5 | `feat(store): implement BM25 + recency boost search scoring` |
| 6 | `feat(store): implement search result snippet generator with highlighting` |
| 7 | `feat(store): implement search executor with tantivy + SQLite hybrid filtering` |
| 8 | `feat(store): expose public search API with parse + execute pipeline` |
| 9 | `feat(ui): implement SearchBar widget with expand/collapse and text input` |
| 10 | `feat(ui): implement search results view with snippet highlighting` |
| 11 | `feat(ui): implement 300ms search debounce for search-as-you-type` |
| 12 | `test(store): add comprehensive unit tests for search parser and query builder` |
| 13 | `test(store): add end-to-end search integration tests` |
| 14 | `feat(ui): wire search bar, debounce, and results into main application` |
