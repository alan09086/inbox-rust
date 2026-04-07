//! Drafts storage — SQLite backend for in-progress compose emails.
//!
//! Drafts are persisted via three layers (SQLite, local Maildir, IMAP Drafts
//! folder) reconciled by `Message-ID`. This module is the SQLite layer. See
//! [`inboxly_core::DraftEmail`] for the canonical shape.
//!
//! # Schema note
//!
//! The SQL column for the RFC 5322 `References` header is named
//! `references_header` because `REFERENCES` is a SQLite reserved word
//! (FOREIGN KEY clause). The Rust-side field is still
//! `DraftEmail::references` — the rename is purely a storage-layer
//! workaround that stops at the `DraftRow` boundary.

use std::path::PathBuf;
use std::str::FromStr;

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{Row, params};
use uuid::Uuid;

use inboxly_core::{AccountId, AttachmentDraft, ComposeMode, Contact, DraftEmail};

use crate::error::{Result, StoreError};
use crate::store::Store;

/// Row representation for the `drafts` table.
///
/// Mirrors [`DraftEmail`] one-to-one, with the recipient lists, attachment
/// list, and compose mode stored as JSON-serialised strings. Timestamps are
/// Unix seconds (`i64`). External callers should prefer the [`Store`]
/// CRUD helpers and reach for `DraftRow` only when a test needs to assert
/// on individual column values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftRow {
    /// Draft id (UUID v4 string from [`DraftEmail::id`]).
    pub id: String,
    /// Owning account id, stored as a string for schema simplicity.
    pub account_id: String,
    /// Canonical RFC 5322 Message-ID. `UNIQUE` across the table.
    pub message_id: String,
    /// Subject line.
    pub subject: String,
    /// Markdown body source.
    pub body_markdown: String,
    /// `Vec<Contact>` serialized as JSON.
    pub to_json: String,
    /// `Vec<Contact>` serialized as JSON.
    pub cc_json: String,
    /// `Vec<Contact>` serialized as JSON.
    pub bcc_json: String,
    /// `Vec<AttachmentDraft>` serialized as JSON.
    pub attachments_json: String,
    /// [`ComposeMode`] serialized as JSON.
    pub mode_json: String,
    /// `In-Reply-To` header value (set when replying).
    pub in_reply_to: Option<String>,
    /// `References` header value — chain of ancestor Message-IDs.
    /// Stored under the column name `references_header` (see module docs).
    pub references_header: Option<String>,
    /// Path to the local Maildir `.Drafts/` file, if saved.
    pub maildir_path: Option<String>,
    /// Created-at timestamp, Unix seconds.
    pub created_at: i64,
    /// Last-updated timestamp, Unix seconds.
    pub updated_at: i64,
}

impl DraftRow {
    /// Serialize a [`DraftEmail`] into a SQLite-ready `DraftRow`.
    ///
    /// # Errors
    /// Returns `StoreError::Json` if any of the JSON-serialised fields fail
    /// to serialise (all types are `Serialize`-derived so this should be
    /// unreachable in practice).
    pub fn from_draft(draft: &DraftEmail) -> Result<Self> {
        Ok(Self {
            id: draft.id.clone(),
            account_id: draft.account_id.to_string(),
            message_id: draft.message_id.clone(),
            subject: draft.subject.clone(),
            body_markdown: draft.body_markdown.clone(),
            to_json: serde_json::to_string(&draft.to)?,
            cc_json: serde_json::to_string(&draft.cc)?,
            bcc_json: serde_json::to_string(&draft.bcc)?,
            attachments_json: serde_json::to_string(&draft.attachments)?,
            mode_json: serde_json::to_string(&draft.mode)?,
            in_reply_to: draft.in_reply_to.clone(),
            references_header: draft.references.clone(),
            maildir_path: draft
                .maildir_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            created_at: draft.created_at.timestamp(),
            updated_at: draft.updated_at.timestamp(),
        })
    }

    /// Deserialize a [`DraftRow`] back into a [`DraftEmail`].
    ///
    /// # Errors
    /// - `StoreError::Json` if a JSON column fails to parse.
    /// - `StoreError::Parse` if `account_id` is not a valid UUID or the
    ///   timestamps are outside the representable `DateTime<Utc>` range.
    pub fn into_draft(self) -> Result<DraftEmail> {
        let to: Vec<Contact> = serde_json::from_str(&self.to_json)?;
        let cc: Vec<Contact> = serde_json::from_str(&self.cc_json)?;
        let bcc: Vec<Contact> = serde_json::from_str(&self.bcc_json)?;
        let attachments: Vec<AttachmentDraft> = serde_json::from_str(&self.attachments_json)?;
        let mode: ComposeMode = serde_json::from_str(&self.mode_json)?;

        let account_uuid = Uuid::from_str(&self.account_id).map_err(|e| {
            StoreError::Parse(format!("invalid account_id uuid {}: {e}", self.account_id))
        })?;
        let account_id = AccountId(account_uuid);

        let created_at: DateTime<Utc> =
            Utc.timestamp_opt(self.created_at, 0)
                .single()
                .ok_or_else(|| {
                    StoreError::Parse(format!("invalid created_at timestamp: {}", self.created_at))
                })?;
        let updated_at: DateTime<Utc> =
            Utc.timestamp_opt(self.updated_at, 0)
                .single()
                .ok_or_else(|| {
                    StoreError::Parse(format!("invalid updated_at timestamp: {}", self.updated_at))
                })?;

        Ok(DraftEmail {
            id: self.id,
            account_id,
            message_id: self.message_id,
            subject: self.subject,
            body_markdown: self.body_markdown,
            to,
            cc,
            bcc,
            attachments,
            mode,
            in_reply_to: self.in_reply_to,
            references: self.references_header,
            maildir_path: self.maildir_path.map(PathBuf::from),
            created_at,
            updated_at,
        })
    }

