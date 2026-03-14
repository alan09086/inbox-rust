//! Rebuild all threads from scratch.
//!
//! Nuclear option: wipes all thread assignments and rebuilds them using
//! the current threading algorithm. Used when the algorithm is updated
//! or data integrity is suspect.

use rusqlite::{params, Connection};

use crate::error::Result;
use super::batch::thread_unthreaded_emails;

/// Rebuild all threads for an account from scratch.
///
/// 1. Deletes all `thread_state` rows for the account's threads.
/// 2. Deletes all `highlights` for the account's threads.
/// 3. Clears all `emails.thread_id` to empty string for the account.
/// 4. Deletes all thread rows for the account.
/// 5. Runs `thread_unthreaded_emails` to reassign everything.
///
/// **Destructive**: pins, snooze, and bundle assignments keyed on old
/// thread IDs will be lost. Callers should warn the user.
///
/// Returns the number of emails re-threaded.
///
/// # Errors
///
/// Returns `StoreError::Sqlite` on database errors.
pub fn rebuild_threads(conn: &Connection, account_id: &str) -> Result<u64> {
    // 1. Delete thread_state rows (FK references threads.id).
    conn.execute(
        "DELETE FROM thread_state WHERE thread_id IN (
            SELECT id FROM threads WHERE account_id = ?1
        )",
        params![account_id],
    )?;

    // 2. Delete highlights for these threads (FK with ON DELETE CASCADE,
    //    but be explicit since we're deleting threads manually).
    conn.execute(
        "DELETE FROM highlights WHERE thread_id IN (
            SELECT id FROM threads WHERE account_id = ?1
        )",
        params![account_id],
    )?;

    // 3. Clear thread assignments on all emails for this account.
    conn.execute(
        "UPDATE emails SET thread_id = '' WHERE account_id = ?1",
        params![account_id],
    )?;

    // 4. Delete all thread rows for this account.
    conn.execute(
        "DELETE FROM threads WHERE account_id = ?1",
        params![account_id],
    )?;

    // 5. Rebuild using the batch threading algorithm.
    thread_unthreaded_emails(conn, account_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch("PRAGMA foreign_keys = ON;").expect("enable FK");
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
            CREATE INDEX idx_threads_root_message_id ON threads(root_message_id);

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
            CREATE INDEX idx_emails_thread_id ON emails(thread_id);

            INSERT INTO accounts VALUES (
                'acct-1', 'test@example.com', 'Test', 'generic',
                'password', 'imap.example.com', 993, 'smtp.example.com', 587
            );",
        )
        .expect("create schema");
        conn
    }

    fn insert_email_with_thread(
        conn: &Connection,
        id: &str,
        thread_id: &str,
        message_id: &str,
        in_reply_to: Option<&str>,
        references: &[&str],
        subject: &str,
        date: i64,
        uid: i64,
    ) {
        let refs_json = if references.is_empty() {
            None
        } else {
            Some(serde_json::to_string(references).expect("serialize refs"))
        };

        conn.execute(
            "INSERT INTO emails (id, account_id, thread_id, from_address, subject, snippet, date,
             maildir_path, imap_uid, imap_folder, message_id_header, in_reply_to, references_json)
             VALUES (?1, 'acct-1', ?2, 'test@example.com', ?3, ?4, ?5, '', ?6, 'INBOX', ?7, ?8, ?9)",
            params![id, thread_id, subject, subject, date, uid, message_id, in_reply_to, refs_json],
        )
        .expect("insert email");
    }

    fn setup_threaded_data(conn: &Connection) {
        // Thread 1: root + reply (thread ID "t1").
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, email_count)
             VALUES ('t1', 'acct-1', 'Thread 1', 1710001000, 1710000000, 2)",
            [],
        )
        .unwrap();
        insert_email_with_thread(conn, "e1", "t1", "root1@ex.com", None, &[], "Thread 1", 1710000000, 1);
        insert_email_with_thread(conn, "e2", "t1", "reply1@ex.com", Some("root1@ex.com"), &["root1@ex.com"], "Re: Thread 1", 1710001000, 2);

        // Thread 2: standalone (thread ID "t2").
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, email_count)
             VALUES ('t2', 'acct-1', 'Thread 2', 1710002000, 1710002000, 1)",
            [],
        )
        .unwrap();
        insert_email_with_thread(conn, "e3", "t2", "standalone@ex.com", None, &[], "Thread 2", 1710002000, 3);

        // Add thread_state for t1.
        conn.execute(
            "INSERT INTO thread_state (thread_id, pinned, done) VALUES ('t1', 1, 0)",
            [],
        )
        .unwrap();
    }

    #[test]
    fn rebuild_preserves_thread_membership() {
        let conn = test_db();
        setup_threaded_data(&conn);

        // Record which emails are grouped together (before rebuild).
        let pre_t1: String = conn.query_row("SELECT thread_id FROM emails WHERE id = 'e1'", [], |row| row.get(0)).unwrap();
        let pre_t2: String = conn.query_row("SELECT thread_id FROM emails WHERE id = 'e2'", [], |row| row.get(0)).unwrap();
        assert_eq!(pre_t1, pre_t2); // e1 and e2 were in same thread.

        let count = rebuild_threads(&conn, "acct-1").unwrap();
        assert_eq!(count, 3);

        // After rebuild, e1 and e2 should still be in the same thread (different IDs).
        let post_t1: String = conn.query_row("SELECT thread_id FROM emails WHERE id = 'e1'", [], |row| row.get(0)).unwrap();
        let post_t2: String = conn.query_row("SELECT thread_id FROM emails WHERE id = 'e2'", [], |row| row.get(0)).unwrap();
        assert_eq!(post_t1, post_t2);

        // e3 should be in a different thread.
        let post_t3: String = conn.query_row("SELECT thread_id FROM emails WHERE id = 'e3'", [], |row| row.get(0)).unwrap();
        assert_ne!(post_t1, post_t3);
    }

    #[test]
    fn rebuild_clears_thread_state() {
        let conn = test_db();
        setup_threaded_data(&conn);

        // Verify thread_state exists before rebuild.
        let count_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM thread_state", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count_before, 1);

        rebuild_threads(&conn, "acct-1").unwrap();

        // thread_state should be empty after rebuild.
        let count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM thread_state", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count_after, 0);
    }

    #[test]
    fn rebuild_correct_metadata() {
        let conn = test_db();
        setup_threaded_data(&conn);

        rebuild_threads(&conn, "acct-1").unwrap();

        // The thread containing e1+e2 should have email_count=2.
        let t1: String = conn.query_row("SELECT thread_id FROM emails WHERE id = 'e1'", [], |row| row.get(0)).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT email_count FROM threads WHERE id = ?1",
                params![t1],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn rebuild_empty_account() {
        let conn = test_db();
        let count = rebuild_threads(&conn, "acct-1").unwrap();
        assert_eq!(count, 0);
    }
}
