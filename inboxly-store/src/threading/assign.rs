//! Core thread assignment algorithm.
//!
//! Given an email's parsed `ThreadingHeaders`, determines which thread the
//! email should belong to using a simplified JWZ algorithm based on
//! `References` and `In-Reply-To` headers. No subject-based grouping.

use rusqlite::{Connection, params};
use uuid::Uuid;

use super::headers::ThreadingHeaders;
use crate::error::{Result, StoreError};

/// Result of thread assignment: the thread ID and whether a new thread was created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadAssignment {
    /// The assigned thread ID.
    pub thread_id: String,
    /// Whether a new thread row was created (true) or an existing thread was reused (false).
    pub created: bool,
}

/// Determine the thread ID for an email based on its threading headers.
///
/// Algorithm:
/// 1. If `References` is non-empty, the thread root is `references[0]`.
///    Look up an existing thread that has an email with that Message-ID,
///    or a placeholder thread keyed on that root. If not found, create a
///    placeholder thread.
///
/// 2. If `References` is empty but `In-Reply-To` is present, look up the
///    email with that Message-ID and return its thread. Otherwise create
///    a placeholder.
///
/// 3. If neither header is present, create a new standalone thread.
///
/// # Errors
///
/// Returns `StoreError::Sqlite` on database errors.
pub fn assign_thread(
    conn: &Connection,
    account_id: &str,
    headers: &ThreadingHeaders,
    email_subject: &str,
    email_date: i64,
) -> Result<ThreadAssignment> {
    // Case 1: References present — use references[0] as root.
    if let Some(root_mid) = headers.references.first() {
        // Self-referencing protection: if the root is the email's own ID,
        // treat as a standalone thread.
        if headers.message_id.as_deref() == Some(root_mid.as_str()) {
            return create_standalone_thread(conn, account_id, email_subject, email_date);
        }

        if let Some(tid) = find_thread_by_message_id(conn, root_mid)? {
            return Ok(ThreadAssignment {
                thread_id: tid,
                created: false,
            });
        }

        if let Some(tid) = find_placeholder_thread(conn, root_mid)? {
            return Ok(ThreadAssignment {
                thread_id: tid,
                created: false,
            });
        }

        // No thread exists — create placeholder keyed on root Message-ID.
        return create_placeholder_thread(conn, account_id, email_subject, email_date, root_mid);
    }

    // Case 2: In-Reply-To only.
    if let Some(irt) = &headers.in_reply_to {
        // Self-referencing protection.
        if headers.message_id.as_deref() == Some(irt.as_str()) {
            return create_standalone_thread(conn, account_id, email_subject, email_date);
        }

        if let Some(tid) = find_thread_by_message_id(conn, irt)? {
            return Ok(ThreadAssignment {
                thread_id: tid,
                created: false,
            });
        }

        if let Some(tid) = find_placeholder_thread(conn, irt)? {
            return Ok(ThreadAssignment {
                thread_id: tid,
                created: false,
            });
        }

        return create_placeholder_thread(conn, account_id, email_subject, email_date, irt);
    }

    // Case 3: No threading headers — new standalone thread.
    create_standalone_thread(conn, account_id, email_subject, email_date)
}

/// Look up the `thread_id` of an email by its `message_id_header` column.
///
/// Returns `None` if no email with that Message-ID exists.
fn find_thread_by_message_id(conn: &Connection, message_id: &str) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT thread_id FROM emails WHERE message_id_header = ?1 LIMIT 1",
        params![message_id],
        |row| row.get::<_, String>(0),
    );
    match result {
        Ok(tid) => Ok(Some(tid)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(StoreError::Sqlite(e)),
    }
}

/// Look up a placeholder thread by its `root_message_id`.
///
/// Placeholder threads are identified by having `root_message_id` set.
fn find_placeholder_thread(conn: &Connection, root_message_id: &str) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT id FROM threads WHERE root_message_id = ?1 LIMIT 1",
        params![root_message_id],
        |row| row.get::<_, String>(0),
    );
    match result {
        Ok(tid) => Ok(Some(tid)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(StoreError::Sqlite(e)),
    }
}

