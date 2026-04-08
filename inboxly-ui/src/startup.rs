//! Startup-time singletons populated by the binary crate, read by the
//! UI components.
//!
//! ## Why this module exists
//!
//! Two pieces of state must be available at the time `App()` first
//! renders, but cannot live in the `inboxly-ui` crate at compile time:
//!
//! 1. **`STARTUP_ACCOUNTS`** — the user's configured email accounts,
//!    parsed from `~/.config/inboxly/config.toml` by `AppConfig::load()`.
//!    Loading config requires `inboxly_core::config::AppConfig`, which
//!    `inboxly-ui` already depends on, but the *file path* is owned by
//!    the binary crate. Before M36 phase 2 the binary set its own
//!    `STARTUP_ACCOUNTS` static and the UI never read it — a latent
//!    bug where every running instance saw `accounts: Vec::new()`.
//!
//! 2. **`OAUTH2_CONTEXTS`** — a map of `email -> Arc<SharedOAuth2State>`
//!    for every account whose `auth_method` is `OAuth2`. Built by the
//!    binary at startup so the keyring lookup happens once and the UI
//!    never has to touch the keyring on the render path. The send
//!    pipeline reads this map keyed by `account_config.email.to_ascii_lowercase()`
//!    and passes the matching `SharedOAuth2` into
//!    `SmtpSender::with_oauth2`.
//!
//! ## Architecture rationale
//!
//! The static lives in the *consumer* (`inboxly-ui`) and is *populated*
//! by the binary (`inboxly`). Putting it in the binary would require the
//! UI to depend on `inboxly` (the binary crate) — a circular dependency
//! that Rust does not permit. Putting it in `inboxly-core` would force
//! `inboxly-core` to depend on `inboxly-imap` (for `SharedOAuth2`),
//! which would invert the existing dependency direction.
//!
//! ## Lazy keyring access (Gemini G6)
//!
//! Both statics are populated *only* by the binary's `main()` and *only*
//! when the binary is launching the desktop UI. CLI subcommands
//! (`inboxly --help`, `oauth2-authorize`, `set-password`,
//! `delete-credentials`) MUST exit before the static initialisers run so
//! that `inboxly --help` never touches the keyring. The order of
//! operations in `inboxly/src/main.rs` enforces this.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use inboxly_core::config::AccountConfig;
use inboxly_imap::auth::shared_oauth2::SharedOAuth2;
use inboxly_store::thread_reader::ThreadReader;
use inboxly_store::{MaildirStore, Store};

/// Account configurations loaded from `~/.config/inboxly/config.toml`.
///
/// The binary's `main()` populates this once before launching Dioxus;
/// the UI's `App()` reads it (via `.get().cloned().unwrap_or_default()`)
/// to seed the `Inboxly::accounts` field on first render.
///
/// `OnceLock` rather than `LazyLock` because the value comes from a
/// fallible config load — the binary decides whether to use the parsed
/// config or fall back to an empty `Vec`, then commits whichever it
/// chose via `.set()`.
pub static STARTUP_ACCOUNTS: OnceLock<Vec<AccountConfig>> = OnceLock::new();

/// Per-account OAuth2 shared state, keyed by lowercased email.
///
/// One entry per `AccountConfig` with `auth_method == OAuth2` whose
/// refresh token was found in the keyring at startup. Accounts without
/// a stored refresh token are absent from the map; the send pipeline
/// surfaces a clear error directing the user to run
/// `inboxly oauth2-authorize <email>`.
///
/// The map is wrapped in an `Arc` so the `App` component can clone the
/// handle into a Dioxus context cheaply (one atomic increment) and
/// every component that needs OAuth2 lookup gets the same backing
/// HashMap. The map itself is immutable after startup — there is
/// deliberately no API to add/remove entries at runtime; that would
/// require restarting Inboxly after running the CLI subcommand.
pub type OAuth2Contexts = Arc<HashMap<String, SharedOAuth2>>;

/// Process-global OAuth2 context map. Populated by the binary's
/// `main()` after the `STARTUP_ACCOUNTS` set; read by the UI's `App()`
/// component via [`oauth2_contexts`].
pub static OAUTH2_CONTEXTS: OnceLock<OAuth2Contexts> = OnceLock::new();

/// Get the OAuth2 contexts map, returning an empty map if the static
/// has not been populated.
///
/// The empty-map fallback is the test path: `cargo test` runs the UI
/// crate's unit tests without the binary's `main()`, so the static
/// never gets `.set()` called on it. Returning an empty `Arc<HashMap>`
/// keeps the rest of the UI code happy without forcing every test to
/// stage OAuth2 plumbing.
#[must_use]
pub fn oauth2_contexts() -> OAuth2Contexts {
    OAUTH2_CONTEXTS
        .get()
        .cloned()
        .unwrap_or_else(|| Arc::new(HashMap::new()))
}

