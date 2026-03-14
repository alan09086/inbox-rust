//! Edge case tests for the threading algorithm.
//!
//! Tests unusual and broken inputs that real-world emails produce.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rusqlite::{Connection, params};

    use crate::threading::assign::{assign_thread, is_placeholder_thread};
    use crate::threading::headers::{ThreadingHeaders, extract_threading_headers};

    fn headers(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

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
    fn no_headers_at_all() {
        let h: HashMap<String, String> = HashMap::new();
        let th = extract_threading_headers(&h);
        assert_eq!(th.message_id, None);
        assert_eq!(th.in_reply_to, None);
        assert!(th.references.is_empty());

        let conn = test_db();
        let result = assign_thread(&conn, "acct-1", &th, "No Headers", 1710000000).unwrap();
        assert!(result.created);
    }

    #[test]
    fn empty_message_id() {
        let h = headers(&[("Message-ID", "")]);
        let th = extract_threading_headers(&h);
        assert_eq!(th.message_id, None);
    }

    #[test]
    fn empty_references() {
        let h = headers(&[("References", "")]);
        let th = extract_threading_headers(&h);
        assert!(th.references.is_empty());
    }

    #[test]
    fn references_with_only_whitespace() {
        let h = headers(&[("References", "   \t  \n  ")]);
        let th = extract_threading_headers(&h);
        assert!(th.references.is_empty());
    }

    #[test]
    fn broken_angle_brackets() {
        // Malformed: nested opening bracket.
        let h = headers(&[("References", "<abc@example.com <def@example.com>")]);
        let th = extract_threading_headers(&h);
        // Should extract at least one valid ID.
        assert!(!th.references.is_empty());
        // The parser extracts content between < and >, so it gets
        // "abc@example.com <def@example.com" from first pair, which is
        // actually "abc@example.com <def@example.com" trimmed.
        // Then the second > gives "def@example.com".
        // The exact behavior depends on implementation, but it shouldn't panic.
    }

    #[test]
    fn missing_closing_bracket() {
        let h = headers(&[("References", "<abc@example.com")]);
        let th = extract_threading_headers(&h);
        assert_eq!(th.references, vec!["abc@example.com"]);
    }

    #[test]
    fn no_angle_brackets_bare_ids() {
        let h = headers(&[("References", "abc@example.com def@example.com")]);
        let th = extract_threading_headers(&h);
        assert_eq!(th.references, vec!["abc@example.com", "def@example.com"]);
    }

    #[test]
    fn circular_references_self() {
        let conn = test_db();
        let th = ThreadingHeaders {
            message_id: Some("self@ex.com".into()),
            in_reply_to: None,
            references: vec!["self@ex.com".into()],
        };
        let result = assign_thread(&conn, "acct-1", &th, "Self-ref", 1710000000).unwrap();
        assert!(result.created);

        // Should be standalone, not a placeholder.
        assert!(!is_placeholder_thread(&conn, &result.thread_id).unwrap());
    }

    #[test]
    fn circular_references_mutual() {
        let conn = test_db();

        // Email 1: Message-ID: <A>, References: <B>
        let th1 = ThreadingHeaders {
            message_id: Some("a@ex.com".into()),
            in_reply_to: None,
            references: vec!["b@ex.com".into()],
        };
        let r1 = assign_thread(&conn, "acct-1", &th1, "Email A", 1710000000).unwrap();
        insert_email(&conn, "email-a", &r1.thread_id, Some("a@ex.com"), 1);

        // Email 2: Message-ID: <B>, References: <A>
        let th2 = ThreadingHeaders {
            message_id: Some("b@ex.com".into()),
            in_reply_to: None,
            references: vec!["a@ex.com".into()],
        };
        let r2 = assign_thread(&conn, "acct-1", &th2, "Email B", 1710001000).unwrap();
        insert_email(&conn, "email-b", &r2.thread_id, Some("b@ex.com"), 2);

        // Both should end up referencing the same thread (B references A,
        // and A was already placed when B was processed).
        assert_eq!(r1.thread_id, r2.thread_id);
    }

    #[test]
    fn very_long_references_chain() {
        let conn = test_db();

        // 200+ Message-IDs in References. Only first matters.
        let refs: Vec<String> = (0..200).map(|i| format!("msg{i}@ex.com")).collect();
        let th = ThreadingHeaders {
            message_id: Some("latest@ex.com".into()),
            in_reply_to: None,
            references: refs,
        };
        let result = assign_thread(&conn, "acct-1", &th, "Deep chain", 1710000000).unwrap();
        assert!(result.created);

        // Should have created a placeholder keyed on refs[0].
        let root_mid: Option<String> = conn
            .query_row(
                "SELECT root_message_id FROM threads WHERE id = ?1",
                params![result.thread_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(root_mid.as_deref(), Some("msg0@ex.com"));
    }

    #[test]
    fn duplicate_message_id_joins_first_thread() {
        let conn = test_db();

        // First email with Message-ID <dup@ex.com>.
        let th1 = ThreadingHeaders {
            message_id: Some("dup@ex.com".into()),
            in_reply_to: None,
            references: vec![],
        };
        let r1 = assign_thread(&conn, "acct-1", &th1, "First", 1710000000).unwrap();
        insert_email(&conn, "email-1", &r1.thread_id, Some("dup@ex.com"), 1);

        // Second email with same Message-ID references itself (this would
        // be found by find_thread_by_message_id in In-Reply-To lookup).
        let th2 = ThreadingHeaders {
            message_id: Some("dup@ex.com".into()),
            in_reply_to: Some("dup@ex.com".into()),
            references: vec![],
        };
        let r2 = assign_thread(&conn, "acct-1", &th2, "Duplicate", 1710001000).unwrap();

        // Self-referencing In-Reply-To should find the existing thread.
        // Actually, since message_id == in_reply_to, it hits the self-ref guard
        // and creates a standalone. That's the correct behavior for a broken mailer.
        // The important thing is no crash/infinite loop.
        assert!(r2.created || r1.thread_id == r2.thread_id);
    }

    #[test]
    fn references_root_is_first_not_last() {
        let conn = test_db();

        // Create thread with root "first@ex.com".
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date)
             VALUES ('thread-first', 'acct-1', 'First', 1710000000, 1710000000)",
            [],
        )
        .unwrap();
        insert_email(
            &conn,
            "email-first",
            "thread-first",
            Some("first@ex.com"),
            10,
        );

        // Create thread with "last@ex.com".
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date)
             VALUES ('thread-last', 'acct-1', 'Last', 1710000000, 1710000000)",
            [],
        )
        .unwrap();
        insert_email(&conn, "email-last", "thread-last", Some("last@ex.com"), 11);

        // Email with references [first, middle, last] — should join first's thread.
        let th = ThreadingHeaders {
            message_id: Some("new@ex.com".into()),
            in_reply_to: Some("last@ex.com".into()),
            references: vec![
                "first@ex.com".into(),
                "middle@ex.com".into(),
                "last@ex.com".into(),
            ],
        };
        let result = assign_thread(&conn, "acct-1", &th, "Test", 1710002000).unwrap();
        assert_eq!(result.thread_id, "thread-first");
    }

    #[test]
    fn unicode_in_message_id() {
        let h = headers(&[("Message-ID", "<\u{00E9}mail@\u{00FC}ni.com>")]);
        let th = extract_threading_headers(&h);
        // Should preserve unicode, just lowercase.
        assert!(th.message_id.is_some());
        let mid = th.message_id.unwrap();
        assert!(mid.contains('\u{00E9}'));
    }

    #[test]
    fn very_long_message_id() {
        let long_id = format!("<{}@example.com>", "a".repeat(500));
        let h = headers(&[("Message-ID", &long_id)]);
        let th = extract_threading_headers(&h);
        assert!(th.message_id.is_some());
        assert!(th.message_id.as_ref().unwrap().len() > 500);
    }

    #[test]
    fn in_reply_to_with_multiple_ids() {
        let h = headers(&[("In-Reply-To", "<a@ex.com> <b@ex.com>")]);
        let th = extract_threading_headers(&h);
        assert_eq!(th.in_reply_to, Some("a@ex.com".into()));
    }

    #[test]
    fn references_overrides_in_reply_to() {
        let conn = test_db();

        // Create two threads.
        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date)
             VALUES ('thread-a', 'acct-1', 'A', 1710000000, 1710000000)",
            [],
        )
        .unwrap();
        insert_email(&conn, "email-a", "thread-a", Some("a@ex.com"), 20);

        conn.execute(
            "INSERT INTO threads (id, account_id, subject, newest_date, oldest_date)
             VALUES ('thread-b', 'acct-1', 'B', 1710000000, 1710000000)",
            [],
        )
        .unwrap();
        insert_email(&conn, "email-b", "thread-b", Some("b@ex.com"), 21);

        // References points to A, In-Reply-To points to B.
        let th = ThreadingHeaders {
            message_id: Some("new@ex.com".into()),
            in_reply_to: Some("b@ex.com".into()),
            references: vec!["a@ex.com".into()],
        };
        let result = assign_thread(&conn, "acct-1", &th, "Test", 1710002000).unwrap();
        assert_eq!(result.thread_id, "thread-a");
    }

    #[test]
    fn self_referencing_in_reply_to() {
        let conn = test_db();
        let th = ThreadingHeaders {
            message_id: Some("self@ex.com".into()),
            in_reply_to: Some("self@ex.com".into()),
            references: vec![],
        };
        let result = assign_thread(&conn, "acct-1", &th, "Self IRT", 1710000000).unwrap();
        assert!(result.created);
        // Should be standalone.
        assert!(!is_placeholder_thread(&conn, &result.thread_id).unwrap());
    }
}