/// Create a new standalone thread (no placeholder).
fn create_standalone_thread(
    conn: &Connection,
    account_id: &str,
    subject: &str,
    date: i64,
) -> Result<ThreadAssignment> {
    let thread_id = Uuid::new_v4().to_string();
    create_thread_row(conn, &thread_id, account_id, subject, date, None)?;
    Ok(ThreadAssignment {
        thread_id,
        created: true,
    })
}

/// Create a placeholder thread keyed on a root Message-ID.
fn create_placeholder_thread(
    conn: &Connection,
    account_id: &str,
    subject: &str,
    date: i64,
    root_message_id: &str,
) -> Result<ThreadAssignment> {
    let thread_id = Uuid::new_v4().to_string();
    create_thread_row(
        conn,
        &thread_id,
        account_id,
        subject,
        date,
        Some(root_message_id),
    )?;
    Ok(ThreadAssignment {
        thread_id,
        created: true,
    })
}

/// Insert a new thread row into the database.
///
/// If `root_message_id` is `Some`, this is a placeholder thread awaiting
/// the root email. If `None`, the thread already contains its root.
fn create_thread_row(
    conn: &Connection,
    thread_id: &str,
    account_id: &str,
    subject: &str,
    date: i64,
    root_message_id: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date,
         email_count, unread_count, has_attachments, snippet, root_message_id)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 0, '', ?6)",
        params![thread_id, account_id, subject, date, date, root_message_id],
    )?;
    Ok(())
}

// === Task 3: Placeholder thread identification helpers ===

/// Check if a thread is a placeholder (root email hasn't arrived yet).
///
/// A placeholder has `root_message_id` set AND no email in the thread has
/// `message_id_header` equal to that root ID.
///
/// # Errors
///
/// Returns `StoreError::Sqlite` on database errors, or `StoreError::NotFound`
/// if no thread with the given ID exists.
pub fn is_placeholder_thread(conn: &Connection, thread_id: &str) -> Result<bool> {
    // First check if the thread has a root_message_id at all.
    let root_mid: Option<String> = conn
        .query_row(
            "SELECT root_message_id FROM threads WHERE id = ?1",
            params![thread_id],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                StoreError::NotFound(format!("thread {thread_id}"))
            }
            other => StoreError::Sqlite(other),
        })?;

    let Some(root_mid) = root_mid else {
        // No root_message_id — not a placeholder.
        return Ok(false);
    };

    // Check if the root email has arrived (exists in the thread).
    let has_root: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM emails
            WHERE thread_id = ?1 AND message_id_header = ?2
        )",
        params![thread_id, root_mid],
        |row| row.get(0),
    )?;

    Ok(!has_root)
}

