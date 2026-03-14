//! Batch threading: process all unthreaded emails or a specific batch.
//!
//! Used during initial sync (M7 populates emails without threading) and
//! for catch-up after re-threading.

use rusqlite::{Connection, params};

use super::assign::assign_thread;
use super::headers::threading_headers_from_fields;
use super::metadata::refresh_all_thread_metadata;
use super::unify::try_unify_placeholder;
use crate::error::Result;

/// Maximum number of emails to process per transaction to avoid holding
/// the write lock too long on very large mailboxes.
const BATCH_SIZE: usize = 5000;

/// Thread all emails in the given account that have `thread_id` as empty
/// string (the schema defines `thread_id TEXT NOT NULL`, so unthreaded
/// emails use empty string rather than NULL).
///
/// Processes emails in date-ascending order (oldest first) so that parent
/// emails are threaded before their replies, minimizing placeholder creation.
///
/// Returns the number of emails threaded.
///
/// # Errors
///
/// Returns `StoreError::Sqlite` on database errors.
pub fn thread_unthreaded_emails(conn: &Connection, account_id: &str) -> Result<u64> {
    let mut total_threaded: u64 = 0;

    loop {
        // Fetch a batch of unthreaded emails, oldest first.
        let batch = fetch_unthreaded_batch(conn, account_id, BATCH_SIZE)?;
        if batch.is_empty() {
            break;
        }

        let batch_len = batch.len() as u64;
        process_email_batch(conn, account_id, &batch)?;
        total_threaded = total_threaded.saturating_add(batch_len);

        // If we got fewer than BATCH_SIZE, we're done.
        if (batch_len as usize) < BATCH_SIZE {
            break;
        }
    }

    // After all assignments, refresh metadata for all threads.
    refresh_all_thread_metadata(conn, account_id)?;

    Ok(total_threaded)
}

/// Thread a specific batch of emails by their IDs.
///
/// Used for incremental threading (e.g., after a sync batch).
/// Only threads emails that actually belong to the given account.
///
/// Returns the number of emails threaded.
///
/// # Errors
///
/// Returns `StoreError::Sqlite` on database errors.
pub fn thread_email_batch(conn: &Connection, account_id: &str, email_ids: &[&str]) -> Result<u64> {
    let mut count: u64 = 0;

    for &email_id in email_ids {
        let email = fetch_email_for_threading(conn, email_id)?;
        let Some(email) = email else {
            continue; // Email not found or wrong account — skip.
        };

        if email.account_id != account_id {
            continue;
        }

        let headers = threading_headers_from_fields(
            email.message_id_header.as_deref(),
            email.in_reply_to.as_deref(),
            email.references_json.as_deref(),
        );

        let assignment = assign_thread(conn, account_id, &headers, &email.subject, email.date)?;

        conn.execute(
            "UPDATE emails SET thread_id = ?1 WHERE id = ?2",
            params![assignment.thread_id, email_id],
        )?;

        // Check if this email resolves a placeholder.
        if let Some(mid) = &headers.message_id
            && let Some(resolved_tid) = try_unify_placeholder(conn, mid, &assignment.thread_id)?
            && resolved_tid != assignment.thread_id
        {
            conn.execute(
                "UPDATE emails SET thread_id = ?1 WHERE id = ?2",
                params![resolved_tid, email_id],
            )?;
        }

        count = count.saturating_add(1);
    }

    // Refresh metadata for all threads in account.
    refresh_all_thread_metadata(conn, account_id)?;

    Ok(count)
}

/// Minimal email data needed for thread assignment.
struct EmailForThreading {
    id: String,
    account_id: String,
    subject: String,
    date: i64,
    message_id_header: Option<String>,
    in_reply_to: Option<String>,
    references_json: Option<String>,
}

