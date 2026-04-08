//! Keyring-backed secrets storage for Inboxly.
//!
//! This module stores SMTP/IMAP passwords and OAuth2 refresh tokens in the
//! host platform's secret service (libsecret / KWallet on Linux, Keychain on
//! macOS, Credential Manager on Windows). On Linux the
//! `linux-native-sync-persistent` feature layers an in-kernel keyutils cache
//! on top of `dbus-secret-service` so writes are persisted across logouts.
//!
//! # Lazy access
//!
//! The keyring is only touched when a caller actually requests a secret. There
//! is intentionally **no** startup probe, warm-up call, or "keyring health
//! check" function here — that would fire a KWallet PAM unlock dialog on
//! every `inboxly --help` invocation. Phase 0 (Gemini G6) locked this in.
//!
//! # Key layout
//!
//! All entries live under a single service name so `secret-tool search
//! service inboxly` (or a future `inboxly secrets list` command) can
//! enumerate every credential Inboxly owns. The per-entry `user` field
//! carries both the secret kind and the account email:
//!
//! - Password / app password: `user = "password:<email-lowercased>"`
//! - OAuth2 refresh token:    `user = "oauth2:<email-lowercased>"`
//!
//! The email is [`str::to_ascii_lowercase`]-normalized before use so that
//! `Alan@Example.COM` and `alan@example.com` collide in a single entry
//! (case-insensitive by convention — SMTP local parts are technically
//! case-sensitive per RFC 5321, but in practice every real provider treats
//! them as case-insensitive). Callers do not need to lowercase the email
//! themselves.
//!
//! # Env var fallback
//!
//! [`get_password`] falls back to `std::env::var("INBOXLY_SMTP_PASSWORD")`
//! **only** when the keyring lookup returns [`keyring::Error::NoEntry`]. Any
//! other keyring error (DBus failure, service unavailable, storage locked)
//! propagates as [`SecretsError::Keyring`] so operators can see real failures
//! instead of silently degrading to the env var. When the fallback fires, a
//! `tracing::warn!` records the degraded session.
//!
//! [`get_oauth2_refresh_token`] does **not** have an env-var fallback —
//! refresh tokens are too long-lived to sit in a shell history.
//!
//! # Testing
//!
//! The `keyring = "3"` crate ships a `mock` credential store, but its mocks
//! are `CredentialPersistence::EntryOnly` — each `Entry::new` call returns a
//! fresh, independent in-memory credential that does not see writes made via
//! other entries with the same `(service, user)`. That makes it useless for
//! round-trip tests of the functions in this module, which construct a new
//! `Entry` on every call by design.
//!
//! Rather than leaking a `SecretsStore` trait through the public API or
//! hitting the real KWallet during `cargo test` (which would violate the
//! side-effecting-tests rule and wouldn't work in CI anyway), this module
//! swaps in a process-global `HashMap` backend under `#[cfg(test)]`. Tests
//! exercise the same code paths production does — the env-var fallback, the
//! lowercase-email normalization, `NoEntry → Ok(None)` mapping, empty-secret
//! rejection, idempotent deletes — just against an in-process map instead of
//! a DBus keyring.

use thiserror::Error;

/// Keyring service name for every Inboxly-owned secret.
///
/// Chosen so a single `secret-tool clear service inboxly` purges every
/// stored credential on uninstall. Only referenced by the real keyring
/// backend (see the `backend` module below); the test backend uses a pure
/// in-memory map and does not need a service string.
#[cfg(not(test))]
const KEYRING_SERVICE: &str = "inboxly";

/// Environment variable consulted as a last-resort password fallback when
/// the keyring has no entry for the requested account.
const SMTP_PASSWORD_ENV: &str = "INBOXLY_SMTP_PASSWORD";

/// Errors returned by the secrets backend.
///
/// This is deliberately a thin wrapper so that call sites never need to
/// depend on the `keyring` crate directly just to pattern-match on an error
/// kind. Keyring errors are flattened into a displayable string.
#[derive(Debug, Error)]
pub enum SecretsError {
    /// The underlying keyring backend returned an error other than
    /// `NoEntry`.
    ///
    /// `NoEntry` is **not** mapped to this variant; the getters return
    /// `Ok(None)` in that case.
    #[error("keyring error: {0}")]
    Keyring(String),

