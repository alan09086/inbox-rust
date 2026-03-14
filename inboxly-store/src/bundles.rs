use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct BundleRow {
    pub id: String,
    pub category: String,
    pub name: String,
    pub color: String,
    pub badge_color: String,
    pub visibility: String,
    pub throttle: String,
    pub sort_order: i64,
}

impl Store {
    pub fn insert_bundle(&self, bundle: &BundleRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO bundles (id, category, name, color, badge_color, visibility, throttle, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                bundle.id,
                bundle.category,
                bundle.name,
                bundle.color,
                bundle.badge_color,
                bundle.visibility,
                bundle.throttle,
                bundle.sort_order,
            ],
        )?;
        Ok(())
    }

    pub fn get_bundle(&self, id: &str) -> Result<BundleRow> {
        self.conn()
            .query_row(
                "SELECT id, category, name, color, badge_color, visibility, throttle, sort_order
                 FROM bundles WHERE id = ?1",
                params![id],
                Self::row_to_bundle,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("bundle {id}"))
                }
                other => StoreError::Sqlite(other),
            })
    }

    /// Get all bundles ordered by sort_order.
    pub fn list_bundles(&self) -> Result<Vec<BundleRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, category, name, color, badge_color, visibility, throttle, sort_order
             FROM bundles ORDER BY sort_order ASC",
        )?;
        let rows = stmt
            .query_map([], Self::row_to_bundle)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Find a bundle by category name (e.g., "Social", "Promos").
    pub fn get_bundle_by_category(&self, category: &str) -> Result<Option<BundleRow>> {
        let result = self.conn().query_row(
            "SELECT id, category, name, color, badge_color, visibility, throttle, sort_order
             FROM bundles WHERE category = ?1",
            params![category],
            Self::row_to_bundle,
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Sqlite(e)),
        }
    }

    pub fn update_bundle(&self, bundle: &BundleRow) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE bundles SET category = ?2, name = ?3, color = ?4, badge_color = ?5,
             visibility = ?6, throttle = ?7, sort_order = ?8
             WHERE id = ?1",
            params![
                bundle.id,
                bundle.category,
                bundle.name,
                bundle.color,
                bundle.badge_color,
                bundle.visibility,
                bundle.throttle,
                bundle.sort_order,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("bundle {}", bundle.id)));
        }
        Ok(())
    }

    pub fn delete_bundle(&self, id: &str) -> Result<()> {
        let changed = self.conn().execute(
            "DELETE FROM bundles WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("bundle {id}")));
        }
        Ok(())
    }

    fn row_to_bundle(row: &rusqlite::Row<'_>) -> rusqlite::Result<BundleRow> {
        Ok(BundleRow {
            id: row.get(0)?,
            category: row.get(1)?,
            name: row.get(2)?,
            color: row.get(3)?,
            badge_color: row.get(4)?,
            visibility: row.get(5)?,
            throttle: row.get(6)?,
            sort_order: row.get(7)?,
        })
    }
}
