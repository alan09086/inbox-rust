use tantivy::schema::{
    DateOptions, FacetOptions, Field, Schema, FAST, INDEXED, STORED, STRING, TEXT,
};

use inboxly_core::{BundleCategory, EmailMeta};
use tantivy::schema::Facet;
use tantivy::DateTime as TantivyDateTime;
use tantivy::TantivyDocument;

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

impl Default for SearchSchema {
    fn default() -> Self {
        Self::new()
    }
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
        let bundle_category = builder.add_facet_field("bundle_category", FacetOptions::default());

        // has_attachment: boolean-like u64 (0 or 1). INDEXED for `has:attachment` filter.
        // FAST for efficient access during scoring if needed.
        let has_attachment = builder.add_u64_field("has_attachment", INDEXED | FAST | STORED);

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
