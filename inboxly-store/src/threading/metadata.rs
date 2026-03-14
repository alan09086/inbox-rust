//! Thread metadata aggregation.
//!
//! Recalculates thread aggregate fields (subject, dates, counts, snippet)
//! from the emails in each thread. Called after thread membership changes.

use rusqlite::{Connection, params};

use crate::error::Result;

/// Contact information for thread participants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipantContact {
    /// Display name (may be empty).
    pub name: String,
    /// Email address.
    pub address: String,
}

/// Recalculate and update the metadata for a single thread.
///
/// Updates: subject, newest_date, oldest_date, email_count, unread_count,
/// has_attachments, snippet.
///
/// - Subject = subject of the oldest email in the thread.
/// - Snippet = snippet of the newest email in the thread.
/// - newest_date = MAX(date) of emails in thread.
/// - oldest_date = MIN(date) of emails in thread.
/// - email_count = COUNT of emails in thread.
/// - unread_count = COUNT of emails where (flags & 1) = 0 (read flag not set).
/// - has_attachments = MAX(has_attachments) of emails in thread.
///
/// If the thread has no emails, sets counts to 0 and dates to 0.
///
/// # Errors
///
/// Returns `StoreError::Sqlite` on database errors.
pub fn refresh_thread_metadata(conn: &Connection, thread_id: &str) -> Result<()> {
    // Check if the thread has any emails.
    let email_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM emails WHERE thread_id = ?1",
        params![thread_id],
        |row| row.get(0),
    )?;

    if email_count == 0 {
        // Thread has no emails — set everything to defaults.
        conn.execute(
            "UPDATE threads SET subject = '', newest_date = 0, oldest_date = 0,
             email_count = 0, unread_count = 0, has_attachments = 0, snippet = ''
             WHERE id = ?1",
            params![thread_id],
        )?;
        return Ok(());
    }

    // Use individual subqueries for maximum SQLite compatibility.
    conn.execute(
        "UPDATE threads SET
            subject = (SELECT subject FROM emails WHERE thread_id = ?1 ORDER BY date ASC LIMIT 1),
            newest_date = (SELECT MAX(date) FROM emails WHERE thread_id = ?1),
            oldest_date = (SELECT MIN(date) FROM emails WHERE thread_id = ?1),
            email_count = (SELECT COUNT(*) FROM emails WHERE thread_id = ?1),
            unread_count = (SELECT COUNT(*) FROM emails WHERE thread_id = ?1 AND (flags & 1) = 0),
            has_attachments = COALESCE((SELECT MAX(has_attachments) FROM emails WHERE thread_id = ?1), 0),
            snippet = (SELECT snippet FROM emails WHERE thread_id = ?1 ORDER BY date DESC LIMIT 1)
         WHERE id = ?1",
        params![thread_id],
    )?;

    Ok(())
}

