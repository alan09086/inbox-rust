use std::net::TcpListener as StdTcpListener;
use std::time::{Duration, Instant};

use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    PkceCodeChallenge, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use tracing::{debug, info, warn};

use crate::error::{ImapError, Result};

/// Gmail-specific OAuth2 configuration.
#[derive(Debug, Clone)]
pub struct GmailOAuth2Config {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub auth_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
    pub redirect_port_range: (u16, u16),
}

impl GmailOAuth2Config {
    pub fn new(client_id: String, client_secret: Option<String>) -> Self {
        Self {
            client_id,
            client_secret,
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            scopes: vec!["https://mail.google.com/".to_string()],
            redirect_port_range: (8080, 8099),
        }
    }
}

/// A resolved OAuth2 token with expiry tracking.
#[derive(Debug, Clone)]
pub struct OAuth2Token {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<Instant>,
}

impl OAuth2Token {
    /// Returns `true` if the token has expired or will expire within 60 seconds.
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(expires_at) => Instant::now() + Duration::from_secs(60) >= expires_at,
            None => false, // No expiry info — assume valid
        }
    }
}

/// Find an available port in the given range for the loopback redirect server.
fn find_available_port(range: (u16, u16)) -> Result<u16> {
    for port in range.0..=range.1 {
        if StdTcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }
    Err(ImapError::OAuth2 {
        reason: format!("No available port in range {:?}", range),
    })
}

/// Run the full OAuth2 authorization code flow with PKCE.
///
/// 1. Starts a local HTTP listener on a loopback port.
/// 2. Generates a PKCE challenge.
/// 3. Builds the authorization URL and opens it in the user's browser.
/// 4. Waits for the redirect callback with the authorization code.
/// 5. Exchanges the code for tokens.
///
/// Returns the access token and optional refresh token.
pub async fn authorize(config: &GmailOAuth2Config) -> Result<OAuth2Token> {
    let port = find_available_port(config.redirect_port_range)?;
    info!(port, "Starting OAuth2 loopback server");

    let client_id = ClientId::new(config.client_id.clone());

    let auth_url = AuthUrl::new(config.auth_url.clone()).map_err(|e| ImapError::OAuth2 {
        reason: format!("Invalid auth URL: {e}"),
    })?;
    let token_url = TokenUrl::new(config.token_url.clone()).map_err(|e| ImapError::OAuth2 {
        reason: format!("Invalid token URL: {e}"),
    })?;
    let redirect_url =
        RedirectUrl::new(format!("http://127.0.0.1:{port}/callback")).map_err(|e| ImapError::OAuth2 {
            reason: format!("Invalid redirect URL: {e}"),
        })?;

    let client = {
        let base = BasicClient::new(client_id)
            .set_auth_uri(auth_url)
            .set_token_uri(token_url)
            .set_redirect_uri(redirect_url);
        match &config.client_secret {
            Some(s) => base.set_client_secret(ClientSecret::new(s.clone())),
            None => base,
        }
    };

    // Generate PKCE challenge
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    // Build authorization URL
    let mut auth_request = client.authorize_url(CsrfToken::new_random);

    for scope in &config.scopes {
        auth_request = auth_request.add_scope(Scope::new(scope.clone()));
    }

    let (auth_url, csrf_state) = auth_request
        .set_pkce_challenge(pkce_challenge)
        .url();

    info!("Opening browser for OAuth2 authorization");
    debug!(url = %auth_url, "Authorization URL");

    // Open browser
    if let Err(e) = open::that(auth_url.as_str()) {
        warn!("Failed to open browser: {e}. Please open this URL manually:\n{auth_url}");
    }

    // Start local HTTP server to capture the redirect
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| ImapError::OAuth2 {
            reason: format!("Failed to bind loopback server: {e}"),
        })?;

    let code = wait_for_callback(&listener, &csrf_state).await?;

    info!("Authorization code received, exchanging for tokens");

    // Exchange code for token
    let http_client = reqwest::Client::new();
    let token_response = client
        .exchange_code(code)
        .set_pkce_verifier(pkce_verifier)
        .request_async(&http_client)
        .await
        .map_err(|e| ImapError::OAuth2 {
            reason: format!("Token exchange failed: {e}"),
        })?;

    let expires_at = token_response.expires_in().map(|d| Instant::now() + d);

    let token = OAuth2Token {
        access_token: token_response.access_token().secret().clone(),
        refresh_token: token_response
            .refresh_token()
            .map(|t: &oauth2::RefreshToken| t.secret().clone()),
        expires_at,
    };

    info!("OAuth2 token acquired successfully");
    Ok(token)
}

