use async_imap::Session;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tracing::info;

use crate::connection::ImapConnection;
use crate::error::{ImapError, Result};

/// Credentials for IMAP LOGIN authentication.
///
/// Used for generic IMAP providers, Fastmail app-specific passwords, etc.
pub struct PasswordCredentials {
    pub username: String,
    pub password: String,
}

// Custom Debug to redact password
impl std::fmt::Debug for PasswordCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordCredentials")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

/// Authenticate via IMAP LOGIN command.
///
/// Consumes the `ImapConnection` and returns an authenticated `Session`.
pub async fn login(
    connection: ImapConnection,
    creds: &PasswordCredentials,
) -> Result<Session<TlsStream<TcpStream>>> {
    info!(username = %creds.username, host = %connection.host, "Authenticating via LOGIN");

    let session = connection
        .client
        .login(&creds.username, &creds.password)
        .await
        .map_err(|(err, _client)| ImapError::AuthFailed {
            reason: format!("LOGIN failed: {err}"),
        })?;

    info!(username = %creds.username, "LOGIN authentication successful");
    Ok(session)
}
