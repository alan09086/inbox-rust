use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct OfflineQueueRow {
    pub id: Option<i64>,     // AUTOINCREMENT, None on insert
    pub action: String,
    pub payload_json: String,
    pub created_at: i64,
}

impl Store {
    /// Enqueue an offline action for replay on reconnect.
    pub fn enqueue_offline_action(&self, action: &str, payload_json: &str) -> Result<i64> {
        self.conn().execute(
            "INSERT INTO offline_queue (action, payload_json) VALUES (?1, ?2)",
            params![action, payload_json],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    /// Get all queued actions in FIFO order.
    pub fn get_offline_queue(&self) -> Result<Vec<OfflineQueueRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, action, payload_json, created_at
             FROM offline_queue ORDER BY id ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(OfflineQueueRow {
                    id: row.get(0)?,
                    action: row.get(1)?,
                    payload_json: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Remove a successfully replayed action from the queue.
    pub fn dequeue_offline_action(&self, id: i64) -> Result<()> {
        let changed = self.conn().execute(
            "DELETE FROM offline_queue WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("offline_queue {id}")));
        }
        Ok(())
    }

    /// Clear the entire offline queue (e.g., after successful full replay).
    pub fn clear_offline_queue(&self) -> Result<u64> {
        let changed = self.conn().execute("DELETE FROM offline_queue", [])?;
        Ok(changed as u64)
    }

    /// Count pending offline actions.
    pub fn count_offline_queue(&self) -> Result<i64> {
        let count: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM offline_queue",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}