/// Single-thread-only wrapper that satisfies the `Sync` bound required
/// by `OnceLock<T>` for a non-`Sync` `T`.
///
/// # Why this exists
///
/// [`Store`] and [`MaildirStore`] hold `rusqlite::Connection` handles
/// and `maildir::Maildir` iterators respectively — both are `Send`
/// but explicitly *not* `Sync` (rusqlite gates `Sync` behind the
/// `unlock_notify` feature flag, which Inboxly does not enable).
///
/// `OnceLock<T>` requires `T: Sync` because statics are accessible
/// from every thread; `Arc<Store>` is `!Send + !Sync` for the same
/// reason. The UI, however, runs exclusively on the Dioxus desktop
/// single-threaded local executor — the binary's `main()` writes the
/// singletons on the main thread, and the UI's `App()` component
/// reads them on the same main thread. There is no other thread that
/// ever touches them.
///
/// # Safety
///
/// Every `unsafe impl Sync` below is sound iff the invariant
/// "read/write only from the main thread" is upheld. This is
/// enforced structurally:
///
/// 1. `main()` calls `.set()` exactly once, from the process's
///    initial thread, before Dioxus launches the UI.
/// 2. `App()` calls `.get()` during its first render, which Dioxus
///    schedules on the same main thread (it's a single-threaded
///    desktop runtime).
/// 3. There is no `tokio::spawn` or `std::thread::spawn` in the UI
///    that handles these singletons — the only async work is
///    `dioxus::prelude::spawn`, which uses the local executor.
///
/// If a future change introduces cross-thread access (e.g. a
/// background sync worker reading `STORE` directly instead of
/// through a channel), **this wrapper becomes unsound** and must be
/// replaced with proper thread-safety (likely by making `Store` wrap
/// the connection in a `Mutex` and implementing `Sync` manually).
#[doc(hidden)]
pub struct MainThreadOnly<T>(T);

impl<T> MainThreadOnly<T> {
    /// Wrap a value, asserting that it will only be accessed from the
    /// main thread.
    ///
    /// # Safety
    ///
    /// The caller must ensure that every subsequent access to the
    /// wrapped value happens on the main thread. See the
    /// [`MainThreadOnly`] docs for the project-wide invariant.
    pub const unsafe fn new(value: T) -> Self {
        Self(value)
    }

    /// Borrow the wrapped value.
    ///
    /// # Safety
    ///
    /// Caller must be on the main thread.
    #[must_use]
    pub const fn get(&self) -> &T {
        &self.0
    }
}

// SAFETY: see the `MainThreadOnly` type docs. Every `.get()` happens
// on the single Dioxus main thread; the invariant is enforced by the
// architecture (no worker threads read these statics) rather than by
// the type system.
//
// Both `Send` and `Sync` are asserted because `OnceLock<T>: Sync`
// requires `T: Send + Sync` (`OnceLock` allows `&` access from any
// thread, and the stored value may be dropped from any thread when
// the `OnceLock` itself is dropped — except here, where the static
// lives for the entire process and is never dropped).
unsafe impl<T> Send for MainThreadOnly<T> {}
unsafe impl<T> Sync for MainThreadOnly<T> {}

/// Shared SQLite store for the running app.
///
/// Populated by the binary's `main()` after [`Paths::ensure_dirs`]
/// and [`Store::open`] succeed; read by [`crate::components::app::App`]
/// on first render to seed `Inboxly::store`. The store holds a
/// non-`Sync` `rusqlite::Connection` so it only works on Dioxus's
/// single-threaded local executor — see
/// [`crate::app::Inboxly::with_store`] for the matching
/// `clippy::arc_with_non_send_sync` allow attribute.
///
/// Wrapped in [`MainThreadOnly`] to bypass the `OnceLock<T>: Sync`
/// bound for a non-`Sync` `Arc<Store>`. The safety invariant ("only
/// accessed from the main thread") matches the pre-existing
/// architecture.
///
/// M36.1 closes the pre-existing M35 gap where the binary never
/// instantiated `Store` and every `Inboxly::store` was `None` at runtime.
///
/// [`Paths::ensure_dirs`]: inboxly_core::config::Paths::ensure_dirs
pub static STORE: OnceLock<MainThreadOnly<Arc<Store>>> = OnceLock::new();

/// Per-account Maildir store map, keyed by
/// `account_id_from_email(email).0.to_string()`.
///
/// Populated by the binary's `main()` alongside [`STORE`]. Each
/// entry's [`MaildirStore::init`] is called at population time so the
/// `.Sent/`, `.Drafts/`, `.Trash/`, `.Spam/` subdirectories exist
/// before any read or write.
///
/// The map is read by [`build_thread_reader_for`] to construct a
/// [`ThreadReader`] for the active account, and by the send/reply
/// pipelines (via the same account-id derivation) to locate the
/// per-account Maildir for SENT/DRAFTS writes.
///
/// See Phase 4 / Phase 5 `run_send_pipeline` for the matching
/// on-demand construction pattern that is still used as a fallback
/// when this map doesn't contain the target account (the maildir
/// initialisation is idempotent, so doing both is safe).
pub type MaildirStoreMap = Arc<HashMap<String, Arc<MaildirStore>>>;

