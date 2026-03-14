use inboxly_store::*;

fn test_store() -> Store {
    Store::open_in_memory().expect("failed to open in-memory store")
}

fn sample_account() -> AccountRow {
    AccountRow {
        id: "acct-001".into(),
        email: "alice@example.com".into(),
        display_name: "Alice".into(),
        provider: "generic".into(),
        auth_method: "password".into(),
        imap_host: "imap.example.com".into(),
        imap_port: 993,
        smtp_host: "smtp.example.com".into(),
        smtp_port: 587,
    }
}

fn sample_thread(account_id: &str) -> ThreadRow {
    ThreadRow {
        id: "thread-001".into(),
        account_id: account_id.into(),
        subject: "Hello World".into(),
        newest_date: 1710000000,
        oldest_date: 1710000000,
        email_count: 1,
        unread_count: 1,
        has_attachments: false,
        snippet: "Hey there...".into(),
    }
}

fn sample_email(account_id: &str, thread_id: &str) -> EmailRow {
    EmailRow {
        id: "<msg001@example.com>".into(),
        account_id: account_id.into(),
        thread_id: thread_id.into(),
        from_name: Some("Bob".into()),
        from_address: "bob@example.com".into(),
        to_json: r#"[{"address":"alice@example.com","name":"Alice"}]"#.into(),
        cc_json: "[]".into(),
        subject: "Hello World".into(),
        snippet: "Hey there...".into(),
        date: 1710000000,
        maildir_path: "/mail/cur/msg001:2,S".into(),
        flags: flags::READ,
        size_bytes: 4096,
        imap_uid: 42,
        imap_folder: "INBOX".into(),
        has_attachments: false,
        body_downloaded: false,
        message_id_header: Some("<msg001@example.com>".into()),
        in_reply_to: None,
        references_json: None,
    }
}

// === Account tests ===

#[test]
fn test_account_crud() {
    let store = test_store();
    let account = sample_account();

    store.insert_account(&account).unwrap();
    let fetched = store.get_account("acct-001").unwrap();
    assert_eq!(fetched.email, "alice@example.com");
    assert_eq!(fetched.imap_port, 993);

    let accounts = store.list_accounts().unwrap();
    assert_eq!(accounts.len(), 1);

    store.delete_account("acct-001").unwrap();
    assert!(store.get_account("acct-001").is_err());
}

#[test]
fn test_account_not_found() {
    let store = test_store();
    let result = store.get_account("nonexistent");
    assert!(matches!(result, Err(StoreError::NotFound(_))));
}

// === Email tests ===

#[test]
fn test_email_crud() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();
    store.insert_thread(&sample_thread("acct-001")).unwrap();

    let email = sample_email("acct-001", "thread-001");
    store.insert_email(&email).unwrap();

    let fetched = store.get_email("<msg001@example.com>").unwrap();
    assert_eq!(fetched.subject, "Hello World");
    assert_eq!(fetched.imap_uid, 42);

    // Test UID lookup
    let by_uid = store.get_email_by_uid("acct-001", "INBOX", 42).unwrap();
    assert!(by_uid.is_some());

    // Test flag update
    store.update_email_flags("<msg001@example.com>", flags::READ | flags::STARRED).unwrap();
    let updated = store.get_email("<msg001@example.com>").unwrap();
    assert_eq!(updated.flags, flags::READ | flags::STARRED);

    // Test thread reassignment
    let thread2 = ThreadRow { id: "thread-002".into(), ..sample_thread("acct-001") };
    store.insert_thread(&thread2).unwrap();
    store.update_email_thread("<msg001@example.com>", "thread-002").unwrap();
    let moved = store.get_email("<msg001@example.com>").unwrap();
    assert_eq!(moved.thread_id, "thread-002");

    store.delete_email("<msg001@example.com>").unwrap();
    assert!(store.get_email("<msg001@example.com>").is_err());
}

#[test]
fn test_email_unique_constraint() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();
    store.insert_thread(&sample_thread("acct-001")).unwrap();

    let email = sample_email("acct-001", "thread-001");
    store.insert_email(&email).unwrap();

    // Same account_id + imap_folder + imap_uid should fail
    let mut dup = email.clone();
    dup.id = "<msg002@example.com>".into();
    let result = store.insert_email(&dup);
    assert!(result.is_err());
}

// === Thread tests ===

#[test]
fn test_thread_crud() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();

    let thread = sample_thread("acct-001");
    store.insert_thread(&thread).unwrap();

    let fetched = store.get_thread("thread-001").unwrap();
    assert_eq!(fetched.subject, "Hello World");

    // Test upsert
    let mut updated = thread.clone();
    updated.email_count = 5;
    updated.snippet = "Updated snippet".into();
    store.upsert_thread(&updated).unwrap();
    let re_fetched = store.get_thread("thread-001").unwrap();
    assert_eq!(re_fetched.email_count, 5);
    assert_eq!(re_fetched.snippet, "Updated snippet");
}

