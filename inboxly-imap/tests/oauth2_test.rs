use inboxly_imap::auth::oauth2::{GmailOAuth2Config, OAuth2Token};

#[test]
fn gmail_oauth2_config_has_correct_endpoints() {
    let config = GmailOAuth2Config::new(
        "test-client-id".to_string(),
        Some("test-client-secret".to_string()),
    );
    assert_eq!(config.auth_url, "https://accounts.google.com/o/oauth2/v2/auth");
    assert_eq!(config.token_url, "https://oauth2.googleapis.com/token");
    assert!(config.scopes.contains(&"https://mail.google.com/".to_string()));
}

#[test]
fn oauth2_token_detects_expiry() {
    use std::time::{Duration, Instant};

    // Token that expires in 120 seconds (well outside the 60s buffer)
    let token = OAuth2Token {
        access_token: "ya29.test".to_string(),
        refresh_token: Some("1//test-refresh".to_string()),
        expires_at: Some(Instant::now() + Duration::from_secs(120)),
    };
    assert!(!token.is_expired());

    // Token that already expired
    let expired = OAuth2Token {
        access_token: "ya29.old".to_string(),
        refresh_token: Some("1//old-refresh".to_string()),
        expires_at: Some(Instant::now() - Duration::from_secs(60)),
    };
    assert!(expired.is_expired());

    // Token with no expiry (treat as not expired)
    let no_expiry = OAuth2Token {
        access_token: "ya29.forever".to_string(),
        refresh_token: None,
        expires_at: None,
    };
    assert!(!no_expiry.is_expired());
}
