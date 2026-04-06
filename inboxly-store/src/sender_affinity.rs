use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct SenderAffinityRow {
    pub sender_domain: String,
    pub sender_address: String,
    pub bundle_category: String,
    pub confidence: f64,
    pub learned_at: i64,
}

impl Store {
    /// Insert or update sender affinity. Confidence increases on repeated actions.
    pub fn upsert_sender_affinity(&self, affinity: &SenderAffinityRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO sender_affinity (sender_domain, sender_address, bundle_category, confidence, learned_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(sender_address, bundle_category) DO UPDATE SET
                confidence = excluded.confidence,
                learned_at = excluded.learned_at",
            params![
                affinity.sender_domain,
                affinity.sender_address,
                affinity.bundle_category,
                affinity.confidence,
                affinity.learned_at,
            ],
        )?;
        Ok(())
    }

    /// Get the best affinity for a sender address (highest confidence).
    pub fn get_sender_affinity(&self, sender_address: &str) -> Result<Option<SenderAffinityRow>> {
        let result = self.conn().query_row(
            "SELECT sender_domain, sender_address, bundle_category, confidence, learned_at
             FROM sender_affinity WHERE sender_address = ?1
             ORDER BY confidence DESC LIMIT 1",
            params![sender_address],
            Self::row_to_sender_affinity,
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Sqlite(e)),
        }
    }

    /// Get all affinities for a domain (for domain-level fallback when no exact address match).
    pub fn get_affinities_by_domain(&self, domain: &str) -> Result<Vec<SenderAffinityRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT sender_domain, sender_address, bundle_category, confidence, learned_at
             FROM sender_affinity WHERE sender_domain = ?1
             ORDER BY confidence DESC",
        )?;
        let rows = stmt
            .query_map(params![domain], Self::row_to_sender_affinity)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_sender_affinity(
        &self,
        sender_address: &str,
        bundle_category: &str,
    ) -> Result<()> {
        self.conn().execute(
            "DELETE FROM sender_affinity WHERE sender_address = ?1 AND bundle_category = ?2",
            params![sender_address, bundle_category],
        )?;
        Ok(())
    }

    /// List all sender affinities.
    pub fn list_all_sender_affinities(&self) -> Result<Vec<SenderAffinityRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT sender_domain, sender_address, bundle_category, confidence, learned_at
             FROM sender_affinity",
        )?;
        let rows = stmt
            .query_map([], Self::row_to_sender_affinity)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Delete all affinities for a sender address (regardless of bundle_category).
    pub fn delete_sender_affinity_by_address(&self, sender_address: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM sender_affinity WHERE sender_address = ?1",
            params![sender_address],
        )?;
        Ok(())
    }

    fn row_to_sender_affinity(row: &rusqlite::Row<'_>) -> rusqlite::Result<SenderAffinityRow> {
        Ok(SenderAffinityRow {
            sender_domain: row.get(0)?,
            sender_address: row.get(1)?,
            bundle_category: row.get(2)?,
            confidence: row.get(3)?,
            learned_at: row.get(4)?,
        })
    }
}
