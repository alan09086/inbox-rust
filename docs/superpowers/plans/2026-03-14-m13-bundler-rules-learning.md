# M13: Bundler User Rules + Sender Learning — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete the bundler categorisation engine by adding user-defined rule CRUD, sender affinity learning, confidence scoring with decay, and the full four-layer evaluation pipeline (user rules → sender learning → header heuristics → uncategorised).

**Prerequisites:** M12 (header heuristics working in `inboxly-bundler`), M3 (SQLite schema with `bundle_rules` + `sender_affinity` tables in `inboxly-store`).

**Architecture:** All new types live in `inboxly-bundler`. Rule evaluation and sender learning are pure functions tested without a database. The `BundlerEngine` (introduced in M12) gains two new evaluation layers that slot in above header heuristics. Database persistence uses the `inboxly-store` `Store` trait. Re-categorisation on user move updates both `sender_affinity` and `thread_state.bundle_id`.

**Tech Stack:** Rust 2024 edition, `regex` for pattern matching, `chrono` for timestamp/decay calculations, `rusqlite` via `inboxly-store`.

**Design doc:** `docs/superpowers/specs/2026-03-14-inboxly-design.md` — Bundler Layer 2 (User Rules), Layer 3 (Sender Learning), Evaluation Order.

---

## SQLite Tables (from M3 schema, already created)

```sql
-- bundle_rules: id, bundle_id, field, operator, value, priority
-- sender_affinity: sender_domain, sender_address, bundle_category, confidence, learned_at
```

These tables already exist from M3. This milestone adds the Rust types, CRUD operations, and evaluation logic that read/write them.

---

### Task 1: RuleField and RuleOp enums

**Files:**
- Create: `inboxly-bundler/src/rules.rs`
- Modify: `inboxly-bundler/src/lib.rs` (add `mod rules; pub use rules::*;`)

**Step 1: Define the enums with serialisation**

In `inboxly-bundler/src/rules.rs`:

```rust
//! User-defined bundle rules — field selectors, operators, and matching logic.

use serde::{Deserialize, Serialize};

/// Which part of an email a rule examines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleField {
    /// Match against the From address (e.g., "alice@example.com").
    From,
    /// Match against any To address.
    To,
    /// Match against the Subject line.
    Subject,
    /// Match against a specific header by name (e.g., "X-Mailer").
    Header(String),
    /// Match against the plaintext body. Note: body may not be available
    /// during initial sync Phase 1 — rule will be re-evaluated in Phase 2.
    Body,
}

/// How to compare the rule's value against the email field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleOp {
    /// Field contains the value as a case-insensitive substring.
    Contains,
    /// Field equals the value exactly (case-insensitive).
    Equals,
    /// Field matches the value as a regular expression.
    Matches,
    /// From address domain equals the value (e.g., "example.com").
    /// Only meaningful with RuleField::From.
    Domain,
}
```

**Step 2: Implement `RuleField` and `RuleOp` string conversion for SQLite storage**

Add `Display` and `FromStr` implementations so they can be stored as TEXT in SQLite:

```rust
use std::fmt;
use std::str::FromStr;

impl fmt::Display for RuleField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleField::From => write!(f, "from"),
            RuleField::To => write!(f, "to"),
            RuleField::Subject => write!(f, "subject"),
            RuleField::Header(name) => write!(f, "header:{name}"),
            RuleField::Body => write!(f, "body"),
        }
    }
}

impl FromStr for RuleField {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "from" => Ok(RuleField::From),
            "to" => Ok(RuleField::To),
            "subject" => Ok(RuleField::Subject),
            "body" => Ok(RuleField::Body),
            s if s.starts_with("header:") => Ok(RuleField::Header(s[7..].to_string())),
            _ => Err(format!("unknown rule field: {s}")),
        }
    }
}

impl fmt::Display for RuleOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleOp::Contains => write!(f, "contains"),
            RuleOp::Equals => write!(f, "equals"),
            RuleOp::Matches => write!(f, "matches"),
            RuleOp::Domain => write!(f, "domain"),
        }
    }
}

impl FromStr for RuleOp {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "contains" => Ok(RuleOp::Contains),
            "equals" => Ok(RuleOp::Equals),
            "matches" => Ok(RuleOp::Matches),
            "domain" => Ok(RuleOp::Domain),
            _ => Err(format!("unknown rule operator: {s}")),
        }
    }
}
```

**Step 3: Add unit tests for serialisation round-trips**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_field_roundtrip() {
        let cases = vec![
            (RuleField::From, "from"),
            (RuleField::To, "to"),
            (RuleField::Subject, "subject"),
            (RuleField::Body, "body"),
            (RuleField::Header("X-Mailer".into()), "header:X-Mailer"),
        ];
        for (field, expected_str) in cases {
            assert_eq!(field.to_string(), expected_str);
            assert_eq!(RuleField::from_str(expected_str).unwrap(), field);
        }
    }

    #[test]
    fn rule_op_roundtrip() {
        let cases = vec![
            (RuleOp::Contains, "contains"),
            (RuleOp::Equals, "equals"),
            (RuleOp::Matches, "matches"),
            (RuleOp::Domain, "domain"),
        ];
        for (op, expected_str) in cases {
            assert_eq!(op.to_string(), expected_str);
            assert_eq!(RuleOp::from_str(expected_str).unwrap(), op);
        }
    }
}
```

**Build & test:** `cargo test -p inboxly-bundler`

**Commit:** `feat(bundler): add RuleField and RuleOp enums with string conversion`

---

### Task 2: BundleRule struct and rule matching engine

**Files:**
- Modify: `inboxly-bundler/src/rules.rs`

**Step 1: Define BundleRule struct**

Add above the tests module in `rules.rs`:

```rust
use regex::Regex;
use uuid::Uuid;

/// A unique identifier for a bundle rule.
pub type RuleId = Uuid;

/// A user-defined rule that assigns emails to a specific bundle.
///
/// Rules are evaluated in priority order (highest priority first).
/// The first matching rule wins. User rules take precedence over
/// sender learning and header heuristics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleRule {
    /// Unique identifier for this rule.
    pub id: RuleId,
    /// Which bundle this rule assigns emails to.
    pub bundle_id: Uuid,
    /// Which email field to examine.
    pub field: RuleField,
    /// How to compare field value against the rule's value.
    pub operator: RuleOp,
    /// The value to match against (substring, exact, regex pattern, or domain).
    pub value: String,
    /// Higher priority rules are evaluated first. Ties broken by insertion order.
    pub priority: i32,
}
```

**Step 2: Define an email-field accessor trait for testability**

Rather than depending on `inboxly-core::EmailMeta` directly for matching, define a trait that the core types implement. This keeps the rule engine testable with mock emails.

```rust
/// Trait providing access to email fields for rule matching.
///
/// Implemented by EmailMeta/EmailContent from inboxly-core, and by
/// test doubles for unit testing the rule engine in isolation.
pub trait RuleMatchable {
    /// The full From address (e.g., "alice@example.com").
    fn from_address(&self) -> &str;
    /// All To addresses.
    fn to_addresses(&self) -> &[String];
    /// The Subject line.
    fn subject(&self) -> &str;
    /// Get a specific header value by name. Returns None if not present.
    /// Only available when full headers are loaded (Phase 2).
    fn header(&self, name: &str) -> Option<&str>;
    /// The plaintext body. Returns None if body not yet fetched (Phase 1).
    fn body_text(&self) -> Option<&str>;
}
```

**Step 3: Implement the matching logic**

```rust
impl BundleRule {
    /// Test whether this rule matches the given email.
    ///
    /// Returns `true` if the rule's field/operator/value match the email.
    /// Returns `false` if the required field is not available (e.g., body
    /// during Phase 1 sync, or a header that doesn't exist).
    pub fn matches(&self, email: &dyn RuleMatchable) -> bool {
        match &self.field {
            RuleField::From => self.test_value(email.from_address()),
            RuleField::To => email.to_addresses().iter().any(|addr| self.test_value(addr)),
            RuleField::Subject => self.test_value(email.subject()),
            RuleField::Header(name) => {
                email.header(name).map_or(false, |v| self.test_value(v))
            }
            RuleField::Body => {
                email.body_text().map_or(false, |v| self.test_value(v))
            }
        }
    }

