//! User-defined bundle rules -- field selectors, operators, matching logic,
//! and pre-compiled regex caching.
//!
//! These types are distinct from the TOML-based heuristic rules in
//! [`crate::rules_toml`]. User rules are persisted in SQLite via
//! [`crate::rule_store::RuleStore`] and take highest precedence in the
//! evaluation pipeline.
//!
//! The core data types ([`BundleRule`], [`RuleId`], [`UserRuleField`],
//! [`UserRuleOp`], [`RuleMatchable`]) have moved to
//! [`inboxly_core::store_traits`] and are re-exported here for
//! backwards compatibility.

use regex::Regex;

// Re-export the core types for backwards compatibility.
pub use inboxly_core::store_traits::{
    BundleRule, RuleId, RuleMatchable, UserRuleField, UserRuleOp,
};

// ---------------------------------------------------------------------------
// UserCompiledRule -- pre-compiled regex cache
// ---------------------------------------------------------------------------

/// A [`BundleRule`] with pre-compiled regex for efficient repeated evaluation.
///
/// Created once when rules are loaded from the database.  The compiled regex
/// is reused for every email evaluation, avoiding per-email compilation cost.
pub struct UserCompiledRule {
    /// The underlying rule.
    pub rule: BundleRule,
    /// Pre-compiled regex (only populated for [`UserRuleOp::Matches`]).
    compiled_regex: Option<Regex>,
}

impl UserCompiledRule {
    /// Compile a [`BundleRule`].  If the operator is `Matches` and the regex
    /// pattern is invalid, the rule is still created but will never match.
    pub fn compile(rule: BundleRule) -> Self {
        let compiled_regex = if rule.operator == UserRuleOp::Matches {
            Regex::new(&rule.value).ok()
        } else {
            None
        };
        Self {
            rule,
            compiled_regex,
        }
    }

