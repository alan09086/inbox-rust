//! CRUD operations for user-defined bundle rules.
//!
//! Defines [`RuleStore`] trait and supporting types for persisting
//! [`BundleRule`]s.  The trait is implemented by `inboxly-store::Store`
//! for production use and by [`mock::MockRuleStore`] for testing.

use std::fmt;
use std::str::FromStr;

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
// RuleId / BundleRule
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
    ///
    /// Note: this recompiles regex on every call for `Matches` rules.
    /// External callers should use [`UserCompiledRule::matches()`] instead,
    /// which caches the compiled regex.
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
                regex::Regex::new(&self.value).is_ok_and(|re| re.is_match(field_value))
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
// RuleStoreError / CreateRuleParams / UpdateRuleParams
// ---------------------------------------------------------------------------

/// Error type for rule store operations.
#[derive(Debug, thiserror::Error)]
pub enum RuleStoreError {
    /// The requested rule was not found.
    #[error("rule not found: {0}")]
    NotFound(RuleId),

    /// The rule field string could not be parsed.
    #[error("invalid rule field: {0}")]
    InvalidField(String),

    /// The rule operator string could not be parsed.
    #[error("invalid rule operator: {0}")]
    InvalidOperator(String),

    /// The rule contains an invalid regex pattern.
    #[error("invalid regex pattern: {0}")]
    InvalidRegex(String),

    /// An error from the underlying database.
    #[error("database error: {0}")]
    Database(String),
}

/// Parameters for creating a new bundle rule.
pub struct CreateRuleParams {
    /// Which bundle this rule assigns emails to.
    pub bundle_id: Uuid,
    /// Which email field to examine.
    pub field: UserRuleField,
    /// How to compare field value against the rule's value.
    pub operator: UserRuleOp,
    /// The value to match against.
    pub value: String,
    /// Priority (higher = evaluated first).
    pub priority: i64,
}

/// Parameters for updating an existing rule.  All fields are optional --
/// only `Some` values are applied.
pub struct UpdateRuleParams {
    /// New field selector.
    pub field: Option<UserRuleField>,
    /// New operator.
    pub operator: Option<UserRuleOp>,
    /// New match value.
    pub value: Option<String>,
    /// New priority.
    pub priority: Option<i64>,
    /// New target bundle.
    pub bundle_id: Option<Uuid>,
}

/// Validate rule parameters before persisting.
///
/// # Errors
///
/// Returns [`RuleStoreError::InvalidRegex`] if the operator is `Matches`
/// and the value is not a valid regex pattern.
pub fn validate_rule(
    _field: &UserRuleField,
    operator: &UserRuleOp,
    value: &str,
) -> Result<(), RuleStoreError> {
    if *operator == UserRuleOp::Matches {
        regex::Regex::new(value).map_err(|e| RuleStoreError::InvalidRegex(e.to_string()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// RuleStore trait
// ---------------------------------------------------------------------------

/// Trait for bundle rule persistence.
///
/// Implemented by `inboxly-store::Store` for production use.
/// A mock implementation is used in tests.
pub trait RuleStore {
    /// Create a new rule.  Returns the created rule with generated ID.
    ///
    /// # Errors
    ///
    /// Returns [`RuleStoreError::InvalidRegex`] if `operator` is `Matches`
    /// and `value` is not a valid regex.  Returns [`RuleStoreError::Database`]
    /// on database failure.
    fn create_rule(&self, params: CreateRuleParams) -> Result<BundleRule, RuleStoreError>;

    /// Get a rule by ID.
    ///
    /// # Errors
    ///
    /// Returns [`RuleStoreError::NotFound`] if the rule does not exist.
    fn get_rule(&self, id: RuleId) -> Result<BundleRule, RuleStoreError>;

    /// List all rules, ordered by priority descending (highest first).
    ///
    /// # Errors
    ///
    /// Returns [`RuleStoreError::Database`] on database failure.
    fn list_rules(&self) -> Result<Vec<BundleRule>, RuleStoreError>;

    /// List rules for a specific bundle, ordered by priority descending.
    ///
    /// # Errors
    ///
    /// Returns [`RuleStoreError::Database`] on database failure.
    fn list_rules_for_bundle(&self, bundle_id: Uuid) -> Result<Vec<BundleRule>, RuleStoreError>;

    /// Update a rule.  Only fields that are `Some` in `params` are changed.
    ///
    /// # Errors
    ///
    /// Returns [`RuleStoreError::NotFound`] if the rule does not exist.
    /// Returns [`RuleStoreError::InvalidRegex`] if updating operator to
    /// `Matches` with an invalid regex value.
    fn update_rule(
        &self,
        id: RuleId,
        params: UpdateRuleParams,
    ) -> Result<BundleRule, RuleStoreError>;

    /// Delete a rule by ID.
    ///
    /// # Errors
    ///
    /// Returns [`RuleStoreError::NotFound`] if the rule does not exist.
    fn delete_rule(&self, id: RuleId) -> Result<(), RuleStoreError>;
}