    /// Apply the operator to test a single string value.
    fn test_value(&self, field_value: &str) -> bool {
        match &self.operator {
            RuleOp::Contains => {
                field_value.to_lowercase().contains(&self.value.to_lowercase())
            }
            RuleOp::Equals => {
                field_value.eq_ignore_ascii_case(&self.value)
            }
            RuleOp::Matches => {
                // Compile regex on each call. For hot paths, callers should
                // pre-compile via CompiledRule (see Task 3).
                Regex::new(&self.value)
                    .map_or(false, |re| re.is_match(field_value))
            }
            RuleOp::Domain => {
                // Extract domain from email address: "user@example.com" → "example.com"
                let domain = field_value
                    .rsplit_once('@')
                    .map(|(_, d)| d)
                    .unwrap_or(field_value);
                domain.eq_ignore_ascii_case(&self.value)
            }
        }
    }
}
```

**Step 4: Add comprehensive tests for matching**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Test double for rule matching.
    struct MockEmail {
        from: String,
        to: Vec<String>,
        subject: String,
        headers: std::collections::HashMap<String, String>,
        body: Option<String>,
    }

    impl MockEmail {
        fn new(from: &str, subject: &str) -> Self {
            Self {
                from: from.into(),
                to: vec![],
                subject: subject.into(),
                headers: std::collections::HashMap::new(),
                body: None,
            }
        }

        fn with_to(mut self, to: &[&str]) -> Self {
            self.to = to.iter().map(|s| s.to_string()).collect();
            self
        }

        fn with_header(mut self, name: &str, value: &str) -> Self {
            self.headers.insert(name.to_string(), value.to_string());
            self
        }

        fn with_body(mut self, body: &str) -> Self {
            self.body = Some(body.to_string());
            self
        }
    }

    impl RuleMatchable for MockEmail {
        fn from_address(&self) -> &str { &self.from }
        fn to_addresses(&self) -> &[String] { &self.to }
        fn subject(&self) -> &str { &self.subject }
        fn header(&self, name: &str) -> Option<&str> {
            self.headers.get(name).map(|s| s.as_str())
        }
        fn body_text(&self) -> Option<&str> { self.body.as_deref() }
    }

    fn make_rule(field: RuleField, op: RuleOp, value: &str) -> BundleRule {
        BundleRule {
            id: Uuid::new_v4(),
            bundle_id: Uuid::new_v4(),
            field,
            operator: op,
            value: value.to_string(),
            priority: 0,
        }
    }

    #[test]
    fn contains_from_case_insensitive() {
        let rule = make_rule(RuleField::From, RuleOp::Contains, "github");
        let email = MockEmail::new("noreply@GitHub.com", "PR merged");
        assert!(rule.matches(&email));
    }

    #[test]
    fn contains_no_match() {
        let rule = make_rule(RuleField::From, RuleOp::Contains, "gitlab");
        let email = MockEmail::new("noreply@github.com", "PR merged");
        assert!(!rule.matches(&email));
    }

    #[test]
    fn equals_subject_case_insensitive() {
        let rule = make_rule(RuleField::Subject, RuleOp::Equals, "Weekly Digest");
        let email = MockEmail::new("bot@example.com", "weekly digest");
        assert!(rule.matches(&email));
    }

    #[test]
    fn equals_subject_no_partial() {
        let rule = make_rule(RuleField::Subject, RuleOp::Equals, "Weekly");
        let email = MockEmail::new("bot@example.com", "Weekly Digest");
        assert!(!rule.matches(&email));
    }

    #[test]
    fn matches_regex_subject() {
        let rule = make_rule(RuleField::Subject, RuleOp::Matches, r"(?i)order\s*#\d+");
        let email = MockEmail::new("shop@store.com", "Your Order #12345 has shipped");
        assert!(rule.matches(&email));
    }

    #[test]
    fn matches_regex_invalid_pattern() {
        let rule = make_rule(RuleField::Subject, RuleOp::Matches, r"[invalid");
        let email = MockEmail::new("a@b.com", "anything");
        assert!(!rule.matches(&email)); // invalid regex → no match, no panic
    }

    #[test]
    fn domain_from_address() {
        let rule = make_rule(RuleField::From, RuleOp::Domain, "example.com");
        let email = MockEmail::new("alice@example.com", "Hello");
        assert!(rule.matches(&email));
    }

    #[test]
    fn domain_case_insensitive() {
        let rule = make_rule(RuleField::From, RuleOp::Domain, "Example.COM");
        let email = MockEmail::new("alice@example.com", "Hello");
        assert!(rule.matches(&email));
    }

    #[test]
    fn domain_different_domain() {
        let rule = make_rule(RuleField::From, RuleOp::Domain, "other.com");
        let email = MockEmail::new("alice@example.com", "Hello");
        assert!(!rule.matches(&email));
    }

    #[test]
    fn header_match() {
        let rule = make_rule(
            RuleField::Header("X-Mailer".into()),
            RuleOp::Contains,
            "campaign",
        );
        let email = MockEmail::new("a@b.com", "Promo")
            .with_header("X-Mailer", "MailChimp Campaign v3");
        assert!(rule.matches(&email));
    }

    #[test]
    fn header_missing_no_match() {
        let rule = make_rule(
            RuleField::Header("X-Custom".into()),
            RuleOp::Contains,
            "value",
        );
        let email = MockEmail::new("a@b.com", "Test");
        assert!(!rule.matches(&email)); // header absent → false
    }

    #[test]
    fn body_match_when_available() {
        let rule = make_rule(RuleField::Body, RuleOp::Contains, "unsubscribe");
        let email = MockEmail::new("a@b.com", "Newsletter")
            .with_body("Click here to unsubscribe from this list.");
        assert!(rule.matches(&email));
    }

    #[test]
    fn body_not_available_returns_false() {
        let rule = make_rule(RuleField::Body, RuleOp::Contains, "anything");
        let email = MockEmail::new("a@b.com", "Newsletter");
        // body is None (Phase 1 sync) → rule does not match
        assert!(!rule.matches(&email));
    }

    #[test]
    fn to_field_matches_any_recipient() {
        let rule = make_rule(RuleField::To, RuleOp::Contains, "team@");
        let email = MockEmail::new("sender@a.com", "Hello")
            .with_to(&["alice@a.com", "team@company.com"]);
        assert!(rule.matches(&email));
    }

    // ... (string roundtrip tests from Task 1 also live here)
}
```

**Build & test:** `cargo test -p inboxly-bundler`

**Commit:** `feat(bundler): add BundleRule struct with field matching engine`

---

### Task 3: CompiledRule — pre-compiled regex cache

**Files:**
- Modify: `inboxly-bundler/src/rules.rs`

**Rationale:** Regex compilation is expensive. When evaluating rules against every incoming email, we need to compile regex patterns once and reuse them. `CompiledRule` wraps a `BundleRule` with an optional pre-compiled `Regex`.

**Step 1: Define CompiledRule**

```rust
/// A BundleRule with pre-compiled regex for efficient repeated evaluation.
///
/// Created once when rules are loaded from the database. The compiled regex
/// is reused for every email evaluation, avoiding per-email compilation cost.
pub struct CompiledRule {
    pub rule: BundleRule,
    compiled_regex: Option<Regex>,
}

impl CompiledRule {
    /// Compile a BundleRule. If the operator is `Matches` and the regex
    /// pattern is invalid, the rule is still created but will never match.
    pub fn compile(rule: BundleRule) -> Self {
        let compiled_regex = if rule.operator == RuleOp::Matches {
            Regex::new(&rule.value).ok()
        } else {
            None
        };
        Self { rule, compiled_regex }
    }

    /// Test whether this rule matches the given email.
    /// Uses the pre-compiled regex if available.
    pub fn matches(&self, email: &dyn RuleMatchable) -> bool {
        if self.rule.operator == RuleOp::Matches {
            // Use pre-compiled regex instead of re-compiling
            let Some(ref re) = self.compiled_regex else {
                return false; // invalid regex pattern
            };
            match &self.rule.field {
                RuleField::From => re.is_match(email.from_address()),
                RuleField::To => email.to_addresses().iter().any(|a| re.is_match(a)),
                RuleField::Subject => re.is_match(email.subject()),
                RuleField::Header(name) => {
                    email.header(name).map_or(false, |v| re.is_match(v))
                }
                RuleField::Body => {
                    email.body_text().map_or(false, |v| re.is_match(v))
                }
            }
        } else {
            // Non-regex operators delegate to BundleRule::matches
            self.rule.matches(email)
        }
    }
}
```

**Step 2: Add tests**

```rust
#[test]
fn compiled_rule_reuses_regex() {
    let rule = make_rule(RuleField::Subject, RuleOp::Matches, r"(?i)invoice\s+\d+");
    let compiled = CompiledRule::compile(rule);
    assert!(compiled.compiled_regex.is_some());

    let email1 = MockEmail::new("a@b.com", "Invoice 42");
    let email2 = MockEmail::new("a@b.com", "Receipt #99");
    assert!(compiled.matches(&email1));
    assert!(!compiled.matches(&email2));
}

#[test]
fn compiled_rule_invalid_regex_never_matches() {
    let rule = make_rule(RuleField::Subject, RuleOp::Matches, r"[bad");
    let compiled = CompiledRule::compile(rule);
    assert!(compiled.compiled_regex.is_none());
    assert!(!compiled.matches(&MockEmail::new("a@b.com", "anything")));
}

#[test]
fn compiled_rule_non_regex_delegates() {
    let rule = make_rule(RuleField::From, RuleOp::Domain, "example.com");
    let compiled = CompiledRule::compile(rule);
    assert!(compiled.compiled_regex.is_none());
    assert!(compiled.matches(&MockEmail::new("user@example.com", "Hi")));
}
```

