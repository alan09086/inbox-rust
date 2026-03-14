//! Bundler engine -- full four-layer evaluation pipeline.
//!
//! Evaluation order (highest precedence first):
//! 1. User rules (explicit control)
//! 2. Sender learning (if confidence > threshold)
//! 3. Header heuristics (zero-config patterns, from M12)
//! 4. Uncategorised (stays in primary inbox)

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::affinity::SenderAffinity;
use crate::evaluator::{AffinityResult, RuleResult, evaluate_affinity, evaluate_rules};
use crate::user_rules::{RuleMatchable, UserCompiledRule};

// ---------------------------------------------------------------------------
// HeuristicMatch -- bridge from M12 heuristic engine output
// ---------------------------------------------------------------------------

/// A header heuristic match from M12's engine.
///
/// This type bridges the M12 heuristic output into the M13 pipeline.
/// The existing [`crate::Bundler::categorise`] returns a
/// `(BundleCategory, BundleId)` pair; callers convert that into this
/// struct before passing to [`BundlerEngine::categorise`].
#[derive(Debug, Clone)]
pub struct HeuristicMatch {
    /// The matched category (e.g., "Social", "Promos", "Forums").
    pub category: String,
    /// Which heuristic pattern matched (for debugging/logging).
    pub pattern: String,
}

// ---------------------------------------------------------------------------
// CategoriseSource / CategoriseResult
// ---------------------------------------------------------------------------

/// The source that determined an email's categorisation.
#[derive(Debug, Clone, PartialEq)]
pub enum CategoriseSource {
    /// Matched a user-defined rule.
    UserRule {
        /// The ID of the rule that matched.
        rule_id: Uuid,
    },
    /// Matched sender learning with sufficient confidence.
    SenderLearning {
        /// The effective confidence score.
        confidence: f64,
    },
    /// Matched a header heuristic pattern.
    HeaderHeuristic,
    /// No categorisation -- email stays in primary inbox.
    Uncategorised,
}

/// Result of categorising an email through the full pipeline.
#[derive(Debug, Clone)]
pub struct CategoriseResult {
    /// The bundle to assign this email to, or `None` for uncategorised.
    pub bundle_id: Option<Uuid>,
    /// The bundle category name (e.g., "Social", "Promos"), or `None`.
    pub bundle_category: Option<String>,
    /// Which layer produced this result.
    pub source: CategoriseSource,
}

// ---------------------------------------------------------------------------
// BundlerEngine
// ---------------------------------------------------------------------------

/// The bundler engine holds pre-loaded rules and provides the full
/// categorisation pipeline.
pub struct BundlerEngine {
    /// User rules, pre-compiled and sorted by priority descending.
    compiled_rules: Vec<UserCompiledRule>,
    /// Category-to-bundle-id mapping for sender learning results.
    /// Populated from the bundles table (e.g., "Social" -> UUID).
    category_bundle_map: HashMap<String, Uuid>,
}

impl BundlerEngine {
    /// Create a new engine with the given rules and category mapping.
    pub fn new(rules: Vec<UserCompiledRule>, category_bundle_map: HashMap<String, Uuid>) -> Self {
        Self {
            compiled_rules: rules,
            category_bundle_map,
        }
    }

    /// Reload rules (e.g., after user creates/edits a rule).
    pub fn reload_rules(&mut self, rules: Vec<UserCompiledRule>) {
        self.compiled_rules = rules;
    }

    /// Categorise an email through the full four-layer pipeline.
    ///
    /// # Arguments
    ///
    /// * `email` -- the email to categorise (implements [`RuleMatchable`])
    /// * `sender_affinity` -- pre-fetched affinity for this sender (`None` if unknown)
    /// * `heuristic_result` -- result from M12's header heuristic engine (`None` = no match)
    /// * `now` -- current timestamp for confidence decay calculation
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
        if let AffinityResult::Confident {
            bundle_category,
            confidence,
        } = evaluate_affinity(sender_affinity, now)
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

