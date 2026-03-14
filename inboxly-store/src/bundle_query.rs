//! Bundle feed queries -- aggregate bundle summaries for the inbox feed.

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::params;

use crate::error::Result;
use crate::store::Store;

/// Summary of a bundle for the inbox feed (collapsed row data).
#[derive(Debug, Clone)]
pub struct BundleSummary {
    /// Bundle ID (string UUID).
    pub bundle_id: String,
    /// Category key (e.g., "social", "promos").
    pub category: String,
    /// Display name.
    pub name: String,
    /// Number of unread emails across all threads in this bundle.
    pub unread_count: u32,
    /// Number of active (not-done) threads in this bundle.
    pub thread_count: u32,
    /// Timestamp of the newest thread in the bundle.
    pub newest_date: DateTime<Utc>,
    /// Top sender names for preview (up to 3).
    pub sender_previews: Vec<SenderPreview>,
}

/// A sender name for the collapsed bundle preview line.
#[derive(Debug, Clone)]
pub struct SenderPreview {
    /// Display name (or email address if no name).
    pub name: String,
    /// Whether this sender has unread messages in the bundle.
    pub is_unread: bool,
}

impl Store {
    /// Query bundle summaries for the inbox feed.
    ///
    /// Aggregates active (not-done) threads grouped by bundle_id.
    /// Returns bundles with at least one active thread, ordered by newest
    /// thread date descending.
    ///
    /// # Errors
    ///
    /// Returns a store error if the query fails.
    pub fn query_bundle_summaries(&self) -> Result<Vec<BundleSummary>> {
        let mut stmt = self.conn().prepare(
            "SELECT
                b.id,
                b.category,
                b.name,
                SUM(t.unread_count) AS total_unread,
                COUNT(t.id) AS thread_count,
                MAX(t.newest_date) AS bundle_newest_date
            FROM bundles b
            INNER JOIN thread_state ts ON ts.bundle_id = b.id
            INNER JOIN threads t ON t.id = ts.thread_id
            WHERE COALESCE(ts.done, 0) = 0
            GROUP BY b.id
            HAVING COUNT(t.id) > 0
            ORDER BY bundle_newest_date DESC",
        )?;

        let mut summaries: Vec<BundleSummary> = stmt
            .query_map([], |row| {
                let newest_epoch: i64 = row.get(5)?;
                let newest_date = Utc
                    .timestamp_opt(newest_epoch, 0)
                    .single()
                    .unwrap_or_else(Utc::now);

                let total_unread: i64 = row.get(3)?;
                let thread_count: i64 = row.get(4)?;

                Ok(BundleSummary {
                    bundle_id: row.get(0)?,
                    category: row.get(1)?,
                    name: row.get(2)?,
                    unread_count: total_unread.try_into().unwrap_or(0),
                    thread_count: thread_count.try_into().unwrap_or(0),
                    newest_date,
                    sender_previews: Vec::new(), // populated below
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Populate sender previews for each bundle.
        for summary in &mut summaries {
            summary.sender_previews = self.query_bundle_sender_previews(&summary.bundle_id)?;
        }

        Ok(summaries)
    }

    /// Query top 3 sender names for a bundle preview.
    fn query_bundle_sender_previews(&self, bundle_id: &str) -> Result<Vec<SenderPreview>> {
        let mut stmt = self.conn().prepare(
            "SELECT DISTINCT
                COALESCE(e.from_name, e.from_address) AS sender,
                (e.flags & 1 = 0) AS is_unread
            FROM emails e
            INNER JOIN thread_state ts ON e.thread_id = ts.thread_id
            WHERE ts.bundle_id = ?1 AND COALESCE(ts.done, 0) = 0
            ORDER BY e.date DESC
            LIMIT 3",
        )?;

        let previews = stmt
            .query_map(params![bundle_id], |row| {
                Ok(SenderPreview {
                    name: row.get(0)?,
                    is_unread: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(previews)
    }

    /// Query individual threads within a bundle for the expanded view.
    ///
    /// Returns thread summaries (same format as `query_inbox_threads`)
    /// filtered to the specified bundle.
    pub fn query_bundle_threads(
        &self,
        bundle_id: &str,
    ) -> Result<Vec<crate::inbox_query::InboxThreadSummary>> {
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
            INNER JOIN thread_state ts ON t.id = ts.thread_id
            LEFT JOIN contacts c ON c.address = (
                SELECT e.from_address FROM emails e
                WHERE e.thread_id = t.id
                ORDER BY e.date DESC LIMIT 1
            )
            WHERE ts.bundle_id = ?1
              AND COALESCE(ts.done, 0) = 0
            ORDER BY t.newest_date DESC",
        )?;

        let rows = stmt
            .query_map(params![bundle_id], crate::inbox_query::map_inbox_thread)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_store() -> Store {
        let store = Store::open_in_memory().expect("in-memory store");
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

    fn setup_bundle(store: &Store, bundle_id: &str, category: &str, name: &str) {
        store
            .conn()
            .execute(
                "INSERT INTO bundles (id, category, name, color, badge_color)
                 VALUES (?1, ?2, ?3, '#000000', '#eeeeee')",
                params![bundle_id, category, name],
            )
            .expect("insert bundle");
    }

    fn setup_thread_in_bundle(
        store: &Store,
        thread_id: &str,
        bundle_id: &str,
        newest_date: i64,
        unread: i64,
    ) {
        use crate::emails::EmailRow;
        use crate::thread_state::ThreadStateRow;
        use crate::threads::ThreadRow;

        store
            .insert_thread(&ThreadRow {
                id: thread_id.to_owned(),
                account_id: "acct1".to_owned(),
                subject: format!("Thread {thread_id}"),
                newest_date,
                oldest_date: newest_date,
                email_count: 1,
                unread_count: unread,
                has_attachments: false,
                snippet: format!("Preview of {thread_id}"),
                root_message_id: None,
            })
            .expect("insert thread");

        store
            .insert_email(&EmailRow {
                id: format!("{thread_id}-email"),
                account_id: "acct1".to_owned(),
                thread_id: thread_id.to_owned(),
                from_name: Some("Sender".to_owned()),
                from_address: "sender@example.com".to_owned(),
                to_json: "[]".to_owned(),
                cc_json: "[]".to_owned(),
                subject: format!("Thread {thread_id}"),
                snippet: format!("Preview of {thread_id}"),
                date: newest_date,
                maildir_path: "/tmp/test".to_owned(),
                flags: if unread > 0 { 0 } else { 1 },
                size_bytes: 100,
                imap_uid: newest_date,
                imap_folder: "INBOX".to_owned(),
                has_attachments: false,
                body_downloaded: false,
                message_id_header: Some(format!("<{thread_id}@test>")),
                in_reply_to: None,
                references_json: None,
            })
            .expect("insert email");

        store
            .insert_thread_state(&ThreadStateRow {
                thread_id: thread_id.to_owned(),
                pinned: false,
                done: false,
                snoozed_until: None,
                snoozed_location_json: None,
                bundle_id: Some(bundle_id.to_owned()),
            })
            .expect("insert thread state");
    }

    #[test]
    fn empty_store_no_bundles() {
        let store = make_test_store();
        let result = store.query_bundle_summaries().expect("query");
        assert!(result.is_empty());
    }

    #[test]
    fn bundle_with_threads() {
        let store = make_test_store();
        let now = Utc::now().timestamp();
        setup_bundle(&store, "b1", "social", "Social");
        setup_thread_in_bundle(&store, "t1", "b1", now, 1);
        setup_thread_in_bundle(&store, "t2", "b1", now - 100, 0);

        let result = store.query_bundle_summaries().expect("query");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "Social");
        assert_eq!(result[0].thread_count, 2);
        assert_eq!(result[0].unread_count, 1);
    }

    #[test]
    fn bundle_threads_query() {
        let store = make_test_store();
        let now = Utc::now().timestamp();
        setup_bundle(&store, "b1", "social", "Social");
        setup_thread_in_bundle(&store, "t1", "b1", now, 0);

        let threads = store.query_bundle_threads("b1").expect("query");
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "t1");
    }

    #[test]
    fn done_threads_excluded_from_bundle() {
        let store = make_test_store();
        let now = Utc::now().timestamp();
        setup_bundle(&store, "b1", "social", "Social");
        setup_thread_in_bundle(&store, "t1", "b1", now, 0);

        // Mark t1 as done.
        store.set_thread_done("t1", true).expect("set done");

        let result = store.query_bundle_summaries().expect("query");
        assert!(result.is_empty());
    }
}