**Build & test:** `cargo test -p inboxly-bundler`

**Commit:** `feat(bundler): add CompiledRule with pre-compiled regex caching`

---

### Task 4: BundleRule CRUD operations (store layer)

**Files:**
- Create: `inboxly-bundler/src/rule_store.rs`
- Modify: `inboxly-bundler/src/lib.rs` (add `mod rule_store; pub use rule_store::*;`)

**Step 1: Define the CRUD trait**

This trait abstracts database access so the bundler crate can be tested without a real SQLite connection.

```rust
//! CRUD operations for bundle rules.

use crate::{BundleRule, RuleField, RuleId, RuleOp};
use uuid::Uuid;

/// Error type for rule store operations.
#[derive(Debug, thiserror::Error)]
pub enum RuleStoreError {
    #[error("rule not found: {0}")]
    NotFound(RuleId),
    #[error("invalid rule field: {0}")]
    InvalidField(String),
    #[error("invalid rule operator: {0}")]
    InvalidOperator(String),
    #[error("invalid regex pattern: {0}")]
    InvalidRegex(String),
    #[error("database error: {0}")]
    Database(String),
}

/// Parameters for creating a new bundle rule.
pub struct CreateRuleParams {
    pub bundle_id: Uuid,
    pub field: RuleField,
    pub operator: RuleOp,
    pub value: String,
    pub priority: i32,
}

/// Parameters for updating an existing rule. All fields are optional —
/// only `Some` values are applied.
pub struct UpdateRuleParams {
    pub field: Option<RuleField>,
    pub operator: Option<RuleOp>,
    pub value: Option<String>,
    pub priority: Option<i32>,
    pub bundle_id: Option<Uuid>,
}

/// Trait for bundle rule persistence.
///
/// Implemented by `inboxly-store::SqliteStore` for production use.
/// A mock implementation is used in tests.
pub trait RuleStore {
    /// Create a new rule. Returns the created rule with generated ID.
    fn create_rule(&self, params: CreateRuleParams) -> Result<BundleRule, RuleStoreError>;

    /// Get a rule by ID.
    fn get_rule(&self, id: RuleId) -> Result<BundleRule, RuleStoreError>;

    /// List all rules, ordered by priority descending (highest first).
    fn list_rules(&self) -> Result<Vec<BundleRule>, RuleStoreError>;

    /// List rules for a specific bundle, ordered by priority descending.
    fn list_rules_for_bundle(&self, bundle_id: Uuid) -> Result<Vec<BundleRule>, RuleStoreError>;

    /// Update a rule. Only fields that are `Some` in `params` are changed.
    fn update_rule(&self, id: RuleId, params: UpdateRuleParams) -> Result<BundleRule, RuleStoreError>;

    /// Delete a rule by ID. Returns error if rule doesn't exist.
    fn delete_rule(&self, id: RuleId) -> Result<(), RuleStoreError>;
}
```

**Step 2: Validate regex patterns on create/update**

Add a validation function:

```rust
/// Validate rule parameters before persisting.
pub fn validate_rule(field: &RuleField, operator: &RuleOp, value: &str) -> Result<(), RuleStoreError> {
    if operator == &RuleOp::Matches {
        regex::Regex::new(value).map_err(|e| RuleStoreError::InvalidRegex(e.to_string()))?;
    }
    if operator == &RuleOp::Domain && !matches!(field, RuleField::From) {
        // Domain operator only makes sense with From field — warn but allow.
        // The rule will still try to extract domain from the field value.
    }
    Ok(())
}
```

**Step 3: In-memory mock implementation for tests**

```rust
#[cfg(test)]
pub(crate) mod mock {
    use super::*;
    use std::sync::Mutex;

    pub struct MockRuleStore {
        rules: Mutex<Vec<BundleRule>>,
    }

    impl MockRuleStore {
        pub fn new() -> Self {
            Self { rules: Mutex::new(Vec::new()) }
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
            self.rules.lock().unwrap().push(rule.clone());
            Ok(rule)
        }

        fn get_rule(&self, id: RuleId) -> Result<BundleRule, RuleStoreError> {
            self.rules.lock().unwrap()
                .iter()
                .find(|r| r.id == id)
                .cloned()
                .ok_or(RuleStoreError::NotFound(id))
        }

        fn list_rules(&self) -> Result<Vec<BundleRule>, RuleStoreError> {
            let mut rules = self.rules.lock().unwrap().clone();
            rules.sort_by(|a, b| b.priority.cmp(&a.priority));
            Ok(rules)
        }

        fn list_rules_for_bundle(&self, bundle_id: Uuid) -> Result<Vec<BundleRule>, RuleStoreError> {
            let mut rules: Vec<_> = self.rules.lock().unwrap()
                .iter()
                .filter(|r| r.bundle_id == bundle_id)
                .cloned()
                .collect();
            rules.sort_by(|a, b| b.priority.cmp(&a.priority));
            Ok(rules)
        }

        fn update_rule(&self, id: RuleId, params: UpdateRuleParams) -> Result<BundleRule, RuleStoreError> {
            let mut rules = self.rules.lock().unwrap();
            let rule = rules.iter_mut()
                .find(|r| r.id == id)
                .ok_or(RuleStoreError::NotFound(id))?;
            if let Some(field) = params.field { rule.field = field; }
            if let Some(op) = params.operator { rule.operator = op; }
            if let Some(value) = params.value { rule.value = value; }
            if let Some(priority) = params.priority { rule.priority = priority; }
            if let Some(bundle_id) = params.bundle_id { rule.bundle_id = bundle_id; }
            validate_rule(&rule.field, &rule.operator, &rule.value)?;
            Ok(rule.clone())
        }

        fn delete_rule(&self, id: RuleId) -> Result<(), RuleStoreError> {
            let mut rules = self.rules.lock().unwrap();
            let pos = rules.iter().position(|r| r.id == id)
                .ok_or(RuleStoreError::NotFound(id))?;
            rules.remove(pos);
            Ok(())
        }
    }
}
```

**Step 4: CRUD tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use super::mock::MockRuleStore;

    #[test]
    fn create_and_get_rule() {
        let store = MockRuleStore::new();
        let bundle_id = Uuid::new_v4();
        let rule = store.create_rule(CreateRuleParams {
            bundle_id,
            field: RuleField::From,
            operator: RuleOp::Domain,
            value: "github.com".into(),
            priority: 10,
        }).unwrap();
        assert_eq!(rule.bundle_id, bundle_id);
        assert_eq!(rule.priority, 10);

        let fetched = store.get_rule(rule.id).unwrap();
        assert_eq!(fetched.id, rule.id);
    }

    #[test]
    fn list_rules_sorted_by_priority() {
        let store = MockRuleStore::new();
        let bid = Uuid::new_v4();
        store.create_rule(CreateRuleParams {
            bundle_id: bid, field: RuleField::From,
            operator: RuleOp::Contains, value: "low".into(), priority: 1,
        }).unwrap();
        store.create_rule(CreateRuleParams {
            bundle_id: bid, field: RuleField::From,
            operator: RuleOp::Contains, value: "high".into(), priority: 100,
        }).unwrap();
        let rules = store.list_rules().unwrap();
        assert_eq!(rules[0].value, "high");
        assert_eq!(rules[1].value, "low");
    }

    #[test]
    fn update_rule_partial() {
        let store = MockRuleStore::new();
        let rule = store.create_rule(CreateRuleParams {
            bundle_id: Uuid::new_v4(), field: RuleField::From,
            operator: RuleOp::Contains, value: "old".into(), priority: 5,
        }).unwrap();
        let updated = store.update_rule(rule.id, UpdateRuleParams {
            field: None, operator: None, value: Some("new".into()),
            priority: Some(99), bundle_id: None,
        }).unwrap();
        assert_eq!(updated.value, "new");
        assert_eq!(updated.priority, 99);
        assert_eq!(updated.field, RuleField::From); // unchanged
    }

    #[test]
    fn delete_rule() {
        let store = MockRuleStore::new();
        let rule = store.create_rule(CreateRuleParams {
            bundle_id: Uuid::new_v4(), field: RuleField::Subject,
            operator: RuleOp::Contains, value: "test".into(), priority: 0,
        }).unwrap();
        store.delete_rule(rule.id).unwrap();
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
            bundle_id: Uuid::new_v4(), field: RuleField::Subject,
            operator: RuleOp::Matches, value: "[invalid".into(), priority: 0,
        });
        assert!(result.is_err());
        match result.unwrap_err() {
            RuleStoreError::InvalidRegex(_) => {}
            other => panic!("expected InvalidRegex, got: {other}"),
        }
    }
}
```

**Build & test:** `cargo test -p inboxly-bundler`

**Commit:** `feat(bundler): add RuleStore trait with CRUD operations and mock`

---

### Task 5: Custom bundle creation

**Files:**
- Create: `inboxly-bundler/src/custom_bundle.rs`
- Modify: `inboxly-bundler/src/lib.rs` (add `mod custom_bundle; pub use custom_bundle::*;`)

**Step 1: Define custom bundle types**

```rust
//! Custom user-defined bundles with name and colour.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Parameters for creating a custom bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBundleParams {
    /// User-visible name (e.g., "Work", "Freelance", "Side Project").
    pub name: String,
    /// Title colour as hex string (e.g., "#e06055").
    pub color: String,
    /// Badge background colour as hex string (e.g., "#faebea").
    pub badge_color: String,
    /// Visibility setting.
    pub visibility: BundleVisibility,
    /// Throttle setting.
    pub throttle: BundleThrottle,
}

