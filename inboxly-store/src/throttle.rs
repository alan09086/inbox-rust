//! Throttle-related database operations.
//!
//! CRUD for per-bundle throttle settings, plus the throttle-aware
//! suppression query used by the inbox feed.

use chrono::{DateTime, Local};
use inboxly_core::BundleId;
use inboxly_core::throttle::BundleThrottle;
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

    /// Get threads for an account, excluding those in suppressed bundles.
    ///
    /// This is the throttle-aware version of `get_threads_by_account`.
    /// Threads assigned to bundles in `suppressed_bundle_ids` are excluded.
    /// Threads not assigned to any bundle are always included.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn get_threads_excluding_bundles(
        &self,
        account_id: &str,
        suppressed_bundle_ids: &[BundleId],
        limit: i64,
        offset: i64,
    ) -> Result<Vec<crate::ThreadRow>> {
        if suppressed_bundle_ids.is_empty() {
            // No suppression -- use the regular query
            return self.get_threads_by_account(account_id, limit, offset);
        }

        // Build a query that joins thread_state and excludes suppressed bundles.
        // Threads with no bundle assignment (bundle_id IS NULL) are always included.
        let placeholders: Vec<String> = suppressed_bundle_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 4))
            .collect();
        let exclusion = placeholders.join(", ");

        let sql = format!(
            "SELECT t.id, t.account_id, t.subject, t.newest_date, t.oldest_date,
                    t.email_count, t.unread_count, t.has_attachments, t.snippet, t.root_message_id
             FROM threads t
             LEFT JOIN thread_state ts ON t.id = ts.thread_id
             WHERE t.account_id = ?1
               AND (ts.bundle_id IS NULL OR ts.bundle_id NOT IN ({exclusion}))
             ORDER BY t.newest_date DESC
             LIMIT ?2 OFFSET ?3"
        );

        let mut stmt = self.conn().prepare(&sql)?;

        // Build parameter list: account_id, limit, offset, then all suppressed IDs
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        param_values.push(Box::new(account_id.to_string()));
        param_values.push(Box::new(limit));
        param_values.push(Box::new(offset));
        for id in suppressed_bundle_ids {
            param_values.push(Box::new(id.to_string()));
        }

        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(params_ref.as_slice(), |row| {
                Ok(crate::ThreadRow {
                    id: row.get(0)?,
                    account_id: row.get(1)?,
                    subject: row.get(2)?,
                    newest_date: row.get(3)?,
                    oldest_date: row.get(4)?,
                    email_count: row.get(5)?,
                    unread_count: row.get(6)?,
                    has_attachments: row.get(7)?,
                    snippet: row.get(8)?,
                    root_message_id: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get threads for an account with automatic throttle filtering.
    ///
    /// Computes which bundles are currently suppressed based on the local time,
    /// then runs the thread query with those bundles excluded.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub fn get_threads_throttled(
        &self,
        account_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<crate::ThreadRow>> {
        let now = Local::now();
        let suppressed = self.get_currently_suppressed_bundle_ids(&now)?;
        self.get_threads_excluding_bundles(account_id, &suppressed, limit, offset)
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

    fn ensure_test_account(store: &Store, account_id: &str) {
        // Insert account if it doesn't exist (ignore duplicate errors)
        let _ = store.conn().execute(
            "INSERT OR IGNORE INTO accounts (id, email, display_name, provider, auth_method, imap_host, imap_port, smtp_host, smtp_port)
             VALUES (?1, 'test@test.com', 'Test', 'test', 'password', 'imap.test.com', 993, 'smtp.test.com', 587)",
            params![account_id],
        );
    }

    fn insert_test_thread_with_bundle(
        store: &Store,
        thread_id: &str,
        account_id: &str,
        bundle_id: Option<&str>,
    ) {
        ensure_test_account(store, account_id);
        store
            .conn()
            .execute(
                "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet)
                 VALUES (?1, ?2, 'test', 1000000, 1000000, 1, 0, 0, 'snippet')",
                params![thread_id, account_id],
            )
            .expect("insert thread");
        store
            .conn()
            .execute(
                "INSERT INTO thread_state (thread_id, bundle_id) VALUES (?1, ?2)",
                params![thread_id, bundle_id],
            )
            .expect("insert thread_state");
    }

    #[test]
    fn throttle_filter_excludes_suppressed_bundle_threads() {
        let store = Store::open_in_memory().expect("open store");
        let account = "acc-1";
        let bundle_id = BundleId::new();
        let bundle_id_str = bundle_id.to_string();

        // Create a throttled bundle
        insert_test_bundle(
            &store,
            &bundle_id,
            &BundleThrottle::Daily {
                delivery_time: NaiveTime::from_hms_opt(23, 59, 0).expect("valid time"),
            },
        );

        // Insert threads: one in the throttled bundle, one unbundled
        insert_test_thread_with_bundle(&store, "t1", account, Some(&bundle_id_str));
        insert_test_thread_with_bundle(&store, "t2", account, None);

        // Exclude the throttled bundle
        let threads = store
            .get_threads_excluding_bundles(account, &[bundle_id], 100, 0)
            .expect("query");

        // Should only see the unbundled thread
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "t2");
    }

    #[test]
    fn throttle_filter_includes_all_when_no_suppression() {
        let store = Store::open_in_memory().expect("open store");
        let account = "acc-1";
        let bundle_id = BundleId::new();
        let bundle_id_str = bundle_id.to_string();

        insert_test_bundle(&store, &bundle_id, &BundleThrottle::Immediate);
        insert_test_thread_with_bundle(&store, "t1", account, Some(&bundle_id_str));
        insert_test_thread_with_bundle(&store, "t2", account, None);

        // No suppression
        let threads = store
            .get_threads_excluding_bundles(account, &[], 100, 0)
            .expect("query");
        assert_eq!(threads.len(), 2);
    }

    #[test]
    fn throttle_filter_unbundled_threads_never_excluded() {
        let store = Store::open_in_memory().expect("open store");
        let account = "acc-1";
        let fake_id = BundleId::new();

        // Thread with no bundle
        insert_test_thread_with_bundle(&store, "t1", account, None);

        // Even if we pass a suppressed bundle ID, unbundled thread stays
        let threads = store
            .get_threads_excluding_bundles(account, &[fake_id], 100, 0)
            .expect("query");
        assert_eq!(threads.len(), 1);
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
