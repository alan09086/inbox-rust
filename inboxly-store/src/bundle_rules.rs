use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct BundleRuleRow {
    pub id: String,
    pub bundle_id: String,
    pub field: String,    // "From", "To", "Subject", "Header", "Body"
    pub operator: String, // "Contains", "Equals", "Matches", "Domain"
    pub value: String,
    pub priority: i64,
}

impl Store {
    pub fn insert_bundle_rule(&self, rule: &BundleRuleRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO bundle_rules (id, bundle_id, field, operator, value, priority)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                rule.id,
                rule.bundle_id,
                rule.field,
                rule.operator,
                rule.value,
                rule.priority,
            ],
        )?;
        Ok(())
    }

    pub fn get_bundle_rule(&self, id: &str) -> Result<BundleRuleRow> {
        self.conn()
            .query_row(
                "SELECT id, bundle_id, field, operator, value, priority
                 FROM bundle_rules WHERE id = ?1",
                params![id],
                Self::row_to_bundle_rule,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("bundle_rule {id}"))
                }
                other => StoreError::Sqlite(other),
            })
    }

    /// Get all rules for a bundle, ordered by priority descending (highest first).
    pub fn get_rules_for_bundle(&self, bundle_id: &str) -> Result<Vec<BundleRuleRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, bundle_id, field, operator, value, priority
             FROM bundle_rules WHERE bundle_id = ?1 ORDER BY priority DESC",
        )?;
        let rows = stmt
            .query_map(params![bundle_id], Self::row_to_bundle_rule)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get ALL rules across all bundles, ordered by priority descending.
    /// Used by the bundler engine to evaluate all rules against an email.
    pub fn get_all_bundle_rules(&self) -> Result<Vec<BundleRuleRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, bundle_id, field, operator, value, priority
             FROM bundle_rules ORDER BY priority DESC",
        )?;
        let rows = stmt
            .query_map([], Self::row_to_bundle_rule)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn update_bundle_rule(&self, rule: &BundleRuleRow) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE bundle_rules SET bundle_id = ?2, field = ?3, operator = ?4, value = ?5, priority = ?6
             WHERE id = ?1",
            params![
                rule.id,
                rule.bundle_id,
                rule.field,
                rule.operator,
                rule.value,
                rule.priority,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("bundle_rule {}", rule.id)));
        }
        Ok(())
    }

    pub fn delete_bundle_rule(&self, id: &str) -> Result<()> {
        let changed = self
            .conn()
            .execute("DELETE FROM bundle_rules WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("bundle_rule {id}")));
        }
        Ok(())
    }

    fn row_to_bundle_rule(row: &rusqlite::Row<'_>) -> rusqlite::Result<BundleRuleRow> {
        Ok(BundleRuleRow {
            id: row.get(0)?,
            bundle_id: row.get(1)?,
            field: row.get(2)?,
            operator: row.get(3)?,
            value: row.get(4)?,
            priority: row.get(5)?,
        })
    }
}