/// Bundle visibility determines where emails appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BundleVisibility {
    /// Show as a collapsible bundle in the inbox feed.
    Bundled,
    /// Show individual emails in the inbox (no grouping).
    Unbundled,
    /// Don't show in inbox at all — only visible via nav drawer bundle list.
    SkipInbox,
}

/// Bundle throttle controls delivery frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BundleThrottle {
    /// Emails appear as they arrive.
    Immediate,
    /// Bundle surfaces once per day.
    Daily,
    /// Bundle surfaces once per week.
    Weekly,
}

/// Trait for custom bundle persistence.
pub trait BundleStore {
    /// Create a new custom bundle. Returns the bundle ID.
    fn create_bundle(&self, params: CreateBundleParams) -> Result<Uuid, BundleStoreError>;

    /// Update a custom bundle's settings.
    fn update_bundle(&self, id: Uuid, params: UpdateBundleParams) -> Result<(), BundleStoreError>;

    /// Delete a custom bundle and all its rules. Emails in this bundle
    /// become uncategorised and are re-evaluated by the engine.
    fn delete_bundle(&self, id: Uuid) -> Result<(), BundleStoreError>;

    /// List all bundles (built-in + custom), ordered by sort_order.
    fn list_bundles(&self) -> Result<Vec<BundleInfo>, BundleStoreError>;
}

/// Summary info for a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleInfo {
    pub id: Uuid,
    pub name: String,
    pub category: String,
    pub color: String,
    pub badge_color: String,
    pub visibility: BundleVisibility,
    pub throttle: BundleThrottle,
    pub is_custom: bool,
    pub sort_order: i32,
}

/// Parameters for updating a custom bundle.
#[derive(Debug, Clone, Default)]
pub struct UpdateBundleParams {
    pub name: Option<String>,
    pub color: Option<String>,
    pub badge_color: Option<String>,
    pub visibility: Option<BundleVisibility>,
    pub throttle: Option<BundleThrottle>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, thiserror::Error)]
pub enum BundleStoreError {
    #[error("bundle not found: {0}")]
    NotFound(Uuid),
    #[error("bundle name already exists: {0}")]
    DuplicateName(String),
    #[error("cannot delete built-in bundle: {0}")]
    BuiltIn(Uuid),
    #[error("database error: {0}")]
    Database(String),
}
```

**Step 2: Add tests with mock**

Similar pattern to Task 4 — create a `MockBundleStore` and test create/update/delete/list flows. Tests should verify:
- Creating a custom bundle returns a valid UUID
- Updating name/colour works
- Deleting a custom bundle succeeds
- Attempting to delete a built-in bundle fails
- List returns bundles in sort_order

**Build & test:** `cargo test -p inboxly-bundler`

**Commit:** `feat(bundler): add custom bundle creation with BundleStore trait`

---

### Task 6: SenderAffinity struct and confidence model

**Files:**
- Create: `inboxly-bundler/src/affinity.rs`
- Modify: `inboxly-bundler/src/lib.rs` (add `mod affinity; pub use affinity::*;`)

**Step 1: Define SenderAffinity and confidence constants**

```rust
//! Sender affinity tracking — learns which bundle a sender belongs to
//! based on user behaviour (manually moving emails between bundles).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Minimum confidence required for sender learning to override header heuristics.
/// Below this threshold, the sender learning result is ignored and the email
/// falls through to header heuristics.
pub const CONFIDENCE_THRESHOLD: f32 = 0.6;

/// Maximum confidence value. Reached after ~5 consistent user actions.
pub const CONFIDENCE_MAX: f32 = 1.0;

/// Confidence increment per user action (move email to bundle).
/// 5 actions: 0.0 → 0.2 → 0.4 → 0.6 → 0.8 → 1.0
pub const CONFIDENCE_INCREMENT: f32 = 0.2;

/// Confidence decrement when user overrides (moves to a different bundle
/// than the learned one). Applied to the OLD affinity before creating
/// or boosting the new one.
pub const CONFIDENCE_OVERRIDE_PENALTY: f32 = 0.3;

/// Half-life for confidence decay in days. After this many days without
/// reinforcement, confidence drops to half its value.
pub const CONFIDENCE_HALF_LIFE_DAYS: f64 = 90.0;

/// A learned association between a sender and a bundle category.
///
/// When a user manually moves an email from sender X to bundle Y,
/// we record (or reinforce) an affinity entry. Future emails from
/// sender X are auto-categorised into bundle Y if confidence exceeds
/// the threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderAffinity {
    /// The sender's email domain (e.g., "github.com").
    pub sender_domain: String,
    /// The sender's full email address (e.g., "noreply@github.com").
    pub sender_address: String,
    /// Which bundle category this sender is associated with.
    pub bundle_category: String,
    /// Confidence score [0.0, 1.0]. Higher = more confident.
    pub confidence: f32,
    /// When this affinity was last reinforced by a user action.
    pub learned_at: DateTime<Utc>,
}
```

**Step 2: Implement confidence calculations as pure functions**

```rust
impl SenderAffinity {
    /// Calculate the effective confidence after time-based decay.
    ///
    /// Uses exponential decay: `confidence * 2^(-days_elapsed / half_life)`.
    /// This means confidence halves every `CONFIDENCE_HALF_LIFE_DAYS` days
    /// without reinforcement.
    pub fn effective_confidence(&self, now: DateTime<Utc>) -> f32 {
        let days_elapsed = (now - self.learned_at).num_seconds() as f64 / 86400.0;
        if days_elapsed <= 0.0 {
            return self.confidence;
        }
        let decay_factor = 2.0_f64.powf(-days_elapsed / CONFIDENCE_HALF_LIFE_DAYS);
        (self.confidence as f64 * decay_factor) as f32
    }

    /// Whether this affinity's effective confidence exceeds the threshold.
    pub fn is_confident(&self, now: DateTime<Utc>) -> bool {
        self.effective_confidence(now) >= CONFIDENCE_THRESHOLD
    }

    /// Reinforce this affinity — user moved another email from this sender
    /// to the same bundle. Bumps confidence and resets the decay clock.
    pub fn reinforce(&mut self, now: DateTime<Utc>) {
        self.confidence = (self.confidence + CONFIDENCE_INCREMENT).min(CONFIDENCE_MAX);
        self.learned_at = now;
    }

    /// Apply override penalty — user moved an email from this sender to
    /// a DIFFERENT bundle. Reduces confidence of this (old) affinity.
    pub fn penalize(&mut self) {
        self.confidence = (self.confidence - CONFIDENCE_OVERRIDE_PENALTY).max(0.0);
    }
}
```

**Step 3: Comprehensive tests for confidence model**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_affinity(confidence: f32, days_ago: i64) -> SenderAffinity {
        let now = Utc::now();
        SenderAffinity {
            sender_domain: "example.com".into(),
            sender_address: "bot@example.com".into(),
            bundle_category: "promos".into(),
            confidence,
            learned_at: now - Duration::days(days_ago),
        }
    }

    #[test]
    fn fresh_affinity_no_decay() {
        let a = make_affinity(0.8, 0);
        let eff = a.effective_confidence(Utc::now());
        assert!((eff - 0.8).abs() < 0.01);
    }

    #[test]
    fn decay_at_half_life() {
        let a = make_affinity(1.0, CONFIDENCE_HALF_LIFE_DAYS as i64);
        let eff = a.effective_confidence(Utc::now());
        // After one half-life, confidence should be ~0.5
        assert!((eff - 0.5).abs() < 0.05);
    }

    #[test]
    fn decay_at_two_half_lives() {
        let a = make_affinity(1.0, (CONFIDENCE_HALF_LIFE_DAYS * 2.0) as i64);
        let eff = a.effective_confidence(Utc::now());
        // After two half-lives, confidence should be ~0.25
        assert!((eff - 0.25).abs() < 0.05);
    }

    #[test]
    fn below_threshold_after_decay() {
        // Start at 0.8, after 90 days (1 half-life) → ~0.4, below 0.6 threshold
        let a = make_affinity(0.8, 90);
        assert!(!a.is_confident(Utc::now()));
    }

    #[test]
    fn above_threshold_when_fresh() {
        let a = make_affinity(0.8, 0);
        assert!(a.is_confident(Utc::now()));
    }

    #[test]
    fn reinforce_increases_confidence() {
        let mut a = make_affinity(0.4, 10);
        let now = Utc::now();
        a.reinforce(now);
        assert!((a.confidence - 0.6).abs() < 0.001);
        assert_eq!(a.learned_at, now); // decay clock reset
    }

    #[test]
    fn reinforce_caps_at_max() {
        let mut a = make_affinity(0.9, 0);
        a.reinforce(Utc::now());
        assert!((a.confidence - 1.0).abs() < 0.001);
    }

    #[test]
    fn penalize_decreases_confidence() {
        let mut a = make_affinity(0.8, 0);
        a.penalize();
        assert!((a.confidence - 0.5).abs() < 0.001);
    }

    #[test]
    fn penalize_floors_at_zero() {
        let mut a = make_affinity(0.1, 0);
        a.penalize();
        assert!((a.confidence).abs() < 0.001);
    }

    #[test]
    fn five_reinforcements_reach_max() {
        let mut a = SenderAffinity {
            sender_domain: "test.com".into(),
            sender_address: "a@test.com".into(),
            bundle_category: "social".into(),
            confidence: 0.0,
            learned_at: Utc::now(),
        };
        let now = Utc::now();
        for _ in 0..5 {
            a.reinforce(now);
        }
        assert!((a.confidence - 1.0).abs() < 0.001);
    }
}
```

