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

/// Error type returned by [`ThreadReader::load_email`].
///
/// Distinct from [`StoreError`] because [`ThreadReader::load_email`] needs
/// to surface a specific "the row exists but the body bytes are not on
/// disk yet" failure mode that the caller (the M36 reply prefill bridge)
/// must distinguish from a generic SQLite error: the former triggers a
/// best-effort body-fetch retry, the latter is a hard error shown to the
/// user. [`ThreadReader::load_thread`] does NOT need this distinction
/// because it tolerates missing bodies (returns `content: None` and lets
/// the UI render the row metadata anyway), so it stays on plain
/// [`StoreError`].
#[derive(Debug, thiserror::Error)]
pub enum ThreadReaderError {
    /// The email row exists in SQLite but the body bytes have not been
    /// downloaded to disk yet (or the on-disk file is unreadable). The
    /// reply prefill bridge in `inboxly-ui` catches this and dispatches
    /// `ComposeReplyFailed { reason: "body not downloaded" }` so the
    /// user sees a "wait for sync" message instead of an empty quote
    /// block. A future post-M36 phase will trigger a real on-demand
    /// body fetch from this branch.
    #[error("email body not downloaded for {email_id}")]
    BodyNotDownloaded {
        /// The email id whose body is missing. Echoed back to the
        /// caller so the eventual body-fetch task knows which message
        /// to fetch.
        email_id: String,
    },
    /// Wrapped SQLite or storage error from the underlying [`Store`].
    /// Includes "row not found" — the caller treats every variant
    /// other than `BodyNotDownloaded` as a hard failure.
    #[error("store error: {0}")]
    Store(#[from] StoreError),
}

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

    /// Load a single email by its id, hydrating it with its slim body
    /// from disk. Used by the M36 Phase 8 reply prefill bridge in
    /// `inboxly-ui` so it can fetch only the parent message of a reply
    /// instead of paying to load the entire thread when the user only
    /// wants to quote one message.
    ///
    /// **Failure modes (intentional):**
    ///
    /// - The row does not exist in SQLite → [`ThreadReaderError::Store`]
    ///   wrapping [`StoreError::NotFound`]. The caller surfaces this as
    ///   a hard error to the user.
    /// - The row exists but `body_downloaded == false` OR
    ///   `maildir_path` is empty → [`ThreadReaderError::BodyNotDownloaded`].
    ///   The caller treats this as the G3 header-only-row fallback and
    ///   (in a future post-M36 phase) triggers an on-demand body fetch.
    /// - The row exists, `body_downloaded == true`, `maildir_path` is
    ///   set, but the on-disk file is unreadable (permission denied,
    ///   missing file, corrupted bytes) → also
    ///   [`ThreadReaderError::BodyNotDownloaded`]. The disk read error
    ///   is logged at `warn!`. We collapse this case into
    ///   `BodyNotDownloaded` because the caller's recovery path is the
    ///   same: re-fetch the body. (Contrast with
    ///   [`ThreadReader::load_thread`], which tolerates a missing body
    ///   on a per-row basis and renders the rest of the thread anyway —
    ///   acceptable for thread display, fatal for reply prefill which
    ///   needs the body to make a quote block.)
    ///
    /// # Errors
    ///
    /// Returns [`ThreadReaderError::Store`] if the SQLite query fails
    /// (including `NotFound` for unknown ids), or
    /// [`ThreadReaderError::BodyNotDownloaded`] if the row exists but
    /// the body bytes are not available on disk.
    pub fn load_email(
        &self,
        email_id: &str,
    ) -> std::result::Result<LoadedEmail, ThreadReaderError> {
        let row = self.store.get_email(email_id)?;
        if !row.body_downloaded || row.maildir_path.is_empty() {
            return Err(ThreadReaderError::BodyNotDownloaded {
                email_id: email_id.to_string(),
            });
        }
        let content = self
            .maildir
            .read_email_slim(Path::new(&row.maildir_path))
            .inspect_err(|e| {
                tracing::warn!(
                    path = %row.maildir_path,
                    error = %e,
                    "ThreadReader::load_email: read_email_slim failed"
                );
            })
            .ok();
        // Disk read failed (logged above) — collapse into the same
        // BodyNotDownloaded branch so the caller's recovery path is
        // uniform. The reply prefill cannot meaningfully proceed
        // without the body bytes anyway.
        if content.is_none() {
            return Err(ThreadReaderError::BodyNotDownloaded {
                email_id: email_id.to_string(),
            });
        }
        Ok(LoadedEmail { row, content })
    }
}

impl std::fmt::Debug for ThreadReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadReader").finish_non_exhaustive()
    }
}
