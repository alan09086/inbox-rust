//! Contact storage and extraction.
//!
//! Manages the `contacts` table — upserting contacts extracted from email
//! headers, looking up contacts by address, and assigning avatar letters/colours.

use rusqlite::params;

use inboxly_core::contact::{avatar_color_index, parse_address, parse_address_list};

use crate::error::{Result, StoreError};
use crate::store::Store;

/// Row representation for the `contacts` table.
#[derive(Debug, Clone, PartialEq)]
pub struct ContactRow {
    /// Lowercase email address (canonical key).
    pub address: String,
    /// Best-known display name. `None` if only the bare address is known.
    pub display_name: Option<String>,
    /// Single uppercase avatar letter (e.g., "A"), or `None` for non-alpha.
    pub avatar_letter: Option<String>,
    /// Index into the 26-colour BigTop palette (0-25), or -1 for default.
    pub avatar_color_index: i64,
    /// Unix epoch of the most recent email involving this contact.
    pub last_seen: i64,
}

impl ContactRow {
    /// Create a `ContactRow` from a raw address and optional display name.
    ///
    /// Derives the avatar letter from the display name (or local part of the
    /// address if no name) and computes the palette index.
    pub fn from_address(address: &str, display_name: Option<&str>, last_seen: i64) -> Self {
        let address = address.trim().to_lowercase();
        let display_name = display_name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty());

        let letter = derive_avatar_letter(display_name.as_deref(), &address);
        let letter_str = if letter == '#' {
            None
        } else {
            Some(letter.to_string())
        };
        let color_idx = avatar_color_index(letter).map_or(-1, |i| i as i64);

        Self {
            address,
            display_name,
            avatar_letter: letter_str,
            avatar_color_index: color_idx,
            last_seen,
        }
    }
}

/// Derives the avatar letter from a display name or email address.
///
/// Priority:
/// 1. First alphabetic character of display name (uppercased)
/// 2. First alphabetic character of the local part of the email address (uppercased)
/// 3. `'#'` if nothing yields a usable character
fn derive_avatar_letter(display_name: Option<&str>, address: &str) -> char {
    // Try display name first
    if let Some(name) = display_name
        && let Some(ch) = name.chars().next()
        && ch.is_ascii_alphabetic()
    {
        return ch.to_ascii_uppercase();
    }

    // Fall back to local part of email
    let local_part = address.split('@').next().unwrap_or(address);
    if let Some(ch) = local_part.chars().next()
        && ch.is_ascii_alphabetic()
    {
        return ch.to_ascii_uppercase();
    }

    // Non-alpha fallback
    '#'
}

