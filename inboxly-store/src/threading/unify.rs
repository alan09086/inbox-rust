//! Thread unification: merge placeholder threads when root emails arrive.

use rusqlite::{Connection, params};

use crate::error::{Result, StoreError};

/// Called when an email is ingested that might resolve a placeholder thread.
///
/// If this email's Message-ID matches a placeholder's `root_message_id`,
/// assign the email to that placeholder thread and clear the placeholder marker.
///
/// Returns the resolved `ThreadId` if unification occurred, `None` otherwise.
///
/// # Errors
///
/// Returns `StoreError::Sqlite` on database errors.
pub fn try_unify_placeholder(
    conn: &Connection,
    email_message_id: &str,
    currently_assigned_thread_id: &str,
) -> Result<Option<String>> {
    // Find any placeholder thread whose root_message_id matches this email.
    let placeholder_tid: Option<String> = {
        let result = conn.query_row(
            "SELECT id FROM threads WHERE root_message_id = ?1 LIMIT 1",
            params![email_message_id],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(tid) => Some(tid),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(StoreError::Sqlite(e)),
        }
    };

    let Some(placeholder_tid) = placeholder_tid else {
        return Ok(None); // No placeholder to unify.
    };

    if placeholder_tid == currently_assigned_thread_id {
        // Email was already assigned to the placeholder thread (normal case).
        // Just clear the placeholder marker.
        conn.execute(
            "UPDATE threads SET root_message_id = NULL WHERE id = ?1",
            params![placeholder_tid],
        )?;
        return Ok(Some(placeholder_tid));
    }

    // Edge case: email was assigned to a different thread, but a placeholder
    // exists for its Message-ID. Merge the placeholder's emails into the
    // email's current thread, then delete the placeholder.
    merge_threads(conn, &placeholder_tid, currently_assigned_thread_id)?;
    Ok(Some(currently_assigned_thread_id.to_string()))
}