**Build & test:** `cargo test -p inboxly-bundler`

**Commit:** `feat(bundler): add SenderAffinity with confidence decay model`

---

### Task 7: AffinityStore trait for sender affinity persistence

**Files:**
- Modify: `inboxly-bundler/src/affinity.rs`

**Step 1: Define the persistence trait**

```rust
/// Trait for sender affinity persistence.
///
/// Implemented by `inboxly-store::SqliteStore` for production use.
pub trait AffinityStore {
    /// Look up the strongest affinity for a sender address.
    /// Returns the affinity with the highest confidence for this address.
    /// If no address-level affinity exists, falls back to domain-level.
    fn get_affinity(&self, sender_address: &str) -> Result<Option<SenderAffinity>, AffinityStoreError>;

    /// Record or reinforce an affinity. If an affinity already exists for
    /// this sender+category, reinforce it. If it exists for a different
    /// category, penalize the old one and create/reinforce the new one.
    fn record_affinity(
        &self,
        sender_address: &str,
        sender_domain: &str,
        bundle_category: &str,
        now: DateTime<Utc>,
    ) -> Result<SenderAffinity, AffinityStoreError>;

    /// List all affinities (for settings UI / export).
    fn list_affinities(&self) -> Result<Vec<SenderAffinity>, AffinityStoreError>;

    /// Delete a specific affinity (user wants to "unlearn" a sender).
    fn delete_affinity(&self, sender_address: &str) -> Result<(), AffinityStoreError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AffinityStoreError {
    #[error("database error: {0}")]
    Database(String),
}
```

**Step 2: Add `record_affinity` logic documentation**

The `record_affinity` method implements the core learning loop:

1. Query existing affinities for `sender_address`
2. If an affinity exists for the SAME `bundle_category`: call `reinforce()`, update in DB
3. If an affinity exists for a DIFFERENT `bundle_category`: call `penalize()` on the old one (update in DB), then upsert a new affinity for the new category with `CONFIDENCE_INCREMENT`
4. If no affinity exists: insert a new one with `confidence = CONFIDENCE_INCREMENT`

**Step 3: Mock implementation and tests**

Follow the same pattern as Task 4 — `MockAffinityStore` with `Mutex<Vec<SenderAffinity>>`, testing:
- First move creates affinity at 0.2
- Second move to same bundle reinforces to 0.4
- Move to different bundle penalises old, creates new
- `get_affinity` returns highest-confidence entry
- Domain fallback when no address-level match

**Build & test:** `cargo test -p inboxly-bundler`

**Commit:** `feat(bundler): add AffinityStore trait for sender learning persistence`

---

### Task 8: Rule evaluation engine — evaluate_rules()

**Files:**
- Create: `inboxly-bundler/src/evaluator.rs`
- Modify: `inboxly-bundler/src/lib.rs` (add `mod evaluator; pub use evaluator::*;`)

**Step 1: Define the evaluator that runs user rules**

```rust
//! Rule evaluation engine — evaluates user-defined rules against an email.

use crate::{BundleRule, CompiledRule, RuleMatchable};
use uuid::Uuid;

/// Result of evaluating an email against user rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleResult {
    /// A user rule matched — email should go to this bundle.
    Matched { bundle_id: Uuid, rule_id: Uuid },
    /// No user rule matched — fall through to next evaluation layer.
    NoMatch,
}

/// Evaluate an email against a list of compiled rules.
///
/// Rules must be pre-sorted by priority descending (highest first).
/// Returns the first matching rule. This is the "User Rules" layer
/// in the evaluation order.
pub fn evaluate_rules(rules: &[CompiledRule], email: &dyn RuleMatchable) -> RuleResult {
    for compiled in rules {
        if compiled.matches(email) {
            return RuleResult::Matched {
                bundle_id: compiled.rule.bundle_id,
                rule_id: compiled.rule.id,
            };
        }
    }
    RuleResult::NoMatch
}
```

**Step 2: Tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BundleRule, CompiledRule, RuleField, RuleOp};
    // Re-use MockEmail from rules.rs tests (extract to a shared test_utils module
    // or duplicate the minimal struct).

    #[test]
    fn first_matching_rule_wins() {
        let bundle_a = Uuid::new_v4();
        let bundle_b = Uuid::new_v4();
        let rules = vec![
            CompiledRule::compile(BundleRule {
                id: Uuid::new_v4(), bundle_id: bundle_a,
                field: RuleField::From, operator: RuleOp::Domain,
                value: "github.com".into(), priority: 100,
            }),
            CompiledRule::compile(BundleRule {
                id: Uuid::new_v4(), bundle_id: bundle_b,
                field: RuleField::Subject, operator: RuleOp::Contains,
                value: "PR".into(), priority: 50,
            }),
        ];
        // Email matches both rules — highest priority (first in list) wins
        let email = MockEmail::new("noreply@github.com", "PR #42 merged");
        match evaluate_rules(&rules, &email) {
            RuleResult::Matched { bundle_id, .. } => assert_eq!(bundle_id, bundle_a),
            RuleResult::NoMatch => panic!("expected match"),
        }
    }

    #[test]
    fn no_rules_returns_no_match() {
        let email = MockEmail::new("alice@example.com", "Hello");
        assert_eq!(evaluate_rules(&[], &email), RuleResult::NoMatch);
    }

    #[test]
    fn no_matching_rule_returns_no_match() {
        let rules = vec![
            CompiledRule::compile(BundleRule {
                id: Uuid::new_v4(), bundle_id: Uuid::new_v4(),
                field: RuleField::From, operator: RuleOp::Domain,
                value: "gitlab.com".into(), priority: 10,
            }),
        ];
        let email = MockEmail::new("alice@github.com", "Hello");
        assert_eq!(evaluate_rules(&rules, &email), RuleResult::NoMatch);
    }
}
```

**Build & test:** `cargo test -p inboxly-bundler`

**Commit:** `feat(bundler): add rule evaluation engine`

---

### Task 9: Sender learning evaluation — evaluate_affinity()

**Files:**
- Modify: `inboxly-bundler/src/evaluator.rs`

**Step 1: Define affinity evaluation**

```rust
use crate::{SenderAffinity, CONFIDENCE_THRESHOLD};
use chrono::{DateTime, Utc};

/// Result of evaluating sender affinity.
#[derive(Debug, Clone, PartialEq)]
pub enum AffinityResult {
    /// Sender has a learned affinity above the confidence threshold.
    Confident {
        bundle_category: String,
        confidence: f32,
    },
    /// Sender has an affinity but it's below the threshold (decayed or weak).
    BelowThreshold {
        bundle_category: String,
        confidence: f32,
    },
    /// No affinity data exists for this sender.
    Unknown,
}

/// Evaluate sender affinity for an email.
///
/// Looks up the sender's address in the affinity data. Returns
/// `Confident` if effective confidence (after decay) exceeds the threshold.
/// This is the "Sender Learning" layer in the evaluation order.
pub fn evaluate_affinity(
    affinity: Option<&SenderAffinity>,
    now: DateTime<Utc>,
) -> AffinityResult {
    let Some(aff) = affinity else {
        return AffinityResult::Unknown;
    };
    let eff = aff.effective_confidence(now);
    if eff >= CONFIDENCE_THRESHOLD {
        AffinityResult::Confident {
            bundle_category: aff.bundle_category.clone(),
            confidence: eff,
        }
    } else {
        AffinityResult::BelowThreshold {
            bundle_category: aff.bundle_category.clone(),
            confidence: eff,
        }
    }
}
```

**Step 2: Tests**

```rust
#[test]
fn affinity_confident_returns_category() {
    let aff = SenderAffinity {
        sender_domain: "shop.com".into(),
        sender_address: "deals@shop.com".into(),
        bundle_category: "promos".into(),
        confidence: 0.8,
        learned_at: Utc::now(),
    };
    match evaluate_affinity(Some(&aff), Utc::now()) {
        AffinityResult::Confident { bundle_category, confidence } => {
            assert_eq!(bundle_category, "promos");
            assert!(confidence > 0.7);
        }
        _ => panic!("expected Confident"),
    }
}