    /// Test whether this rule matches the given email.
    /// Uses the pre-compiled regex if available.
    pub fn matches(&self, email: &dyn RuleMatchable) -> bool {
        if self.rule.operator == UserRuleOp::Matches {
            let Some(ref re) = self.compiled_regex else {
                return false; // invalid regex pattern
            };
            match &self.rule.field {
                UserRuleField::From => re.is_match(email.sender_address()),
                UserRuleField::To => email.to_addresses().iter().any(|a| re.is_match(a)),
                UserRuleField::Subject => re.is_match(email.subject()),
                UserRuleField::Header(name) => email.header(name).is_some_and(|v| re.is_match(v)),
                UserRuleField::Body => email.body_text().is_some_and(|v| re.is_match(v)),
            }
        } else {
            // Non-regex operators delegate to BundleRule::matches
            self.rule.matches(email)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::fixtures::{MockEmail, make_rule};

    // -- RuleField / RuleOp round-trips --------------------------------------

    #[test]
    fn rule_field_roundtrip() {
        use std::str::FromStr;
        let cases = [
            (UserRuleField::From, "from"),
            (UserRuleField::To, "to"),
            (UserRuleField::Subject, "subject"),
            (UserRuleField::Body, "body"),
            (UserRuleField::Header("X-Mailer".into()), "header:X-Mailer"),
        ];
        for (field, expected_str) in &cases {
            assert_eq!(field.to_string(), *expected_str);
            assert_eq!(
                &UserRuleField::from_str(expected_str).expect("parse"),
                field
            );
        }
    }

    #[test]
    fn rule_op_roundtrip() {
        use std::str::FromStr;
        let cases = [
            (UserRuleOp::Contains, "contains"),
            (UserRuleOp::Equals, "equals"),
            (UserRuleOp::Matches, "matches"),
            (UserRuleOp::Domain, "domain"),
        ];
        for (op, expected_str) in &cases {
            assert_eq!(op.to_string(), *expected_str);
            assert_eq!(&UserRuleOp::from_str(expected_str).expect("parse"), op);
        }
    }

    #[test]
    fn unknown_field_returns_error() {
        use std::str::FromStr;
        assert!(UserRuleField::from_str("unknown").is_err());
    }

    #[test]
    fn unknown_op_returns_error() {
        use std::str::FromStr;
        assert!(UserRuleOp::from_str("unknown").is_err());
    }

    // -- BundleRule matching --------------------------------------------------

    #[test]
    fn contains_from_case_insensitive() {
        let rule = make_rule(UserRuleField::From, UserRuleOp::Contains, "github");
        let email = MockEmail::new("noreply@GitHub.com", "PR merged");
        assert!(rule.matches(&email));
    }

    #[test]
    fn contains_no_match() {
        let rule = make_rule(UserRuleField::From, UserRuleOp::Contains, "gitlab");
        let email = MockEmail::new("noreply@github.com", "PR merged");
        assert!(!rule.matches(&email));
    }

    #[test]
    fn equals_subject_case_insensitive() {
        let rule = make_rule(UserRuleField::Subject, UserRuleOp::Equals, "Weekly Digest");
        let email = MockEmail::new("bot@example.com", "weekly digest");
        assert!(rule.matches(&email));
    }

    #[test]
    fn equals_subject_no_partial() {
        let rule = make_rule(UserRuleField::Subject, UserRuleOp::Equals, "Weekly");
        let email = MockEmail::new("bot@example.com", "Weekly Digest");
        assert!(!rule.matches(&email));
    }

    #[test]
    fn matches_regex_subject() {
        let rule = make_rule(
            UserRuleField::Subject,
            UserRuleOp::Matches,
            r"(?i)order\s*#\d+",
        );
        let email = MockEmail::new("shop@store.com", "Your Order #12345 has shipped");
        assert!(rule.matches(&email));
    }

    #[test]
    fn matches_regex_invalid_pattern() {
        let rule = make_rule(UserRuleField::Subject, UserRuleOp::Matches, r"[invalid");
        let email = MockEmail::new("a@b.com", "anything");
        assert!(!rule.matches(&email)); // invalid regex -> no match, no panic
    }

    #[test]
    fn domain_from_address() {
        let rule = make_rule(UserRuleField::From, UserRuleOp::Domain, "example.com");
        let email = MockEmail::new("alice@example.com", "Hello");
        assert!(rule.matches(&email));
    }

    #[test]
    fn domain_case_insensitive() {
        let rule = make_rule(UserRuleField::From, UserRuleOp::Domain, "Example.COM");
        let email = MockEmail::new("alice@example.com", "Hello");
        assert!(rule.matches(&email));
    }

    #[test]
    fn domain_different_domain() {
        let rule = make_rule(UserRuleField::From, UserRuleOp::Domain, "other.com");
        let email = MockEmail::new("alice@example.com", "Hello");
        assert!(!rule.matches(&email));
    }

    #[test]
    fn header_match() {
        let rule = make_rule(
            UserRuleField::Header("X-Mailer".into()),
            UserRuleOp::Contains,
            "campaign",
        );
        let email =
            MockEmail::new("a@b.com", "Promo").with_header("X-Mailer", "MailChimp Campaign v3");
        assert!(rule.matches(&email));
    }

    #[test]
    fn header_missing_no_match() {
        let rule = make_rule(
            UserRuleField::Header("X-Custom".into()),
            UserRuleOp::Contains,
            "value",
        );
        let email = MockEmail::new("a@b.com", "Test");
        assert!(!rule.matches(&email)); // header absent -> false
    }

    #[test]
    fn body_match_when_available() {
        let rule = make_rule(UserRuleField::Body, UserRuleOp::Contains, "unsubscribe");
        let email = MockEmail::new("a@b.com", "Newsletter")
            .with_body("Click here to unsubscribe from this list.");
        assert!(rule.matches(&email));
    }

    #[test]
    fn body_not_available_returns_false() {
        let rule = make_rule(UserRuleField::Body, UserRuleOp::Contains, "anything");
        let email = MockEmail::new("a@b.com", "Newsletter");
        // body is None (Phase 1 sync) -> rule does not match
        assert!(!rule.matches(&email));
    }

    #[test]
    fn to_field_matches_any_recipient() {
        let rule = make_rule(UserRuleField::To, UserRuleOp::Contains, "team@");
        let email =
            MockEmail::new("sender@a.com", "Hello").with_to(&["alice@a.com", "team@company.com"]);
        assert!(rule.matches(&email));
    }

    // -- UserCompiledRule tests -----------------------------------------------

    #[test]
    fn compiled_rule_reuses_regex() {
        let rule = make_rule(
            UserRuleField::Subject,
            UserRuleOp::Matches,
            r"(?i)invoice\s+\d+",
        );
        let compiled = UserCompiledRule::compile(rule);
        assert!(compiled.compiled_regex.is_some());

        let email1 = MockEmail::new("a@b.com", "Invoice 42");
        let email2 = MockEmail::new("a@b.com", "Receipt #99");
        assert!(compiled.matches(&email1));
        assert!(!compiled.matches(&email2));
    }

    #[test]
    fn compiled_rule_invalid_regex_never_matches() {
        let rule = make_rule(UserRuleField::Subject, UserRuleOp::Matches, r"[bad");
        let compiled = UserCompiledRule::compile(rule);
        assert!(compiled.compiled_regex.is_none());
        assert!(!compiled.matches(&MockEmail::new("a@b.com", "anything")));
    }

    #[test]
    fn compiled_rule_non_regex_delegates() {
        let rule = make_rule(UserRuleField::From, UserRuleOp::Domain, "example.com");
        let compiled = UserCompiledRule::compile(rule);
        assert!(compiled.compiled_regex.is_none());
        assert!(compiled.matches(&MockEmail::new("user@example.com", "Hi")));
    }
}
