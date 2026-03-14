use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct ThreadRow {
    pub id: String,
    pub account_id: String,
    pub subject: String,
    pub newest_date: i64,
    pub oldest_date: i64,
    pub email_count: i64,
    pub unread_count: i64,
    pub has_attachments: bool,
    pub snippet: String,
}

impl Store {
    pub fn insert_thread(&self, thread: &ThreadRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                thread.id,
                thread.account_id,
                thread.subject,
                thread.newest_date,
                thread.oldest_date,
                thread.email_count,
                thread.unread_count,
                thread.has_attachments,
                thread.snippet,
            ],
        )?;
        Ok(())
    }

    pub fn get_thread(&self, id: &str) -> Result<ThreadRow> {
        self.conn()
            .query_row(
                "SELECT id, account_id, subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet
                 FROM threads WHERE id = ?1",
                params![id],
                Self::row_to_thread,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("thread {id}"))
                }
                other => StoreError::Sqlite(other),
            })
    }

    /// Get threads for an account, ordered by newest_date descending.
    /// `limit` and `offset` support pagination.
    pub fn get_threads_by_account(
        &self,
        account_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ThreadRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, account_id, subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet
             FROM threads WHERE account_id = ?1 ORDER BY newest_date DESC LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt
            .query_map(params![account_id, limit, offset], Self::row_to_thread)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Update thread aggregate metadata (called after email insert/delete/flag change).
    pub fn update_thread(&self, thread: &ThreadRow) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE threads SET subject = ?2, newest_date = ?3, oldest_date = ?4,
             email_count = ?5, unread_count = ?6, has_attachments = ?7, snippet = ?8
             WHERE id = ?1",
            params![
                thread.id,
                thread.subject,
                thread.newest_date,
                thread.oldest_date,
                thread.email_count,
                thread.unread_count,
                thread.has_attachments,
                thread.snippet,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("thread {}", thread.id)));
        }
        Ok(())
    }

    pub fn delete_thread(&self, id: &str) -> Result<()> {
        let changed = self.conn().execute(
            "DELETE FROM threads WHERE id = ?1",
            params![id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("thread {id}")));
        }
        Ok(())
    }

    /// Insert or update a thread (upsert). Used during sync when a new email
    /// may create a thread or update an existing one.
    pub fn upsert_thread(&self, thread: &ThreadRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                subject = excluded.subject,
                newest_date = excluded.newest_date,
                oldest_date = excluded.oldest_date,
                email_count = excluded.email_count,
                unread_count = excluded.unread_count,
                has_attachments = excluded.has_attachments,
                snippet = excluded.snippet",
            params![
                thread.id,
                thread.account_id,
                thread.subject,
                thread.newest_date,
                thread.oldest_date,
                thread.email_count,
                thread.unread_count,
                thread.has_attachments,
                thread.snippet,
            ],
        )?;
        Ok(())
    }

    fn row_to_thread(row: &rusqlite::Row<'_>) -> rusqlite::Result<ThreadRow> {
        Ok(ThreadRow {
            id: row.get(0)?,
            account_id: row.get(1)?,
            subject: row.get(2)?,
            newest_date: row.get(3)?,
            oldest_date: row.get(4)?,
            email_count: row.get(5)?,
            unread_count: row.get(6)?,
            has_attachments: row.get(7)?,
            snippet: row.get(8)?,
        })
    }
}
