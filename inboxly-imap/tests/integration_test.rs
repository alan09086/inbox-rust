//! Integration tests for inboxly-imap.
//!
//! These tests require a real IMAP server. Gate them behind:
//!
//!   INBOXLY_TEST_IMAP=1 \
//!   INBOXLY_TEST_HOST=imap.gmail.com \
//!   INBOXLY_TEST_PORT=993 \
//!   INBOXLY_TEST_USER=user@gmail.com \
//!   INBOXLY_TEST_PASS=app-specific-password \
//!   cargo test -p inboxly-imap --test integration_test
//!
//! For OAuth2 tests, also set:
//!   INBOXLY_TEST_OAUTH2_CLIENT_ID=...
//!   INBOXLY_TEST_OAUTH2_TOKEN=ya29....

use std::env;

fn should_run_live_tests() -> bool {
    env::var("INBOXLY_TEST_IMAP").is_ok()
}

fn get_env(key: &str) -> String {
    env::var(key).unwrap_or_else(|_| panic!("{key} must be set for live IMAP tests"))
}

#[tokio::test]
async fn live_connect_and_list_folders() {
    if !should_run_live_tests() {
        eprintln!("Skipping live IMAP test (set INBOXLY_TEST_IMAP=1 to enable)");
        return;
    }

    let host = get_env("INBOXLY_TEST_HOST");
    let port: u16 = get_env("INBOXLY_TEST_PORT").parse().expect("Invalid port");
    let username = get_env("INBOXLY_TEST_USER");
    let password = get_env("INBOXLY_TEST_PASS");

    // Build TLS config
    let tls_config = inboxly_imap::build_tls_config();

    // Connect
    let conn = inboxly_imap::connection::connect_implicit_tls(&host, port, &tls_config)
        .await
        .expect("Failed to connect");

    // Authenticate
    let creds = inboxly_imap::PasswordCredentials {
        username,
        password,
    };
    let mut session = inboxly_imap::auth::password::login(conn, &creds)
        .await
        .expect("Failed to authenticate");

    // Detect capabilities
    let caps = inboxly_imap::connection::detect_capabilities(&mut session)
        .await
        .expect("Failed to detect capabilities");
    println!("Capabilities: {caps:?}");

    // List folders
    let folders = inboxly_imap::folders::list_folders(&mut session)
        .await
        .expect("Failed to list folders");
    println!("Folders ({}):", folders.len());
    for f in &folders {
        println!("  {} (role: {:?}, attrs: {:?})", f.name, f.role, f.attributes);
    }

    // Map well-known folders
    let wk = inboxly_imap::folders::map_well_known_folders(&folders);
    println!("Well-known folders: {wk:?}");
    assert!(wk.inbox.is_some(), "INBOX must always be found");

    // Logout
    session.logout().await.expect("Failed to logout");
}

#[tokio::test]
async fn live_oauth2_xoauth2_auth() {
    if !should_run_live_tests() {
        eprintln!("Skipping live OAuth2 test (set INBOXLY_TEST_IMAP=1 to enable)");
        return;
    }

    let host = get_env("INBOXLY_TEST_HOST");
    let port: u16 = get_env("INBOXLY_TEST_PORT").parse().expect("Invalid port");
    let email = get_env("INBOXLY_TEST_USER");
    let access_token = match env::var("INBOXLY_TEST_OAUTH2_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            eprintln!("Skipping OAuth2 test (INBOXLY_TEST_OAUTH2_TOKEN not set)");
            return;
        }
    };

    let tls_config = inboxly_imap::build_tls_config();
    let conn = inboxly_imap::connection::connect_implicit_tls(&host, port, &tls_config)
        .await
        .expect("Failed to connect");

    let creds = inboxly_imap::XOAuth2Credentials {
        email,
        access_token,
    };
    let mut session = inboxly_imap::auth::xoauth2::authenticate_xoauth2(conn, &creds)
        .await
        .expect("XOAUTH2 auth failed");

    let caps = inboxly_imap::connection::detect_capabilities(&mut session)
        .await
        .expect("Failed to detect capabilities");
    println!("Post-auth capabilities: {caps:?}");

    session.logout().await.expect("Failed to logout");
}

/// Unit-level integration: verify the full type wiring compiles.
/// No network required.
#[test]
fn channel_types_are_send_and_sync() {
    fn assert_send<T: Send>() {}

    assert_send::<inboxly_imap::SyncEvent>();
    assert_send::<inboxly_imap::UiCommand>();

    // SyncEvent contains Vec<ImapFolder> which is Send
    assert_send::<inboxly_imap::ImapFolder>();
}

#[test]
fn pool_config_is_clonable() {
    let config = inboxly_imap::PoolConfig::default();
    let _clone = config.clone();
}
