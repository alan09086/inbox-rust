use async_imap::Authenticator;
use tracing::info;

/// Credentials for XOAUTH2 SASL authentication.
pub struct XOAuth2Credentials {
    pub email: String,
    pub access_token: String,
}

impl std::fmt::Debug for XOAuth2Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XOAuth2Credentials")
            .field("email", &self.email)
            .field("access_token", &"[REDACTED]")
            .finish()
    }
}

/// Build the raw XOAUTH2 SASL string (before base64 encoding).
///
/// Format per Google spec:
/// `user=<email>\x01auth=Bearer <token>\x01\x01`
///
/// Where `\x01` is the SOH (Start of Heading) control character.
///
/// Reference: <https://developers.google.com/workspace/gmail/imap/xoauth2-protocol>
pub fn build_xoauth2_string(email: &str, access_token: &str) -> String {
    format!("user={email}\x01auth=Bearer {access_token}\x01\x01")
}

/// SASL authenticator for the XOAUTH2 mechanism.
///
/// Implements `async_imap::Authenticator` so it can be passed to
/// `Client::authenticate("XOAUTH2", &authenticator)`.
pub struct XOAuth2Authenticator {
    response: String,
}

impl XOAuth2Authenticator {
    pub fn new(email: &str, access_token: &str) -> Self {
        Self {
            response: build_xoauth2_string(email, access_token),
        }
    }
}

impl Authenticator for XOAuth2Authenticator {
    type Response = String;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        // XOAUTH2 sends the auth string as the initial response.
        // The challenge from the server is ignored (it's just the
        // continuation prompt).
        info!("Sending XOAUTH2 SASL response");
        self.response.clone()
    }
}

/// Authenticate an IMAP connection using XOAUTH2 SASL.
///
/// Consumes the `ImapConnection`, returns an authenticated `Session`.
pub async fn authenticate_xoauth2(
    connection: crate::connection::ImapConnection,
    creds: &XOAuth2Credentials,
) -> crate::error::Result<async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>>
{
    info!(email = %creds.email, host = %connection.host, "Authenticating via XOAUTH2");

    let auth = XOAuth2Authenticator::new(&creds.email, &creds.access_token);

    let session = connection
        .client
        .authenticate("XOAUTH2", auth)
        .await
        .map_err(|(err, _client)| crate::error::ImapError::AuthFailed {
            reason: format!("XOAUTH2 authentication failed: {err}"),
        })?;

    info!(email = %creds.email, "XOAUTH2 authentication successful");
    Ok(session)
}
