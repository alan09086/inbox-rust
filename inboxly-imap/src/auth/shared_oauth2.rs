//! Shared OAuth2 token cache.
//!
//! Wraps [`GmailOAuth2Config`] with a `Mutex<Option<OAuth2Token>>` so
//! multiple transport layers (IMAP, SMTP) can share the refresh state.
//! If SMTP refreshes the token, subsequent IMAP connections can pick up
//! the fresh value without re-refreshing.
//!
//! In Phase 3, only `SmtpSender` uses this type. The IMAP auth path stays
//! on its per-connection [`crate::auth::OAuth2AuthParams`] form. A later
//! cleanup can migrate IMAP to use this shared cache too (M36+).
//!
//! # Refresh-token rotation (M36 phase 2 / eng review A3)
//!
//! After a successful refresh, [`get_valid_access_token`](Self::get_valid_access_token)
//! compares the freshly returned `refresh_token` against the previously
//! cached one. If they differ (or if the previously cached refresh token
//! was `None`), the optional `persist_callback` is invoked with the
//! account email and the new token, giving the binary crate a hook to
//! re-write the keyring entry.
//!
//! Rotation is **fail-soft**: if the callback panics, the panic is
//! caught via [`std::panic::catch_unwind`] and logged as a `tracing::warn!`,
//! and the send still proceeds with the fresh access token. The
//! authorization server already returned the new refresh token; failing
//! the caller because we couldn't *persist* it would lose the new access
//! token AND make the next session reauthorize from scratch.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use tokio::sync::Mutex;

use super::oauth2::{GmailOAuth2Config, OAuth2Token, refresh_token};
use crate::error::{ImapError, Result};

/// Callback invoked when a refresh-token rotation is detected.
///
/// The arguments are `(email, new_token)`. The callback runs while the
/// internal mutex guard is held; do not perform long-running work inside
/// it. Implementations should be quick (e.g., a single keyring write) and
/// must NEVER call back into the same [`SharedOAuth2State`] (deadlock).
///
/// Failures are silently ignored at the call site (logged as `warn!`)
/// per the M36 phase 2 fail-soft policy.
pub type PersistCallback = Arc<dyn Fn(&str, &OAuth2Token) + Send + Sync>;

/// A shared OAuth2 state combining the stateless config with a
/// mutex-guarded cached token.
pub struct SharedOAuth2State {
    config: GmailOAuth2Config,
    /// Account email — used as the first argument to `persist_callback`
    /// when a rotation is detected, and also helpful for log lines so
    /// multi-account setups can attribute refresh activity per account.
    email: String,
    cached_token: Mutex<Option<OAuth2Token>>,
    /// Optional rotation hook. `None` means "no persistence wired" — the
    /// refresh still works, the new token just lives in memory until the
    /// process exits.
    persist_callback: Option<PersistCallback>,
}

impl std::fmt::Debug for SharedOAuth2State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedOAuth2State")
            .field("config", &self.config)
            .field("email", &self.email)
            .field("cached_token", &"<async-mutex>")
            .field("persist_callback", &self.persist_callback.is_some())
            .finish()
    }
}

impl SharedOAuth2State {
    /// Create a new shared state with no cached token yet and no
    /// rotation persistence callback.
    #[must_use]
    pub fn new(config: GmailOAuth2Config, email: String) -> Self {
        Self {
            config,
            email,
            cached_token: Mutex::new(None),
            persist_callback: None,
        }
    }

    /// Create a shared state seeded with a refresh token (no access token yet).
    ///
    /// The first call to [`Self::get_valid_access_token`] will perform an
    /// initial refresh to populate the access token.
    #[must_use]
    pub fn with_refresh_token(
        config: GmailOAuth2Config,
        email: String,
        refresh_token_str: String,
    ) -> Self {
        Self {
            config,
            email,
            cached_token: Mutex::new(Some(OAuth2Token {
                access_token: String::new(),
                refresh_token: Some(refresh_token_str),
                expires_at: None,
            })),
            persist_callback: None,
        }
    }

    /// Like [`Self::with_refresh_token`] but also installs a rotation
    /// persistence callback (M36 phase 2, eng review A3).
    #[must_use]
    pub fn with_refresh_token_and_callback(
        config: GmailOAuth2Config,
        email: String,
        refresh_token_str: String,
        persist: PersistCallback,
    ) -> Self {
        Self {
            config,
            email,
            cached_token: Mutex::new(Some(OAuth2Token {
                access_token: String::new(),
                refresh_token: Some(refresh_token_str),
                expires_at: None,
            })),
            persist_callback: Some(persist),
        }
    }

    /// Account email this state is associated with.
    #[must_use]
    pub fn email(&self) -> &str {
        &self.email
    }