    /// Re-evaluate an email's bundle assignment using full body content.
    ///
    /// Called when Phase 2 sync delivers a message body for an email that was
    /// previously categorised using headers only. Runs the full pipeline
    /// (user rules -> sender learning -> header heuristics) with body content
    /// available via the `RuleMatchable` trait.
    ///
    /// Returns `Some(new_bundle_id)` if the re-evaluation changed the
    /// assignment, or `None` if the assignment remains the same.
    ///
    /// The caller is responsible for:
    /// 1. Constructing a `RuleMatchable` that includes the body text
    /// 2. Fetching the sender affinity and heuristic result
    /// 3. Comparing the returned bundle_id with the current assignment
    /// 4. Updating `thread_state.bundle_id` if it changed
    pub fn re_evaluate_with_body(
        &self,
        email: &dyn RuleMatchable,
        sender_affinity: Option<&SenderAffinity>,
        heuristic_result: Option<HeuristicMatch>,
        current_bundle_id: Option<Uuid>,
        now: DateTime<Utc>,
    ) -> Option<Uuid> {
        let result = self.categorise(email, sender_affinity, heuristic_result, now);
        if result.bundle_id != current_bundle_id {
            result.bundle_id
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::fixtures::MockEmail;
    use crate::user_rules::{BundleRule, UserCompiledRule, UserRuleField, UserRuleOp};
    use chrono::TimeDelta;

    fn setup_engine() -> (BundlerEngine, Uuid, Uuid, Uuid) {
        let social_id = Uuid::new_v4();
        let promos_id = Uuid::new_v4();
        let custom_id = Uuid::new_v4();

        let mut category_map = HashMap::new();
        category_map.insert("Social".to_owned(), social_id);
        category_map.insert("Promos".to_owned(), promos_id);

        // One user rule: github.com -> custom bundle
        let rules = vec![UserCompiledRule::compile(BundleRule {
            id: Uuid::new_v4(),
            bundle_id: custom_id,
            field: UserRuleField::From,
            operator: UserRuleOp::Domain,
            value: "github.com".into(),
            priority: 100,
        })];

        (
            BundlerEngine::new(rules, category_map),
            social_id,
            promos_id,
            custom_id,
        )
    }

    #[test]
    fn user_rule_beats_sender_learning() {
        let (engine, _, _, custom_id) = setup_engine();
        let email = MockEmail::new("noreply@github.com", "PR merged");
        // Even with a strong social affinity, user rule wins
        let affinity = SenderAffinity {
            sender_domain: "github.com".into(),
            sender_address: "noreply@github.com".into(),
            bundle_category: "Social".into(),
            confidence: 1.0,
            learned_at: Utc::now(),
        };
        let result = engine.categorise(&email, Some(&affinity), None, Utc::now());
        assert_eq!(result.bundle_id, Some(custom_id));
        assert!(matches!(result.source, CategoriseSource::UserRule { .. }));
    }

    #[test]
    fn sender_learning_beats_heuristic() {
        let (engine, _, promos_id, _) = setup_engine();
        let email = MockEmail::new("deals@shop.com", "50% off today!");
        let affinity = SenderAffinity {
            sender_domain: "shop.com".into(),
            sender_address: "deals@shop.com".into(),
            bundle_category: "Promos".into(),
            confidence: 0.8,
            learned_at: Utc::now(),
        };
        let heuristic = HeuristicMatch {
            category: "Social".into(), // heuristic says social
            pattern: "List-Id".into(),
        };
        let result = engine.categorise(&email, Some(&affinity), Some(heuristic), Utc::now());
        // Sender learning wins over heuristic
        assert_eq!(result.bundle_id, Some(promos_id));
        assert_eq!(result.bundle_category, Some("Promos".into()));
        assert!(matches!(
            result.source,
            CategoriseSource::SenderLearning { .. }
        ));
    }

    #[test]
    fn low_confidence_falls_through_to_heuristic() {
        let (engine, social_id, _, _) = setup_engine();
        let email = MockEmail::new("bot@facebook.com", "Friend request");
        let affinity = SenderAffinity {
            sender_domain: "facebook.com".into(),
            sender_address: "bot@facebook.com".into(),
            bundle_category: "Promos".into(),
            confidence: 0.3, // below threshold
            learned_at: Utc::now(),
        };
        let heuristic = HeuristicMatch {
            category: "Social".into(),
            pattern: "From domain".into(),
        };
        let result = engine.categorise(&email, Some(&affinity), Some(heuristic), Utc::now());
        assert_eq!(result.bundle_id, Some(social_id));
        assert_eq!(result.bundle_category, Some("Social".into()));
        assert!(matches!(result.source, CategoriseSource::HeaderHeuristic));
    }

    #[test]
    fn no_match_returns_uncategorised() {
        let (engine, _, _, _) = setup_engine();
        let email = MockEmail::new("friend@personal.com", "Dinner tonight?");
        let result = engine.categorise(&email, None, None, Utc::now());
        assert!(result.bundle_id.is_none());
        assert!(matches!(result.source, CategoriseSource::Uncategorised));
    }

    #[test]
    fn decayed_affinity_falls_through() {
        let (engine, social_id, _, _) = setup_engine();
        let email = MockEmail::new("news@example.com", "Newsletter");
        // High confidence but very old -> decayed below threshold
        let affinity = SenderAffinity {
            sender_domain: "example.com".into(),
            sender_address: "news@example.com".into(),
            bundle_category: "Promos".into(),
            confidence: 0.7,
            learned_at: Utc::now() - TimeDelta::days(200),
        };
        let heuristic = HeuristicMatch {
            category: "Social".into(),
            pattern: "List-Id".into(),
        };
        let result = engine.categorise(&email, Some(&affinity), Some(heuristic), Utc::now());
        // Decayed affinity below threshold -> falls through to heuristic
        assert_eq!(result.bundle_id, Some(social_id));
        assert!(matches!(result.source, CategoriseSource::HeaderHeuristic));
    }

    #[test]
    fn reload_rules_updates_engine() {
        let (mut engine, _, _, _) = setup_engine();

        // Initially github.com matches
        let email = MockEmail::new("noreply@github.com", "PR merged");
        assert!(matches!(
            engine.categorise(&email, None, None, Utc::now()).source,
            CategoriseSource::UserRule { .. }
        ));

        // After reloading with empty rules, no match
        engine.reload_rules(vec![]);
        assert!(matches!(
            engine.categorise(&email, None, None, Utc::now()).source,
            CategoriseSource::Uncategorised
        ));
    }

    // -- Body re-evaluation tests (M14) ----------------------------------------

    #[test]
    fn body_rule_skipped_when_no_body() {
        // Create a rule matching "unsubscribe" in body
        let promo_bundle = Uuid::new_v4();
        let rules = vec![UserCompiledRule::compile(BundleRule {
            id: Uuid::new_v4(),
            bundle_id: promo_bundle,
            field: UserRuleField::Body,
            operator: UserRuleOp::Contains,
            value: "unsubscribe".into(),
            priority: 100,
        })];

        let engine = BundlerEngine::new(rules, HashMap::new());

        // Email with no body -- body rule should not fire
        let email = MockEmail::new("news@example.com", "Newsletter");
        let result = engine.categorise(&email, None, None, Utc::now());
        assert!(matches!(result.source, CategoriseSource::Uncategorised));
    }

    #[test]
    fn body_rule_fires_on_reevaluation() {
        let promo_bundle = Uuid::new_v4();
        let rules = vec![UserCompiledRule::compile(BundleRule {
            id: Uuid::new_v4(),
            bundle_id: promo_bundle,
            field: UserRuleField::Body,
            operator: UserRuleOp::Contains,
            value: "unsubscribe".into(),
            priority: 100,
        })];

        let engine = BundlerEngine::new(rules, HashMap::new());

        // Email with body containing "unsubscribe" -- rule fires
        let email = MockEmail::new("news@example.com", "Newsletter")
            .with_body("Click here to unsubscribe from this list.");
        let result = engine.categorise(&email, None, None, Utc::now());
        assert_eq!(result.bundle_id, Some(promo_bundle));
        assert!(matches!(result.source, CategoriseSource::UserRule { .. }));
    }

    #[test]
    fn reevaluation_no_change_returns_none() {
        let (engine, social_id, _, _) = setup_engine();

        // Email already categorised by heuristic to Social
        let email = MockEmail::new("bot@facebook.com", "Friend request");
        let heuristic = HeuristicMatch {
            category: "Social".into(),
            pattern: "domain".into(),
        };

        // Re-evaluate: same result, should return None
        let change = engine.re_evaluate_with_body(
            &email,
            None,
            Some(heuristic),
            Some(social_id),
            Utc::now(),
        );
        assert!(change.is_none());
    }

    #[test]
    fn body_rule_overrides_header_heuristic() {
        let custom_id = Uuid::new_v4();
        let rules = vec![UserCompiledRule::compile(BundleRule {
            id: Uuid::new_v4(),
            bundle_id: custom_id,
            field: UserRuleField::Body,
            operator: UserRuleOp::Contains,
            value: "unsubscribe".into(),
            priority: 100,
        })];

        let mut category_map = HashMap::new();
        category_map.insert("Social".to_owned(), Uuid::new_v4());

        let engine = BundlerEngine::new(rules, category_map);

        // Header heuristic says Social, but body rule overrides
        let email = MockEmail::new("promo@shop.com", "Sale")
            .with_body("Click to unsubscribe");
        let heuristic = HeuristicMatch {
            category: "Social".into(),
            pattern: "List-Id".into(),
        };

        let result = engine.categorise(&email, None, Some(heuristic), Utc::now());
        // User rule (body match) beats heuristic
        assert_eq!(result.bundle_id, Some(custom_id));
        assert!(matches!(result.source, CategoriseSource::UserRule { .. }));
    }
}
