//! Sender affinity tracking -- learns which bundle a sender belongs to
//! based on user behaviour (manually moving emails between bundles).
//!
//! The confidence model uses exponential decay with a 90-day half-life.
//! Five consistent user actions bring confidence from 0.0 to 1.0 (max).
//! An override (moving to a different bundle) penalises the old affinity.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Confidence constants
// ---------------------------------------------------------------------------

/// Minimum confidence required for sender learning to override header
/// heuristics.  Below this threshold, the sender learning result is
/// ignored and the email falls through to header heuristics.
pub const CONFIDENCE_THRESHOLD: f64 = 0.6;

/// Maximum confidence value.  Reached after ~5 consistent user actions.
pub const CONFIDENCE_MAX: f64 = 1.0;

/// Confidence increment per user action (move email to bundle).
/// 5 actions: 0.0 -> 0.2 -> 0.4 -> 0.6 -> 0.8 -> 1.0
pub const CONFIDENCE_INCREMENT: f64 = 0.2;

/// Confidence decrement when user overrides (moves to a different bundle
/// than the learned one).  Applied to the OLD affinity before creating
/// or boosting the new one.
pub const CONFIDENCE_OVERRIDE_PENALTY: f64 = 0.3;

/// Half-life for confidence decay in days.  After this many days without
/// reinforcement, confidence drops to half its value.
pub const CONFIDENCE_HALF_LIFE_DAYS: f64 = 90.0;

// ---------------------------------------------------------------------------
// SenderAffinity
// ---------------------------------------------------------------------------

/// A learned association between a sender and a bundle category.
///
/// When a user manually moves an email from sender X to bundle Y,
/// we record (or reinforce) an affinity entry.  Future emails from
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
    /// Confidence score \[0.0, 1.0\].  Higher = more confident.
    pub confidence: f64,
    /// When this affinity was last reinforced by a user action.
    pub learned_at: DateTime<Utc>,
}

impl SenderAffinity {
    /// Calculate the effective confidence after time-based decay.
    ///
    /// Uses exponential decay: `confidence * 2^(-days_elapsed / half_life)`.
    /// This means confidence halves every [`CONFIDENCE_HALF_LIFE_DAYS`] days
    /// without reinforcement.
    pub fn effective_confidence(&self, now: DateTime<Utc>) -> f64 {
        let days_elapsed = (now - self.learned_at).num_seconds() as f64 / 86400.0;
        if days_elapsed <= 0.0 {
            return self.confidence;
        }
        let decay_factor = 2.0_f64.powf(-days_elapsed / CONFIDENCE_HALF_LIFE_DAYS);
        self.confidence * decay_factor
    }

    /// Whether this affinity's effective confidence exceeds the threshold.
    pub fn is_confident(&self, now: DateTime<Utc>) -> bool {
        self.effective_confidence(now) >= CONFIDENCE_THRESHOLD
    }

    /// Reinforce this affinity -- user moved another email from this sender
    /// to the same bundle.  Bumps confidence and resets the decay clock.
    pub fn reinforce(&mut self, now: DateTime<Utc>) {
        self.confidence = (self.confidence + CONFIDENCE_INCREMENT).min(CONFIDENCE_MAX);
        self.learned_at = now;
    }

    /// Apply override penalty -- user moved an email from this sender to
    /// a DIFFERENT bundle.  Reduces confidence of this (old) affinity.
    pub fn penalize(&mut self) {
        self.confidence = (self.confidence - CONFIDENCE_OVERRIDE_PENALTY).max(0.0);
    }
}

// ---------------------------------------------------------------------------
// AffinityStore trait
// ---------------------------------------------------------------------------

/// Errors from affinity store operations.
#[derive(Debug, thiserror::Error)]
pub enum AffinityStoreError {
    /// An error from the underlying database.
    #[error("database error: {0}")]
    Database(String),
}

/// Trait for sender affinity persistence.
///
/// Implemented by `inboxly-store::Store` for production use.
pub trait AffinityStore {
    /// Look up the strongest affinity for a sender address.
    /// Returns the affinity with the highest confidence for this address.
    /// If no address-level affinity exists, falls back to domain-level.
    ///
    /// # Errors
    ///
    /// Returns [`AffinityStoreError::Database`] on database failure.
    fn get_affinity(
        &self,
        sender_address: &str,
    ) -> Result<Option<SenderAffinity>, AffinityStoreError>;

    /// Record or reinforce an affinity.
    ///
    /// If an affinity already exists for this sender+category, reinforce it.
    /// If it exists for a different category, penalize the old one and
    /// create/reinforce the new one.
    ///
    /// # Errors
    ///
    /// Returns [`AffinityStoreError::Database`] on database failure.
    fn record_affinity(
        &self,
        sender_address: &str,
        sender_domain: &str,
        bundle_category: &str,
        now: DateTime<Utc>,
    ) -> Result<SenderAffinity, AffinityStoreError>;

    /// List all affinities (for settings UI / export).
    ///
    /// # Errors
    ///
    /// Returns [`AffinityStoreError::Database`] on database failure.
    fn list_affinities(&self) -> Result<Vec<SenderAffinity>, AffinityStoreError>;

    /// Delete a specific affinity (user wants to "unlearn" a sender).
    ///
    /// # Errors
    ///
    /// Returns [`AffinityStoreError::Database`] on database failure.
    fn delete_affinity(&self, sender_address: &str) -> Result<(), AffinityStoreError>;
}
