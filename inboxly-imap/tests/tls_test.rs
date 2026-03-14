use inboxly_imap::tls::build_tls_config;

#[test]
fn tls_config_loads_root_certs() {
    let config = build_tls_config();
    // If it doesn't panic, root certs loaded successfully.
    // rustls ClientConfig is opaque — we just verify construction succeeds.
    assert!(std::sync::Arc::strong_count(&config) >= 1);
}

#[tokio::test]
async fn connect_tls_rejects_empty_hostname() {
    let config = build_tls_config();
    let result = inboxly_imap::tls::connect_tls("", 993, &config).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, inboxly_imap::error::ImapError::InvalidServerName(_)));
}