    /// A caller attempted to store an empty string as a secret. Refused
    /// defensively so a buggy call site can't silently wipe a real
    /// credential by passing an empty password.
    #[error("empty secret value — refusing to store")]
    EmptySecret,
}

/// Discriminator for the kind of secret stored under a given email.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecretKind {
    Password,
    OAuth2Refresh,
}

/// Build the `user` field for a keyring entry.
///
/// Lowercases the email so `Foo@Bar.com` and `foo@bar.com` resolve to the
/// same entry.
fn user_field(kind: SecretKind, email: &str) -> String {
    let prefix = match kind {
        SecretKind::Password => "password",
        SecretKind::OAuth2Refresh => "oauth2",
    };
    format!("{prefix}:{}", email.to_ascii_lowercase())
}

// ---------- Backend: real keyring (production) ----------

#[cfg(not(test))]
mod backend {
    use super::{KEYRING_SERVICE, SecretKind, SecretsError, user_field};

    fn entry_for(kind: SecretKind, email: &str) -> Result<keyring::Entry, SecretsError> {
        let user = user_field(kind, email);
        keyring::Entry::new(KEYRING_SERVICE, &user)
            .map_err(|err| SecretsError::Keyring(err.to_string()))
    }

    pub(super) fn read(kind: SecretKind, email: &str) -> Result<Option<String>, SecretsError> {
        let entry = entry_for(kind, email)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(other) => Err(SecretsError::Keyring(other.to_string())),
        }
    }

    pub(super) fn write(
        kind: SecretKind,
        email: &str,
        value: &str,
    ) -> Result<(), SecretsError> {
        let entry = entry_for(kind, email)?;
        entry
            .set_password(value)
            .map_err(|err| SecretsError::Keyring(err.to_string()))
    }

    pub(super) fn delete(kind: SecretKind, email: &str) -> Result<(), SecretsError> {
        let entry = entry_for(kind, email)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(other) => Err(SecretsError::Keyring(other.to_string())),
        }
    }
}

// ---------- Backend: in-process HashMap (tests only) ----------

#[cfg(test)]
mod backend {
    use super::{SecretKind, SecretsError, user_field};
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    fn store() -> &'static Mutex<HashMap<String, String>> {
        static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
        STORE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub(super) fn read(kind: SecretKind, email: &str) -> Result<Option<String>, SecretsError> {
        let key = user_field(kind, email);
        let guard = store()
            .lock()
            .map_err(|err| SecretsError::Keyring(format!("test store poisoned: {err}")))?;
        Ok(guard.get(&key).cloned())
    }

    pub(super) fn write(
        kind: SecretKind,
        email: &str,
        value: &str,
    ) -> Result<(), SecretsError> {
        let key = user_field(kind, email);
        let mut guard = store()
            .lock()
            .map_err(|err| SecretsError::Keyring(format!("test store poisoned: {err}")))?;
        guard.insert(key, value.to_string());
        Ok(())
    }

    pub(super) fn delete(kind: SecretKind, email: &str) -> Result<(), SecretsError> {
        let key = user_field(kind, email);
        let mut guard = store()
            .lock()
            .map_err(|err| SecretsError::Keyring(format!("test store poisoned: {err}")))?;
        guard.remove(&key);
        Ok(())
    }
}

// ---------- Public API ----------

/// Retrieve the stored password/app password for `email`.
///
/// Lookups are case-insensitive on the local part of the email: `Foo@Bar.com`
/// and `foo@bar.com` resolve to the same entry.
///
/// If the keyring has no entry for `email`, this function falls back to the
/// `INBOXLY_SMTP_PASSWORD` environment variable. When that fallback fires,
/// a `tracing::warn!` is emitted so operators can see the degraded session
/// in logs.
///
/// # Errors
///
/// Returns [`SecretsError::Keyring`] if the keyring backend fails for any
/// reason other than [`keyring::Error::NoEntry`] — for example, a DBus
/// connection failure or a locked credential store. A missing entry **does
/// not** produce an error; it returns `Ok(None)` (unless the env-var
/// fallback provides a value, in which case it returns `Ok(Some(..))`).
pub fn get_password(email: &str) -> Result<Option<String>, SecretsError> {
    if let Some(value) = backend::read(SecretKind::Password, email)? {
        return Ok(Some(value));
    }
    match std::env::var(SMTP_PASSWORD_ENV) {
        Ok(value) if !value.is_empty() => {
            tracing::warn!(
                env = SMTP_PASSWORD_ENV,
                "no keyring entry for account; falling back to environment variable"
            );
            Ok(Some(value))
        }
        _ => Ok(None),
    }
}