/// Refresh an expired OAuth2 token using the refresh token.
pub async fn refresh_token(
    config: &GmailOAuth2Config,
    refresh_token_str: &str,
) -> Result<OAuth2Token> {
    let port = find_available_port(config.redirect_port_range)?;

    let client_id = ClientId::new(config.client_id.clone());
    let auth_url = AuthUrl::new(config.auth_url.clone()).map_err(|e| ImapError::OAuth2 {
        reason: format!("Invalid auth URL: {e}"),
    })?;
    let token_url = TokenUrl::new(config.token_url.clone()).map_err(|e| ImapError::OAuth2 {
        reason: format!("Invalid token URL: {e}"),
    })?;
    let redirect_url =
        RedirectUrl::new(format!("http://127.0.0.1:{port}/callback")).map_err(|e| ImapError::OAuth2 {
            reason: format!("Invalid redirect URL: {e}"),
        })?;

    let client = {
        let base = BasicClient::new(client_id)
            .set_auth_uri(auth_url)
            .set_token_uri(token_url)
            .set_redirect_uri(redirect_url);
        match &config.client_secret {
            Some(s) => base.set_client_secret(ClientSecret::new(s.clone())),
            None => base,
        }
    };

    let http_client = reqwest::Client::new();
    let token_response = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token_str.to_string()))
        .request_async(&http_client)
        .await
        .map_err(|e| ImapError::OAuth2 {
            reason: format!("Token refresh failed: {e}"),
        })?;

    let expires_at = token_response.expires_in().map(|d| Instant::now() + d);

    Ok(OAuth2Token {
        access_token: token_response.access_token().secret().clone(),
        refresh_token: token_response
            .refresh_token()
            .map(|t: &oauth2::RefreshToken| t.secret().clone())
            .or_else(|| Some(refresh_token_str.to_string())),
        expires_at,
    })
}

/// Wait for the OAuth2 redirect callback on the local HTTP server.
///
/// Parses the `code` and `state` query parameters from the redirect URL.
async fn wait_for_callback(
    listener: &tokio::net::TcpListener,
    expected_state: &CsrfToken,
) -> Result<AuthorizationCode> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let (mut stream, _addr) = listener.accept().await.map_err(|e| ImapError::OAuth2 {
        reason: format!("Failed to accept callback connection: {e}"),
    })?;

    let mut reader = BufReader::new(&mut stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .await
        .map_err(|e| ImapError::OAuth2 {
            reason: format!("Failed to read callback request: {e}"),
        })?;

    // Parse query parameters from "GET /callback?code=xxx&state=yyy HTTP/1.1"
    let url_part = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| ImapError::OAuth2 {
            reason: "Malformed callback request".to_string(),
        })?;

    let full_url = format!("http://127.0.0.1{url_part}");
    let parsed = url::Url::parse(&full_url).map_err(|e| ImapError::OAuth2 {
        reason: format!("Failed to parse callback URL: {e}"),
    })?;

    let mut code = None;
    let mut state = None;
    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            _ => {}
        }
    }

    // Verify CSRF state
    let received_state = state.ok_or_else(|| ImapError::OAuth2 {
        reason: "No state parameter in callback".to_string(),
    })?;

    if received_state != expected_state.secret().as_str() {
        return Err(ImapError::OAuth2 {
            reason: "CSRF state mismatch".to_string(),
        });
    }

    let code_str = code.ok_or_else(|| ImapError::OAuth2 {
        reason: "No authorization code in callback".to_string(),
    })?;

    // Send a success response to the browser
    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
        <html><body><h1>Authorization successful!</h1>\
        <p>You can close this tab and return to Inboxly.</p></body></html>";
    let _ = stream.write_all(response.as_bytes()).await;

    Ok(AuthorizationCode::new(code_str))
}