    /// Return a bare valid access token, refreshing from the refresh token if
    /// the cached access token is expired or missing.
    ///
    /// **Returns a String** rather than a borrowed reference so the caller
    /// doesn't hold the mutex across an `.await` boundary during the send.
    ///
    /// If a refresh occurs and the authorization server returns a *new*
    /// refresh token (different from the previously cached one), the
    /// optional `persist_callback` is invoked. Per A3 the callback is
    /// fail-soft: any panic is caught and logged.
    ///
    /// # Errors
    ///
    /// Returns [`ImapError::OAuth2`] if no refresh token is available, or
    /// propagates the error from the refresh HTTP call.
    pub async fn get_valid_access_token(&self) -> Result<String> {
        let mut guard = self.cached_token.lock().await;

        // Check if we have a valid cached access token
        if let Some(token) = guard.as_ref()
            && !token.access_token.is_empty()
            && !token.is_expired()
        {
            return Ok(token.access_token.clone());
        }

        // Need to refresh. Extract the refresh token.
        let refresh_token_str = guard
            .as_ref()
            .and_then(|t| t.refresh_token.clone())
            .ok_or_else(|| ImapError::OAuth2 {
                reason: "no refresh token available to obtain access token".to_string(),
            })?;

        // tokio::sync::Mutex is an async mutex so it's safe to hold across .await.
        // Holding the guard across the refresh ensures concurrent callers don't
        // race-double-refresh against the OAuth2 server.
        let new_token = refresh_token(&self.config, &refresh_token_str).await?;

        // A3: detect rotation. The post-refresh shape comes straight from
        // the auth server, then `oauth2.rs::refresh_token` falls back to
        // the request token if the server omitted one. We compare the
        // returned `refresh_token` field against whatever was cached
        // BEFORE this refresh.
        Self::maybe_invoke_rotation_callback(
            &self.email,
            self.persist_callback.as_ref(),
            guard.as_ref(),
            &new_token,
        );

        let access_token = new_token.access_token.clone();
        *guard = Some(new_token);
        Ok(access_token)
    }

    /// Pure helper: given the previously cached token (if any) and a new
    /// token, decide whether to fire the rotation callback.
    ///
    /// Factored out so the rotation logic is unit-testable WITHOUT
    /// stubbing the network-bound `refresh_token` call. The two test
    /// cases (rotation fires / no-op) construct synthetic before/after
    /// `OAuth2Token` values and verify the callback was or wasn't
    /// invoked. Production callers reach this through
    /// [`Self::get_valid_access_token`].
    ///
    /// Rotation is detected when:
    ///
    /// - The new token has `Some(refresh_token)`, AND
    /// - the new token's refresh_token differs from the previously cached
    ///   one (which may itself be `None` if the cache was empty).
    ///
    /// The callback is invoked inside [`std::panic::catch_unwind`] so a
    /// panicking implementation cannot break the send. Per A3 this is
    /// fail-soft: any panic is logged as a `warn!` and the call continues.
    fn maybe_invoke_rotation_callback(
        email: &str,
        persist_callback: Option<&PersistCallback>,
        previous: Option<&OAuth2Token>,
        new_token: &OAuth2Token,
    ) {
        let Some(new_refresh) = new_token.refresh_token.as_deref() else {
            // The auth server returned no refresh token at all. This is
            // unusual (Google always echoes one) but the spec allows it
            // and our `oauth2.rs::refresh_token` falls back to the
            // request token, so this branch is effectively dead. Be
            // defensive anyway.
            return;
        };
        let previous_refresh = previous.and_then(|t| t.refresh_token.as_deref());
        if Some(new_refresh) == previous_refresh {
            // No rotation — server returned the same token. The plan's
            // A3 decision is "only persist on rotation"; doing nothing
            // here is the correct no-op path.
            return;
        }

        let Some(cb) = persist_callback else {
            // Rotation happened but the caller did not register a
            // persistence hook. The new token is still cached in
            // memory; durability across restarts is on the caller.
            return;
        };

        // catch_unwind: A3 fail-soft. The send already has the fresh
        // access token; a panicking persist callback must not clobber a
        // successful refresh.
        let cb_clone = Arc::clone(cb);
        let email_owned = email.to_string();
        let token_clone = new_token.clone();
        let result = catch_unwind(AssertUnwindSafe(move || {
            cb_clone(&email_owned, &token_clone);
        }));
        if result.is_err() {
            tracing::warn!(
                email = %email,
                "OAuth2 persist_callback panicked during refresh-token rotation; ignoring (fail-soft per A3)"
            );
        }
    }
}

