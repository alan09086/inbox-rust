//! Implements [`AffinityStore`] for [`Store`].
//!
//! Bridges the SQL layer in `sender_affinity.rs` (which works with
//! [`SenderAffinityRow`]) to the [`AffinityStore`] trait (which works with
//! [`SenderAffinity`]).
//!
//! Timestamp values are stored as i64 Unix timestamps (seconds since epoch)
//! and converted to/from `DateTime<Utc>`.

use chrono::{DateTime, TimeZone, Utc};
use inboxly_core::store_traits::{
    AffinityStore, AffinityStoreError, SenderAffinity, CONFIDENCE_INCREMENT, CONFIDENCE_MAX,
    CONFIDENCE_OVERRIDE_PENALTY,
};

use crate::sender_affinity::SenderAffinityRow;
use crate::store::Store;

// ---------------------------------------------------------------------------
// Row ↔ SenderAffinity conversion
// ---------------------------------------------------------------------------

/// Convert a [`SenderAffinityRow`] into a [`SenderAffinity`].
fn row_to_affinity(row: SenderAffinityRow) -> SenderAffinity {
    let learned_at = Utc
        .timestamp_opt(row.learned_at, 0)
        .single()
        .unwrap_or_else(Utc::now);

    SenderAffinity {
        sender_domain: row.sender_domain,
        sender_address: row.sender_address,
        bundle_category: row.bundle_category,
        confidence: row.confidence,
        learned_at,
    }
}

// ---------------------------------------------------------------------------
// AffinityStore impl
// ---------------------------------------------------------------------------

impl AffinityStore for Store {
    fn get_affinity(
        &self,
        sender_address: &str,
    ) -> Result<Option<SenderAffinity>, AffinityStoreError> {
        self.get_sender_affinity(sender_address)
            .map(|opt| opt.map(row_to_affinity))
            .map_err(|e| AffinityStoreError::Database(e.to_string()))
    }

    fn record_affinity(
        &self,
        sender_address: &str,
        sender_domain: &str,
        bundle_category: &str,
        now: DateTime<Utc>,
    ) -> Result<SenderAffinity, AffinityStoreError> {
        let learned_at_ts = now.timestamp();

        // Check for existing affinity on this sender address.
        let existing = self
            .get_sender_affinity(sender_address)
            .map_err(|e| AffinityStoreError::Database(e.to_string()))?;

        let new_confidence = match &existing {
            Some(row) if row.bundle_category == bundle_category => {
                // Same category: reinforce.
                (row.confidence + CONFIDENCE_INCREMENT).min(CONFIDENCE_MAX)
            }
            Some(row) => {
                // Different category: penalize old affinity, then start/reinforce new one.
                let penalized_confidence =
                    (row.confidence - CONFIDENCE_OVERRIDE_PENALTY).max(0.0);
                let old_row = SenderAffinityRow {
                    sender_domain: row.sender_domain.clone(),
                    sender_address: row.sender_address.clone(),
                    bundle_category: row.bundle_category.clone(),
                    confidence: penalized_confidence,
                    learned_at: row.learned_at,
                };
                self.upsert_sender_affinity(&old_row)
                    .map_err(|e| AffinityStoreError::Database(e.to_string()))?;
                // New affinity starts from 0 + one increment.
                CONFIDENCE_INCREMENT
            }
            None => {
                // No existing affinity: start from one increment.
                CONFIDENCE_INCREMENT
            }
        };

        let new_row = SenderAffinityRow {
            sender_domain: sender_domain.to_string(),
            sender_address: sender_address.to_string(),
            bundle_category: bundle_category.to_string(),
            confidence: new_confidence,
            learned_at: learned_at_ts,
        };

        self.upsert_sender_affinity(&new_row)
            .map_err(|e| AffinityStoreError::Database(e.to_string()))?;

        // Re-fetch for the canonical persisted state.
        self.get_affinity(sender_address)?
            .ok_or_else(|| AffinityStoreError::Database("affinity vanished after upsert".into()))
    }

    fn list_affinities(&self) -> Result<Vec<SenderAffinity>, AffinityStoreError> {
        self.list_all_sender_affinities()
            .map(|rows| rows.into_iter().map(row_to_affinity).collect())
            .map_err(|e| AffinityStoreError::Database(e.to_string()))
    }

    fn delete_affinity(&self, sender_address: &str) -> Result<(), AffinityStoreError> {
        self.delete_sender_affinity_by_address(sender_address)
            .map_err(|e| AffinityStoreError::Database(e.to_string()))
    }
}
