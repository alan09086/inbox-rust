//! Rule evaluation engine -- evaluates user-defined rules and sender
//! affinities against an email.
//!
//! Two pure evaluation functions:
//! - [`evaluate_rules`]: checks user rules (Layer 1 in the pipeline)
//! - [`evaluate_affinity`]: checks sender learning (Layer 2 in the pipeline)

use crate::affinity::{SenderAffinity, CONFIDENCE_THRESHOLD};
use crate::user_rules::{RuleMatchable, UserCompiledRule};
use chrono::{DateTime, Utc};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Rule evaluation (Layer 1)
// ---------------------------------------------------------------------------

/// Result of evaluating an email against user rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleResult {
    /// A user rule matched -- email should go to this bundle.
    Matched {
        /// The bundle to assign.
        bundle_id: Uuid,
        /// Which rule matched (for logging/debugging).
        rule_id: Uuid,
    },
    /// No user rule matched -- fall through to next evaluation layer.
    NoMatch,
}

/// Evaluate an email against a list of compiled user rules.
///
/// Rules must be pre-sorted by priority descending (highest first).
/// Returns the first matching rule.  This is the "User Rules" layer
/// in the evaluation order.
pub fn evaluate_rules(rules: &[UserCompiledRule], email: &dyn RuleMatchable) -> RuleResult {
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

// ---------------------------------------------------------------------------
// Affinity evaluation (Layer 2)
// ---------------------------------------------------------------------------

/// Result of evaluating sender affinity.
#[derive(Debug, Clone, PartialEq)]
pub enum AffinityResult {
    /// Sender has a learned affinity above the confidence threshold.
    Confident {
        /// The learned bundle category.
        bundle_category: String,
        /// The effective confidence (after decay).
        confidence: f64,
    },
    /// Sender has an affinity but it's below the threshold (decayed or weak).
    BelowThreshold {
        /// The learned bundle category.
        bundle_category: String,
        /// The effective confidence (after decay).
        confidence: f64,
    },
    /// No affinity data exists for this sender.
    Unknown,
}

/// Evaluate sender affinity for an email.
///
/// Looks up the sender's affinity data.  Returns [`AffinityResult::Confident`]
/// if effective confidence (after decay) exceeds the threshold.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::affinity::SenderAffinity;
    use crate::test_utils::fixtures::MockEmail;
    use crate::user_rules::{BundleRule, UserCompiledRule, UserRuleField, UserRuleOp};
    use chrono::TimeDelta;

    // -- evaluate_rules --------------------------------------------------------

    #[test]
    fn first_matching_rule_wins() {
        let bundle_a = Uuid::new_v4();
        let bundle_b = Uuid::new_v4();
        let rules = vec![
            UserCompiledRule::compile(BundleRule {
                id: Uuid::new_v4(),
                bundle_id: bundle_a,
                field: UserRuleField::From,
                operator: UserRuleOp::Domain,
                value: "github.com".into(),
                priority: 100,
            }),
            UserCompiledRule::compile(BundleRule {
                id: Uuid::new_v4(),
                bundle_id: bundle_b,
                field: UserRuleField::Subject,
                operator: UserRuleOp::Contains,
                value: "PR".into(),
                priority: 50,
            }),
        ];
        // Email matches both rules -- highest priority (first in list) wins
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
        let rules = vec![UserCompiledRule::compile(BundleRule {
            id: Uuid::new_v4(),
            bundle_id: Uuid::new_v4(),
            field: UserRuleField::From,
            operator: UserRuleOp::Domain,
            value: "gitlab.com".into(),
            priority: 10,
        })];
        let email = MockEmail::new("alice@github.com", "Hello");
        assert_eq!(evaluate_rules(&rules, &email), RuleResult::NoMatch);
    }

    // -- evaluate_affinity -----------------------------------------------------

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
            AffinityResult::Confident {
                bundle_category,
                confidence,
            } => {
                assert_eq!(bundle_category, "promos");
                assert!(confidence > 0.7);
            }
            other => panic!("expected Confident, got {other:?}"),
        }
    }

    #[test]
    fn affinity_below_threshold_after_decay() {
        let aff = SenderAffinity {
            sender_domain: "shop.com".into(),
            sender_address: "deals@shop.com".into(),
            bundle_category: "promos".into(),
            confidence: 0.7,
            learned_at: Utc::now() - TimeDelta::days(120),
        };
        match evaluate_affinity(Some(&aff), Utc::now()) {
            AffinityResult::BelowThreshold { .. } => {} // expected
            other => panic!("expected BelowThreshold, got {other:?}"),
        }
    }

    #[test]
    fn no_affinity_returns_unknown() {
        assert!(matches!(
            evaluate_affinity(None, Utc::now()),
            AffinityResult::Unknown
        ));
    }
}
