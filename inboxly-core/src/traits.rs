use crate::bundle::{Bundle, BundleCategory};
use crate::email::{EmailContent, EmailMeta};
use crate::error::Result;
use crate::highlight::Highlight;
use crate::id::{AccountId, BundleId, EmailId, ThreadId};
use crate::inbox::ThreadState;
use crate::thread::Thread;

/// Storage interface — abstracts over SQLite + Maildir + Tantivy.
/// Implemented by `inboxly-store`. All methods are async for database I/O.
pub trait Store: Send + Sync {
    // --- Email operations ---

    /// Insert or update email metadata.
    fn upsert_email_meta(
        &self,
        meta: &EmailMeta,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Retrieve email metadata by ID.
    fn get_email_meta(
        &self,
        id: &EmailId,
    ) -> impl std::future::Future<Output = Result<Option<EmailMeta>>> + Send;

    /// Load full email content from Maildir.
    fn get_email_content(
        &self,
        id: &EmailId,
    ) -> impl std::future::Future<Output = Result<Option<EmailContent>>> + Send;

    /// List email metadata for a thread, ordered by date.
    fn list_emails_for_thread(
        &self,
        thread_id: &ThreadId,
    ) -> impl std::future::Future<Output = Result<Vec<EmailMeta>>> + Send;

    // --- Thread operations ---

    /// Insert or update a thread.
    fn upsert_thread(
        &self,
        thread: &Thread,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Retrieve a thread by ID.
    fn get_thread(
        &self,
        id: &ThreadId,
    ) -> impl std::future::Future<Output = Result<Option<Thread>>> + Send;

    /// List threads for an account, ordered by newest_date descending.
    fn list_threads(
        &self,
        account_id: &AccountId,
        limit: u32,
        offset: u32,
    ) -> impl std::future::Future<Output = Result<Vec<Thread>>> + Send;

    // --- Thread state operations ---

    /// Get or create thread state.
    fn get_thread_state(
        &self,
        thread_id: &ThreadId,
    ) -> impl std::future::Future<Output = Result<ThreadState>> + Send;

    /// Update thread state (pin, done, snooze, bundle assignment).
    fn update_thread_state(
        &self,
        state: &ThreadState,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    // --- Bundle operations ---

    /// List all bundles.
    fn list_bundles(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<Bundle>>> + Send;

    /// Get a bundle by ID.
    fn get_bundle(
        &self,
        id: &BundleId,
    ) -> impl std::future::Future<Output = Result<Option<Bundle>>> + Send;

    /// Insert or update a bundle.
    fn upsert_bundle(
        &self,
        bundle: &Bundle,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// Email categorisation interface — assigns emails to bundles.
/// Implemented by `inboxly-bundler`.
pub trait Bundler: Send + Sync {
    /// Categorise an email and return the bundle category it belongs to.
    /// Returns `None` if the email doesn't match any rules (stays in primary inbox).
    fn categorise(
        &self,
        meta: &EmailMeta,
        content: Option<&EmailContent>,
    ) -> impl std::future::Future<Output = Result<Option<BundleCategory>>> + Send;

    /// Record a user's manual bundle assignment for sender learning.
    fn record_user_assignment(
        &self,
        meta: &EmailMeta,
        category: &BundleCategory,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// Smart extraction interface — detects highlights in email content.
/// Implemented by `inboxly-extract`.
pub trait Extractor: Send + Sync {
    /// Extract highlights from an email's content.
    /// Returns an empty vec if no actionable information is found.
    fn extract(
        &self,
        meta: &EmailMeta,
        content: &EmailContent,
    ) -> impl std::future::Future<Output = Result<Vec<Highlight>>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verify traits are object-safe enough to be used as bounds.
    // (They use RPITIT so they're not dyn-compatible, but they work as generic bounds.)

    fn _assert_store_bound<T: Store>(_t: &T) {}
    fn _assert_bundler_bound<T: Bundler>(_t: &T) {}
    fn _assert_extractor_bound<T: Extractor>(_t: &T) {}

    #[test]
    fn traits_module_compiles() {
        // This test verifies the traits module compiles successfully.
        // Actual implementations are in their respective crates.
        assert!(true);
    }
}