/// Recalculate metadata for all threads in a single account.
///
/// Uses per-thread subqueries for each field. While less elegant than a
/// single aggregate CTE, this is maximally compatible with older SQLite.
///
/// Returns the number of threads updated.
///
/// # Errors
///
/// Returns `StoreError::Sqlite` on database errors.
pub fn refresh_all_thread_metadata(conn: &Connection, account_id: &str) -> Result<u64> {
    // Get all thread IDs for the account.
    let mut stmt = conn.prepare("SELECT id FROM threads WHERE account_id = ?1")?;
    let thread_ids: Vec<String> = stmt
        .query_map(params![account_id], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    for tid in &thread_ids {
        refresh_thread_metadata(conn, tid)?;
    }

    Ok(thread_ids.len() as u64)
}

/// Get aggregated thread participants (all unique `from` addresses).
///
/// Returns contacts in date order (oldest first), deduplicated by address.
///
/// # Errors
///
/// Returns `StoreError::Sqlite` on database errors.
pub fn get_thread_participants(
    conn: &Connection,
    thread_id: &str,
) -> Result<Vec<ParticipantContact>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT from_name, from_address FROM emails
         WHERE thread_id = ?1
         ORDER BY date ASC",
    )?;

    let mut seen = std::collections::HashSet::new();
    let mut contacts = Vec::new();

    let rows = stmt.query_map(params![thread_id], |row| {
        Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?))
    })?;

    for row in rows {
        let (name, address) = row?;
        if seen.insert(address.clone()) {
            contacts.push(ParticipantContact {
                name: name.unwrap_or_default(),
                address,
            });
        }
    }

    Ok(contacts)
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

    fn insert_thread(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date)
             VALUES (?1, 'acct-1', '', 0, 0)",
            params![id],
        )
        .expect("insert thread");
    }

    fn insert_email_full(
        conn: &Connection,
        id: &str,
        thread_id: &str,
        from_name: &str,
        from_address: &str,
        subject: &str,
        snippet: &str,
        date: i64,
        flags: i64,
        has_attachments: bool,
        uid: i64,
    ) {
        conn.execute(
            "INSERT INTO emails (id, account_id, thread_id, from_name, from_address,
             subject, snippet, date, maildir_path, flags, imap_uid, imap_folder, has_attachments)
             VALUES (?1, 'acct-1', ?2, ?3, ?4, ?5, ?6, ?7, '', ?8, ?9, 'INBOX', ?10)",
            params![
                id,
                thread_id,
                from_name,
                from_address,
                subject,
                snippet,
                date,
                flags,
                uid,
                has_attachments
            ],
        )
        .expect("insert email");
    }

    #[test]
    fn single_email_thread() {
        let conn = test_db();
        insert_thread(&conn, "t1");
        insert_email_full(
            &conn,
            "e1",
            "t1",
            "Alice",
            "alice@ex.com",
            "Hello",
            "Hi there",
            1710000000,
            0,
            false,
            1,
        );

        refresh_thread_metadata(&conn, "t1").unwrap();

        let row = conn
            .query_row("SELECT subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet FROM threads WHERE id = 't1'", [], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .unwrap();

        assert_eq!(row.0, "Hello"); // subject
        assert_eq!(row.1, 1710000000); // newest_date
        assert_eq!(row.2, 1710000000); // oldest_date
        assert_eq!(row.3, 1); // email_count
        assert_eq!(row.4, 1); // unread_count (flags=0 means not read)
        assert!(!row.5); // has_attachments
        assert_eq!(row.6, "Hi there"); // snippet
    }

    #[test]
    fn three_email_thread() {
        let conn = test_db();
        insert_thread(&conn, "t1");
        insert_email_full(
            &conn,
            "e1",
            "t1",
            "Alice",
            "alice@ex.com",
            "Original",
            "First message",
            1710000000,
            0,
            false,
            1,
        );
        insert_email_full(
            &conn,
            "e2",
            "t1",
            "Bob",
            "bob@ex.com",
            "Re: Original",
            "Reply 1",
            1710001000,
            0,
            false,
            2,
        );
        insert_email_full(
            &conn,
            "e3",
            "t1",
            "Alice",
            "alice@ex.com",
            "Re: Original",
            "Reply 2",
            1710002000,
            0,
            true,
            3,
        );

        refresh_thread_metadata(&conn, "t1").unwrap();

        let row = conn
            .query_row("SELECT subject, newest_date, oldest_date, email_count, has_attachments, snippet FROM threads WHERE id = 't1'", [], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, bool>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .unwrap();

        assert_eq!(row.0, "Original"); // subject from oldest
        assert_eq!(row.1, 1710002000); // newest_date
        assert_eq!(row.2, 1710000000); // oldest_date
        assert_eq!(row.3, 3); // email_count
        assert!(row.4); // has_attachments (one email has it)
        assert_eq!(row.5, "Reply 2"); // snippet from newest
    }

    #[test]
    fn read_unread_mix() {
        let conn = test_db();
        insert_thread(&conn, "t1");
        // flags=1 means READ
        insert_email_full(
            &conn, "e1", "t1", "A", "a@ex.com", "S", "", 1710000000, 1, false, 1,
        );
        insert_email_full(
            &conn, "e2", "t1", "B", "b@ex.com", "S", "", 1710001000, 0, false, 2,
        );
        insert_email_full(
            &conn, "e3", "t1", "C", "c@ex.com", "S", "", 1710002000, 0, false, 3,
        );

        refresh_thread_metadata(&conn, "t1").unwrap();

        let unread: i64 = conn
            .query_row(
                "SELECT unread_count FROM threads WHERE id = 't1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unread, 2);
    }

    #[test]
    fn empty_thread() {
        let conn = test_db();
        insert_thread(&conn, "t1");

        refresh_thread_metadata(&conn, "t1").unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT email_count FROM threads WHERE id = 't1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn refresh_all_updates_all_threads() {
        let conn = test_db();
        insert_thread(&conn, "t1");
        insert_thread(&conn, "t2");
        insert_email_full(
            &conn,
            "e1",
            "t1",
            "A",
            "a@ex.com",
            "Thread 1",
            "Snippet 1",
            1710000000,
            0,
            false,
            1,
        );
        insert_email_full(
            &conn,
            "e2",
            "t2",
            "B",
            "b@ex.com",
            "Thread 2",
            "Snippet 2",
            1710001000,
            0,
            false,
            2,
        );

        let updated = refresh_all_thread_metadata(&conn, "acct-1").unwrap();
        assert_eq!(updated, 2);

        let s1: String = conn
            .query_row("SELECT subject FROM threads WHERE id = 't1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(s1, "Thread 1");

        let s2: String = conn
            .query_row("SELECT subject FROM threads WHERE id = 't2'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(s2, "Thread 2");
    }

    #[test]
    fn participants_unique_in_date_order() {
        let conn = test_db();
        insert_thread(&conn, "t1");
        insert_email_full(
            &conn,
            "e1",
            "t1",
            "Alice",
            "alice@ex.com",
            "S",
            "",
            1710000000,
            0,
            false,
            1,
        );
        insert_email_full(
            &conn,
            "e2",
            "t1",
            "Bob",
            "bob@ex.com",
            "S",
            "",
            1710001000,
            0,
            false,
            2,
        );
        insert_email_full(
            &conn,
            "e3",
            "t1",
            "Alice",
            "alice@ex.com",
            "S",
            "",
            1710002000,
            0,
            false,
            3,
        );

        let participants = get_thread_participants(&conn, "t1").unwrap();
        assert_eq!(participants.len(), 2);
        assert_eq!(participants[0].name, "Alice");
        assert_eq!(participants[0].address, "alice@ex.com");
        assert_eq!(participants[1].name, "Bob");
        assert_eq!(participants[1].address, "bob@ex.com");
    }
}
