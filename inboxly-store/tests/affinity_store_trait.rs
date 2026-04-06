//! Integration tests for the AffinityStore trait implementation on Store.

use chrono::Utc;
use inboxly_core::store_traits::AffinityStore;
use inboxly_store::Store;

fn make_store() -> Store {
    Store::open_in_memory().expect("failed to open in-memory store")
}

// ---------------------------------------------------------------------------

#[test]
fn record_and_get_affinity() {
    let store = make_store();
    let now = Utc::now();

    let recorded = store
        .record_affinity("noreply@github.com", "github.com", "Notifications", now)
        .expect("record_affinity failed");

    assert_eq!(recorded.sender_address, "noreply@github.com");
    assert_eq!(recorded.sender_domain, "github.com");
    assert_eq!(recorded.bundle_category, "Notifications");
    assert!(recorded.confidence > 0.0, "confidence should be positive");

    let fetched = store
        .get_affinity("noreply@github.com")
        .expect("get_affinity failed");

    let affinity = fetched.expect("expected Some affinity");
    assert_eq!(affinity.sender_address, "noreply@github.com");
    assert_eq!(affinity.sender_domain, "github.com");
    assert_eq!(affinity.bundle_category, "Notifications");
}

#[test]
fn get_nonexistent_affinity_returns_none() {
    let store = make_store();

    let result = store
        .get_affinity("nobody@nowhere.invalid")
        .expect("get_affinity failed");

    assert!(result.is_none(), "expected None for unknown sender address");
}

#[test]
fn list_affinities() {
    let store = make_store();
    let now = Utc::now();

    store
        .record_affinity("alice@example.com", "example.com", "Personal", now)
        .expect("record first affinity failed");

    store
        .record_affinity("updates@shop.com", "shop.com", "Shopping", now)
        .expect("record second affinity failed");

    let all = store.list_affinities().expect("list_affinities failed");
    assert_eq!(all.len(), 2, "expected 2 affinities");

    let addresses: Vec<&str> = all.iter().map(|a| a.sender_address.as_str()).collect();
    assert!(
        addresses.contains(&"alice@example.com"),
        "expected alice@example.com in list"
    );
    assert!(
        addresses.contains(&"updates@shop.com"),
        "expected updates@shop.com in list"
    );
}

#[test]
fn delete_affinity() {
    let store = make_store();
    let now = Utc::now();

    store
        .record_affinity("noreply@github.com", "github.com", "Notifications", now)
        .expect("record_affinity failed");

    store
        .delete_affinity("noreply@github.com")
        .expect("delete_affinity failed");

    let result = store
        .get_affinity("noreply@github.com")
        .expect("get_affinity after delete failed");

    assert!(result.is_none(), "expected None after deletion");
}

#[test]
fn record_affinity_upserts_on_duplicate() {
    let store = make_store();
    let now = Utc::now();

    let first = store
        .record_affinity("noreply@github.com", "github.com", "Notifications", now)
        .expect("first record_affinity failed");

    let second = store
        .record_affinity("noreply@github.com", "github.com", "Notifications", now)
        .expect("second record_affinity failed");

    // Confidence should increase on reinforcement.
    assert!(
        second.confidence >= first.confidence,
        "confidence should be >= after reinforcement (first={}, second={})",
        first.confidence,
        second.confidence
    );

    // Only one record should exist in the store.
    let all = store.list_affinities().expect("list_affinities failed");
    assert_eq!(all.len(), 1, "expected exactly 1 affinity after upsert");
}

#[test]
fn delete_nonexistent_affinity_succeeds_silently() {
    let store = make_store();

    // Should not return an error when the address doesn't exist.
    store
        .delete_affinity("nobody@nowhere.invalid")
        .expect("delete_affinity for unknown address should succeed silently");
}