#[test]
fn affinity_below_threshold_after_decay() {
    let aff = SenderAffinity {
        sender_domain: "shop.com".into(),
        sender_address: "deals@shop.com".into(),
        bundle_category: "promos".into(),
        confidence: 0.7,
        learned_at: Utc::now() - chrono::Duration::days(120),
    };
    match evaluate_affinity(Some(&aff), Utc::now()) {
        AffinityResult::BelowThreshold { .. } => {}
        other => panic!("expected BelowThreshold, got {other:?}"),
    }
}

#[test]
fn no_affinity_returns_unknown() {
    assert!(matches!(evaluate_affinity(None, Utc::now()), AffinityResult::Unknown));
}
```

**Build & test:** `cargo test -p inboxly-bundler`

**Commit:** `feat(bundler): add sender affinity evaluation with confidence threshold`

---

### Task 10: Full evaluation pipeline — BundlerEngine::categorise()

**Files:**
- Create: `inboxly-bundler/src/engine.rs` (or modify if M12 already created it)
- Modify: `inboxly-bundler/src/lib.rs`

**Context:** M12 introduced the bundler engine with header heuristics (Layer 1). This task adds Layers 2 and 3 above it and wires the full evaluation order.

**Step 1: Define CategoriseResult**

```rust
//! Bundler engine — full evaluation pipeline.
//!
//! Evaluation order (highest precedence first):
//! 1. User rules (explicit control)
//! 2. Sender learning (if confidence > threshold)
//! 3. Header heuristics (zero-config patterns)
//! 4. Uncategorised (stays in primary inbox)

use crate::{
    AffinityResult, AffinityStore, CompiledRule, RuleMatchable,
    RuleResult, SenderAffinity,
    evaluate_affinity, evaluate_rules,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// The source that determined an email's categorisation.
#[derive(Debug, Clone, PartialEq)]
pub enum CategoriseSource {
    /// Matched a user-defined rule.
    UserRule { rule_id: Uuid },
    /// Matched sender learning with sufficient confidence.
    SenderLearning { confidence: f32 },
    /// Matched a header heuristic pattern.
    HeaderHeuristic,
    /// No categorisation — email stays in primary inbox.
    Uncategorised,
}

/// Result of categorising an email through the full pipeline.
#[derive(Debug, Clone)]
pub struct CategoriseResult {
    /// The bundle to assign this email to, or None for uncategorised.
    pub bundle_id: Option<Uuid>,
    /// The bundle category name (e.g., "social", "promos"), or None.
    pub bundle_category: Option<String>,
    /// Which layer produced this result.
    pub source: CategoriseSource,
}
```

**Step 2: Implement the pipeline**

```rust
/// The bundler engine holds pre-loaded rules and provides the full
/// categorisation pipeline.
pub struct BundlerEngine {
    /// User rules, pre-compiled and sorted by priority descending.
    compiled_rules: Vec<CompiledRule>,
    /// Category-to-bundle-id mapping for sender learning results.
    /// Populated from the bundles table (e.g., "social" → UUID of Social bundle).
    category_bundle_map: std::collections::HashMap<String, Uuid>,
}

impl BundlerEngine {
    /// Create a new engine with the given rules and category mapping.
    pub fn new(
        rules: Vec<CompiledRule>,
        category_bundle_map: std::collections::HashMap<String, Uuid>,
    ) -> Self {
        Self { compiled_rules: rules, category_bundle_map }
    }

    /// Reload rules (e.g., after user creates/edits a rule).
    pub fn reload_rules(&mut self, rules: Vec<CompiledRule>) {
        self.compiled_rules = rules;
    }

    /// Categorise an email through the full four-layer pipeline.
    ///
    /// Arguments:
    /// - `email`: the email to categorise (implements RuleMatchable)
    /// - `sender_affinity`: pre-fetched affinity for this sender (None if unknown)
    /// - `heuristic_result`: result from M12's header heuristic engine (None = no match)
    /// - `now`: current timestamp for confidence decay calculation
    pub fn categorise(
        &self,
        email: &dyn RuleMatchable,
        sender_affinity: Option<&SenderAffinity>,
        heuristic_result: Option<HeuristicMatch>,
        now: DateTime<Utc>,
    ) -> CategoriseResult {
        // Layer 1: User rules (highest priority)
        if let RuleResult::Matched { bundle_id, rule_id } =
            evaluate_rules(&self.compiled_rules, email)
        {
            return CategoriseResult {
                bundle_id: Some(bundle_id),
                bundle_category: None, // user rules map directly to bundle_id
                source: CategoriseSource::UserRule { rule_id },
            };
        }

        // Layer 2: Sender learning (if confident)
        if let AffinityResult::Confident { bundle_category, confidence } =
            evaluate_affinity(sender_affinity, now)
        {
            let bundle_id = self.category_bundle_map.get(&bundle_category).copied();
            return CategoriseResult {
                bundle_id,
                bundle_category: Some(bundle_category),
                source: CategoriseSource::SenderLearning { confidence },
            };
        }

        // Layer 3: Header heuristics (from M12)
        if let Some(heuristic) = heuristic_result {
            let bundle_id = self.category_bundle_map.get(&heuristic.category).copied();
            return CategoriseResult {
                bundle_id,
                bundle_category: Some(heuristic.category),
                source: CategoriseSource::HeaderHeuristic,
            };
        }

        // Layer 4: Uncategorised
        CategoriseResult {
            bundle_id: None,
            bundle_category: None,
            source: CategoriseSource::Uncategorised,
        }
    }
}

/// A header heuristic match from M12's engine.
/// This type is defined here so M12's heuristic engine can return it.
#[derive(Debug, Clone)]
pub struct HeuristicMatch {
    /// The matched category (e.g., "social", "promos", "forums").
    pub category: String,
    /// Which heuristic pattern matched (for debugging/logging).
    pub pattern: String,
}
```

**Step 3: Comprehensive pipeline tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn setup_engine() -> (BundlerEngine, Uuid, Uuid) {
        let social_id = Uuid::new_v4();
        let promos_id = Uuid::new_v4();
        let custom_id = Uuid::new_v4();

        let mut category_map = HashMap::new();
        category_map.insert("social".to_string(), social_id);
        category_map.insert("promos".to_string(), promos_id);

        // One user rule: github.com → custom bundle
        let rules = vec![
            CompiledRule::compile(BundleRule {
                id: Uuid::new_v4(),
                bundle_id: custom_id,
                field: RuleField::From,
                operator: RuleOp::Domain,
                value: "github.com".into(),
                priority: 100,
            }),
        ];

        (BundlerEngine::new(rules, category_map), social_id, custom_id)
    }

    #[test]
    fn user_rule_beats_sender_learning() {
        let (engine, _, custom_id) = setup_engine();
        let email = MockEmail::new("noreply@github.com", "PR merged");
        // Even with a strong social affinity, user rule wins
        let affinity = SenderAffinity {
            sender_domain: "github.com".into(),
            sender_address: "noreply@github.com".into(),
            bundle_category: "social".into(),
            confidence: 1.0,
            learned_at: Utc::now(),
        };
        let result = engine.categorise(&email, Some(&affinity), None, Utc::now());
        assert_eq!(result.bundle_id, Some(custom_id));
        assert!(matches!(result.source, CategoriseSource::UserRule { .. }));
    }

    #[test]
    fn sender_learning_beats_heuristic() {
        let (engine, _, _) = setup_engine();
        let email = MockEmail::new("deals@shop.com", "50% off today!");
        let affinity = SenderAffinity {
            sender_domain: "shop.com".into(),
            sender_address: "deals@shop.com".into(),
            bundle_category: "promos".into(),
            confidence: 0.8,
            learned_at: Utc::now(),
        };
        let heuristic = HeuristicMatch {
            category: "social".into(), // heuristic says social
            pattern: "List-Id".into(),
        };
        let result = engine.categorise(&email, Some(&affinity), Some(heuristic), Utc::now());
        // Sender learning wins over heuristic
        assert_eq!(result.bundle_category, Some("promos".into()));
        assert!(matches!(result.source, CategoriseSource::SenderLearning { .. }));
    }

    #[test]
    fn low_confidence_falls_through_to_heuristic() {
        let (engine, social_id, _) = setup_engine();
        let email = MockEmail::new("bot@facebook.com", "Friend request");
        let affinity = SenderAffinity {
            sender_domain: "facebook.com".into(),
            sender_address: "bot@facebook.com".into(),
            bundle_category: "promos".into(),
            confidence: 0.3, // below threshold
            learned_at: Utc::now(),
        };
        let heuristic = HeuristicMatch {
            category: "social".into(),
            pattern: "From domain".into(),
        };
        let result = engine.categorise(&email, Some(&affinity), Some(heuristic), Utc::now());
        assert_eq!(result.bundle_category, Some("social".into()));
        assert!(matches!(result.source, CategoriseSource::HeaderHeuristic));
    }

    #[test]
    fn no_match_returns_uncategorised() {
        let (engine, _, _) = setup_engine();
        let email = MockEmail::new("friend@personal.com", "Dinner tonight?");
        let result = engine.categorise(&email, None, None, Utc::now());
        assert!(result.bundle_id.is_none());
        assert!(matches!(result.source, CategoriseSource::Uncategorised));
    }

    #[test]
    fn decayed_affinity_falls_through() {
        let (engine, social_id, _) = setup_engine();
        let email = MockEmail::new("news@example.com", "Newsletter");
        // High confidence but very old → decayed below threshold
        let affinity = SenderAffinity {
            sender_domain: "example.com".into(),
            sender_address: "news@example.com".into(),
            bundle_category: "promos".into(),
            confidence: 0.7,
            learned_at: Utc::now() - chrono::Duration::days(200),
        };
        let heuristic = HeuristicMatch {
            category: "social".into(),
            pattern: "List-Id".into(),
        };
        let result = engine.categorise(&email, Some(&affinity), Some(heuristic), Utc::now());
        // Decayed affinity below threshold → falls through to heuristic
        assert!(matches!(result.source, CategoriseSource::HeaderHeuristic));
    }
}
```

**Build & test:** `cargo test -p inboxly-bundler`

**Commit:** `feat(bundler): add full four-layer evaluation pipeline in BundlerEngine`

---

### Task 11: Re-categorise on user move

**Files:**
- Create: `inboxly-bundler/src/recategorise.rs`
- Modify: `inboxly-bundler/src/lib.rs`

**Step 1: Define the re-categorisation action**

When a user manually moves an email/thread to a different bundle, two things happen:
1. The `thread_state.bundle_id` is updated (immediate effect)
2. The `sender_affinity` is updated (learning for future emails)

```rust
//! Re-categorisation logic — handles user manually moving emails between bundles.

use crate::{AffinityStore, SenderAffinity, CONFIDENCE_INCREMENT};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Describes a user's manual move action.
pub struct MoveAction {
    /// The thread being moved.
    pub thread_id: Uuid,
    /// The sender's email address.
    pub sender_address: String,
    /// The sender's domain.
    pub sender_domain: String,
    /// The target bundle's category name (e.g., "social", "promos", or custom name).
    pub target_bundle_category: String,
    /// The target bundle's ID.
    pub target_bundle_id: Uuid,
}

/// Result of processing a move action.
pub struct MoveResult {
    /// The updated/created sender affinity.
    pub affinity: SenderAffinity,
    /// Whether this was a new affinity (true) or reinforcement of existing (false).
    pub is_new: bool,
    /// If the sender had a previous different affinity, this contains the old category
    /// (indicating an override that penalised the old affinity).
    pub overridden_category: Option<String>,
}

/// Process a user's manual move of an email to a different bundle.
///
/// This function:
/// 1. Looks up existing affinity for the sender
/// 2. If same category: reinforces (bumps confidence, resets decay clock)
/// 3. If different category: penalises old, creates/reinforces new
/// 4. If no prior affinity: creates new at CONFIDENCE_INCREMENT
///
/// The caller is responsible for updating `thread_state.bundle_id` separately.
pub fn process_move<S: AffinityStore>(
    store: &S,
    action: &MoveAction,
    now: DateTime<Utc>,
) -> Result<MoveResult, crate::affinity::AffinityStoreError> {
    let affinity = store.record_affinity(
        &action.sender_address,
        &action.sender_domain,
        &action.target_bundle_category,
        now,
    )?;

    // Determine if this was an override by checking if previous affinity
    // had a different category (the store handles penalisation internally)
    let existing = store.get_affinity(&action.sender_address)?;
    let overridden_category = existing
        .filter(|a| a.bundle_category != action.target_bundle_category)
        .map(|a| a.bundle_category);

    Ok(MoveResult {
        is_new: affinity.confidence <= CONFIDENCE_INCREMENT + 0.001,
        affinity,
        overridden_category,
    })
}
```

**Step 2: Tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_move_creates_new_affinity() {
        let store = MockAffinityStore::new();
        let action = MoveAction {
            thread_id: Uuid::new_v4(),
            sender_address: "news@example.com".into(),
            sender_domain: "example.com".into(),
            target_bundle_category: "promos".into(),
            target_bundle_id: Uuid::new_v4(),
        };
        let result = process_move(&store, &action, Utc::now()).unwrap();
        assert!(result.is_new);
        assert!((result.affinity.confidence - 0.2).abs() < 0.001);
    }

    #[test]
    fn repeated_move_reinforces() {
        let store = MockAffinityStore::new();
        let action = MoveAction {
            thread_id: Uuid::new_v4(),
            sender_address: "news@example.com".into(),
            sender_domain: "example.com".into(),
            target_bundle_category: "promos".into(),
            target_bundle_id: Uuid::new_v4(),
        };
        let now = Utc::now();
        process_move(&store, &action, now).unwrap();
        let result = process_move(&store, &action, now).unwrap();
        assert!(!result.is_new);
        assert!((result.affinity.confidence - 0.4).abs() < 0.001);
    }

    #[test]
    fn move_to_different_bundle_overrides() {
        let store = MockAffinityStore::new();
        let addr = "bot@social.com";
        let domain = "social.com";
        let now = Utc::now();

        // First: learn as "social"
        let action1 = MoveAction {
            thread_id: Uuid::new_v4(),
            sender_address: addr.into(),
            sender_domain: domain.into(),
            target_bundle_category: "social".into(),
            target_bundle_id: Uuid::new_v4(),
        };
        process_move(&store, &action1, now).unwrap();

        // Then: override to "promos"
        let action2 = MoveAction {
            thread_id: Uuid::new_v4(),
            sender_address: addr.into(),
            sender_domain: domain.into(),
            target_bundle_category: "promos".into(),
            target_bundle_id: Uuid::new_v4(),
        };
        let result = process_move(&store, &action2, now).unwrap();
        assert_eq!(result.affinity.bundle_category, "promos");
    }
}
```

**Build & test:** `cargo test -p inboxly-bundler`

**Commit:** `feat(bundler): add re-categorisation logic for user moves`

---

### Task 12: Extract shared test utilities

**Files:**
- Create: `inboxly-bundler/src/test_utils.rs`
- Modify: `inboxly-bundler/src/rules.rs`, `inboxly-bundler/src/evaluator.rs`

**Step 1: Move MockEmail and helper functions to a shared module**

The `MockEmail` struct and `make_rule()` helper are duplicated across test modules. Extract them into `test_utils.rs` (gated behind `#[cfg(test)]`).