// === Thread state tests ===

#[test]
fn test_thread_state_crud() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();
    store.insert_thread(&sample_thread("acct-001")).unwrap();

    let state = store.get_or_create_thread_state("thread-001").unwrap();
    assert!(!state.pinned);
    assert!(!state.done);

    store.set_thread_pinned("thread-001", true).unwrap();
    let pinned = store.get_thread_state("thread-001").unwrap();
    assert!(pinned.pinned);

    let pinned_list = store.get_pinned_threads().unwrap();
    assert_eq!(pinned_list.len(), 1);

    store.set_thread_done("thread-001", true).unwrap();
    // Done threads are excluded from pinned query
    let pinned_list = store.get_pinned_threads().unwrap();
    assert_eq!(pinned_list.len(), 0);
}

// === Sync state tests ===

#[test]
fn test_sync_state_upsert() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();

    let state = SyncStateRow {
        account_id: "acct-001".into(),
        folder_name: "INBOX".into(),
        uid_validity: Some(12345),
        uid_next: Some(100),
        highest_modseq: Some(999),
        last_sync: Some(1710000000),
    };
    store.upsert_sync_state(&state).unwrap();

    let fetched = store.get_sync_state("acct-001", "INBOX").unwrap().unwrap();
    assert_eq!(fetched.uid_next, Some(100));

    // Update via upsert
    let updated = SyncStateRow { uid_next: Some(200), ..state };
    store.upsert_sync_state(&updated).unwrap();
    let re_fetched = store.get_sync_state("acct-001", "INBOX").unwrap().unwrap();
    assert_eq!(re_fetched.uid_next, Some(200));
}

// === Contacts tests ===

#[test]
fn test_contact_upsert_and_search() {
    let store = test_store();
    let contact = ContactRow {
        address: "bob@example.com".into(),
        display_name: Some("Bob".into()),
        avatar_letter: Some("B".into()),
        avatar_color_index: 1,
        last_seen: 1710000000,
    };
    store.upsert_contact(&contact).unwrap();

    let fetched = store.get_contact("bob@example.com").unwrap().unwrap();
    assert_eq!(fetched.display_name, Some("Bob".into()));

    // Search
    let results = store.search_contacts("bob", 10).unwrap();
    assert_eq!(results.len(), 1);

    let results = store.search_contacts("xyz", 10).unwrap();
    assert!(results.is_empty());
}

// === Bundle tests ===

#[test]
fn test_bundle_crud() {
    let store = test_store();
    let bundle = BundleRow {
        id: "bundle-001".into(),
        category: "Social".into(),
        name: "Social".into(),
        color: "#d23f31".into(),
        badge_color: "#faebea".into(),
        visibility: "Bundled".into(),
        throttle: "Immediate".into(),
        sort_order: 0,
    };
    store.insert_bundle(&bundle).unwrap();

    let fetched = store.get_bundle("bundle-001").unwrap();
    assert_eq!(fetched.category, "Social");

    let by_cat = store.get_bundle_by_category("Social").unwrap();
    assert!(by_cat.is_some());

    let all = store.list_bundles().unwrap();
    assert_eq!(all.len(), 1);
}

// === Bundle rules tests ===

#[test]
fn test_bundle_rule_crud() {
    let store = test_store();
    let bundle = BundleRow {
        id: "bundle-001".into(),
        category: "Social".into(),
        name: "Social".into(),
        color: "#d23f31".into(),
        badge_color: "#faebea".into(),
        visibility: "Bundled".into(),
        throttle: "Immediate".into(),
        sort_order: 0,
    };
    store.insert_bundle(&bundle).unwrap();

    let rule = BundleRuleRow {
        id: "rule-001".into(),
        bundle_id: "bundle-001".into(),
        field: "From".into(),
        operator: "Domain".into(),
        value: "facebookmail.com".into(),
        priority: 10,
    };
    store.insert_bundle_rule(&rule).unwrap();

    let rules = store.get_rules_for_bundle("bundle-001").unwrap();
    assert_eq!(rules.len(), 1);

    let all_rules = store.get_all_bundle_rules().unwrap();
    assert_eq!(all_rules.len(), 1);
}

// === Sender affinity tests ===

#[test]
fn test_sender_affinity() {
    let store = test_store();
    let affinity = SenderAffinityRow {
        sender_domain: "example.com".into(),
        sender_address: "news@example.com".into(),
        bundle_category: "Promos".into(),
        confidence: 0.85,
        learned_at: 1710000000,
    };
    store.upsert_sender_affinity(&affinity).unwrap();

    let fetched = store.get_sender_affinity("news@example.com").unwrap().unwrap();
    assert_eq!(fetched.bundle_category, "Promos");
    assert!((fetched.confidence - 0.85).abs() < f64::EPSILON);

    let domain_results = store.get_affinities_by_domain("example.com").unwrap();
    assert_eq!(domain_results.len(), 1);
}

