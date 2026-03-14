use super::error::SyncResult;
use rusqlite::{Connection, params};

/// Persisted sync state for one (account, folder) pair.
#[derive(Debug, Clone)]
pub struct FolderSyncState {
    pub account_id: String,
    pub folder_name: String,
    pub uid_validity: u32,
    pub uid_next: u32,
    pub highest_modseq: Option<u64>,
    /// The last UID we successfully committed to SQLite during Phase 1.
    /// Used for crash recovery — resume from here instead of re-fetching everything.
    pub last_synced_uid: Option<u32>,
}

/// Save (upsert) sync state for a folder.
pub fn save_sync_state(conn: &Connection, state: &FolderSyncState) -> SyncResult<()> {
    conn.execute(
        "INSERT INTO sync_state (account_id, folder_name, uid_validity, uid_next, highest_modseq, last_sync, last_synced_uid)
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'), ?6)
         ON CONFLICT(account_id, folder_name) DO UPDATE SET
            uid_validity = excluded.uid_validity,
            uid_next = excluded.uid_next,
            highest_modseq = excluded.highest_modseq,
            last_sync = excluded.last_sync,
            last_synced_uid = excluded.last_synced_uid",
        params![
            state.account_id,
            state.folder_name,
            state.uid_validity,
            state.uid_next,
            state.highest_modseq,
            state.last_synced_uid,
        ],
    )?;
    Ok(())
}

/// Load sync state for a folder, if it exists.
pub fn load_sync_state(
    conn: &Connection,
    account_id: &str,
    folder_name: &str,
) -> SyncResult<Option<FolderSyncState>> {
    let mut stmt = conn.prepare(
        "SELECT uid_validity, uid_next, highest_modseq, last_synced_uid
         FROM sync_state
         WHERE account_id = ?1 AND folder_name = ?2",
    )?;

    let result = stmt.query_row(params![account_id, folder_name], |row| {
        Ok(FolderSyncState {
            account_id: account_id.to_string(),
            folder_name: folder_name.to_string(),
            uid_validity: row.get(0)?,
            uid_next: row.get(1)?,
            highest_modseq: row.get(2)?,
            last_synced_uid: row.get(3)?,
        })
    });

    match result {
        Ok(state) => Ok(Some(state)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Check if the server's UIDVALIDITY has changed since our last sync.
///
/// Returns `true` if the validity changed (meaning all cached UIDs are stale
/// and the folder must be re-synced from scratch).
/// Returns `false` if validity matches or no prior state exists.
pub fn check_uid_validity(
    conn: &Connection,
    account_id: &str,
    folder_name: &str,
    server_uid_validity: u32,
) -> SyncResult<bool> {
    match load_sync_state(conn, account_id, folder_name)? {
        None => Ok(false), // first sync, no prior state
        Some(state) => Ok(state.uid_validity != server_uid_validity),
    }
}

/// Delete all cached emails for a folder. Called when UIDVALIDITY changes.
///
/// This is a destructive operation — all locally cached metadata for UIDs in this
/// folder become invalid when the server resets UIDVALIDITY.
pub fn invalidate_folder(conn: &Connection, account_id: &str, folder_name: &str) -> SyncResult<()> {
    conn.execute(
        "DELETE FROM emails WHERE account_id = ?1 AND imap_folder = ?2",
        params![account_id, folder_name],
    )?;
    conn.execute(
        "DELETE FROM sync_state WHERE account_id = ?1 AND folder_name = ?2",
        params![account_id, folder_name],
    )?;
    Ok(())
}
