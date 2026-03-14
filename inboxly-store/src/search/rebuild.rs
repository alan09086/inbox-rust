use inboxly_core::{BundleCategory, EmailMeta};

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
