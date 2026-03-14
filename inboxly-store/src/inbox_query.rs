//! Inbox feed queries -- joining threads, state, emails, and contacts
//! for display in the inbox feed.

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::Row;

use crate::error::Result;
use crate::store::Store;

/// Summary of a thread for inbox feed display.
///
/// Joins thread metadata with state and sender contact info.
/// Produced by [`Store::query_inbox_threads`].
#[derive(Debug, Clone)]
pub struct InboxThreadSummary {
    /// Thread ID (string UUID).
    pub id: String,
    /// Thread subject line.
    pub subject: String,
    /// Preview snippet of the newest email.
    pub snippet: String,
    /// Timestamp of the newest email in the thread.
    pub newest_date: DateTime<Utc>,
    /// Number of emails in the thread.
    pub email_count: u32,
    /// Number of unread emails in the thread.
    pub unread_count: u32,
    /// Whether the thread contains emails with attachments.
    pub has_attachments: bool,
    /// Whether the thread is pinned.
    pub pinned: bool,
    /// Sender display name (from newest email).
    pub sender_name: String,
    /// Sender email address (from newest email).
    pub sender_address: String,
    /// Avatar letter for the sender (uppercase, or '#' for non-alpha).
    pub avatar_letter: char,
    /// Avatar colour palette index (0-25 for A-Z, or 0 as default).
    pub avatar_color_index: u8,
}

impl Store {
    /// Query active (non-done) threads with sender and avatar info for the inbox feed.
    ///
    /// Returns threads ordered by newest_date descending. Done threads are excluded.
    /// Each result includes the sender from the newest email and their contact
    /// avatar data.
    ///
    /// # Errors
    ///
    /// Returns a store error if the query fails.
    pub fn query_inbox_threads(&self) -> Result<Vec<InboxThreadSummary>> {
        let mut stmt = self.conn().prepare(
            "SELECT
                t.id,
                t.subject,
                t.snippet,
                t.newest_date,
                t.email_count,
                t.unread_count,
                t.has_attachments,
                COALESCE(ts.pinned, 0) AS pinned,
                COALESCE(
                    (SELECT e.from_name FROM emails e
                     WHERE e.thread_id = t.id
                     ORDER BY e.date DESC LIMIT 1),
                    ''
                ) AS sender_name,
                COALESCE(
                    (SELECT e.from_address FROM emails e
                     WHERE e.thread_id = t.id
                     ORDER BY e.date DESC LIMIT 1),
                    ''
                ) AS sender_address,
                COALESCE(
                    c.avatar_letter,
                    UPPER(SUBSTR(
                        COALESCE(
                            (SELECT e.from_name FROM emails e
                             WHERE e.thread_id = t.id
                             ORDER BY e.date DESC LIMIT 1),
                            (SELECT e.from_address FROM emails e
                             WHERE e.thread_id = t.id
                             ORDER BY e.date DESC LIMIT 1),
                            '?'
                        ), 1, 1
                    ))
                ) AS avatar_letter,
                COALESCE(c.avatar_color_index, 0) AS avatar_color_index
            FROM threads t
            LEFT JOIN thread_state ts ON t.id = ts.thread_id
            LEFT JOIN contacts c ON c.address = (
                SELECT e.from_address FROM emails e
                WHERE e.thread_id = t.id
                ORDER BY e.date DESC LIMIT 1
            )
            WHERE COALESCE(ts.done, 0) = 0
              AND (ts.bundle_id IS NULL)
            ORDER BY t.newest_date DESC",
        )?;

        let rows = stmt
            .query_map([], map_inbox_thread)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }
}

