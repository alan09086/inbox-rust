//! Header-based heuristic matching engine.
//!
//! Compiles [`HeuristicRule`]s into [`CompiledRule`]s with pre-compiled
//! regexes, then evaluates them against email metadata and headers.
//! Rules are evaluated in priority order (highest first); first match wins.

use std::collections::HashMap;

use inboxly_core::{BundleCategory, EmailMeta};
use regex::Regex;

use crate::rules_toml::{HeuristicRule, RuleField, RuleOp};

/// A [`HeuristicRule`] with its regex pre-compiled for fast evaluation.
pub(crate) struct CompiledRule {
    /// Human-readable name for logging.
    pub name: String,
    /// Category to assign when this rule matches.
    pub category: BundleCategory,
    /// Priority (higher = evaluated first).
    #[allow(dead_code)]
    pub priority: i32,
    /// Which email field to evaluate.
    pub field: RuleField,
    /// How to compare.
    pub operator: RuleOp,
    /// The raw value/pattern string.
    pub value: String,
    /// Compiled regex (only for `Matches` and `DomainGlob` operators).
    pub regex: Option<Regex>,
}

/// Compile a list of [`HeuristicRule`]s into [`CompiledRule`]s.
///
/// Rules should already be sorted by priority descending (from
/// [`crate::rules_toml::load_rules`]).
///
/// # Errors
///
/// Returns [`crate::BundlerError::InvalidRegex`] if any rule contains an
/// invalid regex pattern.
pub(crate) fn compile_rules(rules: Vec<HeuristicRule>) -> crate::Result<Vec<CompiledRule>> {
    rules.into_iter().map(compile_one).collect()
}

/// Compile a single rule.
fn compile_one(rule: HeuristicRule) -> crate::Result<CompiledRule> {
    let regex = match &rule.operator {
        RuleOp::Matches => {
            let re = Regex::new(&rule.value).map_err(|e| crate::BundlerError::InvalidRegex {
                rule_name: rule.name.clone(),
                source: e,
            })?;
            Some(re)
        }
        RuleOp::DomainGlob => {
            let pattern = glob_to_regex(&rule.value);
            let re = Regex::new(&pattern).map_err(|e| crate::BundlerError::InvalidRegex {
                rule_name: rule.name.clone(),
                source: e,
            })?;
            Some(re)
        }
        RuleOp::Contains | RuleOp::Equals | RuleOp::Present | RuleOp::PresentWithout => None,
    };

    Ok(CompiledRule {
        name: rule.name,
        category: rule.category,
        priority: rule.priority,
        field: rule.field,
        operator: rule.operator,
        value: rule.value,
        regex,
    })
}

/// Convert a domain glob pattern to a case-insensitive regex.
///
/// Examples:
/// - `"*.amazon.*"` -> `"(?i)^.*\.amazon\..*$"`
/// - `"paypal.*"` -> `"(?i)^paypal\..*$"`
/// - `"github.com"` -> `"(?i)^github\.com$"`
fn glob_to_regex(glob: &str) -> String {
    let mut regex = String::with_capacity(glob.len() + 8);
    regex.push_str("(?i)^");
    for ch in glob.chars() {
        match ch {
            '*' => regex.push_str(".*"),
            '.' => regex.push_str("\\."),
            '?' => regex.push('.'),
            c => regex.push(c),
        }
    }
    regex.push('$');
    regex
}

/// Extract the domain part from an email address or formatted contact string.
///
/// Handles:
/// - `"user@example.com"` -> `"example.com"`
/// - `"Name <user@sub.example.com>"` -> `"sub.example.com"`
fn extract_domain(from: &str) -> Option<&str> {
    // Handle "Name <address>" format
    let addr = if let Some(start) = from.rfind('<') {
        let end = from.rfind('>')?;
        if end <= start {
            return None;
        }
        from.get(start.checked_add(1)?..end)?
    } else {
        from
    };
    addr.rsplit_once('@').map(|(_, domain)| domain)
}

