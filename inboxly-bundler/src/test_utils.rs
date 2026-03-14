//! Shared test utilities for the bundler crate.
//!
//! Contains [`MockEmail`] (test double for [`RuleMatchable`]) and
//! [`make_rule`] helper for constructing test rules.

#[cfg(test)]
pub(crate) mod fixtures {
    use crate::user_rules::{BundleRule, RuleMatchable, UserRuleField, UserRuleOp};
    use std::collections::HashMap;
    use uuid::Uuid;

    /// Test double implementing [`RuleMatchable`].
    pub struct MockEmail {
        pub from: String,
        pub to: Vec<String>,
        pub subject: String,
        pub headers: HashMap<String, String>,
        pub body: Option<String>,
    }

    impl MockEmail {
        pub fn new(from: &str, subject: &str) -> Self {
            Self {
                from: from.into(),
                to: vec![],
                subject: subject.into(),
                headers: HashMap::new(),
                body: None,
            }
        }

        #[allow(dead_code)]
        pub fn with_to(mut self, to: &[&str]) -> Self {
            self.to = to.iter().map(|s| (*s).to_owned()).collect();
            self
        }

        #[allow(dead_code)]
        pub fn with_header(mut self, name: &str, value: &str) -> Self {
            self.headers.insert(name.to_owned(), value.to_owned());
            self
        }

        #[allow(dead_code)]
        pub fn with_body(mut self, body: &str) -> Self {
            self.body = Some(body.to_owned());
            self
        }
    }

    impl RuleMatchable for MockEmail {
        fn sender_address(&self) -> &str {
            &self.from
        }
        fn to_addresses(&self) -> &[String] {
            &self.to
        }
        fn subject(&self) -> &str {
            &self.subject
        }
        fn header(&self, name: &str) -> Option<&str> {
            self.headers.get(name).map(String::as_str)
        }
        fn body_text(&self) -> Option<&str> {
            self.body.as_deref()
        }
    }

    /// Create a [`BundleRule`] for testing with default id/bundle_id/priority.
    pub fn make_rule(field: UserRuleField, op: UserRuleOp, value: &str) -> BundleRule {
        BundleRule {
            id: Uuid::new_v4(),
            bundle_id: Uuid::new_v4(),
            field,
            operator: op,
            value: value.to_owned(),
            priority: 0,
        }
    }
}
