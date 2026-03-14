use inboxly_imap::auth::xoauth2::{XOAuth2Credentials, build_xoauth2_string};

#[test]
fn xoauth2_string_format() {
    let result = build_xoauth2_string("user@gmail.com", "ya29.test-token");
    // Format: user=<email>\x01auth=Bearer <token>\x01\x01
    let expected = "user=user@gmail.com\x01auth=Bearer ya29.test-token\x01\x01";
    assert_eq!(result, expected);
}

#[test]
fn xoauth2_credentials_debug_redacts_token() {
    let creds = XOAuth2Credentials {
        email: "user@gmail.com".to_string(),
        access_token: "ya29.secret-token".to_string(),
    };
    let debug = format!("{creds:?}");
    assert!(!debug.contains("ya29.secret-token"));
    assert!(debug.contains("user@gmail.com"));
    assert!(debug.contains("[REDACTED]"));
}

#[test]
fn xoauth2_string_handles_special_chars_in_email() {
    let result = build_xoauth2_string("user+tag@gmail.com", "token");
    assert!(result.starts_with("user=user+tag@gmail.com\x01"));
}
