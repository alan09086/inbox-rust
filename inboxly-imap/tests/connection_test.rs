use inboxly_imap::connection::parse_capabilities;

#[test]
fn parse_capabilities_detects_condstore() {
    let raw = vec![
        "IMAP4rev1".to_string(),
        "IDLE".to_string(),
        "CONDSTORE".to_string(),
        "SPECIAL-USE".to_string(),
        "XOAUTH2".to_string(),
    ];
    let caps = parse_capabilities(&raw);
    assert!(caps.condstore);
    assert!(caps.idle);
    assert!(caps.special_use);
    assert!(caps.xoauth2);
    assert!(!caps.compress_deflate);
}

#[test]
fn parse_capabilities_handles_empty() {
    let raw: Vec<String> = vec![];
    let caps = parse_capabilities(&raw);
    assert!(!caps.condstore);
    assert!(!caps.idle);
    assert!(!caps.special_use);
    assert!(!caps.xoauth2);
}