/// Merge all emails from `source_thread_id` into `target_thread_id`.
///
/// - Moves all emails from source to target.
/// - Deletes thread_state for the source (if any).
/// - Deletes the source thread row.
///
/// Returns the number of emails moved.
fn merge_threads(conn: &Connection, source_thread_id: &str, target_thread_id: &str) -> Result<u64> {
    // Move all emails from source to target.
    let moved = conn.execute(
        "UPDATE emails SET thread_id = ?1 WHERE thread_id = ?2",
        params![target_thread_id, source_thread_id],
    )?;

    // Delete thread_state for the source thread (FK constraint).
    conn.execute(
        "DELETE FROM thread_state WHERE thread_id = ?1",
        params![source_thread_id],
    )?;

    // Delete highlights for the source thread (FK constraint with ON DELETE CASCADE,
    // but be explicit for safety).
    conn.execute(
        "DELETE FROM highlights WHERE thread_id = ?1",
        params![source_thread_id],
    )?;

    // Delete the source thread row.
    conn.execute(
        "DELETE FROM threads WHERE id = ?1",
        params![source_thread_id],
    )?;

    Ok(moved as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable FK");
        conn.execute_batch(
            "CREATE TABLE accounts (
                id TEXT PRIMARY KEY NOT NULL,
                email TEXT NOT NULL,
                display_name TEXT NOT NULL,
                provider TEXT NOT NULL,
                auth_method TEXT NOT NULL,
                imap_host TEXT NOT NULL,
                imap_port INTEGER NOT NULL,
                smtp_host TEXT NOT NULL,
                smtp_port INTEGER NOT NULL
            );

            CREATE TABLE threads (
                id TEXT PRIMARY KEY NOT NULL,
                account_id TEXT NOT NULL REFERENCES accounts(id),
                subject TEXT NOT NULL DEFAULT '',
                newest_date INTEGER NOT NULL,
                oldest_date INTEGER NOT NULL,
                email_count INTEGER NOT NULL DEFAULT 0,
                unread_count INTEGER NOT NULL DEFAULT 0,
                has_attachments INTEGER NOT NULL DEFAULT 0,
                snippet TEXT NOT NULL DEFAULT '',
                root_message_id TEXT
            );

            CREATE TABLE bundles (
                id TEXT PRIMARY KEY NOT NULL,
                category TEXT NOT NULL,
                name TEXT NOT NULL,
                color TEXT NOT NULL DEFAULT '#000000',
                badge_color TEXT NOT NULL DEFAULT '#eeeeee',
                visibility TEXT NOT NULL DEFAULT 'Bundled',
                throttle TEXT NOT NULL DEFAULT 'Immediate',
                sort_order INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE thread_state (
                thread_id TEXT PRIMARY KEY NOT NULL REFERENCES threads(id),
                pinned INTEGER NOT NULL DEFAULT 0,
                done INTEGER NOT NULL DEFAULT 0,
                snoozed_until INTEGER,
                snoozed_location_json TEXT,
                bundle_id TEXT REFERENCES bundles(id)
            );

            CREATE TABLE highlights (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
                highlight_type TEXT NOT NULL,
                data_json TEXT NOT NULL
            );

            CREATE TABLE emails (
                id TEXT PRIMARY KEY NOT NULL,
                account_id TEXT NOT NULL REFERENCES accounts(id),
                thread_id TEXT NOT NULL,
                from_name TEXT,
                from_address TEXT NOT NULL,
                to_json TEXT NOT NULL DEFAULT '[]',
                cc_json TEXT NOT NULL DEFAULT '[]',
                subject TEXT NOT NULL DEFAULT '',
                snippet TEXT NOT NULL DEFAULT '',
                date INTEGER NOT NULL,
                maildir_path TEXT NOT NULL,
                flags INTEGER NOT NULL DEFAULT 0,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                imap_uid INTEGER NOT NULL DEFAULT 0,
                imap_folder TEXT NOT NULL DEFAULT 'INBOX',
                has_attachments INTEGER NOT NULL DEFAULT 0,
                body_downloaded INTEGER NOT NULL DEFAULT 0,
                message_id_header TEXT,
                in_reply_to TEXT,
                references_json TEXT,
                UNIQUE(account_id, imap_folder, imap_uid)
            );

            INSERT INTO accounts VALUES (
                'acct-1', 'test@example.com', 'Test', 'generic',
                'password', 'imap.example.com', 993, 'smtp.example.com', 587
            );",
        )
        .expect("create schema");
        conn
    }

    fn insert_email(
        conn: &Connection,
        id: &str,
        thread_id: &str,
        message_id: Option<&str>,
        uid: i64,
    ) {
        conn.execute(
            "INSERT INTO emails (id, account_id, thread_id, from_address, subject, date,
             maildir_path, imap_uid, imap_folder, message_id_header)
             VALUES (?1, 'acct-1', ?2, 'test@example.com', 'test', 1710000000, '', ?3, 'INBOX', ?4)",
            params![id, thread_id, uid, message_id],
        )
        .expect("insert email");
    }

    #[test]
    fn root_arrives_placeholder_exists() {
        let conn = test_db();

        // Create placeholder thread awaiting "root@ex.com".
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, root_message_id)
             VALUES ('ph-thread', 'acct-1', 'Placeholder', 1710000000, 1710000000, 'root@ex.com')",
            [],
        )
        .unwrap();

        // Insert orphaned reply in the placeholder thread.
        insert_email(&conn, "reply1", "ph-thread", Some("reply1@ex.com"), 1);

        // Root email arrives, already assigned to the placeholder thread.
        let result = try_unify_placeholder(&conn, "root@ex.com", "ph-thread").unwrap();
        assert_eq!(result.as_deref(), Some("ph-thread"));

        // Verify placeholder marker is cleared.
        let root_mid: Option<String> = conn
            .query_row(
                "SELECT root_message_id FROM threads WHERE id = 'ph-thread'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(root_mid.is_none());
    }

    #[test]
    fn root_arrives_no_placeholder() {
        let conn = test_db();

        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date)
             VALUES ('normal', 'acct-1', 'Normal', 1710000000, 1710000000)",
            [],
        )
        .unwrap();

        let result = try_unify_placeholder(&conn, "no-placeholder@ex.com", "normal").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn merge_scenario() {
        let conn = test_db();

        // Create placeholder thread with 2 orphaned replies.
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, root_message_id)
             VALUES ('ph-thread', 'acct-1', 'Placeholder', 1710000000, 1710000000, 'root@ex.com')",
            [],
        )
        .unwrap();
        insert_email(&conn, "orphan1", "ph-thread", Some("orphan1@ex.com"), 1);
        insert_email(&conn, "orphan2", "ph-thread", Some("orphan2@ex.com"), 2);

        // Root email was assigned to a different (standalone) thread.
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date)
             VALUES ('root-thread', 'acct-1', 'Root', 1710000000, 1710000000)",
            [],
        )
        .unwrap();
        insert_email(&conn, "root-email", "root-thread", Some("root@ex.com"), 3);

        // Unify: placeholder should merge into root-thread.
        let result = try_unify_placeholder(&conn, "root@ex.com", "root-thread").unwrap();
        assert_eq!(result.as_deref(), Some("root-thread"));

        // Verify orphans moved to root-thread.
        let orphan1_tid: String = conn
            .query_row(
                "SELECT thread_id FROM emails WHERE id = 'orphan1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphan1_tid, "root-thread");

        let orphan2_tid: String = conn
            .query_row(
                "SELECT thread_id FROM emails WHERE id = 'orphan2'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphan2_tid, "root-thread");

        // Verify placeholder thread is deleted.
        let ph_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM threads WHERE id = 'ph-thread')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!ph_exists);
    }

    #[test]
    fn unification_clears_placeholder_marker() {
        let conn = test_db();

        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, root_message_id)
             VALUES ('ph', 'acct-1', 'PH', 1710000000, 1710000000, 'root@ex.com')",
            [],
        )
        .unwrap();

        try_unify_placeholder(&conn, "root@ex.com", "ph").unwrap();

        // After unification, is_placeholder should be false.
        let root_mid: Option<String> = conn
            .query_row(
                "SELECT root_message_id FROM threads WHERE id = 'ph'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(root_mid.is_none());
    }
}