/// Convenience alias for the shared-arc form plumbed through the app.
pub type SharedOAuth2 = Arc<SharedOAuth2State>;

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;
    use std::time::Instant;

    use super::*;

    /// Build a config that will never reach the network in unit tests.
    ///
    /// The unit tests below exercise [`maybe_invoke_rotation_callback`]
    /// directly — they never call `get_valid_access_token`, which is the
    /// only path that hits the auth server. The config still has to be
    /// constructible because [`SharedOAuth2State::new`] takes one as a
    /// parameter, but it is intentionally bogus and any test that
    /// accidentally invokes the network will fail loudly.
    fn dummy_config() -> GmailOAuth2Config {
        GmailOAuth2Config::new("test-client-id".to_string(), None)
    }

    /// Test recorder for the persist_callback. Captures every
    /// `(email, refresh_token)` pair the callback is invoked with so
    /// the assertions can inspect call count and arguments.
    #[derive(Default)]
    struct CallbackRecorder {
        invocations: StdMutex<Vec<(String, OAuth2Token)>>,
    }

    impl CallbackRecorder {
        fn callback(self: &Arc<Self>) -> PersistCallback {
            let me = Arc::clone(self);
            Arc::new(move |email: &str, token: &OAuth2Token| {
                me.invocations
                    .lock()
                    .expect("recorder mutex poisoned")
                    .push((email.to_string(), token.clone()));
            })
        }

        fn snapshot(&self) -> Vec<(String, OAuth2Token)> {
            self.invocations
                .lock()
                .expect("recorder mutex poisoned")
                .clone()
        }
    }

    fn token(access: &str, refresh: Option<&str>) -> OAuth2Token {
        OAuth2Token {
            access_token: access.to_string(),
            refresh_token: refresh.map(str::to_string),
            // expires_at intentionally None — these tests never look at
            // expiry; the rotation logic is independent of it.
            expires_at: None::<Instant>,
        }
    }

    /// Rotation callback fires exactly once when the refresh server
    /// returns a NEW refresh token.
    #[test]
    fn rotation_callback_fires_when_refresh_token_changes() {
        let recorder = Arc::new(CallbackRecorder::default());
        let cb = recorder.callback();

        let state = SharedOAuth2State::with_refresh_token_and_callback(
            dummy_config(),
            "test@example.com".to_string(),
            "refresh-1".to_string(),
            cb,
        );

        let previous = token("", Some("refresh-1"));
        let new_token = token("fresh-access-abc", Some("refresh-2-rotated"));
        SharedOAuth2State::maybe_invoke_rotation_callback(
            state.email(),
            state.persist_callback.as_ref(),
            Some(&previous),
            &new_token,
        );

        let calls = recorder.snapshot();
        assert_eq!(calls.len(), 1, "callback should fire exactly once");
        assert_eq!(calls[0].0, "test@example.com");
        assert_eq!(
            calls[0].1.refresh_token.as_deref(),
            Some("refresh-2-rotated")
        );
        assert_eq!(calls[0].1.access_token, "fresh-access-abc");
    }

    /// No-op when the refresh server returns the SAME refresh token.
    #[test]
    fn rotation_callback_does_not_fire_when_refresh_token_unchanged() {
        let recorder = Arc::new(CallbackRecorder::default());
        let cb = recorder.callback();

        let state = SharedOAuth2State::with_refresh_token_and_callback(
            dummy_config(),
            "noop@example.com".to_string(),
            "refresh-stable".to_string(),
            cb,
        );

        let previous = token("", Some("refresh-stable"));
        let new_token = token("fresh-access-xyz", Some("refresh-stable"));
        SharedOAuth2State::maybe_invoke_rotation_callback(
            state.email(),
            state.persist_callback.as_ref(),
            Some(&previous),
            &new_token,
        );

        let calls = recorder.snapshot();
        assert!(
            calls.is_empty(),
            "callback should NOT fire when refresh_token is unchanged, got: {calls:?}"
        );
    }

    /// A panicking callback is caught and does not propagate. The cache
    /// would still be updated by the production code path (the post-
    /// callback `*guard = Some(new_token)` in `get_valid_access_token`).
    #[test]
    fn rotation_callback_panic_is_caught() {
        let panicking: PersistCallback = Arc::new(|_email, _token| {
            panic!("intentional test panic from rotation callback");
        });
        let previous = token("", Some("refresh-1"));
        let new_token = token("fresh-access", Some("refresh-2"));
        // Must not panic — the catch_unwind in
        // `maybe_invoke_rotation_callback` swallows the panic.
        SharedOAuth2State::maybe_invoke_rotation_callback(
            "panic@example.com",
            Some(&panicking),
            Some(&previous),
            &new_token,
        );
    }
}