/// Evaluate all compiled rules against an email's metadata and headers.
///
/// Returns the category of the first matching rule, or `None` if no rule
/// matches (email stays in primary inbox).
///
/// `headers` is the full set of email headers (parsed from the `.eml` file).
pub(crate) fn evaluate(
    rules: &[CompiledRule],
    email: &EmailMeta,
    headers: &HashMap<String, String>,
) -> Option<BundleCategory> {
    // Rules are pre-sorted by priority descending -- first match wins
    for rule in rules {
        if matches_rule(rule, email, headers) {
            tracing::debug!(
                rule = rule.name,
                category = ?rule.category,
                email_id = %email.id,
                "header heuristic matched"
            );
            return Some(rule.category.clone());
        }
    }
    None
}

/// Check whether a single rule matches the given email.
fn matches_rule(rule: &CompiledRule, email: &EmailMeta, headers: &HashMap<String, String>) -> bool {
    match &rule.field {
        RuleField::From => {
            // Format as "Name <address>" or bare "address"
            let from_str = format!("{}", email.from);
            match_value(&from_str, &rule.operator, &rule.value, rule.regex.as_ref())
        }
        RuleField::Header(header_name) => match_header(rule, header_name, headers),
        RuleField::Subject => match_value(
            &email.subject,
            &rule.operator,
            &rule.value,
            rule.regex.as_ref(),
        ),
        RuleField::SenderDomain => {
            // Extract domain from the from address
            if let Some(domain) = extract_domain(&email.from.address) {
                match_value(domain, &rule.operator, &rule.value, rule.regex.as_ref())
            } else {
                false
            }
        }
    }
}

/// Match against a specific email header.
fn match_header(rule: &CompiledRule, header_name: &str, headers: &HashMap<String, String>) -> bool {
    match &rule.operator {
        RuleOp::Present => {
            // Case-insensitive header presence check
            headers.keys().any(|k| k.eq_ignore_ascii_case(header_name))
        }
        RuleOp::PresentWithout => {
            // Value format: "present_header|absent_header"
            if let Some((present, absent)) = rule.value.split_once('|') {
                let has_present = headers.keys().any(|k| k.eq_ignore_ascii_case(present));
                let has_absent = headers.keys().any(|k| k.eq_ignore_ascii_case(absent));
                has_present && !has_absent
            } else {
                false
            }
        }
        _ => {
            // Get header value (case-insensitive lookup)
            let header_val = headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(header_name))
                .map(|(_, v)| v.as_str());

            match header_val {
                Some(val) if !val.is_empty() => {
                    match_value(val, &rule.operator, &rule.value, rule.regex.as_ref())
                }
                _ => false,
            }
        }
    }
}

