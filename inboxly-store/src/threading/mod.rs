//! Email threading algorithm based on References/In-Reply-To headers.
//!
//! Implements simplified JWZ threading: groups emails by `References[0]`
//! (thread root), with placeholder threads for orphaned replies that get
//! resolved when the root email arrives. No subject-based grouping.
//!
//! # Module overview
//!
//! - [`headers`] — Parse threading headers from email header maps.
//! - [`assign`] — Core thread assignment algorithm.
//! - [`unify`] — Merge placeholder threads when root emails arrive.
//! - [`metadata`] — Recalculate thread aggregate metadata.
//! - [`batch`] — Batch threading for unthreaded emails.
//! - [`rebuild`] — Wipe and rebuild all threads from scratch.

pub mod assign;
pub mod batch;
pub mod headers;
pub mod metadata;
pub mod rebuild;
pub mod unify;

#[cfg(test)]
mod edge_case_tests;

// Re-export public API.
pub use assign::{
    ThreadAssignment, assign_thread, is_placeholder_thread, list_placeholder_threads,
};
pub use batch::{thread_email_batch, thread_unthreaded_emails};
pub use headers::{ThreadingHeaders, extract_threading_headers, threading_headers_from_fields};
pub use metadata::{get_thread_participants, refresh_all_thread_metadata, refresh_thread_metadata};
pub use rebuild::rebuild_threads;
pub use unify::try_unify_placeholder;
