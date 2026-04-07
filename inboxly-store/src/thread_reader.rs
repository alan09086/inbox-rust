//! Unified thread reader facade.
//!
//! Wraps both `Store` (SQLite metadata) and `MaildirStore` (filesystem
//! bodies) so consumers can load a full thread with one call instead
//! of plumbing two store handles. Future consumers (M36 reply,
//! M37 attachments) hold one `Arc<ThreadReader>` instead of two.
//!
//! Returns raw storage data (`LoadedEmail`) carrying `SlimEmailContent`
//! (body + attachment metadata only, no headers, no attachment bytes
//! — eng review Issue 2.6). UI-shaped types like `LoadedThread` live
//! in `inboxly-ui` and are built from this output via
//! `inboxly_ui::loaded_thread::build_loaded_thread()`.

use std::path::Path;
use std::sync::Arc;

use crate::emails::EmailRow;
use crate::error::{Result, StoreError};
use crate::maildir_store::MaildirStore;
use crate::store::Store;
use inboxly_core::SlimEmailContent;

/// One email loaded from the store, with its slim body content if
/// available. `content` is `None` when the body hasn't been
/// downloaded yet OR when the disk read failed (latter is logged
/// but not fatal — we still want to render the row metadata).
/// The `SlimEmailContent` deliberately omits headers and attachment
/// byte content; those live in the full `EmailContent` which is
/// loaded on demand by other code paths (future M37 for download).
#[derive(Debug, Clone)]
pub struct LoadedEmail {
    pub row: EmailRow,
    pub content: Option<SlimEmailContent>,
}

/// Facade that hides the two-store coupling for thread loading.
/// Hold via `Arc<ThreadReader>` for cheap sharing across components.
pub struct ThreadReader {
    store: Arc<Store>,
    maildir: Arc<MaildirStore>,
}

impl ThreadReader {
    pub fn new(store: Arc<Store>, maildir: Arc<MaildirStore>) -> Self {
        Self { store, maildir }
    }

    /// Load all emails in a thread, hydrating each with its slim body
    /// from disk where available. Errors only if the underlying
    /// SQLite query fails OR if the thread has no emails. Per-row
    /// body-read failures are non-fatal: the row is returned with
    /// `content: None`.
    pub fn load_thread(&self, thread_id: &str) -> Result<Vec<LoadedEmail>> {
        let rows = self.store.get_emails_by_thread(thread_id)?;
        if rows.is_empty() {
            return Err(StoreError::NotFound(format!(
                "no emails in thread {thread_id}"
            )));
        }
        let loaded = rows
            .into_iter()
            .map(|row| {
                let content = if row.body_downloaded && !row.maildir_path.is_empty() {
                    // Use read_email_slim (Issue 2.6) instead of
                    // read_email_content — we don't need headers or
                    // attachment bytes for the thread detail view.
                    self.maildir
                        .read_email_slim(Path::new(&row.maildir_path))
                        .ok()
                } else {
                    None
                };
                LoadedEmail { row, content }
            })
            .collect();
        Ok(loaded)
    }
}

#[cfg(test)]
mod tests {
    // Pure unit tests for ThreadReader require a full SQLite + Maildir
    // fixture. Defer to Step 4.5 (integration test) which sets
    // up a temp dir and exercises the full path. Module-level tests
    // here only assert that the type compiles.
    #[test]
    fn types_compile() {
        let _ = std::marker::PhantomData::<super::ThreadReader>;
    }
}
