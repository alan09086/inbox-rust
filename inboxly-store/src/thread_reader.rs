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
///
/// **Threading note:** `ThreadReader` is `!Send + !Sync` because
/// `Store` holds a `rusqlite::Connection`. It works fine with
/// Dioxus's single-threaded `spawn` executor but cannot be moved
/// into `tokio::spawn` or `std::thread::spawn`. M36/M37 consumers
/// should hold it via `use_context::<Signal<Option<Arc<ThreadReader>>>>()`
/// or pass it through component props, not via background tasks.
pub struct ThreadReader {
    store: Arc<Store>,
    maildir: Arc<MaildirStore>,
}

impl ThreadReader {
    /// Build a new `ThreadReader` from existing store handles. Both
    /// `Store` and `MaildirStore` are held by `Arc` for cheap sharing
    /// across components.
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
                        .inspect_err(|e| {
                            tracing::warn!(
                                path = %row.maildir_path,
                                error = %e,
                                "ThreadReader: failed to read email body for thread detail"
                            );
                        })
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

impl std::fmt::Debug for ThreadReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadReader").finish_non_exhaustive()
    }
}
