pub mod oauth2;
pub mod password;
pub mod shared_oauth2;
pub mod xoauth2;

pub use oauth2::{GmailOAuth2Config, OAuth2Token};
pub use password::PasswordCredentials;
pub use shared_oauth2::{PersistCallback, SharedOAuth2, SharedOAuth2State};
pub use xoauth2::XOAuth2Credentials;

use async_imap::Session;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tracing::info;

use crate::connection::ImapConnection;
use crate::error::{ImapError, Result};

/// Credentials required for OAuth2 authentication.
pub struct OAuth2AuthParams {
    /// OAuth2 client ID.
    pub client_id: String,
    /// OAuth2 client secret (optional for public clients).
    pub client_secret: Option<String>,
    /// Previously acquired token (may be expired or absent).
    pub token_cache: Option<OAuth2Token>,
}

/// Authenticate an IMAP connection using the method specified by the account.
///
/// Dispatches to the appropriate auth path:
/// - `AuthMethod::Password` / `AuthMethod::AppPassword` → LOGIN
/// - `AuthMethod::OAuth2` → XOAUTH2 SASL (acquires/refreshes token as needed)
///
/// # Arguments
/// - `connection`: Unauthenticated IMAP connection to authenticate.
/// - `auth_method`: The authentication method from account configuration.
/// - `email`: The email address (used as SASL username for XOAUTH2).
/// - `password_creds`: Credentials for PASSWORD / APP_PASSWORD auth.
/// - `oauth2_params`: Params for OAuth2 auth (client ID/secret + token cache).
pub async fn authenticate(
    connection: ImapConnection,
    auth_method: &inboxly_core::AuthMethod,
    email: &str,
    password_creds: Option<PasswordCredentials>,
    oauth2_params: Option<OAuth2AuthParams>,
) -> Result<Session<TlsStream<TcpStream>>> {
    match auth_method {
        inboxly_core::AuthMethod::Password | inboxly_core::AuthMethod::AppPassword => {
            let creds = password_creds.ok_or_else(|| ImapError::AuthFailed {
                reason: "Password credentials required for LOGIN auth".to_string(),
            })?;
            info!(method = "LOGIN", username = %creds.username, "Authenticating");
            password::login(connection, &creds).await
        }

        inboxly_core::AuthMethod::OAuth2 => {
            let params = oauth2_params.ok_or_else(|| ImapError::AuthFailed {
                reason: "OAuth2 params required for XOAUTH2 auth".to_string(),
            })?;

            info!(method = "XOAUTH2", email = %email, "Authenticating");

            // Use cached token if valid, otherwise acquire/refresh
            let token = match params.token_cache {
                Some(ref t) if !t.is_expired() => t.clone(),
                Some(ref t) if t.refresh_token.is_some() => {
                    info!("Token expired, refreshing");
                    let config = GmailOAuth2Config::new(
                        params.client_id.clone(),
                        params.client_secret.clone(),
                    );
                    oauth2::refresh_token(&config, t.refresh_token.as_ref().unwrap()).await?
                }
                _ => {
                    info!("No valid token, starting OAuth2 authorization flow");
                    let config = GmailOAuth2Config::new(
                        params.client_id.clone(),
                        params.client_secret.clone(),
                    );
                    oauth2::authorize(&config).await?
                }
            };

            let creds = XOAuth2Credentials {
                email: email.to_string(),
                access_token: token.access_token.clone(),
            };
            xoauth2::authenticate_xoauth2(connection, &creds).await
        }
    }
}
