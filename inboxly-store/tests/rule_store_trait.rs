//! Integration tests for the RuleStore trait implementation on Store.

use inboxly_core::store_traits::{
    CreateRuleParams, RuleStore, RuleStoreError, UpdateRuleParams, UserRuleField, UserRuleOp,
};
use inboxly_store::{BundleRow, Store};
use uuid::Uuid;

fn make_store() -> Store {
    Store::open_in_memory().expect("failed to open in-memory store")
}

fn make_bundle_id() -> Uuid {
    Uuid::new_v4()
}

/// Insert a bundle row to satisfy the foreign-key constraint on bundle_rules.
fn insert_bundle(store: &Store, bundle_id: Uuid) {
    store
        .insert_bundle(&BundleRow {
            id: bundle_id.to_string(),
            category: "user".to_string(),
            name: "Test Bundle".to_string(),
            color: "#000000".to_string(),
            badge_color: "#eeeeee".to_string(),
            visibility: "Bundled".to_string(),
            throttle: r#"{"mode":"Immediate"}"#.to_string(),
            sort_order: 0,
        })
        .expect("failed to insert test bundle");
}

// ---------------------------------------------------------------------------

#[test]
fn create_and_get_rule() {
    let store = make_store();
    let bundle_id = make_bundle_id();
    insert_bundle(&store, bundle_id);

    let rule = store
        .create_rule(CreateRuleParams {
            bundle_id,
            field: UserRuleField::From,
            operator: UserRuleOp::Contains,
            value: "alice@example.com".to_string(),
            priority: 10,
        })
        .expect("create_rule failed");

    assert_eq!(rule.bundle_id, bundle_id);
    assert_eq!(rule.field, UserRuleField::From);
    assert_eq!(rule.operator, UserRuleOp::Contains);
    assert_eq!(rule.value, "alice@example.com");
    assert_eq!(rule.priority, 10);

    let fetched = store.get_rule(rule.id).expect("get_rule failed");
    assert_eq!(fetched.id, rule.id);
    assert_eq!(fetched.bundle_id, bundle_id);
    assert_eq!(fetched.field, UserRuleField::From);
    assert_eq!(fetched.operator, UserRuleOp::Contains);
    assert_eq!(fetched.value, "alice@example.com");
    assert_eq!(fetched.priority, 10);
}

#[test]
fn list_rules_and_filter_by_bundle() {
    let store = make_store();
    let bundle_a = make_bundle_id();
    let bundle_b = make_bundle_id();
    insert_bundle(&store, bundle_a);
    insert_bundle(&store, bundle_b);

    store
        .create_rule(CreateRuleParams {
            bundle_id: bundle_a,
            field: UserRuleField::Subject,
            operator: UserRuleOp::Contains,
            value: "newsletter".to_string(),
            priority: 5,
        })
        .expect("create_rule bundle_a failed");

    store
        .create_rule(CreateRuleParams {
            bundle_id: bundle_b,
            field: UserRuleField::From,
            operator: UserRuleOp::Domain,
            value: "spam.com".to_string(),
            priority: 3,
        })
        .expect("create_rule bundle_b failed");

    let all = store.list_rules().expect("list_rules failed");
    assert_eq!(all.len(), 2);

    let for_a = store
        .list_rules_for_bundle(bundle_a)
        .expect("list_rules_for_bundle bundle_a failed");
    assert_eq!(for_a.len(), 1);
    assert_eq!(for_a[0].bundle_id, bundle_a);
    assert_eq!(for_a[0].field, UserRuleField::Subject);

    let for_b = store
        .list_rules_for_bundle(bundle_b)
        .expect("list_rules_for_bundle bundle_b failed");
    assert_eq!(for_b.len(), 1);
    assert_eq!(for_b[0].bundle_id, bundle_b);
    assert_eq!(for_b[0].field, UserRuleField::From);
}

#[test]
fn update_rule() {
    let store = make_store();
    let bundle_id = make_bundle_id();
    insert_bundle(&store, bundle_id);

    let rule = store
        .create_rule(CreateRuleParams {
            bundle_id,
            field: UserRuleField::From,
            operator: UserRuleOp::Contains,
            value: "old_value".to_string(),
            priority: 1,
        })
        .expect("create_rule failed");

    let updated = store
        .update_rule(
            rule.id,
            UpdateRuleParams {
                field: None,
                operator: None,
                value: Some("new_value".to_string()),
                priority: Some(99),
                bundle_id: None,
            },
        )
        .expect("update_rule failed");

    assert_eq!(updated.id, rule.id);
    assert_eq!(updated.value, "new_value");
    assert_eq!(updated.priority, 99);
    // Unchanged fields should remain.
    assert_eq!(updated.field, UserRuleField::From);
    assert_eq!(updated.operator, UserRuleOp::Contains);
}

#[test]
fn delete_rule() {
    let store = make_store();
    let bundle_id = make_bundle_id();
    insert_bundle(&store, bundle_id);

    let rule = store
        .create_rule(CreateRuleParams {
            bundle_id,
            field: UserRuleField::Body,
            operator: UserRuleOp::Equals,
            value: "unsubscribe".to_string(),
            priority: 0,
        })
        .expect("create_rule failed");

    store.delete_rule(rule.id).expect("delete_rule failed");

    let err = store
        .get_rule(rule.id)
        .expect_err("expected NotFound after delete");
    assert!(
        matches!(err, RuleStoreError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );
}

#[test]
fn get_nonexistent_rule_returns_not_found() {
    let store = make_store();
    let fake_id = Uuid::new_v4();

    let err = store
        .get_rule(fake_id)
        .expect_err("expected NotFound for non-existent id");
    assert!(
        matches!(err, RuleStoreError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );
}

#[test]
fn invalid_regex_in_rule_value_when_operator_is_regex() {
    let store = make_store();
    let bundle_id = make_bundle_id();
    insert_bundle(&store, bundle_id);

    let err = store
        .create_rule(CreateRuleParams {
            bundle_id,
            field: UserRuleField::Subject,
            operator: UserRuleOp::Matches,
            value: "[invalid regex".to_string(),
            priority: 0,
        })
        .expect_err("expected InvalidRegex error");

    assert!(
        matches!(err, RuleStoreError::InvalidRegex(_)),
        "expected InvalidRegex, got {err:?}"
    );
}

#[test]
fn update_nonexistent_rule_returns_not_found() {
    let store = make_store();
    let fake_id = Uuid::new_v4();

    let err = store
        .update_rule(
            fake_id,
            UpdateRuleParams {
                field: None,
                operator: None,
                value: Some("something".to_string()),
                priority: None,
                bundle_id: None,
            },
        )
        .expect_err("expected NotFound for non-existent id");

    assert!(
        matches!(err, RuleStoreError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );
}
