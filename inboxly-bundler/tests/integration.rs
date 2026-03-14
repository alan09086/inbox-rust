//! Integration tests for the bundler crate.
//!
//! These tests use an in-memory SQLite store and fixture email data
//! to verify the full categorisation pipeline.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};

use inboxly_bundler::{Bundler, system_bundles};
use inboxly_core::{BundleCategory, Contact, EmailMeta};
use inboxly_store::{AccountRow, EmailRow, Store, ThreadRow, ThreadStateRow};

/// Global counter for unique IMAP UIDs in tests.
static NEXT_UID: AtomicI64 = AtomicI64::new(1);

/// Create an in-memory store for testing.
fn test_store() -> Store {
    Store::open_in_memory().expect("failed to create in-memory store")
}

/// Insert a test account and return its ID.
fn insert_test_account(store: &Store) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    store
        .insert_account(&AccountRow {
            id: id.clone(),
            email: "test@example.com".to_string(),
            display_name: "Test User".to_string(),
            provider: "imap".to_string(),
            auth_method: "password".to_string(),
            imap_host: "imap.example.com".to_string(),
            imap_port: 993,
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
        })
        .expect("insert account");
    id
}

/// Insert a test thread and its thread_state, then an email row.
/// Returns the thread ID.
fn insert_test_email(store: &Store, account_id: &str, from_address: &str, subject: &str) -> String {
    let thread_id = uuid::Uuid::new_v4().to_string();
    let email_id = format!("<{}@test>", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().timestamp();

    // Insert thread first (FK target for thread_state and emails)
    store
        .insert_thread(&ThreadRow {
            id: thread_id.clone(),
            account_id: account_id.to_string(),
            subject: subject.to_string(),
            newest_date: now,
            oldest_date: now,
            email_count: 1,
            unread_count: 1,
            has_attachments: false,
            snippet: subject.to_string(),
            root_message_id: None,
        })
        .expect("insert thread");

    // Insert thread_state (so get_uncategorised_thread_ids finds it)
    store
        .insert_thread_state(&ThreadStateRow {
            thread_id: thread_id.clone(),
            pinned: false,
            done: false,
            snoozed_until: None,
            snoozed_location_json: None,
            bundle_id: None,
        })
        .expect("insert thread_state");

    // Insert the email
    store
        .insert_email(&EmailRow {
            id: email_id,
            account_id: account_id.to_string(),
            thread_id: thread_id.clone(),
            from_name: None,
            from_address: from_address.to_string(),
            to_json: "[]".to_string(),
            cc_json: "[]".to_string(),
            subject: subject.to_string(),
            snippet: subject.to_string(),
            date: now,
            maildir_path: String::new(),
            flags: 0,
            size_bytes: 100,
            imap_uid: NEXT_UID.fetch_add(1, Ordering::Relaxed),
            imap_folder: "INBOX".to_string(),
            has_attachments: false,
            body_downloaded: false,
            message_id_header: None,
            in_reply_to: None,
            references_json: None,
        })
        .expect("insert email");

    thread_id
}

/// Create a test EmailMeta with the given from address and subject.
fn fixture_email(from_addr: &str, subject: &str) -> EmailMeta {
    let mut meta = EmailMeta::test_default();
    meta.from = Contact::new("", from_addr);
    meta.subject = subject.to_string();
    meta
}

// ----- System Bundles Tests -----

#[test]
fn ensure_system_bundles_creates_all_eight() {
    let store = test_store();
    let ids = system_bundles::ensure_system_bundles(&store).expect("ensure bundles");
    assert_eq!(ids.len(), 8);

    // Verify each bundle exists in the store
    let all = store.list_bundles().expect("list");
    assert_eq!(all.len(), 8);
}

#[test]
fn ensure_system_bundles_is_idempotent() {
    let store = test_store();

    let ids1 = system_bundles::ensure_system_bundles(&store).expect("first call");
    let ids2 = system_bundles::ensure_system_bundles(&store).expect("second call");

    assert_eq!(ids1, ids2, "bundle IDs should be identical across calls");

    // Should still only have 8 bundles total
    let all = store.list_bundles().expect("list");
    assert_eq!(all.len(), 8);
}

// ----- Bundler::categorise Tests -----

#[test]
fn categorise_facebook_email_as_social() {
    let bundler = Bundler::new(None).expect("create bundler");
    let email = fixture_email("notification@facebookmail.com", "You have a new message");
    let headers = HashMap::new();

    let result = bundler.categorise(&email, &headers);
    assert!(result.is_some());
    let (category, _bundle_id) = result.expect("should categorise");
    assert_eq!(category, BundleCategory::Social);
}

#[test]
fn categorise_mailchimp_as_promos() {
    let bundler = Bundler::new(None).expect("create bundler");
    let email = fixture_email("deals@store.com", "Weekly newsletter");
    let mut headers = HashMap::new();
    headers.insert("X-Mailer".to_string(), "Mailchimp v3.0".to_string());

    let result = bundler.categorise(&email, &headers);
    assert!(result.is_some());
    let (category, _) = result.expect("should categorise");
    assert_eq!(category, BundleCategory::Promos);
}

#[test]
fn categorise_mailing_list_as_forums() {
    let bundler = Bundler::new(None).expect("create bundler");
    let email = fixture_email("user@lists.example.com", "Re: RFC discussion");
    let mut headers = HashMap::new();
    headers.insert("List-Id".to_string(), "<dev.lists.example.com>".to_string());

    let result = bundler.categorise(&email, &headers);
    assert!(result.is_some());
    let (category, _) = result.expect("should categorise");
    assert_eq!(category, BundleCategory::Forums);
}

#[test]
fn categorise_personal_email_returns_none() {
    let bundler = Bundler::new(None).expect("create bundler");
    let email = fixture_email("friend@gmail.com", "Hey, want to grab coffee?");
    let headers = HashMap::new();

    let result = bundler.categorise(&email, &headers);
    assert!(
        result.is_none(),
        "personal emails should not be categorised"
    );
}

// ----- Bundler::categorise_all Tests -----

#[test]
fn categorise_all_assigns_bundles_in_store() {
    let store = test_store();
    let bundler = Bundler::new(None).expect("create bundler");
    system_bundles::ensure_system_bundles(&store).expect("ensure bundles");

    let account_id = insert_test_account(&store);

    // Insert 3 threads with different senders
    let _github_tid = insert_test_email(
        &store,
        &account_id,
        "notifications@github.com",
        "New PR review requested",
    );
    let _amazon_tid = insert_test_email(
        &store,
        &account_id,
        "ship-confirm@ship.amazon.ca",
        "Your package has shipped",
    );
    let _personal_tid =
        insert_test_email(&store, &account_id, "friend@personal.com", "Dinner plans");

    // Headers are empty (no .eml files for in-memory test), but domain-based
    // rules should still match github.com (Social) and *.amazon.* (Purchases).
    let categorised = bundler.categorise_all(&store).expect("categorise all");

    // GitHub -> Social, Amazon -> Purchases, personal -> uncategorised
    assert_eq!(categorised, 2, "should categorise 2 out of 3 threads");
}

// ----- Bundler::categorise_thread Tests -----

#[test]
fn categorise_thread_writes_bundle_assignment() {
    let store = test_store();
    let bundler = Bundler::new(None).expect("create bundler");
    system_bundles::ensure_system_bundles(&store).expect("ensure bundles");

    let account_id = insert_test_account(&store);
    let thread_id = insert_test_email(
        &store,
        &account_id,
        "noreply@booking.com",
        "Your reservation",
    );

    let result = bundler
        .categorise_thread(&store, &thread_id)
        .expect("categorise thread");
    assert_eq!(result, Some(BundleCategory::Travel));

    // Verify it was persisted
    let state = store.get_thread_state(&thread_id).expect("get state");
    assert!(state.bundle_id.is_some(), "bundle_id should be set");
}

#[test]
fn categorise_thread_returns_none_for_personal() {
    let store = test_store();
    let bundler = Bundler::new(None).expect("create bundler");
    system_bundles::ensure_system_bundles(&store).expect("ensure bundles");

    let account_id = insert_test_account(&store);
    let thread_id = insert_test_email(
        &store,
        &account_id,
        "alice@personal.email",
        "Lunch tomorrow?",
    );

    let result = bundler
        .categorise_thread(&store, &thread_id)
        .expect("categorise thread");
    assert_eq!(result, None);

    // bundle_id should remain None
    let state = store.get_thread_state(&thread_id).expect("get state");
    assert!(state.bundle_id.is_none());
}

// ----- Bundler metadata Tests -----

#[test]
fn bundler_reports_rule_count() {
    let bundler = Bundler::new(None).expect("create bundler");
    assert!(
        bundler.rule_count() >= 20,
        "should have at least 20 default rules, got {}",
        bundler.rule_count()
    );
}

#[test]
fn all_system_categories_have_bundle_ids() {
    let bundler = Bundler::new(None).expect("create bundler");
    let categories = [
        BundleCategory::Social,
        BundleCategory::Promos,
        BundleCategory::Updates,
        BundleCategory::Finance,
        BundleCategory::Purchases,
        BundleCategory::Travel,
        BundleCategory::Forums,
        BundleCategory::LowPriority,
    ];

    for cat in &categories {
        assert!(
            bundler.bundle_id_for_category(cat).is_some(),
            "missing bundle ID for category {cat:?}"
        );
    }
}
