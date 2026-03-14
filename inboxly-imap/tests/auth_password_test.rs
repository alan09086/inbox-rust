use inboxly_imap::auth::password::PasswordCredentials;

#[test]
fn password_credentials_stores_fields() {
    let creds = PasswordCredentials {
        username: "user@example.com".to_string(),
        password: "hunter2".to_string(),
    };
    assert_eq!(creds.username, "user@example.com");
    assert_eq!(creds.password, "hunter2");
}

#[test]
fn password_credentials_debug_redacts_password() {
    let creds = PasswordCredentials {
        username: "user@example.com".to_string(),
        password: "supersecret".to_string(),
    };
    let debug = format!("{creds:?}");
    assert!(!debug.contains("supersecret"), "Password must not appear in Debug output");
    assert!(debug.contains("user@example.com"));
    assert!(debug.contains("[REDACTED]"));
}
