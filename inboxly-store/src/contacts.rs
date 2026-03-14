use rusqlite::params;

use crate::error::{Result, StoreError};
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct ContactRow {
    pub address: String,
    pub display_name: Option<String>,
    pub avatar_letter: Option<String>,
    pub avatar_color_index: i64,
    pub last_seen: i64,
}

impl Store {
    /// Insert or update a contact. Called on email ingest to keep the contact
    /// cache fresh without re-parsing headers.
    pub fn upsert_contact(&self, contact: &ContactRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO contacts (address, display_name, avatar_letter, avatar_color_index, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(address) DO UPDATE SET
                display_name = COALESCE(excluded.display_name, contacts.display_name),
                avatar_letter = COALESCE(excluded.avatar_letter, contacts.avatar_letter),
                avatar_color_index = excluded.avatar_color_index,
                last_seen = MAX(excluded.last_seen, contacts.last_seen)",
            params![
                contact.address,
                contact.display_name,
                contact.avatar_letter,
                contact.avatar_color_index,
                contact.last_seen,
            ],
        )?;
        Ok(())
    }

    pub fn get_contact(&self, address: &str) -> Result<Option<ContactRow>> {
        let result = self.conn().query_row(
            "SELECT address, display_name, avatar_letter, avatar_color_index, last_seen
             FROM contacts WHERE address = ?1",
            params![address],
            |row| {
                Ok(ContactRow {
                    address: row.get(0)?,
                    display_name: row.get(1)?,
                    avatar_letter: row.get(2)?,
                    avatar_color_index: row.get(3)?,
                    last_seen: row.get(4)?,
                })
            },
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Sqlite(e)),
        }
    }

    /// Search contacts by address or display name prefix (for autocomplete).
    pub fn search_contacts(&self, query: &str, limit: i64) -> Result<Vec<ContactRow>> {
        let pattern = format!("{query}%");
        let mut stmt = self.conn().prepare(
            "SELECT address, display_name, avatar_letter, avatar_color_index, last_seen
             FROM contacts
             WHERE address LIKE ?1 OR display_name LIKE ?1
             ORDER BY last_seen DESC
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![pattern, limit], |row| {
                Ok(ContactRow {
                    address: row.get(0)?,
                    display_name: row.get(1)?,
                    avatar_letter: row.get(2)?,
                    avatar_color_index: row.get(3)?,
                    last_seen: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_contact(&self, address: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM contacts WHERE address = ?1",
            params![address],
        )?;
        Ok(())
    }
}
