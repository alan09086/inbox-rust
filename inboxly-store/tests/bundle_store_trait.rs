//! Integration tests for the BundleStore trait implementation on Store.

use inboxly_core::store_traits::{
    BundleStore, BundleStoreError, CreateBundleParams, UpdateBundleParams,
};
use inboxly_core::{BundleThrottle, BundleVisibility};
use inboxly_store::Store;
use uuid::Uuid;

fn make_store() -> Store {
    Store::open_in_memory().expect("failed to open in-memory store")
}

fn basic_create_params(name: &str) -> CreateBundleParams {
    CreateBundleParams {
        name: name.to_string(),
        color: "#336699".to_string(),
        badge_color: "#eef4fa".to_string(),
        visibility: BundleVisibility::Bundled,
        throttle: BundleThrottle::Immediate,
    }
}

// ---------------------------------------------------------------------------

#[test]
fn create_and_list_bundles() {
    let store = make_store();

    let id = store
        .create_bundle(basic_create_params("Work"))
        .expect("create bundle");

    assert_ne!(id, Uuid::nil(), "returned ID should not be nil");

    let bundles = store.list_bundles().expect("list bundles");
    let found = bundles.iter().find(|b| b.id == id);
    assert!(found.is_some(), "created bundle should appear in list");

    let bundle = found.unwrap();
    assert_eq!(bundle.name, "Work");
    assert_eq!(bundle.color, "#336699");
    assert_eq!(bundle.badge_color, "#eef4fa");
    assert_eq!(bundle.visibility, BundleVisibility::Bundled);
    assert_eq!(bundle.throttle, BundleThrottle::Immediate);
    assert!(bundle.is_custom, "user-created bundle should be is_custom=true");
}

#[test]
fn update_bundle() {
    let store = make_store();

    let id = store
        .create_bundle(basic_create_params("OldName"))
        .expect("create");

    store
        .update_bundle(
            id,
            UpdateBundleParams {
                name: Some("NewName".to_string()),
                sort_order: Some(42),
                ..UpdateBundleParams::default()
            },
        )
        .expect("update");

    let bundles = store.list_bundles().expect("list");
    let bundle = bundles.iter().find(|b| b.id == id).expect("find bundle");

    assert_eq!(bundle.name, "NewName");
    assert_eq!(bundle.sort_order, 42);
    // Unchanged fields should be preserved
    assert_eq!(bundle.color, "#336699");
    assert_eq!(bundle.badge_color, "#eef4fa");
}

#[test]
fn delete_bundle() {
    let store = make_store();

    let id = store
        .create_bundle(basic_create_params("Temp"))
        .expect("create");

    // Should be present
    let before = store.list_bundles().expect("list before");
    assert!(before.iter().any(|b| b.id == id));

    store.delete_bundle(id).expect("delete");

    // Should be gone
    let after = store.list_bundles().expect("list after");
    assert!(!after.iter().any(|b| b.id == id));
}

#[test]
fn delete_nonexistent_bundle_returns_not_found() {
    let store = make_store();
    let missing_id = Uuid::new_v4();

    let result = store.delete_bundle(missing_id);
    assert!(
        matches!(result, Err(BundleStoreError::NotFound(id)) if id == missing_id),
        "expected NotFound, got: {:?}",
        result
    );
}

#[test]
fn update_nonexistent_bundle_returns_not_found() {
    let store = make_store();
    let missing_id = Uuid::new_v4();

    let result = store.update_bundle(
        missing_id,
        UpdateBundleParams {
            name: Some("DoesNotMatter".to_string()),
            ..UpdateBundleParams::default()
        },
    );
    assert!(
        matches!(result, Err(BundleStoreError::NotFound(id)) if id == missing_id),
        "expected NotFound, got: {:?}",
        result
    );
}