```rust
//! Shared test utilities for the bundler crate.

#[cfg(test)]
pub(crate) mod fixtures {
    use crate::{BundleRule, RuleField, RuleMatchable, RuleOp};
    use std::collections::HashMap;
    use uuid::Uuid;

    /// Test double implementing RuleMatchable.
    pub struct MockEmail {
        pub from: String,
        pub to: Vec<String>,
        pub subject: String,
        pub headers: HashMap<String, String>,
        pub body: Option<String>,
    }

    impl MockEmail {
        pub fn new(from: &str, subject: &str) -> Self { /* ... */ }
        pub fn with_to(mut self, to: &[&str]) -> Self { /* ... */ }
        pub fn with_header(mut self, name: &str, value: &str) -> Self { /* ... */ }
        pub fn with_body(mut self, body: &str) -> Self { /* ... */ }
    }

    impl RuleMatchable for MockEmail {
        fn from_address(&self) -> &str { &self.from }
        fn to_addresses(&self) -> &[String] { &self.to }
        fn subject(&self) -> &str { &self.subject }
        fn header(&self, name: &str) -> Option<&str> {
            self.headers.get(name).map(|s| s.as_str())
        }
        fn body_text(&self) -> Option<&str> { self.body.as_deref() }
    }

    pub fn make_rule(field: RuleField, op: RuleOp, value: &str) -> BundleRule {
        BundleRule {
            id: Uuid::new_v4(),
            bundle_id: Uuid::new_v4(),
            field,
            operator: op,
            value: value.to_string(),
            priority: 0,
        }
    }
}
```

**Step 2: Update existing tests to use `crate::test_utils::fixtures::*`**

Replace inline MockEmail definitions in `rules.rs` and `evaluator.rs` tests with imports from the shared module.

**Build & test:** `cargo test -p inboxly-bundler` — all existing tests must still pass.

**Commit:** `refactor(bundler): extract shared test fixtures to test_utils module`

---

### Task 13: Integration test — full pipeline end-to-end

**Files:**
- Create: `inboxly-bundler/tests/integration_pipeline.rs`

**Step 1: Write an integration test that exercises the complete flow**

This test simulates the real-world scenario:
1. Create rules in a mock store
2. Create sender affinities in a mock store
3. Run emails through the full pipeline
4. Verify correct categorisation at each layer
5. Simulate user moves and verify re-categorisation
6. Verify confidence decay over time

