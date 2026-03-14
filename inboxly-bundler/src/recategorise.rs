//! Re-categorisation logic -- handles user manually moving emails
//! between bundles.
//!
//! When a user moves an email/thread to a different bundle:
//! 1. `thread_state.bundle_id` is updated (immediate effect, by caller)
//! 2. `sender_affinity` is updated (learning for future emails, by this module)

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::affinity::{AffinityStore, AffinityStoreError, SenderAffinity, CONFIDENCE_INCREMENT};

/// Describes a user's manual move action.
pub struct MoveAction {
    /// The thread being moved.
    pub thread_id: Uuid,
    /// The sender's email address.
    pub sender_address: String,
    /// The sender's domain.
    pub sender_domain: String,
    /// The target bundle's category name (e.g., "Social", "Promos", or custom name).
    pub target_bundle_category: String,
    /// The target bundle's ID.
    pub target_bundle_id: Uuid,
}

/// Result of processing a move action.
pub struct MoveResult {
    /// The updated/created sender affinity.
    pub affinity: SenderAffinity,
    /// Whether this was a new affinity (`true`) or reinforcement of existing (`false`).
    pub is_new: bool,
}

/// Process a user's manual move of an email to a different bundle.
///
/// This function:
/// 1. Records/reinforces the sender affinity for the target category
/// 2. If a different-category affinity existed, it was penalised by the store
///
/// The caller is responsible for updating `thread_state.bundle_id` separately.
///
/// # Errors
///
/// Returns [`AffinityStoreError`] if the database operation fails.
pub fn process_move<S: AffinityStore>(
    store: &S,
    action: &MoveAction,
    now: DateTime<Utc>,
) -> Result<MoveResult, AffinityStoreError> {
    let affinity = store.record_affinity(
        &action.sender_address,
        &action.sender_domain,
        &action.target_bundle_category,
        now,
    )?;

    // Determine if this was a new affinity by checking confidence level.
    // A freshly created affinity has exactly CONFIDENCE_INCREMENT.
    let is_new = (affinity.confidence - CONFIDENCE_INCREMENT).abs() < 0.001;

    Ok(MoveResult { affinity, is_new })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::affinity::mock::MockAffinityStore;

    #[test]
    fn first_move_creates_new_affinity() {
        let store = MockAffinityStore::new();
        let action = MoveAction {
            thread_id: Uuid::new_v4(),
            sender_address: "news@example.com".into(),
            sender_domain: "example.com".into(),
            target_bundle_category: "Promos".into(),
            target_bundle_id: Uuid::new_v4(),
        };
        let result = process_move(&store, &action, Utc::now()).expect("move");
        assert!(result.is_new);
        assert!(
            (result.affinity.confidence - 0.2).abs() < 0.001,
            "expected 0.2, got {}",
            result.affinity.confidence
        );
    }

    #[test]
    fn repeated_move_reinforces() {
        let store = MockAffinityStore::new();
        let action = MoveAction {
            thread_id: Uuid::new_v4(),
            sender_address: "news@example.com".into(),
            sender_domain: "example.com".into(),
            target_bundle_category: "Promos".into(),
            target_bundle_id: Uuid::new_v4(),
        };
        let now = Utc::now();
        process_move(&store, &action, now).expect("first");
        let result = process_move(&store, &action, now).expect("second");
        assert!(!result.is_new);
        assert!(
            (result.affinity.confidence - 0.4).abs() < 0.001,
            "expected 0.4, got {}",
            result.affinity.confidence
        );
    }

    #[test]
    fn move_to_different_bundle_overrides() {
        let store = MockAffinityStore::new();
        let addr = "bot@social.com";
        let domain = "social.com";
        let now = Utc::now();

        // First: learn as "Social"
        let action1 = MoveAction {
            thread_id: Uuid::new_v4(),
            sender_address: addr.into(),
            sender_domain: domain.into(),
            target_bundle_category: "Social".into(),
            target_bundle_id: Uuid::new_v4(),
        };
        process_move(&store, &action1, now).expect("social");

        // Then: override to "Promos"
        let action2 = MoveAction {
            thread_id: Uuid::new_v4(),
            sender_address: addr.into(),
            sender_domain: domain.into(),
            target_bundle_category: "Promos".into(),
            target_bundle_id: Uuid::new_v4(),
        };
        let result = process_move(&store, &action2, now).expect("promos");
        assert_eq!(result.affinity.bundle_category, "Promos");
    }
}