    /// `rusqlite` row mapper. Column order must match every SELECT list
    /// in this module.
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            account_id: row.get(1)?,
            message_id: row.get(2)?,
            subject: row.get(3)?,
            body_markdown: row.get(4)?,
            to_json: row.get(5)?,
            cc_json: row.get(6)?,
            bcc_json: row.get(7)?,
            attachments_json: row.get(8)?,
            mode_json: row.get(9)?,
            in_reply_to: row.get(10)?,
            references_header: row.get(11)?,
            maildir_path: row.get(12)?,
            created_at: row.get(13)?,
            updated_at: row.get(14)?,
        })
    }
}

/// Canonical column list for every SELECT in this module.
///
/// Defined once so `get_draft` / `list_drafts` can't drift apart.
const SELECT_COLUMNS: &str = "id, account_id, message_id, subject, body_markdown, \
    to_json, cc_json, bcc_json, attachments_json, mode_json, \
    in_reply_to, references_header, maildir_path, created_at, updated_at";

impl Store {
    /// Insert a new draft.
    ///
    /// # Errors
    /// - `StoreError::Sqlite` if the `message_id` UNIQUE constraint is
    ///   violated (duplicate draft Message-ID) or any other SQL error.
    /// - `StoreError::Json` if a recipient/attachment/mode field fails to
    ///   serialise.
    pub fn insert_draft(&self, draft: &DraftEmail) -> Result<()> {
        let row = DraftRow::from_draft(draft)?;
        self.conn().execute(
            "INSERT INTO drafts (
                id, account_id, message_id, subject, body_markdown,
                to_json, cc_json, bcc_json, attachments_json, mode_json,
                in_reply_to, references_header, maildir_path, created_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                row.id,
                row.account_id,
                row.message_id,
                row.subject,
                row.body_markdown,
                row.to_json,
                row.cc_json,
                row.bcc_json,
                row.attachments_json,
                row.mode_json,
                row.in_reply_to,
                row.references_header,
                row.maildir_path,
                row.created_at,
                row.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Update an existing draft in place (keyed by `id`).
    ///
    /// `created_at` is deliberately left untouched: it's the creation time,
    /// not a last-edit timestamp. `updated_at` is whatever the caller put on
    /// the [`DraftEmail`] — the caller is expected to bump it before saving.
    ///
    /// # Errors
    /// - `StoreError::NotFound` if no draft with that id exists.
    /// - `StoreError::Sqlite` on SQL errors.
    /// - `StoreError::Json` if a recipient/attachment/mode field fails to
    ///   serialise.
    pub fn update_draft(&self, draft: &DraftEmail) -> Result<()> {
        let row = DraftRow::from_draft(draft)?;
        let changed = self.conn().execute(
            "UPDATE drafts SET
                account_id        = ?2,
                message_id        = ?3,
                subject           = ?4,
                body_markdown     = ?5,
                to_json           = ?6,
                cc_json           = ?7,
                bcc_json          = ?8,
                attachments_json  = ?9,
                mode_json         = ?10,
                in_reply_to       = ?11,
                references_header = ?12,
                maildir_path      = ?13,
                updated_at        = ?14
             WHERE id = ?1",
            params![
                row.id,
                row.account_id,
                row.message_id,
                row.subject,
                row.body_markdown,
                row.to_json,
                row.cc_json,
                row.bcc_json,
                row.attachments_json,
                row.mode_json,
                row.in_reply_to,
                row.references_header,
                row.maildir_path,
                row.updated_at,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("draft {}", row.id)));
        }
        Ok(())
    }

    /// Fetch a draft by id. Returns `Ok(None)` if not found.
    ///
    /// # Errors
    /// `StoreError::Sqlite` on SQL errors other than "no rows".
    pub fn get_draft(&self, id: &str) -> Result<Option<DraftRow>> {
        let sql = format!("SELECT {SELECT_COLUMNS} FROM drafts WHERE id = ?1");
        match self.conn().query_row(&sql, params![id], DraftRow::from_row) {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(other) => Err(StoreError::Sqlite(other)),
        }
    }

    /// List all drafts for an account, newest-updated first.
    ///
    /// # Errors
    /// `StoreError::Sqlite` on SQL errors.
    pub fn list_drafts(&self, account_id: &str) -> Result<Vec<DraftRow>> {
        let sql = format!(
            "SELECT {SELECT_COLUMNS} FROM drafts \
             WHERE account_id = ?1 \
             ORDER BY updated_at DESC"
        );
        let conn = self.conn();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![account_id], DraftRow::from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Delete a draft by id.
    ///
    /// # Errors
    /// - `StoreError::NotFound` if no draft with that id exists.
    /// - `StoreError::Sqlite` on SQL errors.
    pub fn delete_draft(&self, id: &str) -> Result<()> {
        let changed = self
            .conn()
            .execute("DELETE FROM drafts WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("draft {id}")));
        }
        Ok(())
    }
}
