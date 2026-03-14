use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct ThreadStateRow {
    pub thread_id: String,
    pub pinned: bool,
    pub done: bool,
    pub snoozed_until: Option<i64>,
    pub snoozed_location_json: Option<String>,
    pub bundle_id: Option<String>,
}

impl Store {
    pub fn insert_thread_state(&self, state: &ThreadStateRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO thread_state (thread_id, pinned, done, snoozed_until, snoozed_location_json, bundle_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                state.thread_id,
                state.pinned,
                state.done,
                state.snoozed_until,
                state.snoozed_location_json,
                state.bundle_id,
            ],
        )?;
        Ok(())
    }

    pub fn get_thread_state(&self, thread_id: &str) -> Result<ThreadStateRow> {
        self.conn()
            .query_row(
                "SELECT thread_id, pinned, done, snoozed_until, snoozed_location_json, bundle_id
                 FROM thread_state WHERE thread_id = ?1",
                params![thread_id],
                Self::row_to_thread_state,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("thread_state {thread_id}"))
                }
                other => StoreError::Sqlite(other),
            })
    }

    /// Get or create thread state. Returns existing row or inserts default and returns it.
    pub fn get_or_create_thread_state(&self, thread_id: &str) -> Result<ThreadStateRow> {
        match self.get_thread_state(thread_id) {
            Ok(state) => Ok(state),
            Err(StoreError::NotFound(_)) => {
                let state = ThreadStateRow {
                    thread_id: thread_id.to_string(),
                    pinned: false,
                    done: false,
                    snoozed_until: None,
                    snoozed_location_json: None,
                    bundle_id: None,
                };
                self.insert_thread_state(&state)?;
                Ok(state)
            }
            Err(e) => Err(e),
        }
    }

    pub fn set_thread_pinned(&self, thread_id: &str, pinned: bool) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE thread_state SET pinned = ?2 WHERE thread_id = ?1",
            params![thread_id, pinned],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("thread_state {thread_id}")));
        }
        Ok(())
    }

    pub fn set_thread_done(&self, thread_id: &str, done: bool) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE thread_state SET done = ?2 WHERE thread_id = ?1",
            params![thread_id, done],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("thread_state {thread_id}")));
        }
        Ok(())
    }

    pub fn set_thread_snoozed(
        &self,
        thread_id: &str,
        until: Option<i64>,
        location_json: Option<&str>,
    ) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE thread_state SET snoozed_until = ?2, snoozed_location_json = ?3 WHERE thread_id = ?1",
            params![thread_id, until, location_json],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("thread_state {thread_id}")));
        }
        Ok(())
    }

    pub fn set_thread_bundle(&self, thread_id: &str, bundle_id: Option<&str>) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE thread_state SET bundle_id = ?2 WHERE thread_id = ?1",
            params![thread_id, bundle_id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("thread_state {thread_id}")));
        }
        Ok(())
    }

    /// Get all pinned, not-done threads (for the Pinned section of the inbox feed).
    pub fn get_pinned_threads(&self) -> Result<Vec<ThreadStateRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT thread_id, pinned, done, snoozed_until, snoozed_location_json, bundle_id
             FROM thread_state WHERE pinned = 1 AND done = 0",
        )?;
        let rows = stmt
            .query_map([], Self::row_to_thread_state)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get all snoozed threads (for the Snoozed view and the snooze scheduler).
    pub fn get_snoozed_threads(&self) -> Result<Vec<ThreadStateRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT thread_id, pinned, done, snoozed_until, snoozed_location_json, bundle_id
             FROM thread_state WHERE snoozed_until IS NOT NULL",
        )?;
        let rows = stmt
            .query_map([], Self::row_to_thread_state)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get all threads assigned to a bundle.
    pub fn get_threads_by_bundle(&self, bundle_id: &str) -> Result<Vec<ThreadStateRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT thread_id, pinned, done, snoozed_until, snoozed_location_json, bundle_id
             FROM thread_state WHERE bundle_id = ?1",
        )?;
        let rows = stmt
            .query_map(params![bundle_id], Self::row_to_thread_state)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get thread IDs where `bundle_id IS NULL` (not yet categorised).
    ///
    /// Used by the bundler's `categorise_all()` to find threads that need
    /// automatic categorisation.
    pub fn get_uncategorised_thread_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn().prepare(
            "SELECT thread_id FROM thread_state WHERE bundle_id IS NULL",
        )?;
        let ids = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    pub fn delete_thread_state(&self, thread_id: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM thread_state WHERE thread_id = ?1",
            params![thread_id],
        )?;
        Ok(())
    }

    fn row_to_thread_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<ThreadStateRow> {
        Ok(ThreadStateRow {
            thread_id: row.get(0)?,
            pinned: row.get(1)?,
            done: row.get(2)?,
            snoozed_until: row.get(3)?,
            snoozed_location_json: row.get(4)?,
            bundle_id: row.get(5)?,
        })
    }
}
