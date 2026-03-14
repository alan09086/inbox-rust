use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct SyncStateRow {
    pub account_id: String,
    pub folder_name: String,
    pub uid_validity: Option<i64>,
    pub uid_next: Option<i64>,
    pub highest_modseq: Option<i64>,
    pub last_sync: Option<i64>,
}

impl Store {
    pub fn upsert_sync_state(&self, state: &SyncStateRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO sync_state (account_id, folder_name, uid_validity, uid_next, highest_modseq, last_sync)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(account_id, folder_name) DO UPDATE SET
                uid_validity = excluded.uid_validity,
                uid_next = excluded.uid_next,
                highest_modseq = excluded.highest_modseq,
                last_sync = excluded.last_sync",
            params![
                state.account_id,
                state.folder_name,
                state.uid_validity,
                state.uid_next,
                state.highest_modseq,
                state.last_sync,
            ],
        )?;
        Ok(())
    }

    pub fn get_sync_state(
        &self,
        account_id: &str,
        folder_name: &str,
    ) -> Result<Option<SyncStateRow>> {
        let result = self.conn().query_row(
            "SELECT account_id, folder_name, uid_validity, uid_next, highest_modseq, last_sync
             FROM sync_state WHERE account_id = ?1 AND folder_name = ?2",
            params![account_id, folder_name],
            |row| {
                Ok(SyncStateRow {
                    account_id: row.get(0)?,
                    folder_name: row.get(1)?,
                    uid_validity: row.get(2)?,
                    uid_next: row.get(3)?,
                    highest_modseq: row.get(4)?,
                    last_sync: row.get(5)?,
                })
            },
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Sqlite(e)),
        }
    }

    /// Get all sync states for an account (one per synced folder).
    pub fn get_sync_states_for_account(&self, account_id: &str) -> Result<Vec<SyncStateRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT account_id, folder_name, uid_validity, uid_next, highest_modseq, last_sync
             FROM sync_state WHERE account_id = ?1",
        )?;
        let rows = stmt
            .query_map(params![account_id], |row| {
                Ok(SyncStateRow {
                    account_id: row.get(0)?,
                    folder_name: row.get(1)?,
                    uid_validity: row.get(2)?,
                    uid_next: row.get(3)?,
                    highest_modseq: row.get(4)?,
                    last_sync: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_sync_state(&self, account_id: &str, folder_name: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM sync_state WHERE account_id = ?1 AND folder_name = ?2",
            params![account_id, folder_name],
        )?;
        Ok(())
    }

    /// Delete all sync state for an account (used when removing an account).
    pub fn delete_sync_states_for_account(&self, account_id: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM sync_state WHERE account_id = ?1",
            params![account_id],
        )?;
        Ok(())
    }
}