impl Store {
    /// Insert or update a contact. Called on email ingest to keep the contact
    /// cache fresh without re-parsing headers.
    ///
    /// On conflict:
    /// - `display_name` is updated only if the new record provides one
    ///   (COALESCE keeps the existing name if the new one is NULL).
    /// - `avatar_letter` follows the same COALESCE logic.
    /// - `last_seen` is set to the maximum of the old and new values.
    pub fn upsert_contact(&self, contact: &ContactRow) -> Result<()> {
        self.conn().execute(
            "INSERT INTO contacts (address, display_name, avatar_letter, avatar_color_index, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(address) DO UPDATE SET
                display_name = COALESCE(excluded.display_name, contacts.display_name),
                avatar_letter = COALESCE(excluded.avatar_letter, contacts.avatar_letter),
                avatar_color_index = CASE
                    WHEN excluded.display_name IS NOT NULL THEN excluded.avatar_color_index
                    ELSE contacts.avatar_color_index
                END,
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

    /// Look up a contact by email address.
    pub fn get_contact(&self, address: &str) -> Result<Option<ContactRow>> {
        let address = address.trim().to_lowercase();
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

    /// Returns all contacts, ordered by most recently seen first.
    pub fn list_all_contacts(&self) -> Result<Vec<ContactRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT address, display_name, avatar_letter, avatar_color_index, last_seen
             FROM contacts ORDER BY last_seen DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
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

    /// Delete a contact by address.
    pub fn delete_contact(&self, address: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM contacts WHERE address = ?1", params![address])?;
        Ok(())
    }

    /// Extract and upsert contacts from a single email's headers.
    ///
    /// Parses From, To, and Cc header values, creates `ContactRow` entries,
    /// and upserts them. This should be called as part of the email ingest
    /// pipeline (after inserting the email into the `emails` table).
    ///
    /// # Arguments
    /// - `from` — the raw `From` header value (single address)
    /// - `to` — the raw `To` header value (comma-separated list), or `None`
    /// - `cc` — the raw `Cc` header value (comma-separated list), or `None`
    /// - `email_date` — unix epoch of the email's Date header
    pub fn extract_contacts_from_headers(
        &self,
        from: &str,
        to: Option<&str>,
        cc: Option<&str>,
        email_date: i64,
    ) -> Result<()> {
        // Parse From (single address)
        if let Some(parsed) = parse_address(from) {
            let contact =
                ContactRow::from_address(&parsed.address, parsed.name.as_deref(), email_date);
            self.upsert_contact(&contact)?;
        }

        // Parse To (address list)
        if let Some(to_raw) = to {
            for parsed in &parse_address_list(to_raw) {
                let contact =
                    ContactRow::from_address(&parsed.address, parsed.name.as_deref(), email_date);
                self.upsert_contact(&contact)?;
            }
        }

        // Parse Cc (address list)
        if let Some(cc_raw) = cc {
            for parsed in &parse_address_list(cc_raw) {
                let contact =
                    ContactRow::from_address(&parsed.address, parsed.name.as_deref(), email_date);
                self.upsert_contact(&contact)?;
            }
        }

        Ok(())
    }

    /// Batch-extract contacts from all existing emails in the database.
    ///
    /// Scans the `emails` table and extracts contacts from the `from_name`,
    /// `from_address`, `to_json`, and `cc_json` columns. This is idempotent —
    /// running it multiple times produces the same result due to upsert semantics.
    ///
    /// Returns the number of contact upsert operations performed.
    pub fn backfill_contacts_from_emails(&self) -> Result<usize> {
        let mut stmt = self
            .conn()
            .prepare("SELECT from_name, from_address, to_json, cc_json, date FROM emails")?;

        let mut count = 0;

        let rows = stmt.query_map([], |row| {
            let from_name: Option<String> = row.get(0)?;
            let from_address: String = row.get(1)?;
            let to_json: String = row.get(2)?;
            let cc_json: String = row.get(3)?;
            let date: i64 = row.get(4)?;
            Ok((from_name, from_address, to_json, cc_json, date))
        })?;

        for row in rows {
            let (from_name, from_address, to_json, cc_json, date) = row?;

            // Upsert the From contact
            let from_contact = ContactRow::from_address(&from_address, from_name.as_deref(), date);
            self.upsert_contact(&from_contact)?;
            count += 1;

            // Parse To contacts from JSON array
            count += self.upsert_contacts_from_json(&to_json, date)?;

            // Parse Cc contacts from JSON array
            count += self.upsert_contacts_from_json(&cc_json, date)?;
        }

        Ok(count)
    }

    /// Parse a JSON array of contact objects and upsert them.
    ///
    /// Expected format: `[{"name": "Alice", "address": "a@b.com"}, ...]`
    /// or `[{"address": "a@b.com"}, ...]` (name optional).
    ///
    /// Returns the number of contacts upserted. Invalid JSON is silently
    /// skipped (returns 0).
    fn upsert_contacts_from_json(&self, json: &str, date: i64) -> Result<usize> {
        // Deserialise JSON — if it fails, skip silently (defensive)
        let entries: Vec<serde_json::Value> = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(_) => return Ok(0),
        };

        let mut count = 0;
        for entry in &entries {
            let address = match entry.get("address").and_then(|v| v.as_str()) {
                Some(a) if a.contains('@') => a,
                _ => continue,
            };

            let name = entry.get("name").and_then(|v| v.as_str());
            let contact = ContactRow::from_address(address, name, date);
            self.upsert_contact(&contact)?;
            count += 1;
        }

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open_in_memory().expect("failed to open test store")
    }

    // --- ContactRow::from_address tests ---

    #[test]
    fn from_address_with_name() {
        let c = ContactRow::from_address("Alice@Example.com", Some("Alice Smith"), 1_700_000_000);
        assert_eq!(c.address, "alice@example.com");
        assert_eq!(c.display_name, Some("Alice Smith".to_string()));
        assert_eq!(c.avatar_letter, Some("A".to_string()));
        assert_eq!(c.avatar_color_index, 0); // A
        assert_eq!(c.last_seen, 1_700_000_000);
    }

    #[test]
    fn from_address_bare() {
        let c = ContactRow::from_address("bob@example.com", None, 1_700_000_000);
        assert_eq!(c.display_name, None);
        assert_eq!(c.avatar_letter, Some("B".to_string()));
        assert_eq!(c.avatar_color_index, 1); // B
    }

    #[test]
    fn from_address_empty_name_treated_as_none() {
        let c = ContactRow::from_address("charlie@example.com", Some("  "), 1_700_000_000);
        assert_eq!(c.display_name, None);
        assert_eq!(c.avatar_letter, Some("C".to_string()));
    }

    #[test]
    fn from_address_numeric_name_falls_back_to_address() {
        let c = ContactRow::from_address("dave@example.com", Some("123 Service"), 1_700_000_000);
        assert_eq!(c.avatar_letter, Some("D".to_string()));
    }

    #[test]
    fn from_address_numeric_address_and_no_name() {
        let c = ContactRow::from_address("123@example.com", None, 1_700_000_000);
        assert_eq!(c.avatar_letter, None); // '#' maps to None
        assert_eq!(c.avatar_color_index, -1);
    }

    // --- Store contact CRUD tests ---

    #[test]
    fn insert_and_retrieve_contact() {
        let store = test_store();
        let contact = ContactRow::from_address("alice@example.com", Some("Alice"), 1_700_000_000);
        store.upsert_contact(&contact).unwrap();

        let retrieved = store.get_contact("alice@example.com").unwrap().unwrap();
        assert_eq!(retrieved.address, "alice@example.com");
        assert_eq!(retrieved.display_name, Some("Alice".to_string()));
        assert_eq!(retrieved.avatar_letter, Some("A".to_string()));
        assert_eq!(retrieved.avatar_color_index, 0);
        assert_eq!(retrieved.last_seen, 1_700_000_000);
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let store = test_store();
        let contact = ContactRow::from_address("alice@example.com", Some("Alice"), 1_700_000_000);
        store.upsert_contact(&contact).unwrap();

        let retrieved = store.get_contact("ALICE@EXAMPLE.COM").unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let store = test_store();
        assert!(store.get_contact("nobody@x.com").unwrap().is_none());
    }

    #[test]
    fn upsert_updates_name_when_newer_has_name() {
        let store = test_store();

        // First insert with name
        let c1 = ContactRow::from_address("bob@b.com", Some("Bob"), 1000);
        store.upsert_contact(&c1).unwrap();

        // Newer insert with different name
        let c2 = ContactRow::from_address("bob@b.com", Some("Robert"), 2000);
        store.upsert_contact(&c2).unwrap();

        let retrieved = store.get_contact("bob@b.com").unwrap().unwrap();
        assert_eq!(retrieved.display_name, Some("Robert".to_string()));
        assert_eq!(retrieved.last_seen, 2000);
    }

    #[test]
    fn upsert_does_not_overwrite_name_with_none() {
        let store = test_store();

        let c1 = ContactRow::from_address("x@y.com", Some("Xavier"), 1000);
        store.upsert_contact(&c1).unwrap();

        // Newer but no display name — should keep "Xavier"
        let c2 = ContactRow::from_address("x@y.com", None, 2000);
        store.upsert_contact(&c2).unwrap();

        let retrieved = store.get_contact("x@y.com").unwrap().unwrap();
        assert_eq!(retrieved.display_name, Some("Xavier".to_string()));
        assert_eq!(retrieved.last_seen, 2000);
    }

    #[test]
    fn list_all_ordered_by_last_seen_desc() {
        let store = test_store();

        store
            .upsert_contact(&ContactRow::from_address("a@x.com", Some("Alpha"), 100))
            .unwrap();
        store
            .upsert_contact(&ContactRow::from_address("b@x.com", Some("Beta"), 300))
            .unwrap();
        store
            .upsert_contact(&ContactRow::from_address("c@x.com", Some("Charlie"), 200))
            .unwrap();

        let all = store.list_all_contacts().unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].address, "b@x.com"); // 300
        assert_eq!(all[1].address, "c@x.com"); // 200
        assert_eq!(all[2].address, "a@x.com"); // 100
    }

