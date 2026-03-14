//! Throttle-related database operations.
//!
//! CRUD for per-bundle throttle settings, plus the throttle-aware
//! suppression query used by the inbox feed.

use chrono::{DateTime, Local};
use inboxly_core::throttle::BundleThrottle;
use inboxly_core::BundleId;
use rusqlite::params;

use crate::error::Result;
use crate::store::Store;

impl Store {
    /// Get the throttle configuration for a bundle.
    ///
    /// # Errors
    ///
    /// Returns `StoreError::NotFound` if the bundle does not exist,
    /// or `StoreError::Json` if the stored JSON is malformed.
    pub fn get_bundle_throttle(&self, bundle_id: &BundleId) -> Result<BundleThrottle> {
        let json: String = self
            .conn()
            .query_row(
                "SELECT throttle FROM bundles WHERE id = ?1",
                params![bundle_id.to_string()],
                |row| row.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    crate::error::StoreError::NotFound(format!("bundle {bundle_id}"))
                }
                other => crate::error::StoreError::Sqlite(other),
            })?;
        let throttle: BundleThrottle = serde_json::from_str(&json)?;
        Ok(throttle)
    }

    /// Set the throttle configuration for a bundle.
    ///
    /// Takes effect immediately -- the next inbox feed query will reflect
    /// the new throttle setting.
    ///
    /// # Errors
    ///
    /// Returns `StoreError::NotFound` if the bundle does not exist.
    pub fn set_bundle_throttle(
        &self,
        bundle_id: &BundleId,
        throttle: &BundleThrottle,
    ) -> Result<()> {
        let json = serde_json::to_string(throttle)?;
        let changed = self.conn().execute(
            "UPDATE bundles SET throttle = ?1 WHERE id = ?2",
            params![json, bundle_id.to_string()],
        )?;
        if changed == 0 {
            return Err(crate::error::StoreError::NotFound(format!(
                "bundle {bundle_id}"
            )));
        }
        Ok(())
    }

    /// Get all bundles that have non-Immediate throttle settings.
    ///
    /// Returns `(BundleId, BundleThrottle)` pairs for bundles with active throttling.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails or stored JSON is malformed.
    pub fn get_throttled_bundles(&self) -> Result<Vec<(BundleId, BundleThrottle)>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, throttle FROM bundles WHERE throttle != '{\"mode\":\"Immediate\"}'",
        )?;
        let rows = stmt.query_map([], |row| {
            let id_str: String = row.get(0)?;
            let json: String = row.get(1)?;
            Ok((id_str, json))
        })?;

        let mut result = Vec::new();
        for row in rows {
            let (id_str, json) = row?;
            let bundle_id: BundleId = id_str
                .parse()
                .map_err(|e| crate::error::StoreError::Parse(format!("invalid bundle id: {e}")))?;
            let throttle: BundleThrottle = serde_json::from_str(&json)?;
            if throttle.is_throttled() {
                result.push((bundle_id, throttle));
            }
        }
        Ok(result)
    }

    /// Returns the set of bundle IDs that are currently throttled (window not open).
    ///
    /// Used by the inbox feed query to filter out suppressed bundles.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn get_currently_suppressed_bundle_ids(
        &self,
        local_now: &DateTime<Local>,
    ) -> Result<Vec<BundleId>> {
        let throttled = self.get_throttled_bundles()?;
        let suppressed = throttled
            .into_iter()
            .filter(|(_, throttle)| !throttle.is_window_open(local_now))
            .map(|(id, _)| id)
            .collect();
        Ok(suppressed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;
    use inboxly_core::throttle::WeekdayWrapper;

    fn insert_test_bundle(store: &Store, id: &BundleId, throttle: &BundleThrottle) {
        let json = serde_json::to_string(throttle).expect("serialize throttle");
        store
            .conn()
            .execute(
                "INSERT INTO bundles (id, category, name, throttle) VALUES (?1, 'Promos', 'Test', ?2)",
                params![id.to_string(), json],
            )
            .expect("insert bundle");
    }

    #[test]
    fn get_set_throttle_roundtrip() {
        let store = Store::open_in_memory().expect("open store");
        let id = BundleId::new();
        let throttle = BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).expect("valid time"),
        };
        insert_test_bundle(&store, &id, &BundleThrottle::Immediate);

        store.set_bundle_throttle(&id, &throttle).expect("set");
        let loaded = store.get_bundle_throttle(&id).expect("get");
        assert_eq!(throttle, loaded);
    }

    #[test]
    fn get_throttled_bundles_excludes_immediate() {
        let store = Store::open_in_memory().expect("open store");
        let id1 = BundleId::new();
        let id2 = BundleId::new();
        insert_test_bundle(&store, &id1, &BundleThrottle::Immediate);
        insert_test_bundle(
            &store,
            &id2,
            &BundleThrottle::Daily {
                delivery_time: NaiveTime::from_hms_opt(9, 0, 0).expect("valid time"),
            },
        );

        let throttled = store.get_throttled_bundles().expect("list");
        assert_eq!(throttled.len(), 1);
        assert_eq!(throttled[0].0, id2);
    }

    #[test]
    fn set_throttle_on_missing_bundle_returns_not_found() {
        let store = Store::open_in_memory().expect("open store");
        let id = BundleId::new();
        let result = store.set_bundle_throttle(&id, &BundleThrottle::Immediate);
        assert!(result.is_err());
    }

    #[test]
    fn get_throttle_on_missing_bundle_returns_not_found() {
        let store = Store::open_in_memory().expect("open store");
        let id = BundleId::new();
        let result = store.get_bundle_throttle(&id);
        assert!(result.is_err());
    }

    #[test]
    fn weekly_throttle_roundtrip() {
        let store = Store::open_in_memory().expect("open store");
        let id = BundleId::new();
        let throttle = BundleThrottle::Weekly {
            delivery_day: WeekdayWrapper(chrono::Weekday::Mon),
            delivery_time: NaiveTime::from_hms_opt(8, 0, 0).expect("valid time"),
        };
        insert_test_bundle(&store, &id, &throttle);
        let loaded = store.get_bundle_throttle(&id).expect("get");
        assert_eq!(throttle, loaded);
    }

    #[test]
    fn migration_converts_old_throttle_format() {
        // Simulate a pre-v4 database with plain string throttle values
        let store = Store::open_in_memory().expect("open store");
        let id = BundleId::new();

        // Insert with old-format string (bypassing the new default)
        store
            .conn()
            .execute(
                "INSERT INTO bundles (id, category, name, throttle) VALUES (?1, 'Promos', 'Test', 'Immediate')",
                params![id.to_string()],
            )
            .expect("insert");

        // Manually run the migration logic on this row
        store
            .conn()
            .execute_batch(
                "UPDATE bundles SET throttle = '{\"mode\":\"Immediate\"}'
                 WHERE throttle = 'Immediate' AND throttle NOT LIKE '{%}';",
            )
            .expect("migrate");

        let loaded = store.get_bundle_throttle(&id).expect("get");
        assert_eq!(loaded, BundleThrottle::Immediate);
    }
}
