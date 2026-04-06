//! Sender affinity tracking -- re-exported from `inboxly-core`.
//!
//! The full implementation of the confidence model, [`SenderAffinity`] struct,
//! and [`AffinityStore`] trait have moved to [`inboxly_core::store_traits`].
//! This module re-exports everything for backwards compatibility.

pub use inboxly_core::store_traits::{
    AffinityStore, AffinityStoreError, CONFIDENCE_HALF_LIFE_DAYS, CONFIDENCE_INCREMENT,
    CONFIDENCE_MAX, CONFIDENCE_OVERRIDE_PENALTY, CONFIDENCE_THRESHOLD, SenderAffinity,
};

// ---------------------------------------------------------------------------
// In-memory mock for tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod mock {
    use super::*;
    use chrono::{DateTime, Utc};
    use std::sync::Mutex;

    /// In-memory mock implementation of [`AffinityStore`] for unit tests.
    pub struct MockAffinityStore {
        affinities: Mutex<Vec<SenderAffinity>>,
    }

    impl MockAffinityStore {
        pub fn new() -> Self {
            Self {
                affinities: Mutex::new(Vec::new()),
            }
        }
    }

    impl AffinityStore for MockAffinityStore {
        fn get_affinity(
            &self,
            sender_address: &str,
        ) -> Result<Option<SenderAffinity>, AffinityStoreError> {
            let affs = self.affinities.lock().expect("mock lock poisoned");
            // Address-level lookup first, highest confidence
            let result = affs
                .iter()
                .filter(|a| a.sender_address == sender_address)
                .max_by(|a, b| {
                    a.confidence
                        .partial_cmp(&b.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .cloned();
            if result.is_some() {
                return Ok(result);
            }
            // Domain-level fallback
            let domain = sender_address
                .rsplit_once('@')
                .map_or(sender_address, |(_, d)| d);
            Ok(affs
                .iter()
                .filter(|a| a.sender_domain == domain)
                .max_by(|a, b| {
                    a.confidence
                        .partial_cmp(&b.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .cloned())
        }

        fn record_affinity(
            &self,
            sender_address: &str,
            sender_domain: &str,
            bundle_category: &str,
            now: DateTime<Utc>,
        ) -> Result<SenderAffinity, AffinityStoreError> {
            let mut affs = self.affinities.lock().expect("mock lock poisoned");

            // Check for existing affinity with SAME category
            if let Some(existing) = affs.iter_mut().find(|a| {
                a.sender_address == sender_address && a.bundle_category == bundle_category
            }) {
                existing.reinforce(now);
                return Ok(existing.clone());
            }

            // Check for existing affinity with DIFFERENT category -> penalize
            for aff in affs.iter_mut() {
                if aff.sender_address == sender_address && aff.bundle_category != bundle_category {
                    aff.penalize();
                }
            }

            // Create new affinity
            let new_aff = SenderAffinity {
                sender_domain: sender_domain.to_owned(),
                sender_address: sender_address.to_owned(),
                bundle_category: bundle_category.to_owned(),
                confidence: CONFIDENCE_INCREMENT,
                learned_at: now,
            };
            affs.push(new_aff.clone());
            Ok(new_aff)
        }

        fn list_affinities(&self) -> Result<Vec<SenderAffinity>, AffinityStoreError> {
            Ok(self.affinities.lock().expect("mock lock poisoned").clone())
        }

        fn delete_affinity(&self, sender_address: &str) -> Result<(), AffinityStoreError> {
            let mut affs = self.affinities.lock().expect("mock lock poisoned");
            affs.retain(|a| a.sender_address != sender_address);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    fn make_affinity(confidence: f64, days_ago: i64) -> SenderAffinity {
        let now = chrono::Utc::now();
        SenderAffinity {
            sender_domain: "example.com".into(),
            sender_address: "bot@example.com".into(),
            bundle_category: "promos".into(),
            confidence,
            learned_at: now - TimeDelta::days(days_ago),
        }
    }

    // -- Confidence calculations -----------------------------------------------

    #[test]
    fn fresh_affinity_no_decay() {
        let a = make_affinity(0.8, 0);
        let eff = a.effective_confidence(chrono::Utc::now());
        assert!((eff - 0.8).abs() < 0.01, "expected ~0.8, got {eff}");
    }

    #[test]
    fn decay_at_half_life() {
        let a = make_affinity(1.0, CONFIDENCE_HALF_LIFE_DAYS as i64);
        let eff = a.effective_confidence(chrono::Utc::now());
        // After one half-life, confidence should be ~0.5
        assert!((eff - 0.5).abs() < 0.05, "expected ~0.5, got {eff}");
    }

    #[test]
    fn decay_at_two_half_lives() {
        let a = make_affinity(1.0, (CONFIDENCE_HALF_LIFE_DAYS * 2.0) as i64);
        let eff = a.effective_confidence(chrono::Utc::now());
        // After two half-lives, confidence should be ~0.25
        assert!((eff - 0.25).abs() < 0.05, "expected ~0.25, got {eff}");
    }

    #[test]
    fn below_threshold_after_decay() {
        // Start at 0.8, after 90 days (1 half-life) -> ~0.4, below 0.6 threshold
        let a = make_affinity(0.8, 90);
        assert!(!a.is_confident(chrono::Utc::now()));
    }

    #[test]
    fn above_threshold_when_fresh() {
        let a = make_affinity(0.8, 0);
        assert!(a.is_confident(chrono::Utc::now()));
    }

    #[test]
    fn reinforce_increases_confidence() {
        let mut a = make_affinity(0.4, 10);
        let now = chrono::Utc::now();
        a.reinforce(now);
        assert!(
            (a.confidence - 0.6).abs() < 0.001,
            "expected 0.6, got {}",
            a.confidence
        );
        assert_eq!(a.learned_at, now); // decay clock reset
    }

    #[test]
    fn reinforce_caps_at_max() {
        let mut a = make_affinity(0.9, 0);
        a.reinforce(chrono::Utc::now());
        assert!(
            (a.confidence - 1.0).abs() < 0.001,
            "expected 1.0, got {}",
            a.confidence
        );
    }

    #[test]
    fn penalize_decreases_confidence() {
        let mut a = make_affinity(0.8, 0);
        a.penalize();
        assert!(
            (a.confidence - 0.5).abs() < 0.001,
            "expected 0.5, got {}",
            a.confidence
        );
    }

    #[test]
    fn penalize_floors_at_zero() {
        let mut a = make_affinity(0.1, 0);
        a.penalize();
        assert!(
            a.confidence.abs() < 0.001,
            "expected 0.0, got {}",
            a.confidence
        );
    }

    #[test]
    fn five_reinforcements_reach_max() {
        let mut a = SenderAffinity {
            sender_domain: "test.com".into(),
            sender_address: "a@test.com".into(),
            bundle_category: "social".into(),
            confidence: 0.0,
            learned_at: chrono::Utc::now(),
        };
        let now = chrono::Utc::now();
        for _ in 0..5 {
            a.reinforce(now);
        }
        assert!(
            (a.confidence - 1.0).abs() < 0.001,
            "expected 1.0, got {}",
            a.confidence
        );
    }

    // -- AffinityStore mock tests ----------------------------------------------

    #[test]
    fn first_record_creates_affinity() {
        let store = mock::MockAffinityStore::new();
        let now = chrono::Utc::now();
        let aff = store
            .record_affinity("news@example.com", "example.com", "promos", now)
            .expect("record");
        assert!(
            (aff.confidence - CONFIDENCE_INCREMENT).abs() < 0.001,
            "expected {CONFIDENCE_INCREMENT}, got {}",
            aff.confidence
        );
    }

    #[test]
    fn repeated_record_reinforces() {
        let store = mock::MockAffinityStore::new();
        let now = chrono::Utc::now();
        store
            .record_affinity("news@example.com", "example.com", "promos", now)
            .expect("first");
        let aff = store
            .record_affinity("news@example.com", "example.com", "promos", now)
            .expect("second");
        assert!(
            (aff.confidence - 0.4).abs() < 0.001,
            "expected 0.4, got {}",
            aff.confidence
        );
    }

    #[test]
    fn override_penalises_old_creates_new() {
        let store = mock::MockAffinityStore::new();
        let now = chrono::Utc::now();
        store
            .record_affinity("bot@social.com", "social.com", "social", now)
            .expect("social");
        let new_aff = store
            .record_affinity("bot@social.com", "social.com", "promos", now)
            .expect("promos");
        assert_eq!(new_aff.bundle_category, "promos");
        assert!(
            (new_aff.confidence - CONFIDENCE_INCREMENT).abs() < 0.001,
            "expected {CONFIDENCE_INCREMENT}, got {}",
            new_aff.confidence
        );

        // Old affinity should be penalised
        let all = store.list_affinities().expect("list");
        let old = all
            .iter()
            .find(|a| a.bundle_category == "social")
            .expect("find old");
        assert!(
            old.confidence < CONFIDENCE_INCREMENT,
            "old confidence should be penalised, got {}",
            old.confidence
        );
    }

    #[test]
    fn get_affinity_returns_highest_confidence() {
        let store = mock::MockAffinityStore::new();
        let now = chrono::Utc::now();
        // Two different categories for same sender
        store
            .record_affinity("news@example.com", "example.com", "promos", now)
            .expect("promos");
        store
            .record_affinity("news@example.com", "example.com", "promos", now)
            .expect("promos2"); // reinforced to 0.4
        store
            .record_affinity("news@example.com", "example.com", "social", now)
            .expect("social"); // new at 0.2 (but promos was penalised)

        let best = store
            .get_affinity("news@example.com")
            .expect("get")
            .expect("some");
        // The "social" one is at 0.2, the "promos" one was penalised from 0.4 to 0.1
        assert_eq!(best.bundle_category, "social");
    }

    #[test]
    fn delete_affinity_removes_all_for_address() {
        let store = mock::MockAffinityStore::new();
        let now = chrono::Utc::now();
        store
            .record_affinity("a@b.com", "b.com", "promos", now)
            .expect("record");
        store.delete_affinity("a@b.com").expect("delete");
        assert!(store.get_affinity("a@b.com").expect("get").is_none());
    }
}
