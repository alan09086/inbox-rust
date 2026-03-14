//! User-defined bundle rules -- field selectors, operators, matching logic,
//! and pre-compiled regex caching.
//!
//! These types are distinct from the TOML-based heuristic rules in
//! [`crate::rules_toml`]. User rules are persisted in SQLite via
//! [`crate::rule_store::RuleStore`] and take highest precedence in the
//! evaluation pipeline.

use std::fmt;
use std::str::FromStr;

use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// RuleField
// ---------------------------------------------------------------------------

/// Which part of an email a user rule examines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserRuleField {
    /// Match against the From address (e.g., "alice@example.com").
    From,
    /// Match against any To address.
    To,
    /// Match against the Subject line.
    Subject,
    /// Match against a specific header by name (e.g., "X-Mailer").
    Header(String),
    /// Match against the plaintext body.  Note: body may not be available
    /// during initial sync Phase 1 -- rule will be re-evaluated in Phase 2.
    Body,
}

impl fmt::Display for UserRuleField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::From => write!(f, "from"),
            Self::To => write!(f, "to"),
            Self::Subject => write!(f, "subject"),
            Self::Header(name) => write!(f, "header:{name}"),
            Self::Body => write!(f, "body"),
        }
    }
}

impl FromStr for UserRuleField {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "from" => Ok(Self::From),
            "to" => Ok(Self::To),
            "subject" => Ok(Self::Subject),
            "body" => Ok(Self::Body),
            other if other.starts_with("header:") => {
                let header_name = other.get(7..).unwrap_or_default();
                Ok(Self::Header(header_name.to_owned()))
            }
            _ => Err(format!("unknown user rule field: {s}")),
        }
    }
}

// ---------------------------------------------------------------------------
// UserRuleOp
// ---------------------------------------------------------------------------

/// How to compare the rule's value against the email field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserRuleOp {
    /// Field contains the value as a case-insensitive substring.
    Contains,
    /// Field equals the value exactly (case-insensitive).
    Equals,
    /// Field matches the value as a regular expression.
    Matches,
    /// From-address domain equals the value (e.g., "example.com").
    /// Only meaningful with [`UserRuleField::From`].
    Domain,
}

impl fmt::Display for UserRuleOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Contains => write!(f, "contains"),
            Self::Equals => write!(f, "equals"),
            Self::Matches => write!(f, "matches"),
            Self::Domain => write!(f, "domain"),
        }
    }
}

impl FromStr for UserRuleOp {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "contains" => Ok(Self::Contains),
            "equals" => Ok(Self::Equals),
            "matches" => Ok(Self::Matches),
            "domain" => Ok(Self::Domain),
            _ => Err(format!("unknown user rule operator: {s}")),
        }
    }
}

// ---------------------------------------------------------------------------
// RuleMatchable trait
// ---------------------------------------------------------------------------

/// Trait providing access to email fields for rule matching.
///
/// Implemented by core types (via bridge functions) and by test doubles
/// for unit testing the rule engine in isolation.
pub trait RuleMatchable {
    /// The sender's From address (e.g., "alice@example.com").
    fn sender_address(&self) -> &str;
    /// All To addresses.
    fn to_addresses(&self) -> &[String];
    /// The Subject line.
    fn subject(&self) -> &str;
    /// Get a specific header value by name.  Returns `None` if not present.
    fn header(&self, name: &str) -> Option<&str>;
    /// The plaintext body.  Returns `None` if body not yet fetched (Phase 1).
    fn body_text(&self) -> Option<&str>;
}

// ---------------------------------------------------------------------------
// BundleRule
// ---------------------------------------------------------------------------

/// A unique identifier for a bundle rule (UUID v4 string in the database).
pub type RuleId = Uuid;

/// A user-defined rule that assigns emails to a specific bundle.
///
/// Rules are evaluated in priority order (highest priority first).
/// The first matching rule wins.  User rules take precedence over
/// sender learning and header heuristics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleRule {
    /// Unique identifier for this rule.
    pub id: RuleId,
    /// Which bundle this rule assigns emails to.
    pub bundle_id: Uuid,
    /// Which email field to examine.
    pub field: UserRuleField,
    /// How to compare field value against the rule's value.
    pub operator: UserRuleOp,
    /// The value to match against (substring, exact, regex pattern, or domain).
    pub value: String,
    /// Higher priority rules are evaluated first.  Ties broken by insertion order.
    pub priority: i64,
}

impl BundleRule {
    /// Test whether this rule matches the given email.
    ///
    /// Returns `true` if the rule's field/operator/value match the email.
    /// Returns `false` if the required field is not available (e.g., body
    /// during Phase 1 sync, or a header that doesn't exist).
    pub fn matches(&self, email: &dyn RuleMatchable) -> bool {
        match &self.field {
            UserRuleField::From => self.test_value(email.sender_address()),
            UserRuleField::To => email
                .to_addresses()
                .iter()
                .any(|addr| self.test_value(addr)),
            UserRuleField::Subject => self.test_value(email.subject()),
            UserRuleField::Header(name) => email.header(name).is_some_and(|v| self.test_value(v)),
            UserRuleField::Body => email.body_text().is_some_and(|v| self.test_value(v)),
        }
    }

    /// Apply the operator to test a single string value.
    fn test_value(&self, field_value: &str) -> bool {
        match &self.operator {
            UserRuleOp::Contains => field_value
                .to_lowercase()
                .contains(&self.value.to_lowercase()),
            UserRuleOp::Equals => field_value.eq_ignore_ascii_case(&self.value),
            UserRuleOp::Matches => {
                // Compile regex on each call.  For hot paths, callers should
                // pre-compile via UserCompiledRule.
                Regex::new(&self.value).is_ok_and(|re| re.is_match(field_value))
            }
            UserRuleOp::Domain => {
                // Extract domain from email address: "user@example.com" -> "example.com"
                let domain = field_value.rsplit_once('@').map_or(field_value, |(_, d)| d);
                domain.eq_ignore_ascii_case(&self.value)
            }
        }
    }
}

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
    use std::collections::HashMap;

    // -- Test double ---------------------------------------------------------

    struct MockEmail {
        from: String,
        to: Vec<String>,
        subject: String,
        headers: HashMap<String, String>,
        body: Option<String>,
    }

    impl MockEmail {
        fn new(from: &str, subject: &str) -> Self {
            Self {
                from: from.into(),
                to: vec![],
                subject: subject.into(),
                headers: HashMap::new(),
                body: None,
            }
        }

        fn with_to(mut self, to: &[&str]) -> Self {
            self.to = to.iter().map(|s| (*s).to_owned()).collect();
            self
        }

        fn with_header(mut self, name: &str, value: &str) -> Self {
            self.headers.insert(name.to_owned(), value.to_owned());
            self
        }

        fn with_body(mut self, body: &str) -> Self {
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

    fn make_rule(field: UserRuleField, op: UserRuleOp, value: &str) -> BundleRule {
        BundleRule {
            id: Uuid::new_v4(),
            bundle_id: Uuid::new_v4(),
            field,
            operator: op,
            value: value.to_owned(),
            priority: 0,
        }
    }

    // -- RuleField / RuleOp round-trips --------------------------------------

    #[test]
    fn rule_field_roundtrip() {
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
        assert!(UserRuleField::from_str("unknown").is_err());
    }

    #[test]
    fn unknown_op_returns_error() {
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
