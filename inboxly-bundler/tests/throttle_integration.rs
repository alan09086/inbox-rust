//! Integration tests for bundle throttling end-to-end behaviour.
//!
//! These tests verify the full throttle lifecycle:
//! 1. Email arrives -> assigned to throttled bundle -> suppressed from feed
//! 2. Delivery window opens -> scheduler emits event -> feed refresh shows email
//! 3. User changes throttle setting -> takes effect immediately
//! 4. Multiple throttled bundles operate independently

use chrono::{Local, NaiveTime};
use inboxly_bundler::system_bundles;
use inboxly_core::throttle::{BundleThrottle, WeekdayWrapper};
use inboxly_core::BundleId;
use inboxly_store::{AccountRow, Store, ThreadRow, ThreadStateRow};

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

/// Insert a test bundle with a specific throttle and return its ID.
fn insert_throttled_bundle(store: &Store, name: &str, throttle: &BundleThrottle) -> BundleId {
    let id = BundleId::new();
    let json = serde_json::to_string(throttle).expect("serialize throttle");
    store
        .insert_bundle(&inboxly_store::BundleRow {
            id: id.to_string(),
            category: name.to_string(),
            name: name.to_string(),
            color: "#000000".to_string(),
            badge_color: "#eeeeee".to_string(),
            visibility: "Bundled".to_string(),
            throttle: json,
            sort_order: 0,
        })
        .expect("insert bundle");
    id
}

/// Insert a thread assigned to a bundle.
fn insert_thread_in_bundle(store: &Store, account_id: &str, bundle_id: &BundleId) -> String {
    let thread_id = uuid::Uuid::new_v4().to_string();
    store
        .insert_thread(&ThreadRow {
            id: thread_id.clone(),
            account_id: account_id.to_string(),
            subject: "Test thread".to_string(),
            newest_date: 1_000_000,
            oldest_date: 1_000_000,
            email_count: 1,
            unread_count: 1,
            has_attachments: false,
            snippet: "test snippet".to_string(),
            root_message_id: None,
        })
        .expect("insert thread");
    store
        .insert_thread_state(&ThreadStateRow {
            thread_id: thread_id.clone(),
            pinned: false,
            done: false,
            snoozed_until: None,
            snoozed_location_json: None,
            bundle_id: Some(bundle_id.to_string()),
        })
        .expect("insert thread_state");
    thread_id
}

/// Insert an unbundled thread.
fn insert_unbundled_thread(store: &Store, account_id: &str) -> String {
    let thread_id = uuid::Uuid::new_v4().to_string();
    store
        .insert_thread(&ThreadRow {
            id: thread_id.clone(),
            account_id: account_id.to_string(),
            subject: "Unbundled thread".to_string(),
            newest_date: 1_000_000,
            oldest_date: 1_000_000,
            email_count: 1,
            unread_count: 0,
            has_attachments: false,
            snippet: "no bundle".to_string(),
            root_message_id: None,
        })
        .expect("insert thread");
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
    thread_id
}

#[test]
fn throttled_bundle_emails_hidden_when_window_closed() {
    let store = test_store();
    let account_id = insert_test_account(&store);

    // Create a daily throttle at 23:59 -- window almost certainly closed
    // (unless tests run at exactly 23:59)
    let bundle_id = insert_throttled_bundle(
        &store,
        "Late Bundle",
        &BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(23, 59, 0).expect("valid time"),
        },
    );

    insert_thread_in_bundle(&store, &account_id, &bundle_id);
    insert_unbundled_thread(&store, &account_id);

    // Get suppressed bundles
    let now = Local::now();
    let suppressed = store
        .get_currently_suppressed_bundle_ids(&now)
        .expect("get suppressed");

    // Query with exclusion
    let threads = store
        .get_threads_excluding_bundles(&account_id, &suppressed, 100, 0)
        .expect("query");

    // If the window is closed (most likely), only the unbundled thread shows
    if now.time() < NaiveTime::from_hms_opt(23, 59, 0).expect("valid time") {
        assert_eq!(threads.len(), 1, "only unbundled thread should be visible");
    }
}

#[test]
fn immediate_bundle_always_visible() {
    let store = test_store();
    let account_id = insert_test_account(&store);

    let bundle_id = insert_throttled_bundle(&store, "Social", &BundleThrottle::Immediate);

    insert_thread_in_bundle(&store, &account_id, &bundle_id);

    let now = Local::now();
    let suppressed = store
        .get_currently_suppressed_bundle_ids(&now)
        .expect("get suppressed");

    // Immediate bundles are never suppressed
    assert!(
        !suppressed.contains(&bundle_id),
        "Immediate bundle should not be suppressed"
    );

    let threads = store
        .get_threads_excluding_bundles(&account_id, &suppressed, 100, 0)
        .expect("query");
    assert_eq!(threads.len(), 1, "thread should be visible");
}

#[test]
fn changing_throttle_takes_effect_immediately() {
    let store = test_store();
    let account_id = insert_test_account(&store);

    // Create a throttled bundle
    let bundle_id = insert_throttled_bundle(
        &store,
        "Promos",
        &BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(23, 59, 0).expect("valid time"),
        },
    );

    insert_thread_in_bundle(&store, &account_id, &bundle_id);

    let now = Local::now();
    if now.time() < NaiveTime::from_hms_opt(23, 59, 0).expect("valid time") {
        // Bundle should be suppressed
        let suppressed = store
            .get_currently_suppressed_bundle_ids(&now)
            .expect("get");
        assert!(suppressed.contains(&bundle_id));

        // Change to Immediate
        store
            .set_bundle_throttle(&bundle_id, &BundleThrottle::Immediate)
            .expect("set");

        // Should no longer be suppressed
        let suppressed = store
            .get_currently_suppressed_bundle_ids(&now)
            .expect("get");
        assert!(!suppressed.contains(&bundle_id));

        // Thread should now be visible
        let threads = store
            .get_threads_excluding_bundles(&account_id, &suppressed, 100, 0)
            .expect("query");
        assert_eq!(threads.len(), 1);
    }
}

