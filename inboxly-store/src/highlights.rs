use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

/// Row representation for the highlights table.
///
/// `highlight_type` is one of: "TrackingNumber", "Flight", "Hotel", "Event", "Payment"
/// `data_json` contains the type-specific fields as JSON (see Highlight enum in core).
#[derive(Debug, Clone)]
pub struct HighlightRow {
    pub id: Option<i64>, // AUTOINCREMENT, None on insert
    pub thread_id: String,
    pub highlight_type: String,
    pub data_json: String,
}

impl Store {
    pub fn insert_highlight(&self, highlight: &HighlightRow) -> Result<i64> {
        self.conn().execute(
            "INSERT INTO highlights (thread_id, highlight_type, data_json)
             VALUES (?1, ?2, ?3)",
            params![
                highlight.thread_id,
                highlight.highlight_type,
                highlight.data_json,
            ],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    /// Get all highlights for a thread.
    pub fn get_highlights_for_thread(&self, thread_id: &str) -> Result<Vec<HighlightRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, thread_id, highlight_type, data_json
             FROM highlights WHERE thread_id = ?1",
        )?;
        let rows = stmt
            .query_map(params![thread_id], |row| {
                Ok(HighlightRow {
                    id: row.get(0)?,
                    thread_id: row.get(1)?,
                    highlight_type: row.get(2)?,
                    data_json: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get all highlights of a given type (e.g., all "Flight" highlights for trip assembly).
    pub fn get_highlights_by_type(&self, highlight_type: &str) -> Result<Vec<HighlightRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, thread_id, highlight_type, data_json
             FROM highlights WHERE highlight_type = ?1",
        )?;
        let rows = stmt
            .query_map(params![highlight_type], |row| {
                Ok(HighlightRow {
                    id: row.get(0)?,
                    thread_id: row.get(1)?,
                    highlight_type: row.get(2)?,
                    data_json: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Delete all highlights for a thread (used when re-extracting after body download).
    pub fn delete_highlights_for_thread(&self, thread_id: &str) -> Result<u64> {
        let changed = self.conn().execute(
            "DELETE FROM highlights WHERE thread_id = ?1",
            params![thread_id],
        )?;
        Ok(changed as u64)
    }

    pub fn delete_highlight(&self, id: i64) -> Result<()> {
        let changed = self
            .conn()
            .execute("DELETE FROM highlights WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("highlight {id}")));
        }
        Ok(())
    }
}