/// Process-global per-account Maildir store map. Populated by the
/// binary's `main()` after [`STORE`]; read by
/// [`build_thread_reader_for`] and the send pipeline.
///
/// Wrapped in [`MainThreadOnly`] for the same reason as [`STORE`]:
/// [`MaildirStore`] is not `Sync` because it holds filesystem
/// iterators that aren't thread-safe.
pub static MAILDIR_STORES: OnceLock<MainThreadOnly<MaildirStoreMap>> = OnceLock::new();

/// Get the shared [`Store`] handle, returning `None` if the static
/// has not been populated.
///
/// Callers MUST be on the Dioxus main thread — see [`MainThreadOnly`]
/// for the safety invariant. In practice every caller is already on
/// the main thread because the only read sites are the UI's `App()`
/// component and the `Inboxly` message handlers, both of which run
/// on the Dioxus local executor.
#[must_use]
pub fn store() -> Option<Arc<Store>> {
    STORE.get().map(|wrapper| wrapper.get().clone())
}

/// Get the Maildir store map, returning an empty map if the static has
/// not been populated.
///
/// Matches the test-fallback pattern of [`oauth2_contexts`] — unit
/// tests in this crate run without the binary's `main()` so the
/// static is never `.set()`.
#[must_use]
pub fn maildir_stores() -> MaildirStoreMap {
    MAILDIR_STORES
        .get()
        .map(|wrapper| wrapper.get().clone())
        .unwrap_or_else(|| Arc::new(HashMap::new()))
}

/// Build a [`ThreadReader`] for the given `account_id` using the
/// current [`STORE`] and [`MAILDIR_STORES`] singletons.
///
/// Returns `None` if either singleton is unset or the map doesn't
/// contain the requested account — the caller (App first-render and
/// `SwitchAccount`) is expected to handle the `None` case by leaving
/// `Inboxly::thread_reader = None`, which makes the reply/forward
/// pipeline surface a clear error rather than silently misbehaving.
///
/// # Clippy
///
/// `clippy::arc_with_non_send_sync` is allowed because
/// [`ThreadReader`] wraps the non-`Send` [`Store`]. The whole UI
/// already runs on the Dioxus single-threaded local executor; see
/// [`crate::app::Inboxly::with_store`] for the matching attribute.
#[must_use]
#[allow(clippy::arc_with_non_send_sync)]
pub fn build_thread_reader_for(account_id: &str) -> Option<Arc<ThreadReader>> {
    let store_handle = store()?;
    let maildir_map = MAILDIR_STORES.get().map(|w| w.get().clone())?;
    let maildir = maildir_map.get(account_id).cloned()?;
    Some(Arc::new(ThreadReader::new(store_handle, maildir)))
}

#[cfg(test)]
mod tests {
    //! Tests for the M36.1 data-layer startup singletons.
    //!
    //! We can't directly `.set()` the `OnceLock`s here because other
    //! tests in the workspace might rely on them being unset — a
    //! `.set()` is permanent for the lifetime of the process. So the
    //! tests below focus on the fallback path: unset singletons must
    //! return `None`/empty-map without panicking, which is the only
    //! observable property the UI code depends on at test time.

    use super::{
        MAILDIR_STORES, STORE, build_thread_reader_for, maildir_stores, store,
    };

    /// [`STORE`] and [`MAILDIR_STORES`] start unset in the test
    /// harness, and the getter helpers must return their documented
    /// empty/`None` values without panicking. This is the path the
    /// unit tests in `inboxly-ui` take, so any regression here would
    /// cascade into every test that constructs an `Inboxly` via
    /// `App()`'s render-time helpers.
    #[test]
    fn singletons_default_to_empty_in_tests() {
        // The `.get()` here is deliberately non-destructive: we are
        // only reading, never setting. If another test in the same
        // process has previously set STORE (it shouldn't — nothing in
        // this crate calls `STORE.set()`), this assertion would fail
        // and flag the contamination.
        assert!(
            STORE.get().is_none(),
            "STORE should be unset in unit tests — the binary's main() is the only setter"
        );
        assert!(
            MAILDIR_STORES.get().is_none(),
            "MAILDIR_STORES should be unset in unit tests"
        );
        // The high-level getter should also return `None`.
        assert!(store().is_none());
    }

    /// The `maildir_stores` helper returns an empty map when the
    /// static is unset so UI callers can unconditionally iterate /
    /// lookup without a null check.
    #[test]
    fn maildir_stores_returns_empty_map_when_unset() {
        let map = maildir_stores();
        assert!(
            map.is_empty(),
            "unset MAILDIR_STORES should return an empty map"
        );
    }

    /// [`build_thread_reader_for`] returns `None` when the singletons
    /// are unset (test path) — guarantees the App first-render helper
    /// does not panic when tests exercise the component layer.
    #[test]
    fn build_thread_reader_for_returns_none_when_unset() {
        let reader = build_thread_reader_for("some-account-id");
        assert!(
            reader.is_none(),
            "with unset singletons, thread reader construction must fail soft to None"
        );
    }
}
