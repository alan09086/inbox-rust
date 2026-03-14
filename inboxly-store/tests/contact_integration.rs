//! Integration test: contact extraction pipeline.
//!
//! Exercises the full flow: inserting emails, extracting contacts,
//! verifying avatar assignments and display name resolution.

use inboxly_core::contact::{AVATAR_PALETTE, AvatarColor, avatar_color_for_letter};
use inboxly_store::{AccountRow, EmailRow, Store};

fn test_store_with_account() -> Store {
    let store = Store::open_in_memory().expect("failed to open test store");

    let account = AccountRow {
        id: "acct-test".into(),
        email: "test@test.com".into(),
        display_name: "Test Account".into(),
        provider: "other".into(),
        auth_method: "password".into(),
        imap_host: "imap.test.com".into(),
        imap_port: 993,
        smtp_host: "smtp.test.com".into(),
        smtp_port: 587,
    };
    store.insert_account(&account).unwrap();

    store
}

fn insert_email(
    store: &Store,
    id: &str,
    from_name: Option<&str>,
    from_address: &str,
    to_json: &str,
    cc_json: &str,
    date: i64,
) {
    let email = EmailRow {
        id: id.into(),
        account_id: "acct-test".into(),
        thread_id: "thread-1".into(),
        from_name: from_name.map(String::from),
        from_address: from_address.into(),
        to_json: to_json.into(),
        cc_json: cc_json.into(),
        subject: "Test subject".into(),
        snippet: "Test snippet".into(),
        date,
        maildir_path: format!("/tmp/{id}"),
        flags: 0,
        size_bytes: 100,
        imap_uid: date, // use date as uid for uniqueness
        imap_folder: "INBOX".into(),
        has_attachments: false,
        body_downloaded: false,
        message_id_header: Some(format!("<{id}@test.com>")),
        in_reply_to: None,
        references_json: None,
    };
    store.insert_email(&email).unwrap();
}

#[test]
fn full_pipeline_extract_and_query() {
    let store = test_store_with_account();

    // Insert 3 emails
    insert_email(
        &store,
        "msg1",
        Some("Sarah Connor"),
        "sarah@skynet.com",
        r#"[{"name": "John Connor", "address": "john@resistance.net"}]"#,
        "[]",
        1000,
    );
    insert_email(
        &store,
        "msg2",
        Some("Kyle Reese"),
        "kyle@resistance.net",
        r#"[{"address": "sarah@skynet.com"}]"#,
        r#"[{"name": "Sarah C.", "address": "sarah@skynet.com"}]"#,
        2000,
    );
    insert_email(
        &store,
        "msg3",
        Some("Sarah C."),
        "sarah@skynet.com",
        r#"[{"name": "Kyle Reese", "address": "kyle@resistance.net"}]"#,
        "[]",
        3000,
    );

    store.backfill_contacts_from_emails().unwrap();

    // Verify contact count
    let all = store.list_all_contacts().unwrap();
    assert_eq!(all.len(), 3); // sarah, john, kyle

    // Sarah: most recent name is "Sarah C." from msg3 (date=3000)
    let sarah = store.get_contact("sarah@skynet.com").unwrap().unwrap();
    assert_eq!(sarah.display_name, Some("Sarah C.".to_string()));
    assert_eq!(sarah.avatar_letter, Some("S".to_string()));
    assert_eq!(sarah.avatar_color_index, 18); // S = index 18
    assert_eq!(sarah.last_seen, 3000);

    // John: only appeared in msg1
    let john = store.get_contact("john@resistance.net").unwrap().unwrap();
    assert_eq!(john.display_name, Some("John Connor".to_string()));
    assert_eq!(john.avatar_letter, Some("J".to_string()));
    assert_eq!(john.avatar_color_index, 9); // J = index 9

    // Kyle: appeared in msg2 (from) and msg3 (to)
    let kyle = store.get_contact("kyle@resistance.net").unwrap().unwrap();
    assert_eq!(kyle.display_name, Some("Kyle Reese".to_string()));
    assert_eq!(kyle.avatar_letter, Some("K".to_string()));
}

#[test]
fn every_letter_maps_to_unique_colour() {
    // Verify that A-Z all produce valid palette entries
    for (i, ch) in ('A'..='Z').enumerate() {
        let color = avatar_color_for_letter(ch);
        assert_eq!(color, AVATAR_PALETTE[i], "Mismatch for letter {ch}");
        // Ensure not the default colour (each letter is distinct)
        assert_ne!(
            color,
            AvatarColor::new(0xef, 0xef, 0xef),
            "Letter {ch} should not map to default"
        );
    }
}

#[test]
fn contact_header_extraction_deduplicates() {
    let store = test_store_with_account();

    // Same person in From and To
    store
        .extract_contacts_from_headers(
            "Alice <alice@a.com>",
            Some("alice@a.com, bob@b.com"),
            None,
            1000,
        )
        .unwrap();

    let all = store.list_all_contacts().unwrap();
    // alice appears twice (from + to) but should only have 1 entry
    assert_eq!(all.len(), 2); // alice + bob

    let alice = store.get_contact("alice@a.com").unwrap().unwrap();
    // The From header had the name "Alice", the To was bare — should keep "Alice"
    assert_eq!(alice.display_name, Some("Alice".to_string()));
}
