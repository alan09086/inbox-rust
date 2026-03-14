use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

/// Row representation for the emails table.
///
/// JSON fields (`to_json`, `cc_json`, `references_json`) are stored as
/// serialised JSON strings. Callers use `serde_json` to parse them into
/// `Vec<Contact>` or `Vec<String>` as needed.
#[derive(Debug, Clone)]
pub struct EmailRow {
    pub id: String,
    pub account_id: String,
    pub thread_id: String,
    pub from_name: Option<String>,
    pub from_address: String,
    pub to_json: String,
    pub cc_json: String,
    pub subject: String,
    pub snippet: String,
    pub date: i64,
    pub maildir_path: String,
    pub flags: i64,
    pub size_bytes: i64,
    pub imap_uid: i64,
    pub imap_folder: String,
    pub has_attachments: bool,
    pub body_downloaded: bool,
    pub message_id_header: Option<String>,
    pub in_reply_to: Option<String>,
    pub references_json: Option<String>,
}

/// Bitmask constants for `EmailRow::flags`.
pub mod flags {
    pub const READ: i64 = 1;
    pub const STARRED: i64 = 2;
    pub const ANSWERED: i64 = 4;
    pub const DRAFT: i64 = 8;
    pub const DELETED: i64 = 16;
}

impl Store {
    pub fn insert_email(&self, email: &EmailRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO emails (id, account_id, thread_id, from_name, from_address, to_json, cc_json,
             subject, snippet, date, maildir_path, flags, size_bytes, imap_uid, imap_folder,
             has_attachments, body_downloaded, message_id_header, in_reply_to, references_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                email.id,
                email.account_id,
                email.thread_id,
                email.from_name,
                email.from_address,
                email.to_json,
                email.cc_json,
                email.subject,
                email.snippet,
                email.date,
                email.maildir_path,
                email.flags,
                email.size_bytes,
                email.imap_uid,
                email.imap_folder,
                email.has_attachments,
                email.body_downloaded,
                email.message_id_header,
                email.in_reply_to,
                email.references_json,
            ],
        )?;
        Ok(())
    }

    pub fn get_email(&self, id: &str) -> Result<EmailRow> {
        self.conn()
            .query_row(
                "SELECT id, account_id, thread_id, from_name, from_address, to_json, cc_json,
                 subject, snippet, date, maildir_path, flags, size_bytes, imap_uid, imap_folder,
                 has_attachments, body_downloaded, message_id_header, in_reply_to, references_json
                 FROM emails WHERE id = ?1",
                params![id],
                Self::row_to_email,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => StoreError::NotFound(format!("email {id}")),
                other => StoreError::Sqlite(other),
            })
    }

    /// Get all emails for a thread, ordered by date ascending.
    pub fn get_emails_by_thread(&self, thread_id: &str) -> Result<Vec<EmailRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, account_id, thread_id, from_name, from_address, to_json, cc_json,
             subject, snippet, date, maildir_path, flags, size_bytes, imap_uid, imap_folder,
             has_attachments, body_downloaded, message_id_header, in_reply_to, references_json
             FROM emails WHERE thread_id = ?1 ORDER BY date ASC",
        )?;
        let rows = stmt
            .query_map(params![thread_id], Self::row_to_email)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get all emails for an account in a folder, ordered by date descending.
    pub fn get_emails_by_folder(&self, account_id: &str, folder: &str) -> Result<Vec<EmailRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, account_id, thread_id, from_name, from_address, to_json, cc_json,
             subject, snippet, date, maildir_path, flags, size_bytes, imap_uid, imap_folder,
             has_attachments, body_downloaded, message_id_header, in_reply_to, references_json
             FROM emails WHERE account_id = ?1 AND imap_folder = ?2 ORDER BY date DESC",
        )?;
        let rows = stmt
            .query_map(params![account_id, folder], Self::row_to_email)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Look up an email by IMAP UID (unique within account + folder).
    pub fn get_email_by_uid(
        &self,
        account_id: &str,
        folder: &str,
        uid: i64,
    ) -> Result<Option<EmailRow>> {
        let result = self.conn().query_row(
            "SELECT id, account_id, thread_id, from_name, from_address, to_json, cc_json,
             subject, snippet, date, maildir_path, flags, size_bytes, imap_uid, imap_folder,
             has_attachments, body_downloaded, message_id_header, in_reply_to, references_json
             FROM emails WHERE account_id = ?1 AND imap_folder = ?2 AND imap_uid = ?3",
            params![account_id, folder, uid],
            Self::row_to_email,
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Sqlite(e)),
        }
    }

    /// Update flags for an email.
    pub fn update_email_flags(&self, id: &str, flags: i64) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE emails SET flags = ?2 WHERE id = ?1",
            params![id, flags],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("email {id}")));
        }
        Ok(())
    }

    /// Reassign an email to a different thread (used during thread unification).
    pub fn update_email_thread(&self, email_id: &str, new_thread_id: &str) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE emails SET thread_id = ?2 WHERE id = ?1",
            params![email_id, new_thread_id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("email {email_id}")));
        }
        Ok(())
    }

    pub fn delete_email(&self, id: &str) -> Result<()> {
        let changed = self
            .conn()
            .execute("DELETE FROM emails WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("email {id}")));
        }
        Ok(())
    }

    /// Get the highest IMAP UID stored for a given account + folder.
    pub fn get_max_uid(&self, account_id: &str, folder: &str) -> Result<Option<i64>> {
        let result = self.conn().query_row(
            "SELECT MAX(imap_uid) FROM emails WHERE account_id = ?1 AND imap_folder = ?2",
            params![account_id, folder],
            |row| row.get::<_, Option<i64>>(0),
        )?;
        Ok(result)
    }

    fn row_to_email(row: &rusqlite::Row<'_>) -> rusqlite::Result<EmailRow> {
        Ok(EmailRow {
            id: row.get(0)?,
            account_id: row.get(1)?,
            thread_id: row.get(2)?,
            from_name: row.get(3)?,
            from_address: row.get(4)?,
            to_json: row.get(5)?,
            cc_json: row.get(6)?,
            subject: row.get(7)?,
            snippet: row.get(8)?,
            date: row.get(9)?,
            maildir_path: row.get(10)?,
            flags: row.get(11)?,
            size_bytes: row.get(12)?,
            imap_uid: row.get(13)?,
            imap_folder: row.get(14)?,
            has_attachments: row.get(15)?,
            body_downloaded: row.get(16)?,
            message_id_header: row.get(17)?,
            in_reply_to: row.get(18)?,
            references_json: row.get(19)?,
        })
    }

    // -- Phase 2 (M8) query methods --

    /// Mark an email's body as downloaded and update its Maildir path.
    pub fn mark_body_downloaded(&self, email_id: &str, maildir_path: &str) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE emails SET body_downloaded = 1, maildir_path = ?1 WHERE id = ?2",
            params![maildir_path, email_id],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("email {email_id}")));
        }
        Ok(())
    }

    /// Check if an email's body has been downloaded.
    pub fn is_body_downloaded(&self, email_id: &str) -> Result<bool> {
        let downloaded: bool = self
            .conn()
            .query_row(
                "SELECT body_downloaded FROM emails WHERE id = ?1",
                params![email_id],
                |row| row.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("email {email_id}"))
                }
                other => StoreError::Sqlite(other),
            })?;
        Ok(downloaded)
    }

    /// Get the Maildir path for an email.
    pub fn get_maildir_path(&self, email_id: &str) -> Result<Option<String>> {
        let path: String = self
            .conn()
            .query_row(
                "SELECT maildir_path FROM emails WHERE id = ?1",
                params![email_id],
                |row| row.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    StoreError::NotFound(format!("email {email_id}"))
                }
                other => StoreError::Sqlite(other),
            })?;
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }

    /// Count emails in a folder that have not had their body downloaded yet.
    pub fn count_emails_without_body(&self, account_id: &str, folder: &str) -> Result<u64> {
        let count: u64 = self.conn().query_row(
            "SELECT COUNT(*) FROM emails WHERE account_id = ?1 AND imap_folder = ?2 AND body_downloaded = 0",
            params![account_id, folder],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Get UIDs of emails without bodies, ordered by UID descending (newest first).
    /// Returns at most `limit` UIDs.
    pub fn get_uids_without_body(
        &self,
        account_id: &str,
        folder: &str,
        limit: usize,
    ) -> Result<Vec<i64>> {
        let mut stmt = self.conn().prepare(
            "SELECT imap_uid FROM emails
             WHERE account_id = ?1 AND imap_folder = ?2 AND body_downloaded = 0
             ORDER BY imap_uid DESC
             LIMIT ?3",
        )?;
        let uids = stmt
            .query_map(params![account_id, folder, limit as i64], |row| row.get(0))?
            .collect::<std::result::Result<Vec<i64>, _>>()?;
        Ok(uids)
    }

    /// Look up an email's ID by its IMAP UID within an account + folder.
    pub fn get_email_id_by_uid(
        &self,
        account_id: &str,
        folder: &str,
        uid: i64,
    ) -> Result<Option<String>> {
        let result = self.conn().query_row(
            "SELECT id FROM emails WHERE account_id = ?1 AND imap_folder = ?2 AND imap_uid = ?3",
            params![account_id, folder, uid],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Sqlite(e)),
        }
    }

    // -- M9: Incremental sync query methods --

    /// Get all IMAP UIDs for an account + folder.
    pub fn get_uids_in_folder(&self, account_id: &str, folder: &str) -> Result<Vec<i64>> {
        let mut stmt = self.conn().prepare(
            "SELECT imap_uid FROM emails
             WHERE account_id = ?1 AND imap_folder = ?2
             ORDER BY imap_uid ASC",
        )?;
        let uids = stmt
            .query_map(params![account_id, folder], |row| row.get(0))?
            .collect::<std::result::Result<Vec<i64>, _>>()?;
        Ok(uids)
    }

    /// Get IMAP UIDs for emails received since a given unix timestamp.
    ///
    /// Used by the non-CONDSTORE fallback to scope flag sync to a recent window
    /// (e.g., last 30 days) rather than scanning the entire mailbox.
    pub fn get_uids_since(
        &self,
        account_id: &str,
        folder: &str,
        since_unix: i64,
    ) -> Result<Vec<i64>> {
        let mut stmt = self.conn().prepare(
            "SELECT imap_uid FROM emails
             WHERE account_id = ?1 AND imap_folder = ?2 AND date >= ?3
             ORDER BY imap_uid ASC",
        )?;
        let uids = stmt
            .query_map(params![account_id, folder, since_unix], |row| row.get(0))?
            .collect::<std::result::Result<Vec<i64>, _>>()?;
        Ok(uids)
    }

    /// Mark an email as deleted by setting the DELETED flag bit.
    ///
    /// This is a soft-delete: the row remains in SQLite with the deleted flag set.
    /// The email can be purged later during a maintenance pass.
    pub fn mark_email_deleted_by_uid(
        &self,
        account_id: &str,
        folder: &str,
        uid: i64,
    ) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE emails SET flags = flags | ?4
             WHERE account_id = ?1 AND imap_folder = ?2 AND imap_uid = ?3",
            params![account_id, folder, uid, flags::DELETED],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("email uid={uid} in {folder}")));
        }
        Ok(())
    }

    /// Update flags for an email identified by account + folder + UID.
    ///
    /// Used by incremental sync when we know the IMAP UID but not the email ID.
    pub fn update_flags_by_uid(
        &self,
        account_id: &str,
        folder: &str,
        uid: i64,
        new_flags: i64,
    ) -> Result<()> {
        let changed = self.conn().execute(
            "UPDATE emails SET flags = ?4
             WHERE account_id = ?1 AND imap_folder = ?2 AND imap_uid = ?3",
            params![account_id, folder, uid, new_flags],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("email uid={uid} in {folder}")));
        }
        Ok(())
    }

    /// Upsert an email row (insert or update on conflict).
    ///
    /// Used by incremental sync to insert newly discovered emails.
    /// On conflict (same id), updates all fields except id.
    pub fn upsert_email(&self, email: &EmailRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO emails (id, account_id, thread_id, from_name, from_address, to_json, cc_json,
             subject, snippet, date, maildir_path, flags, size_bytes, imap_uid, imap_folder,
             has_attachments, body_downloaded, message_id_header, in_reply_to, references_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
             ON CONFLICT(id) DO UPDATE SET
                flags = excluded.flags,
                size_bytes = excluded.size_bytes,
                snippet = excluded.snippet,
                body_downloaded = excluded.body_downloaded,
                maildir_path = excluded.maildir_path",
            params![
                email.id,
                email.account_id,
                email.thread_id,
                email.from_name,
                email.from_address,
                email.to_json,
                email.cc_json,
                email.subject,
                email.snippet,
                email.date,
                email.maildir_path,
                email.flags,
                email.size_bytes,
                email.imap_uid,
                email.imap_folder,
                email.has_attachments,
                email.body_downloaded,
                email.message_id_header,
                email.in_reply_to,
                email.references_json,
            ],
        )?;
        Ok(())
    }
}