    #[test]
    fn contact_with_non_alpha_name_gets_default_colour() {
        let store = test_store();

        let contact = ContactRow::from_address("123@example.com", Some("42 Service"), 1000);
        store.upsert_contact(&contact).unwrap();

        let retrieved = store.get_contact("123@example.com").unwrap().unwrap();
        // '1' is not alpha, '1' in address also not alpha => '#' => None
        assert_eq!(retrieved.avatar_letter, None);
        assert_eq!(retrieved.avatar_color_index, -1);
    }

    // --- extract_contacts_from_headers tests ---

    #[test]
    fn extract_from_headers_full_email() {
        let store = test_store();

        store
            .extract_contacts_from_headers(
                "Alice Smith <alice@example.com>",
                Some("Bob <bob@b.com>, charlie@c.com"),
                Some("\"Davis, Eve\" <eve@d.com>"),
                1_700_000_000,
            )
            .unwrap();

        // Should have 4 contacts
        let all = store.list_all_contacts().unwrap();
        assert_eq!(all.len(), 4);

        let alice = store.get_contact("alice@example.com").unwrap().unwrap();
        assert_eq!(alice.display_name, Some("Alice Smith".to_string()));
        assert_eq!(alice.avatar_letter, Some("A".to_string()));

        let bob = store.get_contact("bob@b.com").unwrap().unwrap();
        assert_eq!(bob.display_name, Some("Bob".to_string()));

        let charlie = store.get_contact("charlie@c.com").unwrap().unwrap();
        assert_eq!(charlie.display_name, None);
        assert_eq!(charlie.avatar_letter, Some("C".to_string()));

        let eve = store.get_contact("eve@d.com").unwrap().unwrap();
        assert_eq!(eve.display_name, Some("Davis, Eve".to_string()));
    }

