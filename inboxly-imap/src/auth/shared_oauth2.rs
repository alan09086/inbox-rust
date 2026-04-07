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

use std::sync::Arc;

use tokio::sync::Mutex;

use super::oauth2::{GmailOAuth2Config, OAuth2Token, refresh_token};
use crate::error::{ImapError, Result};

/// A shared OAuth2 state combining the stateless config with a
/// mutex-guarded cached token.
#[derive(Debug)]
pub struct SharedOAuth2State {
    config: GmailOAuth2Config,
    cached_token: Mutex<Option<OAuth2Token>>,
}

impl SharedOAuth2State {
    /// Create a new shared state with no cached token yet.
    #[must_use]
    pub fn new(config: GmailOAuth2Config) -> Self {
        Self {
            config,
            cached_token: Mutex::new(None),
        }
    }

    /// Create a shared state seeded with a refresh token (no access token yet).
    ///
    /// The first call to [`Self::get_valid_access_token`] will perform an
    /// initial refresh to populate the access token.
    #[must_use]
    pub fn with_refresh_token(config: GmailOAuth2Config, refresh_token_str: String) -> Self {
        Self {
            config,
            cached_token: Mutex::new(Some(OAuth2Token {
                access_token: String::new(),
                refresh_token: Some(refresh_token_str),
                expires_at: None,
            })),
        }
    }

    /// Return a bare valid access token, refreshing from the refresh token if
    /// the cached access token is expired or missing.
    ///
    /// **Returns a String** rather than a borrowed reference so the caller
    /// doesn't hold the mutex across an `.await` boundary during the send.
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
        let access_token = new_token.access_token.clone();
        *guard = Some(new_token);
        Ok(access_token)
    }
}

/// Convenience alias for the shared-arc form plumbed through the app.
pub type SharedOAuth2 = Arc<SharedOAuth2State>;
