//! Integration test for the full M13 bundler pipeline.
//!
//! Exercises the complete flow: user rules, sender learning, heuristics,
//! and uncategorised -- verifying correct precedence at each layer.

use std::collections::HashMap;

use chrono::{TimeDelta, Utc};
use inboxly_bundler::user_rules::{
    BundleRule, RuleMatchable, UserCompiledRule, UserRuleField, UserRuleOp,
};
use inboxly_bundler::{BundlerEngine, CategoriseSource, HeuristicMatch, SenderAffinity};
use uuid::Uuid;

// -- Test double (re-defined here since test_utils is pub(crate)) ----------

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

// -- Tests -----------------------------------------------------------------

#[test]
fn full_pipeline_four_layers() {
    // Setup: bundle IDs
    let social_id = Uuid::new_v4();
    let promos_id = Uuid::new_v4();
    let custom_work_id = Uuid::new_v4();

    let mut category_map = HashMap::new();
    category_map.insert("Social".to_owned(), social_id);
    category_map.insert("Promos".to_owned(), promos_id);

    // User rule: anything from @work.com -> custom "Work" bundle
    let rules = vec![UserCompiledRule::compile(BundleRule {
        id: Uuid::new_v4(),
        bundle_id: custom_work_id,
        field: UserRuleField::From,
        operator: UserRuleOp::Domain,
        value: "work.com".into(),
        priority: 100,
    })];

    let engine = BundlerEngine::new(rules, category_map);
    let now = Utc::now();

    // Email 1: from @work.com -> user rule wins
    let e1 = MockEmail::new("boss@work.com", "Q1 Review");
    let r1 = engine.categorise(&e1, None, None, now);
    assert_eq!(r1.bundle_id, Some(custom_work_id));
    assert!(matches!(r1.source, CategoriseSource::UserRule { .. }));

    // Email 2: from @shop.com with strong affinity -> sender learning
    let aff = SenderAffinity {
        sender_domain: "shop.com".into(),
        sender_address: "deals@shop.com".into(),
        bundle_category: "Promos".into(),
        confidence: 0.8,
        learned_at: now,
    };
    let e2 = MockEmail::new("deals@shop.com", "Sale ends today!");
    let r2 = engine.categorise(&e2, Some(&aff), None, now);
    assert!(matches!(r2.source, CategoriseSource::SenderLearning { .. }));
    assert_eq!(r2.bundle_category, Some("Promos".into()));

    // Email 3: from unknown sender with heuristic match -> heuristic
    let e3 = MockEmail::new("bot@forum.org", "New post in thread");
    let heuristic = HeuristicMatch {
        category: "Social".into(),
        pattern: "List-Id present".into(),
    };
    let r3 = engine.categorise(&e3, None, Some(heuristic), now);
    assert!(matches!(r3.source, CategoriseSource::HeaderHeuristic));
    assert_eq!(r3.bundle_id, Some(social_id));

    // Email 4: personal email, no matches -> uncategorised
    let e4 = MockEmail::new("friend@personal.com", "Dinner?");
    let r4 = engine.categorise(&e4, None, None, now);
    assert!(r4.bundle_id.is_none());
    assert!(matches!(r4.source, CategoriseSource::Uncategorised));
}

#[test]
fn confidence_lifecycle() {
    // Simulate: user moves emails 5 times -> full confidence -> time passes -> decay
    let now = Utc::now();
    let mut aff = SenderAffinity {
        sender_domain: "newsletter.com".into(),
        sender_address: "weekly@newsletter.com".into(),
        bundle_category: "Promos".into(),
        confidence: 0.0,
        learned_at: now,
    };

    // 5 reinforcements -> max confidence
    for _ in 0..5 {
        aff.reinforce(now);
    }
    assert!(
        (aff.confidence - 1.0).abs() < 0.001,
        "expected 1.0, got {}",
        aff.confidence
    );
    assert!(aff.is_confident(now));

    // After 90 days (1 half-life) -> confidence ~0.5 -> below threshold
    let future = now + TimeDelta::days(90);
    assert!(!aff.is_confident(future));

    // User reinforces again -> resets clock, bumps to 1.0 (was 1.0, capped)
    aff.reinforce(future);
    assert!(aff.is_confident(future));
}

#[test]
fn user_rule_overrides_everything() {
    let custom_id = Uuid::new_v4();
    let social_id = Uuid::new_v4();

    let mut category_map = HashMap::new();
    category_map.insert("Social".to_owned(), social_id);

    let rules = vec![UserCompiledRule::compile(BundleRule {
        id: Uuid::new_v4(),
        bundle_id: custom_id,
        field: UserRuleField::Subject,
        operator: UserRuleOp::Contains,
        value: "URGENT".into(),
        priority: 200,
    })];

    let engine = BundlerEngine::new(rules, category_map);
    let now = Utc::now();

    // All three layers would match, but user rule should win
    let email = MockEmail::new("bot@social.com", "URGENT: action needed");
    let affinity = SenderAffinity {
        sender_domain: "social.com".into(),
        sender_address: "bot@social.com".into(),
        bundle_category: "Social".into(),
        confidence: 1.0,
        learned_at: now,
    };
    let heuristic = HeuristicMatch {
        category: "Social".into(),
        pattern: "domain match".into(),
    };

    let result = engine.categorise(&email, Some(&affinity), Some(heuristic), now);
    assert_eq!(result.bundle_id, Some(custom_id));
    assert!(matches!(result.source, CategoriseSource::UserRule { .. }));
}

#[test]
fn regex_rule_matches_pattern() {
    let bundle_id = Uuid::new_v4();
    let rules = vec![UserCompiledRule::compile(BundleRule {
        id: Uuid::new_v4(),
        bundle_id,
        field: UserRuleField::Subject,
        operator: UserRuleOp::Matches,
        value: r"(?i)order\s*#\d+".into(),
        priority: 100,
    })];

    let engine = BundlerEngine::new(rules, HashMap::new());
    let now = Utc::now();

    let email = MockEmail::new("shop@store.com", "Your Order #12345 has shipped");
    let result = engine.categorise(&email, None, None, now);
    assert_eq!(result.bundle_id, Some(bundle_id));
    assert!(matches!(result.source, CategoriseSource::UserRule { .. }));

    // Non-matching email
    let email2 = MockEmail::new("shop@store.com", "Thank you for shopping");
    let result2 = engine.categorise(&email2, None, None, now);
    assert!(matches!(result2.source, CategoriseSource::Uncategorised));
}
