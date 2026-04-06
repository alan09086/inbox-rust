//! CRUD operations for user-defined bundle rules.
//!
//! The type definitions and [`RuleStore`] trait have moved to
//! [`inboxly_core::store_traits`].  This module re-exports everything for
//! backwards compatibility.

pub use inboxly_core::store_traits::{
    BundleRule, CreateRuleParams, RuleId, RuleStore, RuleStoreError, UpdateRuleParams,
    UserRuleField, UserRuleOp, validate_rule,
};

// ---------------------------------------------------------------------------
// In-memory mock for tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod mock {
    use super::*;
    use std::sync::Mutex;
    use uuid::Uuid;

    /// In-memory mock implementation of [`RuleStore`] for unit tests.
    pub struct MockRuleStore {
        rules: Mutex<Vec<BundleRule>>,
    }

    impl MockRuleStore {
        pub fn new() -> Self {
            Self {
                rules: Mutex::new(Vec::new()),
            }
        }
    }

    impl RuleStore for MockRuleStore {
        fn create_rule(&self, params: CreateRuleParams) -> Result<BundleRule, RuleStoreError> {
            validate_rule(&params.field, &params.operator, &params.value)?;
            let rule = BundleRule {
                id: Uuid::new_v4(),
                bundle_id: params.bundle_id,
                field: params.field,
                operator: params.operator,
                value: params.value,
                priority: params.priority,
            };
            self.rules
                .lock()
                .expect("mock lock poisoned")
                .push(rule.clone());
            Ok(rule)
        }

        fn get_rule(&self, id: RuleId) -> Result<BundleRule, RuleStoreError> {
            self.rules
                .lock()
                .expect("mock lock poisoned")
                .iter()
                .find(|r| r.id == id)
                .cloned()
                .ok_or(RuleStoreError::NotFound(id))
        }

        fn list_rules(&self) -> Result<Vec<BundleRule>, RuleStoreError> {
            let mut rules = self.rules.lock().expect("mock lock poisoned").clone();
            rules.sort_by(|a, b| b.priority.cmp(&a.priority));
            Ok(rules)
        }

        fn list_rules_for_bundle(
            &self,
            bundle_id: Uuid,
        ) -> Result<Vec<BundleRule>, RuleStoreError> {
            let mut rules: Vec<_> = self
                .rules
                .lock()
                .expect("mock lock poisoned")
                .iter()
                .filter(|r| r.bundle_id == bundle_id)
                .cloned()
                .collect();
            rules.sort_by(|a, b| b.priority.cmp(&a.priority));
            Ok(rules)
        }

        fn update_rule(
            &self,
            id: RuleId,
            params: UpdateRuleParams,
        ) -> Result<BundleRule, RuleStoreError> {
            let mut rules = self.rules.lock().expect("mock lock poisoned");
            let rule = rules
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or(RuleStoreError::NotFound(id))?;
            if let Some(field) = params.field {
                rule.field = field;
            }
            if let Some(op) = params.operator {
                rule.operator = op;
            }
            if let Some(value) = params.value {
                rule.value = value;
            }
            if let Some(priority) = params.priority {
                rule.priority = priority;
            }
            if let Some(bundle_id) = params.bundle_id {
                rule.bundle_id = bundle_id;
            }
            validate_rule(&rule.field, &rule.operator, &rule.value)?;
            Ok(rule.clone())
        }

        fn delete_rule(&self, id: RuleId) -> Result<(), RuleStoreError> {
            let mut rules = self.rules.lock().expect("mock lock poisoned");
            let pos = rules
                .iter()
                .position(|r| r.id == id)
                .ok_or(RuleStoreError::NotFound(id))?;
            rules.remove(pos);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mock::MockRuleStore;
    use uuid::Uuid;

    #[test]
    fn create_and_get_rule() {
        let store = MockRuleStore::new();
        let bundle_id = Uuid::new_v4();
        let rule = store
            .create_rule(CreateRuleParams {
                bundle_id,
                field: UserRuleField::From,
                operator: UserRuleOp::Domain,
                value: "github.com".into(),
                priority: 10,
            })
            .expect("create");
        assert_eq!(rule.bundle_id, bundle_id);
        assert_eq!(rule.priority, 10);

        let fetched = store.get_rule(rule.id).expect("get");
        assert_eq!(fetched.id, rule.id);
    }

    #[test]
    fn list_rules_sorted_by_priority() {
        let store = MockRuleStore::new();
        let bid = Uuid::new_v4();
        store
            .create_rule(CreateRuleParams {
                bundle_id: bid,
                field: UserRuleField::From,
                operator: UserRuleOp::Contains,
                value: "low".into(),
                priority: 1,
            })
            .expect("create low");
        store
            .create_rule(CreateRuleParams {
                bundle_id: bid,
                field: UserRuleField::From,
                operator: UserRuleOp::Contains,
                value: "high".into(),
                priority: 100,
            })
            .expect("create high");
        let rules = store.list_rules().expect("list");
        assert_eq!(rules[0].value, "high");
        assert_eq!(rules[1].value, "low");
    }

    #[test]
    fn list_rules_for_specific_bundle() {
        let store = MockRuleStore::new();
        let bid_a = Uuid::new_v4();
        let bid_b = Uuid::new_v4();
        store
            .create_rule(CreateRuleParams {
                bundle_id: bid_a,
                field: UserRuleField::From,
                operator: UserRuleOp::Contains,
                value: "a".into(),
                priority: 1,
            })
            .expect("create a");
        store
            .create_rule(CreateRuleParams {
                bundle_id: bid_b,
                field: UserRuleField::From,
                operator: UserRuleOp::Contains,
                value: "b".into(),
                priority: 2,
            })
            .expect("create b");
        let rules = store.list_rules_for_bundle(bid_a).expect("list for a");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].value, "a");
    }

    #[test]
    fn update_rule_partial() {
        let store = MockRuleStore::new();
        let rule = store
            .create_rule(CreateRuleParams {
                bundle_id: Uuid::new_v4(),
                field: UserRuleField::From,
                operator: UserRuleOp::Contains,
                value: "old".into(),
                priority: 5,
            })
            .expect("create");
        let updated = store
            .update_rule(
                rule.id,
                UpdateRuleParams {
                    field: None,
                    operator: None,
                    value: Some("new".into()),
                    priority: Some(99),
                    bundle_id: None,
                },
            )
            .expect("update");
        assert_eq!(updated.value, "new");
        assert_eq!(updated.priority, 99);
        assert_eq!(updated.field, UserRuleField::From); // unchanged
    }

    #[test]
    fn delete_rule_succeeds() {
        let store = MockRuleStore::new();
        let rule = store
            .create_rule(CreateRuleParams {
                bundle_id: Uuid::new_v4(),
                field: UserRuleField::Subject,
                operator: UserRuleOp::Contains,
                value: "test".into(),
                priority: 0,
            })
            .expect("create");
        store.delete_rule(rule.id).expect("delete");
        assert!(store.get_rule(rule.id).is_err());
    }

    #[test]
    fn delete_nonexistent_rule_errors() {
        let store = MockRuleStore::new();
        assert!(store.delete_rule(Uuid::new_v4()).is_err());
    }

    #[test]
    fn create_rule_validates_regex() {
        let store = MockRuleStore::new();
        let result = store.create_rule(CreateRuleParams {
            bundle_id: Uuid::new_v4(),
            field: UserRuleField::Subject,
            operator: UserRuleOp::Matches,
            value: "[invalid".into(),
            priority: 0,
        });
        assert!(result.is_err());
        match result.expect_err("should be error") {
            RuleStoreError::InvalidRegex(_) => {} // expected
            other => panic!("expected InvalidRegex, got: {other}"),
        }
    }

    #[test]
    fn update_rule_validates_regex() {
        let store = MockRuleStore::new();
        let rule = store
            .create_rule(CreateRuleParams {
                bundle_id: Uuid::new_v4(),
                field: UserRuleField::Subject,
                operator: UserRuleOp::Contains,
                value: "ok".into(),
                priority: 0,
            })
            .expect("create");
        let result = store.update_rule(
            rule.id,
            UpdateRuleParams {
                field: None,
                operator: Some(UserRuleOp::Matches),
                value: Some("[bad".into()),
                priority: None,
                bundle_id: None,
            },
        );
        assert!(result.is_err());
    }
}