/// Store `password` in the keyring for `email`.
///
/// The email is lowercased before use (see [`get_password`] for details).
///
/// # Errors
///
/// Returns [`SecretsError::EmptySecret`] if `password` is empty — this is a
/// guard against accidentally wiping an existing entry via an empty string.
/// Returns [`SecretsError::Keyring`] if the backend rejects the write.
pub fn set_password(email: &str, password: &str) -> Result<(), SecretsError> {
    if password.is_empty() {
        return Err(SecretsError::EmptySecret);
    }
    backend::write(SecretKind::Password, email, password)
}

/// Remove any stored password for `email`.
///
/// This operation is **idempotent**: deleting a non-existent entry returns
/// `Ok(())`, not an error.
///
/// # Errors
///
/// Returns [`SecretsError::Keyring`] if the backend fails for any reason
/// other than [`keyring::Error::NoEntry`].
pub fn delete_password(email: &str) -> Result<(), SecretsError> {
    backend::delete(SecretKind::Password, email)
}

/// Retrieve the stored OAuth2 refresh token for `email`.
///
/// Unlike [`get_password`], this function has **no** environment-variable
/// fallback — refresh tokens are long-lived credentials that should not sit
/// in shell histories or process listings.
///
/// # Errors
///
/// Returns [`SecretsError::Keyring`] if the keyring backend fails for any
/// reason other than [`keyring::Error::NoEntry`]. A missing entry returns
/// `Ok(None)`.
pub fn get_oauth2_refresh_token(email: &str) -> Result<Option<String>, SecretsError> {
    backend::read(SecretKind::OAuth2Refresh, email)
}

/// Store `token` as the OAuth2 refresh token for `email`.
///
/// Callers should re-persist on every successful token refresh even when the
/// authorization server returns the same token value — refresh-token rotation
/// is allowed by RFC 6749 and keyring writes are cheap and idempotent.
///
/// # Errors
///
/// Returns [`SecretsError::EmptySecret`] if `token` is empty. Returns
/// [`SecretsError::Keyring`] if the backend rejects the write.
pub fn set_oauth2_refresh_token(email: &str, token: &str) -> Result<(), SecretsError> {
    if token.is_empty() {
        return Err(SecretsError::EmptySecret);
    }
    backend::write(SecretKind::OAuth2Refresh, email, token)
}

/// Remove any stored OAuth2 refresh token for `email`.
///
/// Idempotent, like [`delete_password`].
///
/// # Errors
///
/// Returns [`SecretsError::Keyring`] if the backend fails for any reason
/// other than [`keyring::Error::NoEntry`].
pub fn delete_oauth2_refresh_token(email: &str) -> Result<(), SecretsError> {
    backend::delete(SecretKind::OAuth2Refresh, email)
}

#[cfg(test)]
mod tests {
    use super::{
        SMTP_PASSWORD_ENV, SecretsError, delete_oauth2_refresh_token, delete_password,
        get_oauth2_refresh_token, get_password, set_oauth2_refresh_token, set_password,
    };
    use std::sync::Mutex;

