use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct ReminderRow {
    pub id: String,
    pub title: String,
    pub due_at: Option<i64>,
    pub location_lat: Option<f64>,
    pub location_lng: Option<f64>,
    pub location_label: Option<String>,
    pub recurring: Option<String>,
    pub done: bool,
}

impl Store {
    pub fn insert_reminder(&self, reminder: &ReminderRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO reminders (id, title, due_at, location_lat, location_lng, location_label, recurring, done)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                reminder.id,
                reminder.title,
                reminder.due_at,
                reminder.location_lat,
                reminder.location_lng,
                reminder.location_label,
                reminder.recurring,
                reminder.done,
            ],
        )?;
        Ok(())
    }

    pub fn get_reminder(&self, id: &str) -> Result<ReminderRow> {
        self.conn()
            .query_row(
                "SELECT id, title, due_at, location_lat, location_lng, location_label, recurring, done
                 FROM reminders WHERE id = ?1",
                params![id],
                Self::row_to_reminder,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("reminder {id}"))
                }
                other => StoreError::Sqlite(other),
            })
    }

    /// Get all active (not done) reminders, ordered by due_at ascending.
    pub fn get_active_reminders(&self) -> Result<Vec<ReminderRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, title, due_at, location_lat, location_lng, location_label, recurring, done
             FROM reminders WHERE done = 0 ORDER BY due_at ASC",
        )?;
        let rows = stmt
            .query_map([], Self::row_to_reminder)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get reminders that are due (due_at <= now). Used by the snooze scheduler.
    pub fn get_due_reminders(&self, now: i64) -> Result<Vec<ReminderRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, title, due_at, location_lat, location_lng, location_label, recurring, done
             FROM reminders WHERE done = 0 AND due_at IS NOT NULL AND due_at <= ?1",
        )?;
        let rows = stmt
            .query_map(params![now], Self::row_to_reminder)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get location-based reminders (for geofence checking).
    pub fn get_location_reminders(&self) -> Result<Vec<ReminderRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, title, due_at, location_lat, location_lng, location_label, recurring, done
             FROM reminders WHERE done = 0 AND location_lat IS NOT NULL",
        )?;
        let rows = stmt
            .query_map([], Self::row_to_reminder)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn set_reminder_done(&self, id: &str, done: bool) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE reminders SET done = ?2 WHERE id = ?1",
            params![id, done],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("reminder {id}")));
        }
        Ok(())
    }

    pub fn update_reminder(&self, reminder: &ReminderRow) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE reminders SET title = ?2, due_at = ?3, location_lat = ?4, location_lng = ?5,
             location_label = ?6, recurring = ?7, done = ?8
             WHERE id = ?1",
            params![
                reminder.id,
                reminder.title,
                reminder.due_at,
                reminder.location_lat,
                reminder.location_lng,
                reminder.location_label,
                reminder.recurring,
                reminder.done,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("reminder {}", reminder.id)));
        }
        Ok(())
    }

    pub fn delete_reminder(&self, id: &str) -> Result<()> {
        let changed = self
            .conn()
            .execute("DELETE FROM reminders WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("reminder {id}")));
        }
        Ok(())
    }

    fn row_to_reminder(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReminderRow> {
        Ok(ReminderRow {
            id: row.get(0)?,
            title: row.get(1)?,
            due_at: row.get(2)?,
            location_lat: row.get(3)?,
            location_lng: row.get(4)?,
            location_label: row.get(5)?,
            recurring: row.get(6)?,
            done: row.get(7)?,
        })
    }
}