    #[test]
    fn extract_from_headers_updates_existing_contact() {
        let store = test_store();

        // First email — Alice with no display name
        store
            .extract_contacts_from_headers("alice@a.com", None, None, 1000)
            .unwrap();

        let alice = store.get_contact("alice@a.com").unwrap().unwrap();
        assert_eq!(alice.display_name, None);
        assert_eq!(alice.avatar_letter, Some("A".to_string())); // from address

        // Second email — Alice now has a display name
        store
            .extract_contacts_from_headers("Alice Smith <alice@a.com>", None, None, 2000)
            .unwrap();

        let alice = store.get_contact("alice@a.com").unwrap().unwrap();
        assert_eq!(alice.display_name, Some("Alice Smith".to_string()));
        assert_eq!(alice.last_seen, 2000);
    }

    // --- backfill tests ---

    fn insert_test_emails(store: &Store) {
        store
            .conn()
            .execute_batch(
                "INSERT INTO accounts (id, email, display_name, provider, auth_method,
                imap_host, imap_port, smtp_host, smtp_port)
             VALUES ('acct1', 'test@test.com', 'Test', 'other', 'password',
                'imap.test.com', 993, 'smtp.test.com', 587);

             INSERT INTO emails (id, account_id, thread_id, from_name, from_address,
                to_json, cc_json, subject, snippet, date, maildir_path, imap_uid, imap_folder)
             VALUES
                ('msg1', 'acct1', 't1', 'Alice Smith', 'alice@a.com',
                 '[{\"name\": \"Bob\", \"address\": \"bob@b.com\"}]',
                 '[]', 'Hello', 'Hi there', 1000, '/tmp/msg1', 1, 'INBOX'),
                ('msg2', 'acct1', 't1', NULL, 'charlie@c.com',
                 '[{\"address\": \"alice@a.com\"}]',
                 '[{\"name\": \"Dave\", \"address\": \"dave@d.com\"}]',
                 'Re: Hello', 'Reply', 2000, '/tmp/msg2', 2, 'INBOX'),
                ('msg3', 'acct1', 't1', 'Alice S.', 'alice@a.com',
                 '[]', '[]', 'Re: Re: Hello', 'Another', 3000, '/tmp/msg3', 3, 'INBOX');",
            )
            .unwrap();
    }

    #[test]
    fn backfill_extracts_all_contacts() {
        let store = test_store();
        insert_test_emails(&store);

        let count = store.backfill_contacts_from_emails().unwrap();
        assert!(count > 0);

        // Should have 4 unique contacts: alice, bob, charlie, dave
        let all = store.list_all_contacts().unwrap();
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn backfill_resolves_display_name_from_most_recent() {
        let store = test_store();
        insert_test_emails(&store);
        store.backfill_contacts_from_emails().unwrap();

        // Alice appears in msg1 (date=1000, name="Alice Smith"),
        // msg2 To (date=2000, no name), msg3 (date=3000, name="Alice S.")
        // Most recent with a name is "Alice S." at date 3000
        let alice = store.get_contact("alice@a.com").unwrap().unwrap();
        assert_eq!(alice.display_name, Some("Alice S.".to_string()));
        assert_eq!(alice.last_seen, 3000);
    }

    #[test]
    fn backfill_is_idempotent() {
        let store = test_store();
        insert_test_emails(&store);

        store.backfill_contacts_from_emails().unwrap();
        let first_run = store.list_all_contacts().unwrap();

        store.backfill_contacts_from_emails().unwrap();
        let second_run = store.list_all_contacts().unwrap();

        assert_eq!(first_run.len(), second_run.len());
        for (a, b) in first_run.iter().zip(second_run.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn upsert_contacts_from_json_handles_invalid_json() {
        let store = test_store();
        // Should not panic or error
        let count = store.upsert_contacts_from_json("not json", 1000).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn upsert_contacts_from_json_skips_entries_without_address() {
        let store = test_store();
        let json = r#"[{"name": "Ghost"}, {"address": "valid@x.com"}]"#;
        let count = store.upsert_contacts_from_json(json, 1000).unwrap();
        assert_eq!(count, 1);
    }
}
