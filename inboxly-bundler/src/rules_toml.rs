//! TOML rule definitions and parsing.
//!
//! Defines [`HeuristicRule`], [`RuleField`], and [`RuleOp`] types that are
//! deserialized from TOML. Default rules are embedded in the binary via
//! [`include_str!`]. User overrides can replace or extend the defaults.

use std::path::Path;

use inboxly_core::BundleCategory;
use serde::{Deserialize, Serialize};

/// A single header-based heuristic rule definition.
///
/// Loaded from TOML, compiled into a [`crate::heuristics::CompiledRule`]
/// for evaluation at runtime.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HeuristicRule {
    /// Human-readable name for debugging/logging.
    pub name: String,
    /// Which category this rule assigns when matched.
    pub category: BundleCategory,
    /// Priority -- higher number = evaluated first.
    ///
    /// Default heuristics use 0-100. User overrides should use 200+.
    pub priority: i32,
    /// What email field to match against.
    pub field: RuleField,
    /// How to compare the field value against `value`.
    pub operator: RuleOp,
    /// The value or pattern to match.
    pub value: String,
}

/// Which email field to evaluate.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum RuleField {
    /// From address (full "name <address>" or just address).
    From,
    /// A specific email header by name (e.g., "List-Id", "Precedence").
    Header(String),
    /// Subject line.
    Subject,
    /// Sender domain (extracted from the From address).
    SenderDomain,
}

/// How to compare the field value against the rule value.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum RuleOp {
    /// Field value contains the string (case-insensitive).
    Contains,
    /// Field value equals the string exactly (case-insensitive).
    Equals,
    /// Field value matches a regex pattern.
    Matches,
    /// Header is present (value is ignored).
    Present,
    /// Compound: one header is present AND another is absent.
    ///
    /// Value format: `"present_header|absent_header"`.
    PresentWithout,
    /// Sender domain matches a glob pattern (e.g., `"*.amazon.*"`).
    DomainGlob,
}

/// Container for TOML deserialization of a rule set.
#[derive(Debug, Deserialize)]
struct RuleSet {
    rules: Vec<HeuristicRule>,
}

/// The default rules compiled into the binary.
pub(crate) const DEFAULT_RULES_TOML: &str = include_str!("default_rules.toml");

/// Parse heuristic rules from a TOML string.
///
/// # Errors
///
/// Returns [`crate::BundlerError::TomlParse`] if the TOML is malformed.
pub fn parse_rules(toml_str: &str) -> crate::Result<Vec<HeuristicRule>> {
    let rule_set: RuleSet = toml::from_str(toml_str)?;
    Ok(rule_set.rules)
}

/// Load default rules, optionally merging with user overrides from a file.
///
/// User rules with the same `name` as a default rule replace the default.
/// User rules with new names are appended. The result is sorted by priority
/// descending (highest priority evaluated first).
///
/// # Errors
///
/// Returns an error if the default or user TOML is malformed, or if the
/// user config file cannot be read.
pub fn load_rules(user_config_path: Option<&Path>) -> crate::Result<Vec<HeuristicRule>> {
    let mut rules = parse_rules(DEFAULT_RULES_TOML)?;

    if let Some(path) = user_config_path {
        if path.exists() {
            let user_toml = std::fs::read_to_string(path)?;
            let user_rules = parse_rules(&user_toml)?;

            // Replace defaults with same name, append new ones
            for user_rule in user_rules {
                if let Some(existing) = rules.iter_mut().find(|r| r.name == user_rule.name) {
                    *existing = user_rule;
                } else {
                    rules.push(user_rule);
                }
            }
        }
    }

    // Sort by priority descending -- highest priority evaluated first
    rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    Ok(rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default_rules() {
        let rules = parse_rules(DEFAULT_RULES_TOML).expect("default rules should parse");
        assert!(
            rules.len() >= 20,
            "should have at least 20 default rules, got {}",
            rules.len()
        );
    }

    #[test]
    fn rules_sorted_by_priority_descending() {
        let rules = load_rules(None).expect("default rules should load");
        for window in rules.windows(2) {
            assert!(
                window[0].priority >= window[1].priority,
                "rules should be sorted descending: {} (pri={}) before {} (pri={})",
                window[0].name,
                window[0].priority,
                window[1].name,
                window[1].priority,
            );
        }
    }

    #[test]
    fn user_override_replaces_default() {
        let user_toml = r#"
[[rules]]
name = "precedence-bulk"
category = "Promos"
priority = 100
field = { Header = "Precedence" }
operator = "Equals"
value = "bulk"
"#;
        let temp_dir = std::env::temp_dir().join("inboxly-test-rules-override");
        let _ = std::fs::create_dir_all(&temp_dir);
        let path = temp_dir.join("heuristics.toml");
        std::fs::write(&path, user_toml).expect("write temp file");

        let rules = load_rules(Some(&path)).expect("load with override");
        let bulk_rule = rules
            .iter()
            .find(|r| r.name == "precedence-bulk")
            .expect("precedence-bulk should exist");
        assert!(matches!(bulk_rule.category, BundleCategory::Promos));
        assert_eq!(bulk_rule.priority, 100);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn user_adds_new_rule() {
        let user_toml = r#"
[[rules]]
name = "custom-work-domain"
category = "Updates"
priority = 200
field = "SenderDomain"
operator = "DomainGlob"
value = "mycompany.com"
"#;
        let temp_dir = std::env::temp_dir().join("inboxly-test-rules-add");
        let _ = std::fs::create_dir_all(&temp_dir);
        let path = temp_dir.join("heuristics.toml");
        std::fs::write(&path, user_toml).expect("write temp file");

        let rules = load_rules(Some(&path)).expect("load with new rule");
        assert!(rules.iter().any(|r| r.name == "custom-work-domain"));
        // New rule should be first (priority 200)
        assert_eq!(rules[0].name, "custom-work-domain");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn nonexistent_user_config_uses_defaults_only() {
        let path = std::path::PathBuf::from("/nonexistent/path/heuristics.toml");
        let rules = load_rules(Some(&path)).expect("should succeed with nonexistent path");
        assert!(rules.len() >= 20);
    }

    #[test]
    fn each_rule_has_nonempty_name() {
        let rules = parse_rules(DEFAULT_RULES_TOML).expect("parse");
        for rule in &rules {
            assert!(!rule.name.is_empty(), "all rules must have a name");
        }
    }
}
