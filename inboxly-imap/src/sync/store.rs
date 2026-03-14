use rusqlite::{Connection, params};
use super::envelope::EnvelopeData;
use super::error::SyncResult;

/// Insert a batch of envelopes into the `emails` table within a single transaction.
///
/// Uses `INSERT OR IGNORE` to skip duplicates (same account_id + imap_folder + imap_uid).
/// Returns the number of rows actually inserted (excluding ignored duplicates).
pub fn batch_insert_envelopes(conn: &Connection, envelopes: &[EnvelopeData]) -> SyncResult<usize> {
    if envelopes.is_empty() {
        return Ok(0);
    }

    let tx = conn.unchecked_transaction()?;
    let mut inserted = 0usize;

    {
        let mut stmt = tx.prepare_cached(
            "INSERT OR IGNORE INTO emails (
                id, account_id, thread_id,
                from_name, from_address, to_json, cc_json,
                subject, snippet, date, maildir_path,
                flags, size_bytes, imap_uid, imap_folder,
                has_attachments, message_id_header, in_reply_to, references_json
            ) VALUES (
                ?1, ?2, NULL,
                ?3, ?4, ?5, ?6,
                ?7, '', ?8, '',
                ?9, ?10, ?11, ?12,
                0, ?13, ?14, ?15
            )",
        )?;

        for env in envelopes {
            let changes = stmt.execute(params![
                env.message_id,       // id (Message-ID as primary key)
                env.account_id,
                env.from_name,
                env.from_address,
                env.to_json,
                env.cc_json,
                env.subject,
                env.date_unix,
                env.flags,
                env.size_bytes,
                env.imap_uid,
                env.imap_folder,
                env.message_id,       // message_id_header (same as id)
                env.in_reply_to,
                env.references_json,
            ])?;
            inserted += changes;
        }
    }

    tx.commit()?;
    Ok(inserted)
}

/// Count emails in a specific folder for an account.
pub fn count_emails_in_folder(
    conn: &Connection,
    account_id: &str,
    folder: &str,
) -> SyncResult<u32> {
    let count: u32 = conn.query_row(
        "SELECT COUNT(*) FROM emails WHERE account_id = ?1 AND imap_folder = ?2",
        params![account_id, folder],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Get the lowest synced UID for a folder, used for resume calculation.
pub fn lowest_synced_uid(
    conn: &Connection,
    account_id: &str,
    folder: &str,
) -> SyncResult<Option<u32>> {
    let result: Option<u32> = conn.query_row(
        "SELECT MIN(imap_uid) FROM emails WHERE account_id = ?1 AND imap_folder = ?2",
        params![account_id, folder],
        |row| row.get(0),
    )?;
    Ok(result)
}

/// Get the highest synced UID for a folder.
pub fn highest_synced_uid(
    conn: &Connection,
    account_id: &str,
    folder: &str,
) -> SyncResult<Option<u32>> {
    let result: Option<u32> = conn.query_row(
        "SELECT MAX(imap_uid) FROM emails WHERE account_id = ?1 AND imap_folder = ?2",
        params![account_id, folder],
        |row| row.get(0),
    )?;
    Ok(result)
}