/// Fetch a batch of unthreaded emails (thread_id is empty string), oldest first.
fn fetch_unthreaded_batch(
    conn: &Connection,
    account_id: &str,
    limit: usize,
) -> Result<Vec<EmailForThreading>> {
    let mut stmt = conn.prepare(
        "SELECT id, account_id, subject, date, message_id_header, in_reply_to, references_json
         FROM emails
         WHERE account_id = ?1 AND thread_id = ''
         ORDER BY date ASC
         LIMIT ?2",
    )?;

    let rows = stmt
        .query_map(params![account_id, limit as i64], |row| {
            Ok(EmailForThreading {
                id: row.get(0)?,
                account_id: row.get(1)?,
                subject: row.get(2)?,
                date: row.get(3)?,
                message_id_header: row.get(4)?,
                in_reply_to: row.get(5)?,
                references_json: row.get(6)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(rows)
}

/// Fetch a single email's threading-relevant fields by ID.
fn fetch_email_for_threading(
    conn: &Connection,
    email_id: &str,
) -> Result<Option<EmailForThreading>> {
    let result = conn.query_row(
        "SELECT id, account_id, subject, date, message_id_header, in_reply_to, references_json
         FROM emails WHERE id = ?1",
        params![email_id],
        |row| {
            Ok(EmailForThreading {
                id: row.get(0)?,
                account_id: row.get(1)?,
                subject: row.get(2)?,
                date: row.get(3)?,
                message_id_header: row.get(4)?,
                in_reply_to: row.get(5)?,
                references_json: row.get(6)?,
            })
        },
    );

    match result {
        Ok(email) => Ok(Some(email)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(crate::error::StoreError::Sqlite(e)),
    }
}

/// Process a batch of emails: assign threads and handle unification.
fn process_email_batch(
    conn: &Connection,
    account_id: &str,
    batch: &[EmailForThreading],
) -> Result<()> {
    for email in batch {
        let headers = threading_headers_from_fields(
            email.message_id_header.as_deref(),
            email.in_reply_to.as_deref(),
            email.references_json.as_deref(),
        );

        let assignment = assign_thread(conn, account_id, &headers, &email.subject, email.date)?;

        conn.execute(
            "UPDATE emails SET thread_id = ?1 WHERE id = ?2",
            params![assignment.thread_id, email.id],
        )?;

        // Check if this email resolves a placeholder.
        if let Some(mid) = &headers.message_id
            && let Some(resolved_tid) = try_unify_placeholder(conn, mid, &assignment.thread_id)?
            && resolved_tid != assignment.thread_id
        {
            conn.execute(
                "UPDATE emails SET thread_id = ?1 WHERE id = ?2",
                params![resolved_tid, email.id],
            )?;
        }
    }

    Ok(())
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
            CREATE INDEX idx_threads_root_message_id ON threads(root_message_id);

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

    fn insert_unthreaded_email(
        conn: &Connection,
        id: &str,
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
             VALUES (?1, 'acct-1', '', 'test@example.com', ?2, ?3, ?4, '', ?5, 'INBOX', ?6, ?7, ?8)",
            params![id, subject, subject, date, uid, message_id, in_reply_to, refs_json],
        )
        .expect("insert email");
    }

    #[test]
    fn batch_three_threads() {
        let conn = test_db();

        // Thread 1: root + reply.
        insert_unthreaded_email(
            &conn,
            "e1",
            "root1@ex.com",
            None,
            &[],
            "Thread 1",
            1710000000,
            1,
        );
        insert_unthreaded_email(
            &conn,
            "e2",
            "reply1@ex.com",
            Some("root1@ex.com"),
            &["root1@ex.com"],
            "Re: Thread 1",
            1710001000,
            2,
        );

        // Thread 2: standalone.
        insert_unthreaded_email(
            &conn,
            "e3",
            "standalone@ex.com",
            None,
            &[],
            "Thread 2",
            1710002000,
            3,
        );

        // Thread 3: two replies to same root.
        insert_unthreaded_email(
            &conn,
            "e4",
            "root2@ex.com",
            None,
            &[],
            "Thread 3",
            1710003000,
            4,
        );
        insert_unthreaded_email(
            &conn,
            "e5",
            "reply2a@ex.com",
            Some("root2@ex.com"),
            &["root2@ex.com"],
            "Re: Thread 3",
            1710004000,
            5,
        );
        insert_unthreaded_email(
            &conn,
            "e6",
            "reply2b@ex.com",
            Some("root2@ex.com"),
            &["root2@ex.com"],
            "Re: Thread 3",
            1710005000,
            6,
        );

        let count = thread_unthreaded_emails(&conn, "acct-1").unwrap();
        assert_eq!(count, 6);

        // Verify 3 threads were created.
        let thread_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM threads WHERE account_id = 'acct-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(thread_count, 3);

        // Verify e1 and e2 share a thread.
        let t1: String = conn
            .query_row("SELECT thread_id FROM emails WHERE id = 'e1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        let t2: String = conn
            .query_row("SELECT thread_id FROM emails WHERE id = 'e2'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(t1, t2);

        // Verify e3 is standalone.
        let t3: String = conn
            .query_row("SELECT thread_id FROM emails WHERE id = 'e3'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_ne!(t3, t1);

        // Verify e4, e5, e6 share a thread.
        let t4: String = conn
            .query_row("SELECT thread_id FROM emails WHERE id = 'e4'", [], |row| {
                row.get(0)
            })
            .unwrap();
        let t5: String = conn
            .query_row("SELECT thread_id FROM emails WHERE id = 'e5'", [], |row| {
                row.get(0)
            })
            .unwrap();
        let t6: String = conn
            .query_row("SELECT thread_id FROM emails WHERE id = 'e6'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(t4, t5);
        assert_eq!(t5, t6);
    }

    #[test]
    fn oldest_first_minimizes_placeholders() {
        let conn = test_db();

        // Insert root first (oldest date), then reply.
        insert_unthreaded_email(
            &conn,
            "e1",
            "root@ex.com",
            None,
            &[],
            "Original",
            1710000000,
            1,
        );
        insert_unthreaded_email(
            &conn,
            "e2",
            "reply@ex.com",
            Some("root@ex.com"),
            &["root@ex.com"],
            "Re: Original",
            1710001000,
            2,
        );

        thread_unthreaded_emails(&conn, "acct-1").unwrap();

        // Should be zero placeholder threads (root processed before reply).
        let placeholder_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM threads WHERE root_message_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(placeholder_count, 0);
    }

    #[test]
    fn idempotent_batch() {
        let conn = test_db();

        insert_unthreaded_email(&conn, "e1", "msg@ex.com", None, &[], "Test", 1710000000, 1);

        let count1 = thread_unthreaded_emails(&conn, "acct-1").unwrap();
        assert_eq!(count1, 1);

        // Running again should find no unthreaded emails.
        let count2 = thread_unthreaded_emails(&conn, "acct-1").unwrap();
        assert_eq!(count2, 0);
    }

    #[test]
    fn thread_email_batch_specific_ids() {
        let conn = test_db();

        insert_unthreaded_email(&conn, "e1", "msg1@ex.com", None, &[], "T1", 1710000000, 1);
        insert_unthreaded_email(&conn, "e2", "msg2@ex.com", None, &[], "T2", 1710001000, 2);
        insert_unthreaded_email(&conn, "e3", "msg3@ex.com", None, &[], "T3", 1710002000, 3);

        // Only thread e1 and e3.
        let count = thread_email_batch(&conn, "acct-1", &["e1", "e3"]).unwrap();
        assert_eq!(count, 2);

        // e2 should still be unthreaded.
        let e2_tid: String = conn
            .query_row("SELECT thread_id FROM emails WHERE id = 'e2'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(e2_tid, ""); // Still unthreaded.

        // e1 and e3 should have thread_ids.
        let e1_tid: String = conn
            .query_row("SELECT thread_id FROM emails WHERE id = 'e1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(!e1_tid.is_empty());
    }

    #[test]
    fn large_batch_completes() {
        let conn = test_db();

        // Insert 200 standalone emails.
        for i in 0..200 {
            insert_unthreaded_email(
                &conn,
                &format!("e{i}"),
                &format!("msg{i}@ex.com"),
                None,
                &[],
                &format!("Subject {i}"),
                1710000000 + i64::from(i),
                i64::from(i) + 1,
            );
        }

        let count = thread_unthreaded_emails(&conn, "acct-1").unwrap();
        assert_eq!(count, 200);

        let thread_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM threads", [], |row| row.get(0))
            .unwrap();
        assert_eq!(thread_count, 200);
    }
}