// === Reminder tests ===

#[test]
fn test_reminder_crud() {
    let store = test_store();
    let reminder = ReminderRow {
        id: "rem-001".into(),
        title: "Buy milk".into(),
        due_at: Some(1710100000),
        location_lat: None,
        location_lng: None,
        location_label: None,
        recurring: None,
        done: false,
    };
    store.insert_reminder(&reminder).unwrap();

    let active = store.get_active_reminders().unwrap();
    assert_eq!(active.len(), 1);

    let due = store.get_due_reminders(1710200000).unwrap();
    assert_eq!(due.len(), 1);

    let not_due = store.get_due_reminders(1710000000).unwrap();
    assert!(not_due.is_empty());

    store.set_reminder_done("rem-001", true).unwrap();
    let active = store.get_active_reminders().unwrap();
    assert!(active.is_empty());
}

// === Highlight tests ===

#[test]
fn test_highlight_crud() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();
    store.insert_thread(&sample_thread("acct-001")).unwrap();

    let highlight = HighlightRow {
        id: None,
        thread_id: "thread-001".into(),
        highlight_type: "TrackingNumber".into(),
        data_json: r#"{"carrier":"UPS","number":"1Z999AA10123456784"}"#.into(),
    };
    let id = store.insert_highlight(&highlight).unwrap();
    assert!(id > 0);

    let highlights = store.get_highlights_for_thread("thread-001").unwrap();
    assert_eq!(highlights.len(), 1);

    let by_type = store.get_highlights_by_type("TrackingNumber").unwrap();
    assert_eq!(by_type.len(), 1);

    let deleted = store.delete_highlights_for_thread("thread-001").unwrap();
    assert_eq!(deleted, 1);
}

// === Settings tests ===

#[test]
fn test_settings_crud() {
    let store = test_store();

    assert!(store.get_setting("theme").unwrap().is_none());

    store.set_setting("theme", "dark").unwrap();
    assert_eq!(store.get_setting("theme").unwrap().unwrap(), "dark");

    // Upsert
    store.set_setting("theme", "light").unwrap();
    assert_eq!(store.get_setting("theme").unwrap().unwrap(), "light");

    let all = store.get_all_settings().unwrap();
    assert_eq!(all.len(), 1);

    store.delete_setting("theme").unwrap();
    assert!(store.get_setting("theme").unwrap().is_none());
}

// === Offline queue tests ===

#[test]
fn test_offline_queue() {
    let store = test_store();

    let id1 = store.enqueue_offline_action("mark_done", r#"{"thread_id":"t1"}"#).unwrap();
    let id2 = store.enqueue_offline_action("pin", r#"{"thread_id":"t2"}"#).unwrap();
    assert!(id2 > id1);

    let queue = store.get_offline_queue().unwrap();
    assert_eq!(queue.len(), 2);
    assert_eq!(queue[0].action, "mark_done");
    assert_eq!(queue[1].action, "pin");

    store.dequeue_offline_action(id1).unwrap();
    assert_eq!(store.count_offline_queue().unwrap(), 1);

    store.clear_offline_queue().unwrap();
    assert_eq!(store.count_offline_queue().unwrap(), 0);
}

// === Transaction tests ===

#[test]
fn test_transaction_commit() {
    let mut store = test_store();
    store.insert_account(&sample_account()).unwrap();

    store.transaction(|conn| {
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet)
             VALUES ('tx-thread', 'acct-001', 'Transaction test', 0, 0, 0, 0, 0, '')",
            [],
        )?;
        Ok(())
    }).unwrap();

    let thread = store.get_thread("tx-thread").unwrap();
    assert_eq!(thread.subject, "Transaction test");
}

#[test]
fn test_transaction_rollback() {
    let mut store = test_store();
    store.insert_account(&sample_account()).unwrap();
    store.insert_thread(&sample_thread("acct-001")).unwrap();

    let result: std::result::Result<(), StoreError> = store.transaction(|conn| {
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet)
             VALUES ('tx-thread-2', 'acct-001', 'Will rollback', 0, 0, 0, 0, 0, '')",
            [],
        )?;
        // Force an error
        Err(StoreError::Constraint("intentional failure".into()))
    });
    assert!(result.is_err());

    // Thread should not exist
    assert!(store.get_thread("tx-thread-2").is_err());
}

// === Rebuild test ===

#[test]
fn test_rebuild() {
    let mut store = test_store();
    store.insert_account(&sample_account()).unwrap();
    store.set_setting("theme", "dark").unwrap();

    store.rebuild().unwrap();

    // All data gone
    assert!(store.list_accounts().unwrap().is_empty());
    assert!(store.get_setting("theme").unwrap().is_none());

    // But tables still exist — can insert again
    store.insert_account(&sample_account()).unwrap();
    assert_eq!(store.list_accounts().unwrap().len(), 1);
}