/// Map a query row to an `InboxThreadSummary`.
///
/// The row must have columns in the order produced by `query_inbox_threads`
/// and `query_bundle_threads`.
pub(crate) fn map_inbox_thread(row: &Row<'_>) -> rusqlite::Result<InboxThreadSummary> {
    let newest_date_epoch: i64 = row.get(3)?;
    let newest_date = Utc
        .timestamp_opt(newest_date_epoch, 0)
        .single()
        .unwrap_or_else(Utc::now);

    let email_count_raw: i64 = row.get(4)?;
    let unread_count_raw: i64 = row.get(5)?;

    let avatar_letter_str: String = row.get(10)?;
    let avatar_letter = avatar_letter_str
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .next()
        .unwrap_or('?');

    let avatar_color_idx: i64 = row.get(11)?;

    Ok(InboxThreadSummary {
        id: row.get(0)?,
        subject: row.get(1)?,
        snippet: row.get(2)?,
        newest_date,
        email_count: email_count_raw.try_into().unwrap_or(0),
        unread_count: unread_count_raw.try_into().unwrap_or(0),
        has_attachments: row.get(6)?,
        pinned: row.get(7)?,
        sender_name: row.get(8)?,
        sender_address: row.get(9)?,
        avatar_letter,
        avatar_color_index: avatar_color_idx.try_into().unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_store() -> Store {
        let store = Store::open_in_memory().expect("in-memory store");
        // Insert a test account to satisfy FK constraints.
        store
            .conn()
            .execute(
                "INSERT INTO accounts (id, email, display_name, provider, auth_method,
                 imap_host, imap_port, smtp_host, smtp_port)
                 VALUES ('acct1', 'test@example.com', 'Test', 'generic', 'password',
                 'imap.example.com', 993, 'smtp.example.com', 587)",
                [],
            )
            .expect("insert test account");
        store
    }

    fn insert_test_thread(
        store: &Store,
        id: &str,
        subject: &str,
        newest_date: i64,
        pinned: bool,
        done: bool,
    ) {
        use crate::threads::ThreadRow;

        store
            .insert_thread(&ThreadRow {
                id: id.to_owned(),
                account_id: "acct1".to_owned(),
                subject: subject.to_owned(),
                newest_date,
                oldest_date: newest_date,
                email_count: 1,
                unread_count: 0,
                has_attachments: false,
                snippet: format!("Preview of {subject}"),
                root_message_id: None,
            })
            .expect("insert thread");

        // Insert a matching email so the sender subqueries work.
        use crate::emails::EmailRow;
        store
            .insert_email(&EmailRow {
                id: format!("{id}-email"),
                account_id: "acct1".to_owned(),
                thread_id: id.to_owned(),
                from_name: Some("Test Sender".to_owned()),
                from_address: "test@example.com".to_owned(),
                to_json: "[]".to_owned(),
                cc_json: "[]".to_owned(),
                subject: subject.to_owned(),
                snippet: format!("Preview of {subject}"),
                date: newest_date,
                maildir_path: "/tmp/test".to_owned(),
                flags: 0,
                size_bytes: 100,
                imap_uid: newest_date, // use timestamp as unique UID
                imap_folder: "INBOX".to_owned(),
                has_attachments: false,
                body_downloaded: false,
                message_id_header: Some(format!("<{id}@test>")),
                in_reply_to: None,
                references_json: None,
            })
            .expect("insert email");

        // Insert thread_state if pinned or done.
        if pinned || done {
            use crate::thread_state::ThreadStateRow;
            store
                .insert_thread_state(&ThreadStateRow {
                    thread_id: id.to_owned(),
                    pinned,
                    done,
                    snoozed_until: None,
                    snoozed_location_json: None,
                    bundle_id: None,
                })
                .expect("insert thread_state");
        }
    }

    #[test]
    fn empty_store_returns_no_threads() {
        let store = make_test_store();
        let result = store.query_inbox_threads().expect("query");
        assert!(result.is_empty());
    }

    #[test]
    fn returns_active_threads() {
        let store = make_test_store();
        let now = Utc::now().timestamp();
        insert_test_thread(&store, "t1", "Hello World", now, false, false);

        let result = store.query_inbox_threads().expect("query");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].subject, "Hello World");
        assert_eq!(result[0].sender_name, "Test Sender");
        assert_eq!(result[0].sender_address, "test@example.com");
    }

    #[test]
    fn excludes_done_threads() {
        let store = make_test_store();
        let now = Utc::now().timestamp();
        insert_test_thread(&store, "t1", "Active", now, false, false);
        insert_test_thread(&store, "t2", "Done", now - 100, false, true);

        let result = store.query_inbox_threads().expect("query");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].subject, "Active");
    }

    #[test]
    fn includes_pinned_threads() {
        let store = make_test_store();
        let now = Utc::now().timestamp();
        insert_test_thread(&store, "t1", "Pinned", now, true, false);

        let result = store.query_inbox_threads().expect("query");
        assert_eq!(result.len(), 1);
        assert!(result[0].pinned);
    }

    #[test]
    fn ordered_by_newest_date_desc() {
        let store = make_test_store();
        let now = Utc::now().timestamp();
        insert_test_thread(&store, "t1", "Old", now - 1000, false, false);
        insert_test_thread(&store, "t2", "New", now, false, false);
        insert_test_thread(&store, "t3", "Mid", now - 500, false, false);

        let result = store.query_inbox_threads().expect("query");
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].subject, "New");
        assert_eq!(result[1].subject, "Mid");
        assert_eq!(result[2].subject, "Old");
    }
}