/// List all placeholder thread IDs (threads with `root_message_id` set
/// where the root email has not yet arrived).
///
/// # Errors
///
/// Returns `StoreError::Sqlite` on database errors.
pub fn list_placeholder_threads(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.id FROM threads t
         WHERE t.root_message_id IS NOT NULL
         AND NOT EXISTS (
             SELECT 1 FROM emails e
             WHERE e.thread_id = t.id AND e.message_id_header = t.root_message_id
         )",
    )?;

    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threading::headers::ThreadingHeaders;

    /// Create an in-memory database with the full schema for testing.
    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable FK");

        // Minimal schema matching the real migrations.
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

    /// Insert a minimal email row for testing thread assignment lookups.
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
    fn no_threading_headers_creates_new_thread() {
        let conn = test_db();
        let headers = ThreadingHeaders {
            message_id: Some("new@ex.com".into()),
            in_reply_to: None,
            references: vec![],
        };
        let result = assign_thread(&conn, "acct-1", &headers, "Test", 1710000000).unwrap();
        assert!(result.created);

        // Verify thread exists with NULL root_message_id.
        let root_mid: Option<String> = conn
            .query_row(
                "SELECT root_message_id FROM threads WHERE id = ?1",
                params![result.thread_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(root_mid.is_none());
    }

    #[test]
    fn references_with_existing_root_joins_thread() {
        let conn = test_db();

        // Create an existing thread with root email.
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date)
             VALUES ('thread-A', 'acct-1', 'Root', 1710000000, 1710000000)",
            [],
        )
        .unwrap();
        insert_email(&conn, "email-root", "thread-A", Some("root@ex.com"), 1);

        let headers = ThreadingHeaders {
            message_id: Some("reply@ex.com".into()),
            in_reply_to: Some("root@ex.com".into()),
            references: vec!["root@ex.com".into()],
        };
        let result = assign_thread(&conn, "acct-1", &headers, "Re: Root", 1710001000).unwrap();
        assert!(!result.created);
        assert_eq!(result.thread_id, "thread-A");
    }

    #[test]
    fn references_with_missing_root_creates_placeholder() {
        let conn = test_db();

        let headers = ThreadingHeaders {
            message_id: Some("reply@ex.com".into()),
            in_reply_to: Some("root@ex.com".into()),
            references: vec!["root@ex.com".into(), "mid@ex.com".into()],
        };
        let result = assign_thread(&conn, "acct-1", &headers, "Re: Root", 1710001000).unwrap();
        assert!(result.created);

        // Verify placeholder.
        let root_mid: Option<String> = conn
            .query_row(
                "SELECT root_message_id FROM threads WHERE id = ?1",
                params![result.thread_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(root_mid.as_deref(), Some("root@ex.com"));
    }

    #[test]
    fn second_reply_joins_existing_placeholder() {
        let conn = test_db();

        let headers1 = ThreadingHeaders {
            message_id: Some("reply1@ex.com".into()),
            in_reply_to: None,
            references: vec!["root@ex.com".into()],
        };
        let r1 = assign_thread(&conn, "acct-1", &headers1, "Re: Root", 1710001000).unwrap();
        assert!(r1.created);

        let headers2 = ThreadingHeaders {
            message_id: Some("reply2@ex.com".into()),
            in_reply_to: None,
            references: vec!["root@ex.com".into(), "reply1@ex.com".into()],
        };
        let r2 = assign_thread(&conn, "acct-1", &headers2, "Re: Root", 1710002000).unwrap();
        assert!(!r2.created);
        assert_eq!(r1.thread_id, r2.thread_id);
    }

    #[test]
    fn in_reply_to_only_joins_existing_email_thread() {
        let conn = test_db();

        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date)
             VALUES ('thread-B', 'acct-1', 'Topic', 1710000000, 1710000000)",
            [],
        )
        .unwrap();
        insert_email(&conn, "email-parent", "thread-B", Some("parent@ex.com"), 2);

        let headers = ThreadingHeaders {
            message_id: Some("child@ex.com".into()),
            in_reply_to: Some("parent@ex.com".into()),
            references: vec![],
        };
        let result = assign_thread(&conn, "acct-1", &headers, "Re: Topic", 1710001000).unwrap();
        assert!(!result.created);
        assert_eq!(result.thread_id, "thread-B");
    }

    #[test]
    fn in_reply_to_only_nonexistent_creates_placeholder() {
        let conn = test_db();

        let headers = ThreadingHeaders {
            message_id: Some("child@ex.com".into()),
            in_reply_to: Some("missing@ex.com".into()),
            references: vec![],
        };
        let result = assign_thread(&conn, "acct-1", &headers, "Re: Missing", 1710001000).unwrap();
        assert!(result.created);

        let root_mid: Option<String> = conn
            .query_row(
                "SELECT root_message_id FROM threads WHERE id = ?1",
                params![result.thread_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(root_mid.as_deref(), Some("missing@ex.com"));
    }

    #[test]
    fn references_takes_precedence_over_in_reply_to() {
        let conn = test_db();

        // Create two threads.
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date)
             VALUES ('thread-refs', 'acct-1', 'Refs', 1710000000, 1710000000)",
            [],
        )
        .unwrap();
        insert_email(
            &conn,
            "email-refs-root",
            "thread-refs",
            Some("refs-root@ex.com"),
            3,
        );

        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date)
             VALUES ('thread-irt', 'acct-1', 'IRT', 1710000000, 1710000000)",
            [],
        )
        .unwrap();
        insert_email(
            &conn,
            "email-irt-parent",
            "thread-irt",
            Some("irt-parent@ex.com"),
            4,
        );

        // Email with References pointing to thread-refs and In-Reply-To pointing to thread-irt.
        let headers = ThreadingHeaders {
            message_id: Some("combo@ex.com".into()),
            in_reply_to: Some("irt-parent@ex.com".into()),
            references: vec!["refs-root@ex.com".into()],
        };
        let result = assign_thread(&conn, "acct-1", &headers, "Combo", 1710001000).unwrap();
        assert!(!result.created);
        assert_eq!(result.thread_id, "thread-refs");
    }

    #[test]
    fn self_referencing_creates_standalone() {
        let conn = test_db();

        let headers = ThreadingHeaders {
            message_id: Some("self@ex.com".into()),
            in_reply_to: None,
            references: vec!["self@ex.com".into()],
        };
        let result = assign_thread(&conn, "acct-1", &headers, "Self", 1710001000).unwrap();
        assert!(result.created);

        // Should be standalone (no root_message_id).
        let root_mid: Option<String> = conn
            .query_row(
                "SELECT root_message_id FROM threads WHERE id = ?1",
                params![result.thread_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(root_mid.is_none());
    }

    // === Task 3 tests ===

    #[test]
    fn placeholder_thread_detected() {
        let conn = test_db();

        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, root_message_id)
             VALUES ('ph-thread', 'acct-1', 'Placeholder', 1710000000, 1710000000, 'awaited@ex.com')",
            [],
        )
        .unwrap();

        assert!(is_placeholder_thread(&conn, "ph-thread").unwrap());
    }

    #[test]
    fn placeholder_resolved_when_root_arrives() {
        let conn = test_db();

        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, root_message_id)
             VALUES ('ph-thread2', 'acct-1', 'Placeholder', 1710000000, 1710000000, 'root@ex.com')",
            [],
        )
        .unwrap();

        assert!(is_placeholder_thread(&conn, "ph-thread2").unwrap());

        // Insert root email.
        insert_email(&conn, "root-email", "ph-thread2", Some("root@ex.com"), 10);

        assert!(!is_placeholder_thread(&conn, "ph-thread2").unwrap());
    }

    #[test]
    fn non_placeholder_thread() {
        let conn = test_db();

        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date)
             VALUES ('normal-thread', 'acct-1', 'Normal', 1710000000, 1710000000)",
            [],
        )
        .unwrap();

        assert!(!is_placeholder_thread(&conn, "normal-thread").unwrap());
    }

    #[test]
    fn list_placeholder_threads_returns_correct_set() {
        let conn = test_db();

        // Normal thread.
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date)
             VALUES ('normal', 'acct-1', 'Normal', 1710000000, 1710000000)",
            [],
        )
        .unwrap();

        // Placeholder thread.
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, root_message_id)
             VALUES ('ph1', 'acct-1', 'PH1', 1710000000, 1710000000, 'awaited1@ex.com')",
            [],
        )
        .unwrap();

        // Resolved placeholder (root email present).
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date, root_message_id)
             VALUES ('ph2', 'acct-1', 'PH2', 1710000000, 1710000000, 'resolved@ex.com')",
            [],
        )
        .unwrap();
        insert_email(&conn, "resolved-email", "ph2", Some("resolved@ex.com"), 20);

        let placeholders = list_placeholder_threads(&conn).unwrap();
        assert_eq!(placeholders.len(), 1);
        assert_eq!(placeholders[0], "ph1");
    }
}