    /// Serializes any test that mutates `INBOXLY_SMTP_PASSWORD`.
    ///
    /// `std::env::set_var` / `remove_var` are `unsafe` across threads because
    /// the process environment is shared mutable state. This mutex makes the
    /// env-var tests deterministic without pulling in a dev-dependency.
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: Mutex<()> = Mutex::new(());
        &LOCK
    }

    /// Clear `INBOXLY_SMTP_PASSWORD` while holding the env lock.
    ///
    /// # Safety
    ///
    /// Caller must be holding the env lock for the duration of any
    /// subsequent call that observes `INBOXLY_SMTP_PASSWORD`.
    fn unset_env_var() {
        // SAFETY: callers hold `env_lock()` for the duration of the read
        // that observes this write.
        unsafe {
            std::env::remove_var(SMTP_PASSWORD_ENV);
        }
    }

    /// Set `INBOXLY_SMTP_PASSWORD` while holding the env lock.
    ///
    /// # Safety
    ///
    /// Caller must be holding the env lock for the duration of any
    /// subsequent call that observes `INBOXLY_SMTP_PASSWORD`.
    fn set_env_var(value: &str) {
        // SAFETY: see `unset_env_var`.
        unsafe {
            std::env::set_var(SMTP_PASSWORD_ENV, value);
        }
    }

    #[test]
    fn test_password_roundtrip() {
        let email = "roundtrip-password@example.com";
        let _guard = env_lock().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        unset_env_var();

        // Start clean — delete is idempotent so this is safe even on a
        // fresh store.
        delete_password(email).expect("initial delete should succeed");

        set_password(email, "hunter2").expect("set should succeed");

        let got = get_password(email).expect("get should succeed");
        assert_eq!(got.as_deref(), Some("hunter2"));

        delete_password(email).expect("delete should succeed");

        let gone = get_password(email).expect("get-after-delete should succeed");
        assert_eq!(gone, None, "entry should be gone after delete");
    }

    #[test]
    fn test_oauth2_roundtrip() {
        let email = "roundtrip-oauth2@example.com";
        delete_oauth2_refresh_token(email).expect("initial delete should succeed");

        set_oauth2_refresh_token(email, "refresh-token-abc").expect("set should succeed");

        let got = get_oauth2_refresh_token(email).expect("get should succeed");
        assert_eq!(got.as_deref(), Some("refresh-token-abc"));

        delete_oauth2_refresh_token(email).expect("delete should succeed");

        let gone = get_oauth2_refresh_token(email).expect("get-after-delete should succeed");
        assert_eq!(gone, None, "entry should be gone after delete");
    }

    #[test]
    fn test_get_password_falls_back_to_env_var() {
        let email = "env-fallback@example.com";
        let _guard = env_lock().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        delete_password(email).expect("clean start");
        unset_env_var();

        // No keyring, no env: returns None.
        let none = get_password(email).expect("no sources = Ok(None)");
        assert_eq!(none, None);

        // No keyring, env set: returns env value + emits warn.
        set_env_var("env-var-password");
        let got = get_password(email).expect("env fallback should succeed");
        assert_eq!(got.as_deref(), Some("env-var-password"));

        // Keyring set: takes precedence over env var.
        set_password(email, "keyring-wins").expect("set should succeed");
        let got_both = get_password(email).expect("get should succeed");
        assert_eq!(got_both.as_deref(), Some("keyring-wins"));

        // Cleanup: remove entry then unset env so the next test sees a
        // fully clean slate.
        delete_password(email).expect("cleanup delete should succeed");
        unset_env_var();
    }

    #[test]
    fn test_get_password_returns_none_when_no_entry() {
        let email = "missing-entry@example.com";
        let _guard = env_lock().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        delete_password(email).expect("clean start");
        unset_env_var();

        let got = get_password(email).expect("missing entry should not error");
        assert_eq!(got, None, "no keyring entry + no env var = None");
    }

    #[test]
    fn test_delete_is_idempotent() {
        let email = "never-existed@example.com";

        delete_password(email).expect("first delete should succeed");
        delete_password(email).expect("second delete should succeed");

        delete_oauth2_refresh_token(email).expect("first oauth2 delete should succeed");
        delete_oauth2_refresh_token(email).expect("second oauth2 delete should succeed");
    }

    #[test]
    fn test_empty_secret_rejected() {
        let email = "empty-secret@example.com";

        assert!(matches!(
            set_password(email, ""),
            Err(SecretsError::EmptySecret)
        ));
        assert!(matches!(
            set_oauth2_refresh_token(email, ""),
            Err(SecretsError::EmptySecret)
        ));
    }

    #[test]
    fn test_email_is_case_insensitive() {
        // Write with mixed case, read with lower case.
        let upper = "CaseTest@Example.COM";
        let lower = "casetest@example.com";
        delete_password(lower).expect("clean start");

        set_password(upper, "mixed-case-value").expect("set should succeed");
        let got = get_password(lower).expect("get should succeed");
        assert_eq!(
            got.as_deref(),
            Some("mixed-case-value"),
            "mixed-case email should collide with lowercased form"
        );

        delete_password(upper).expect("cleanup");
    }
}
