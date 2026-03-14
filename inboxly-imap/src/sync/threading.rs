use super::error::SyncResult;
use rusqlite::{Connection, params};
use uuid::Uuid;

/// Assign `thread_id` to all emails that have `thread_id IS NULL`.
///
/// Basic algorithm (Phase 1 — full threading is M10):
/// 1. For each un-threaded email, check `in_reply_to`.
/// 2. If `in_reply_to` points to a message already in `emails` that HAS a thread_id,
///    join that thread.
/// 3. Otherwise, create a new thread.
///
/// Returns the number of emails that were assigned a thread_id.
pub fn assign_thread_ids(conn: &Connection, account_id: &str) -> SyncResult<u32> {
    // Fetch all un-threaded emails for this account, ordered by date ascending
    // so parents are processed before replies when possible.
    let mut select_stmt = conn.prepare(
        "SELECT id, in_reply_to, subject, date
         FROM emails
         WHERE account_id = ?1 AND thread_id IS NULL
         ORDER BY date ASC",
    )?;

    let unthreaded: Vec<(String, Option<String>, String, i64)> = select_stmt
        .query_map(params![account_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    if unthreaded.is_empty() {
        return Ok(0);
    }

    let tx = conn.unchecked_transaction()?;
    let mut assigned = 0u32;

    for (msg_id, in_reply_to, subject, date) in &unthreaded {
        let thread_id = if let Some(parent_msg_id) = in_reply_to {
            // Try to find parent's thread_id
            let parent_tid: Option<String> = tx
                .query_row(
                    "SELECT thread_id FROM emails WHERE message_id_header = ?1 AND thread_id IS NOT NULL",
                    params![parent_msg_id],
                    |row| row.get(0),
                )
                .ok();

            parent_tid.unwrap_or_else(|| Uuid::new_v4().to_string())
        } else {
            Uuid::new_v4().to_string()
        };

        // Update the email row
        tx.execute(
            "UPDATE emails SET thread_id = ?1 WHERE id = ?2",
            params![thread_id, msg_id],
        )?;

        // Upsert threads table row
        tx.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, email_count, snippet)
             VALUES (?1, ?2, ?3, ?4, ?4, 1, '')
             ON CONFLICT(id) DO UPDATE SET
                newest_date = MAX(threads.newest_date, excluded.newest_date),
                oldest_date = MIN(threads.oldest_date, excluded.oldest_date),
                email_count = email_count + 1",
            params![thread_id, account_id, subject, date],
        )?;

        assigned += 1;
    }

    tx.commit()?;
    Ok(assigned)
}
