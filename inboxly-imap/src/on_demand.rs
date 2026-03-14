//! On-demand single-email body fetch.
//!
//! When the user opens an email whose body hasn't been downloaded by
//! Phase 2 yet, this module fetches the single RFC822 body, processes
//! it, and notifies the UI.
