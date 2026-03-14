use chrono::Datelike;
use inboxly_imap::sync::envelope::{
    parse_envelope_date, decode_envelope_bytes, extract_address_string,
    extract_contacts_json, EnvelopeData,
};

#[test]
fn parse_rfc2822_date() {
    let raw = b"Mon, 10 Mar 2026 14:30:00 +0000";
    let dt = parse_envelope_date(raw).unwrap();
    // 2026-03-10 14:30:00 UTC = 1773153000
    assert_eq!(dt.timestamp(), 1773153000);
}

#[test]
fn parse_rfc2822_date_no_day_name() {
    let raw = b"10 Mar 2026 14:30:00 +0000";
    let dt = parse_envelope_date(raw).unwrap();
    assert_eq!(dt.year(), 2026);
    assert_eq!(dt.month(), 3);
}

#[test]
fn parse_invalid_date_returns_error() {
    let raw = b"not a date";
    let result = parse_envelope_date(raw);
    assert!(result.is_err());
}

#[test]
fn decode_utf8_bytes() {
    let raw = b"Hello World";
    assert_eq!(decode_envelope_bytes(raw), "Hello World");
}

#[test]
fn decode_empty_returns_empty() {
    assert_eq!(decode_envelope_bytes(b""), "");
}

#[test]
fn extract_single_address() {
    assert_eq!(
        extract_address_string(Some("Alan Gaudet"), Some("alan"), Some("example.com")),
        ("Alan Gaudet".to_string(), "alan@example.com".to_string())
    );
}

#[test]
fn extract_address_no_name() {
    let (name, addr) = extract_address_string(None, Some("info"), Some("shop.com"));
    assert_eq!(name, "");
    assert_eq!(addr, "info@shop.com");
}

#[test]
fn contacts_json_round_trip() {
    let json = extract_contacts_json(&[
        ("Alice".to_string(), "alice@a.com".to_string()),
        ("Bob".to_string(), "bob@b.com".to_string()),
    ]);
    assert!(json.contains("alice@a.com"));
    assert!(json.contains("Bob"));
}

#[test]
fn envelope_data_to_insert_params() {
    let data = EnvelopeData {
        message_id: "<abc@example.com>".to_string(),
        account_id: "acc-1".to_string(),
        imap_uid: 42,
        imap_folder: "INBOX".to_string(),
        from_name: "Sender".to_string(),
        from_address: "sender@example.com".to_string(),
        to_json: "[]".to_string(),
        cc_json: "[]".to_string(),
        subject: "Test Subject".to_string(),
        date_unix: 1773338200,
        size_bytes: 4096,
        flags: 0,
        in_reply_to: None,
        references_json: None,
    };
    assert_eq!(data.imap_uid, 42);
    assert_eq!(data.subject, "Test Subject");
}