```rust
//! Integration test for the full bundler pipeline.

use inboxly_bundler::*;
use chrono::{Duration, Utc};
use std::collections::HashMap;
use uuid::Uuid;

#[test]
fn full_pipeline_four_layers() {
    // Setup: bundle IDs
    let social_id = Uuid::new_v4();
    let promos_id = Uuid::new_v4();
    let custom_work_id = Uuid::new_v4();

    let mut category_map = HashMap::new();
    category_map.insert("social".to_string(), social_id);
    category_map.insert("promos".to_string(), promos_id);

    // User rule: anything from @work.com → custom "Work" bundle
    let rules = vec![
        CompiledRule::compile(BundleRule {
            id: Uuid::new_v4(),
            bundle_id: custom_work_id,
            field: RuleField::From,
            operator: RuleOp::Domain,
            value: "work.com".into(),
            priority: 100,
        }),
    ];

    let engine = BundlerEngine::new(rules, category_map);
    let now = Utc::now();

    // Email 1: from @work.com → user rule wins
    let e1 = MockEmail::new("boss@work.com", "Q1 Review");
    let r1 = engine.categorise(&e1, None, None, now);
    assert_eq!(r1.bundle_id, Some(custom_work_id));
    assert!(matches!(r1.source, CategoriseSource::UserRule { .. }));

    // Email 2: from @shop.com with strong affinity → sender learning
    let aff = SenderAffinity {
        sender_domain: "shop.com".into(),
        sender_address: "deals@shop.com".into(),
        bundle_category: "promos".into(),
        confidence: 0.8,
        learned_at: now,
    };
    let e2 = MockEmail::new("deals@shop.com", "Sale ends today!");
    let r2 = engine.categorise(&e2, Some(&aff), None, now);
    assert!(matches!(r2.source, CategoriseSource::SenderLearning { .. }));

    // Email 3: from unknown sender with List-Id header → heuristic
    let e3 = MockEmail::new("bot@forum.org", "New post in thread");
    let heuristic = HeuristicMatch {
        category: "social".into(),
        pattern: "List-Id present".into(),
    };
    let r3 = engine.categorise(&e3, None, Some(heuristic), now);
    assert!(matches!(r3.source, CategoriseSource::HeaderHeuristic));

    // Email 4: personal email, no matches → uncategorised
    let e4 = MockEmail::new("friend@personal.com", "Dinner?");
    let r4 = engine.categorise(&e4, None, None, now);
    assert!(matches!(r4.source, CategoriseSource::Uncategorised));
}

#[test]
fn confidence_lifecycle() {
    // Simulate: user moves emails 5 times → full confidence → time passes → decay
    let now = Utc::now();
    let mut aff = SenderAffinity {
        sender_domain: "newsletter.com".into(),
        sender_address: "weekly@newsletter.com".into(),
        bundle_category: "promos".into(),
        confidence: 0.0,
        learned_at: now,
    };

    // 5 reinforcements → max confidence
    for _ in 0..5 {
        aff.reinforce(now);
    }
    assert!((aff.confidence - 1.0).abs() < 0.001);
    assert!(aff.is_confident(now));

    // After 90 days (1 half-life) → confidence ~0.5 → below threshold
    let future = now + Duration::days(90);
    assert!(!aff.is_confident(future));

    // User reinforces again → resets clock, bumps confidence
    aff.reinforce(future);
    assert!(aff.is_confident(future));
}
```

**Build & test:** `cargo test -p inboxly-bundler`

**Commit:** `test(bundler): add integration test for full categorisation pipeline`

---

### Task 14: Wire up lib.rs exports and documentation

**Files:**
- Modify: `inboxly-bundler/src/lib.rs`
- Modify: `inboxly-bundler/Cargo.toml` (verify dependencies)

**Step 1: Final lib.rs structure**

```rust
//! Inboxly bundler — email categorisation engine.
//!
//! Three-layer categorisation with clear precedence:
//! 1. User rules (highest priority) — explicit pattern matching
//! 2. Sender learning (if confidence > threshold) — learns from user moves
//! 3. Header heuristics (zero config) — pattern matching on email headers
//!
//! Emails that don't match any layer remain uncategorised in the primary inbox.

pub mod affinity;
pub mod custom_bundle;
pub mod engine;
pub mod evaluator;
pub mod heuristics; // from M12
pub mod recategorise;
pub mod rule_store;
pub mod rules;

#[cfg(test)]
mod test_utils;

// Re-export primary types
pub use affinity::{
    AffinityStore, AffinityStoreError, SenderAffinity,
    CONFIDENCE_HALF_LIFE_DAYS, CONFIDENCE_INCREMENT, CONFIDENCE_MAX,
    CONFIDENCE_OVERRIDE_PENALTY, CONFIDENCE_THRESHOLD,
};
pub use custom_bundle::{
    BundleInfo, BundleStore, BundleStoreError, BundleThrottle, BundleVisibility,
    CreateBundleParams, UpdateBundleParams,
};
pub use engine::{BundlerEngine, CategoriseResult, CategoriseSource, HeuristicMatch};
pub use evaluator::{AffinityResult, RuleResult, evaluate_affinity, evaluate_rules};
pub use recategorise::{MoveAction, MoveResult, process_move};
pub use rule_store::{
    CreateRuleParams, RuleStore, RuleStoreError, UpdateRuleParams, validate_rule,
};
pub use rules::{BundleRule, CompiledRule, RuleField, RuleId, RuleMatchable, RuleOp};
```

**Step 2: Verify Cargo.toml dependencies**

```toml
[dependencies]
chrono = { version = "0.4", features = ["serde"] }
regex = "1"
serde = { version = "1", features = ["derive"] }
thiserror = "2"
uuid = { version = "1", features = ["v4", "serde"] }

# inboxly-core = { path = "../inboxly-core" }  # already from M12
# inboxly-store = { path = "../inboxly-store" }  # already from M12

[dev-dependencies]
```

**Build & test:** `cargo test -p inboxly-bundler && cargo clippy -p inboxly-bundler -- -D warnings`

**Commit:** `feat(bundler): wire up M13 exports and verify crate structure`

---

## Files Changed Summary

| File | Action | Description |
|------|--------|-------------|
| `inboxly-bundler/src/rules.rs` | Create | RuleField, RuleOp, BundleRule, CompiledRule, RuleMatchable trait, matching engine |
| `inboxly-bundler/src/rule_store.rs` | Create | RuleStore trait, CRUD operations, CreateRuleParams, UpdateRuleParams, validation |
| `inboxly-bundler/src/affinity.rs` | Create | SenderAffinity, confidence model, decay, AffinityStore trait |
| `inboxly-bundler/src/custom_bundle.rs` | Create | Custom bundle types, BundleStore trait, BundleVisibility, BundleThrottle |
| `inboxly-bundler/src/evaluator.rs` | Create | evaluate_rules(), evaluate_affinity(), RuleResult, AffinityResult |
| `inboxly-bundler/src/engine.rs` | Create/Modify | BundlerEngine::categorise(), full four-layer pipeline, CategoriseSource |
| `inboxly-bundler/src/recategorise.rs` | Create | process_move(), MoveAction, MoveResult, re-categorisation on user move |
| `inboxly-bundler/src/test_utils.rs` | Create | Shared MockEmail, mock stores, test fixtures |
| `inboxly-bundler/src/lib.rs` | Modify | Module declarations, re-exports |
| `inboxly-bundler/Cargo.toml` | Modify | Verify dependencies (regex, chrono, uuid, thiserror, serde) |
| `inboxly-bundler/tests/integration_pipeline.rs` | Create | End-to-end integration test |

## Testing Strategy

- **Unit tests**: Each module has inline `#[cfg(test)]` tests covering:
  - Enum serialisation round-trips
  - All operator types (Contains, Equals, Matches, Domain)
  - Edge cases (missing body, missing headers, invalid regex)
  - Confidence arithmetic (increment, decrement, cap, floor)
  - Decay calculations at 0, 1, and 2 half-lives
  - CRUD operations (create, read, update, delete, not-found errors)
  - Priority ordering
  - Regex validation on create/update

- **Integration tests**: `tests/integration_pipeline.rs` covers:
  - Full four-layer evaluation order
  - User rule overriding sender learning
  - Sender learning overriding heuristics
  - Low-confidence affinity falling through
  - Decayed affinity falling through
  - Confidence lifecycle (reinforce → decay → reinforce)
  - Re-categorisation flow

- **Build verification**: `cargo test -p inboxly-bundler && cargo clippy -p inboxly-bundler -- -D warnings`

## Success Criteria

- All four evaluation layers execute in correct precedence order
- User rules always override sender learning and heuristics
- Sender learning only fires when effective confidence > 0.6
- Confidence decay follows exponential curve with 90-day half-life
- 5 reinforcements reach max confidence (1.0)
- Override penalty reduces old affinity by 0.3
- Invalid regex patterns are rejected on rule creation, not at match time
- Pre-compiled regex avoids per-email compilation overhead
- Re-categorisation updates both thread_state and sender_affinity
- All tests pass, clippy clean with -D warnings
