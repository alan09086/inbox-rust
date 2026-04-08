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