/// Compare a haystack string against a rule value using the specified operator.
fn match_value(haystack: &str, op: &RuleOp, value: &str, regex: Option<&Regex>) -> bool {
    match op {
        RuleOp::Contains => haystack.to_lowercase().contains(&value.to_lowercase()),
        RuleOp::Equals => haystack.eq_ignore_ascii_case(value),
        RuleOp::Matches | RuleOp::DomainGlob => regex.is_some_and(|r| r.is_match(haystack)),
        // Present and PresentWithout are handled at the field level
        RuleOp::Present | RuleOp::PresentWithout => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inboxly_core::Contact;

    fn make_email(from_address: &str, subject: &str) -> EmailMeta {
        let mut meta = EmailMeta::test_default();
        meta.from = Contact::new("", from_address);
        meta.subject = subject.to_string();
        meta
    }

    fn default_compiled_rules() -> Vec<CompiledRule> {
        let rules = crate::rules_toml::parse_rules(crate::rules_toml::DEFAULT_RULES_TOML)
            .expect("default rules parse");
        let sorted = {
            let mut r = rules;
            r.sort_by(|a, b| b.priority.cmp(&a.priority));
            r
        };
        compile_rules(sorted).expect("compile rules")
    }

    #[test]
    fn extract_domain_plain_address() {
        assert_eq!(extract_domain("user@example.com"), Some("example.com"));
    }

    #[test]
    fn extract_domain_with_name() {
        assert_eq!(
            extract_domain("John Doe <john@sub.example.com>"),
            Some("sub.example.com")
        );
    }

    #[test]
    fn extract_domain_bare_no_at() {
        assert_eq!(extract_domain("noatsign"), None);
    }

    #[test]
    fn glob_to_regex_wildcard() {
        let re = glob_to_regex("*.amazon.*");
        let compiled = Regex::new(&re).expect("valid regex");
        assert!(compiled.is_match("email.amazon.com"));
        assert!(compiled.is_match("ship.amazon.ca"));
        assert!(!compiled.is_match("notamazon.com"));
    }

    #[test]
    fn glob_to_regex_exact_domain() {
        let re = glob_to_regex("github.com");
        let compiled = Regex::new(&re).expect("valid regex");
        assert!(compiled.is_match("github.com"));
        assert!(compiled.is_match("GITHUB.COM"));
        assert!(!compiled.is_match("notgithub.com"));
    }

    #[test]
    fn glob_to_regex_suffix() {
        let re = glob_to_regex("paypal.*");
        let compiled = Regex::new(&re).expect("valid regex");
        assert!(compiled.is_match("paypal.com"));
        assert!(compiled.is_match("paypal.co.uk"));
        assert!(!compiled.is_match("notpaypal.com"));
    }

    #[test]
    fn list_id_matches_forums() {
        let compiled = default_compiled_rules();
        let email = make_email("user@lists.example.com", "Re: Discussion");
        let mut headers = HashMap::new();
        headers.insert("List-Id".to_string(), "<dev.lists.example.com>".to_string());

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Forums));
    }

    #[test]
    fn list_unsubscribe_without_list_id_matches_promos() {
        let compiled = default_compiled_rules();
        let email = make_email("marketing@store.com", "50% off sale!");
        let mut headers = HashMap::new();
        headers.insert(
            "List-Unsubscribe".to_string(),
            "<mailto:unsub@store.com>".to_string(),
        );
        // No List-Id header

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Promos));
    }

    #[test]
    fn facebook_sender_matches_social() {
        let compiled = default_compiled_rules();
        let email = make_email(
            "notification@facebookmail.com",
            "You have a new friend request",
        );
        let headers = HashMap::new();

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Social));
    }

    #[test]
    fn amazon_sender_matches_purchases() {
        let compiled = default_compiled_rules();
        let email = make_email("ship-confirm@ship.amazon.ca", "Your order has shipped");
        let headers = HashMap::new();

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Purchases));
    }

    #[test]
    fn paypal_sender_matches_finance() {
        let compiled = default_compiled_rules();
        let email = make_email("service@paypal.com", "You sent a payment");
        let headers = HashMap::new();

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Finance));
    }

    #[test]
    fn precedence_bulk_matches_low_priority() {
        let compiled = default_compiled_rules();
        let email = make_email("system@random.com", "Automated report");
        let mut headers = HashMap::new();
        headers.insert("Precedence".to_string(), "bulk".to_string());

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::LowPriority));
    }

    #[test]
    fn mailchimp_mailer_matches_promos() {
        let compiled = default_compiled_rules();
        let email = make_email("newsletter@store.com", "Weekly deals");
        let mut headers = HashMap::new();
        headers.insert("X-Mailer".to_string(), "Mailchimp 2.0".to_string());

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Promos));
    }

    #[test]
    fn unknown_sender_returns_none() {
        let compiled = default_compiled_rules();
        let email = make_email("friend@personal.com", "Hey, lunch tomorrow?");
        let headers = HashMap::new();

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, None);
    }

    #[test]
    fn higher_priority_rule_wins() {
        // GitHub.com (Social, priority 55) should beat List-Id (Forums, priority 50)
        let compiled = default_compiled_rules();
        let email = make_email("notifications@github.com", "New issue opened");
        let mut headers = HashMap::new();
        headers.insert("List-Id".to_string(), "<repo.github.com>".to_string());

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Social));
    }

    #[test]
    fn booking_sender_matches_travel() {
        let compiled = default_compiled_rules();
        let email = make_email("noreply@booking.com", "Your reservation is confirmed");
        let headers = HashMap::new();

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Travel));
    }

    #[test]
    fn noreply_matches_updates() {
        let compiled = default_compiled_rules();
        let email = make_email("noreply@someservice.com", "Your account was updated");
        let headers = HashMap::new();

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Updates));
    }

    #[test]
    fn finance_subject_pattern() {
        let compiled = default_compiled_rules();
        let email = make_email("alerts@mybank.com", "Your monthly statement is ready");
        let headers = HashMap::new();

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Finance));
    }

    #[test]
    fn stripe_matches_purchases() {
        let compiled = default_compiled_rules();
        let email = make_email("receipts@stripe.com", "Your receipt from Acme Inc");
        let headers = HashMap::new();

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Purchases));
    }
}