#[test]
fn multiple_throttled_bundles_independent() {
    let store = test_store();
    let account_id = insert_test_account(&store);

    // Two bundles: one suppressed (23:59), one immediate
    let late_id = insert_throttled_bundle(
        &store,
        "Late",
        &BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(23, 59, 0).expect("valid time"),
        },
    );
    let instant_id = insert_throttled_bundle(&store, "Instant", &BundleThrottle::Immediate);

    insert_thread_in_bundle(&store, &account_id, &late_id);
    insert_thread_in_bundle(&store, &account_id, &instant_id);

    let now = Local::now();
    let suppressed = store
        .get_currently_suppressed_bundle_ids(&now)
        .expect("get");

    if now.time() < NaiveTime::from_hms_opt(23, 59, 0).expect("valid time") {
        // Only the late bundle should be suppressed
        assert!(suppressed.contains(&late_id));
        assert!(!suppressed.contains(&instant_id));

        let threads = store
            .get_threads_excluding_bundles(&account_id, &suppressed, 100, 0)
            .expect("query");
        // Only the instant bundle's thread should be visible
        assert_eq!(threads.len(), 1);
    }
}

#[test]
fn unbundled_threads_never_throttled() {
    let store = test_store();
    let account_id = insert_test_account(&store);

    // Insert only unbundled threads
    insert_unbundled_thread(&store, &account_id);
    insert_unbundled_thread(&store, &account_id);

    // Even with aggressive suppression, unbundled threads always show
    let fake_ids = vec![BundleId::new(), BundleId::new()];
    let threads = store
        .get_threads_excluding_bundles(&account_id, &fake_ids, 100, 0)
        .expect("query");
    assert_eq!(threads.len(), 2, "all unbundled threads should be visible");
}

#[test]
fn weekly_throttle_window_behaviour() {
    let store = test_store();

    let bundle_id = insert_throttled_bundle(
        &store,
        "Weekly Digest",
        &BundleThrottle::Weekly {
            delivery_day: WeekdayWrapper(chrono::Weekday::Mon),
            delivery_time: NaiveTime::from_hms_opt(8, 0, 0).expect("valid time"),
        },
    );

    // The weekly throttle should be stored and retrievable
    let loaded = store.get_bundle_throttle(&bundle_id).expect("get");
    assert!(loaded.is_throttled());

    if let BundleThrottle::Weekly {
        delivery_day,
        delivery_time,
    } = loaded
    {
        assert_eq!(delivery_day.0, chrono::Weekday::Mon);
        assert_eq!(
            delivery_time,
            NaiveTime::from_hms_opt(8, 0, 0).expect("valid time")
        );
    } else {
        panic!("expected Weekly throttle");
    }
}

#[test]
fn system_bundles_have_json_throttle_after_creation() {
    let store = test_store();
    let ids = system_bundles::ensure_system_bundles(&store).expect("ensure");
    assert_eq!(ids.len(), 8);

    // All system bundles should have valid JSON throttle
    for bundle_id in &ids {
        let throttle = store.get_bundle_throttle(bundle_id).expect("get throttle");
        // Just verify it deserializes without error
        match &throttle {
            BundleThrottle::Immediate | BundleThrottle::Daily { .. } | BundleThrottle::Weekly { .. } => {}
        }
    }

    // Promos should be Daily at 5 PM
    let promos_id = system_bundles::system_bundle_id(&inboxly_core::BundleCategory::Promos);
    let promos_throttle = store.get_bundle_throttle(&promos_id).expect("get promos");
    assert!(matches!(
        promos_throttle,
        BundleThrottle::Daily { delivery_time } if delivery_time == NaiveTime::from_hms_opt(17, 0, 0).expect("valid time")
    ));
}

#[tokio::test]
async fn scheduler_detects_window_opening_integration() {
    use inboxly_bundler::{ThrottleEvent, ThrottleSchedulerConfig, spawn_throttle_scheduler};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio::sync::Mutex;
    use tokio::time::Duration;

    let bundle_a = BundleId::new();
    let call_count = Arc::new(Mutex::new(0u32));
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let bundle_clone = bundle_a;
    let count_clone = Arc::clone(&call_count);

    let query_fn = move || {
        let count = Arc::clone(&count_clone);
        let b = bundle_clone;
        async move {
            let mut c = count.lock().await;
            *c = c.saturating_add(1);
            if *c <= 1 {
                Ok(vec![b]) // suppressed
            } else {
                Ok(vec![]) // window opened
            }
        }
    };

    let config = ThrottleSchedulerConfig {
        check_interval_secs: 0, // fast ticks for test
    };

    let handle = spawn_throttle_scheduler(config, query_fn, event_tx);

    let event = tokio::time::timeout(Duration::from_secs(2), event_rx.recv())
        .await
        .expect("timed out")
        .expect("channel closed");

    match event {
        ThrottleEvent::WindowOpened(ids) => {
            assert_eq!(ids.len(), 1);
            assert_eq!(ids[0], bundle_a);
        }
    }

    handle.abort();
}
