# M11: Contacts + Avatar System — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Extract contacts from email headers and assign consistent avatar letters and colours.

**Architecture:** Contact extraction runs as a hook in the email ingest pipeline. The 26-colour palette from Google's BigTop APK provides consistent visual identity per sender. Contacts are deduplicated by email address (case-insensitive), with display names resolved from the most recent occurrence.

**Tech Stack:** Rust, rusqlite, inboxly-core

**Prerequisites:** M3 (SQLite store with contacts table), M7 (emails populated in the database)

**Crates touched:** `inboxly-core` (types), `inboxly-store` (extraction logic + DB operations)

---

## Context for Implementer

### Crate: `inboxly-core`

**New file you'll create:**
- `src/contact.rs` — `Contact` struct, `AvatarColor` palette, display name utilities

**Existing files you'll modify:**
- `src/lib.rs` — add `pub mod contact;`

### Crate: `inboxly-store`

**New file you'll create:**
- `src/contacts.rs` — `ContactStore` with all DB operations and extraction logic

**Existing files you'll modify:**
- `src/lib.rs` — add `pub mod contacts;`, wire contact extraction into email ingest
- `src/email.rs` (or equivalent ingest path) — call contact extraction after email insert

### SQLite Schema (from M3)

The `contacts` table already exists from M3:

```sql
CREATE TABLE contacts (
    address       TEXT PRIMARY KEY,  -- lowercase email address
    display_name  TEXT,
    avatar_letter TEXT NOT NULL,     -- single uppercase character
    avatar_color_index INTEGER NOT NULL,  -- 0-25 index into palette
    last_seen     INTEGER NOT NULL   -- unix epoch
);
```

### Avatar Letter Tile Colours (A-Z) — BigTop APK

```
A=#e06055  B=#ed6192  C=#ba68c8  D=#9575cd  E=#7986cb  F=#5e97f6  G=#4fc3f7
H=#58d0e1  I=#4fb6ac  J=#57bb8a  K=#9ccc65  L=#d4e157  M=#fdd835  N=#f6bf32
O=#f5a631  P=#f18864  Q=#c2c2c2  R=#90a4ae  S=#a1887f  T=#a3a3a3  U=#afb6e0
V=#b39ddb  W=#c2c2c2  X=#80deea  Y=#bcaaa4  Z=#aed581  default=#efefef
```

### Email Headers Used

Contact data comes from three RFC 2822 header fields:
- `From:` — single address (the sender)
- `To:` — comma-separated list of recipients
- `Cc:` — comma-separated list of carbon copy recipients

Each can contain a display name and email address:
- `"Alan Gaudet" <alan@example.com>` — display name + address
- `alan@example.com` — bare address (no display name)
- `<alan@example.com>` — angle-bracket address (no display name)

### Build & Test

```bash
cd /mnt/TempNVME/projects/inbox-rust
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

---

## Task 1: Define the Avatar Colour Palette in `inboxly-core`

**Files:**
- Create: `inboxly-core/src/contact.rs`
- Modify: `inboxly-core/src/lib.rs` (add module declaration)

### Step 1: Add module declaration

In `inboxly-core/src/lib.rs`, add:

```rust
pub mod contact;
```

### Step 2: Define the `AvatarColor` type and the 26-colour palette

Create `inboxly-core/src/contact.rs`:

```rust
//! Contact types and avatar colour palette.
//!
//! The 26-colour palette is extracted from Google's BigTop APK (Inbox by Google).
//! Each letter A-Z maps to a unique colour for consistent sender identification.

/// RGB colour for avatar backgrounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AvatarColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl AvatarColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Returns the colour as a CSS hex string (e.g., "#e06055").
    pub fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// The 26-colour BigTop palette, indexed A=0 through Z=25.
pub const AVATAR_PALETTE: [AvatarColor; 26] = [
    AvatarColor::new(0xe0, 0x60, 0x55), // A - #e06055
    AvatarColor::new(0xed, 0x61, 0x92), // B - #ed6192
    AvatarColor::new(0xba, 0x68, 0xc8), // C - #ba68c8
    AvatarColor::new(0x95, 0x75, 0xcd), // D - #9575cd
    AvatarColor::new(0x79, 0x86, 0xcb), // E - #7986cb
    AvatarColor::new(0x5e, 0x97, 0xf6), // F - #5e97f6
    AvatarColor::new(0x4f, 0xc3, 0xf7), // G - #4fc3f7
    AvatarColor::new(0x58, 0xd0, 0xe1), // H - #58d0e1
    AvatarColor::new(0x4f, 0xb6, 0xac), // I - #4fb6ac
    AvatarColor::new(0x57, 0xbb, 0x8a), // J - #57bb8a
    AvatarColor::new(0x9c, 0xcc, 0x65), // K - #9ccc65
    AvatarColor::new(0xd4, 0xe1, 0x57), // L - #d4e157
    AvatarColor::new(0xfd, 0xd8, 0x35), // M - #fdd835
    AvatarColor::new(0xf6, 0xbf, 0x32), // N - #f6bf32
    AvatarColor::new(0xf5, 0xa6, 0x31), // O - #f5a631
    AvatarColor::new(0xf1, 0x88, 0x64), // P - #f18864
    AvatarColor::new(0xc2, 0xc2, 0xc2), // Q - #c2c2c2
    AvatarColor::new(0x90, 0xa4, 0xae), // R - #90a4ae
    AvatarColor::new(0xa1, 0x88, 0x7f), // S - #a1887f
    AvatarColor::new(0xa3, 0xa3, 0xa3), // T - #a3a3a3
    AvatarColor::new(0xaf, 0xb6, 0xe0), // U - #afb6e0
    AvatarColor::new(0xb3, 0x9d, 0xdb), // V - #b39ddb
    AvatarColor::new(0xc2, 0xc2, 0xc2), // W - #c2c2c2
    AvatarColor::new(0x80, 0xde, 0xea), // X - #80deea
    AvatarColor::new(0xbc, 0xaa, 0xa4), // Y - #bcaaa4
    AvatarColor::new(0xae, 0xd5, 0x81), // Z - #aed581
];

/// Default colour for contacts whose name starts with a non-letter character.
pub const AVATAR_COLOR_DEFAULT: AvatarColor = AvatarColor::new(0xef, 0xef, 0xef);

/// Returns the palette index (0-25) for a given letter, or `None` for non-ASCII-alpha.
pub fn avatar_color_index(letter: char) -> Option<usize> {
    let upper = letter.to_ascii_uppercase();
    if upper.is_ascii_uppercase() {
        Some((upper as u8 - b'A') as usize)
    } else {
        None
    }
}

/// Returns the `AvatarColor` for a given letter (A-Z), or the default colour.
pub fn avatar_color_for_letter(letter: char) -> AvatarColor {
    match avatar_color_index(letter) {
        Some(idx) => AVATAR_PALETTE[idx],
        None => AVATAR_COLOR_DEFAULT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_has_26_entries() {
        assert_eq!(AVATAR_PALETTE.len(), 26);
    }

    #[test]
    fn a_maps_to_correct_colour() {
        let color = avatar_color_for_letter('A');
        assert_eq!(color, AvatarColor::new(0xe0, 0x60, 0x55));
        assert_eq!(color.to_hex(), "#e06055");
    }

    #[test]
    fn z_maps_to_correct_colour() {
        let color = avatar_color_for_letter('Z');
        assert_eq!(color, AvatarColor::new(0xae, 0xd5, 0x81));
        assert_eq!(color.to_hex(), "#aed581");
    }

    #[test]
    fn lowercase_letter_maps_correctly() {
        assert_eq!(avatar_color_for_letter('a'), avatar_color_for_letter('A'));
        assert_eq!(avatar_color_for_letter('m'), avatar_color_for_letter('M'));
    }

    #[test]
    fn non_alpha_returns_default() {
        assert_eq!(avatar_color_for_letter('1'), AVATAR_COLOR_DEFAULT);
        assert_eq!(avatar_color_for_letter('@'), AVATAR_COLOR_DEFAULT);
        assert_eq!(avatar_color_for_letter(' '), AVATAR_COLOR_DEFAULT);
    }

    #[test]
    fn avatar_color_index_bounds() {
        assert_eq!(avatar_color_index('A'), Some(0));
        assert_eq!(avatar_color_index('Z'), Some(25));
        assert_eq!(avatar_color_index('M'), Some(12));
        assert_eq!(avatar_color_index('5'), None);
    }

    #[test]
    fn hex_format_is_lowercase_with_hash() {
        let color = AvatarColor::new(0x00, 0xff, 0x80);
        assert_eq!(color.to_hex(), "#00ff80");
    }
}
```

### Step 3: Verify

```bash
cargo test -p inboxly-core
cargo clippy -p inboxly-core -- -D warnings
```

**Commit:** `feat(core): add avatar colour palette and Contact types (M11)`

---

## Task 2: Define the `Contact` Struct and Display Name Utilities

**Files:**
- Modify: `inboxly-core/src/contact.rs` (add `Contact` struct and name parsing)

### Step 1: Add the `Contact` struct

Add to `inboxly-core/src/contact.rs`, above the tests module:

```rust
/// A resolved contact with display name and avatar assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    /// Lowercase email address (canonical key).
    pub address: String,
    /// Best-known display name. `None` if only the bare address is known.
    pub display_name: Option<String>,
    /// The avatar letter (single uppercase ASCII character, or '#' for non-alpha).
    pub avatar_letter: char,
    /// Index into `AVATAR_PALETTE` (0-25), or `None` for the default colour.
    pub avatar_color_index: Option<usize>,
    /// Unix epoch of the most recent email involving this contact.
    pub last_seen: i64,
}

impl Contact {
    /// Create a new `Contact` from an email address and optional display name.
    ///
    /// - Address is normalised to lowercase.
    /// - Avatar letter is derived from the first character of the display name,
    ///   or the first character of the local part of the address if no name.
    /// - Avatar colour index is derived from the avatar letter.
    pub fn new(address: &str, display_name: Option<&str>, last_seen: i64) -> Self {
        let address = address.trim().to_lowercase();
        let display_name = display_name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty());

        let avatar_letter = derive_avatar_letter(display_name.as_deref(), &address);
        let avatar_color_index = avatar_color_index(avatar_letter);

        Self {
            address,
            display_name,
            avatar_letter,
            avatar_color_index,
            last_seen,
        }
    }

    /// Returns the `AvatarColor` for this contact.
    pub fn avatar_color(&self) -> AvatarColor {
        avatar_color_for_letter(self.avatar_letter)
    }

    /// Returns the best display string: display name if available, otherwise the address.
    pub fn display_string(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.address)
    }
}

/// Derives the avatar letter from a display name or email address.
///
/// Priority:
/// 1. First character of display name (uppercased)
/// 2. First character of the local part of the email address (uppercased)
/// 3. '#' if nothing yields a usable character
fn derive_avatar_letter(display_name: Option<&str>, address: &str) -> char {
    // Try display name first
    if let Some(name) = display_name {
        if let Some(ch) = name.chars().next() {
            if ch.is_ascii_alphabetic() {
                return ch.to_ascii_uppercase();
            }
        }
    }

    // Fall back to local part of email
    let local_part = address.split('@').next().unwrap_or(address);
    if let Some(ch) = local_part.chars().next() {
        if ch.is_ascii_alphabetic() {
            return ch.to_ascii_uppercase();
        }
    }

    // Non-alpha fallback
    '#'
}
```

### Step 2: Add tests for Contact creation and display name logic

Add to the existing `tests` module in the same file:

```rust
    #[test]
    fn contact_from_name_and_address() {
        let c = Contact::new("Alice@Example.com", Some("Alice Smith"), 1700000000);
        assert_eq!(c.address, "alice@example.com");
        assert_eq!(c.display_name, Some("Alice Smith".to_string()));
        assert_eq!(c.avatar_letter, 'A');
        assert_eq!(c.avatar_color_index, Some(0));
        assert_eq!(c.avatar_color(), AvatarColor::new(0xe0, 0x60, 0x55));
    }

    #[test]
    fn contact_from_bare_address() {
        let c = Contact::new("bob@example.com", None, 1700000000);
        assert_eq!(c.display_name, None);
        assert_eq!(c.avatar_letter, 'B');
        assert_eq!(c.avatar_color_index, Some(1));
        assert_eq!(c.display_string(), "bob@example.com");
    }

    #[test]
    fn contact_with_empty_display_name_uses_address() {
        let c = Contact::new("charlie@example.com", Some("  "), 1700000000);
        assert_eq!(c.display_name, None);
        assert_eq!(c.avatar_letter, 'C');
    }

    #[test]
    fn contact_with_numeric_name_falls_back_to_address() {
        let c = Contact::new("dave@example.com", Some("123 Service"), 1700000000);
        // '1' is not alpha, falls back to 'd' from address
        assert_eq!(c.avatar_letter, 'D');
    }

    #[test]
    fn contact_with_numeric_address_and_no_name() {
        let c = Contact::new("123@example.com", None, 1700000000);
        // '1' is not alpha, fallback to '#'
        assert_eq!(c.avatar_letter, '#');
        assert_eq!(c.avatar_color_index, None);
        assert_eq!(c.avatar_color(), AVATAR_COLOR_DEFAULT);
    }

    #[test]
    fn display_string_prefers_name() {
        let c = Contact::new("x@y.com", Some("Xavier"), 0);
        assert_eq!(c.display_string(), "Xavier");
    }

    #[test]
    fn display_string_falls_back_to_address() {
        let c = Contact::new("x@y.com", None, 0);
        assert_eq!(c.display_string(), "x@y.com");
    }

    #[test]
    fn derive_avatar_letter_from_name() {
        assert_eq!(derive_avatar_letter(Some("Zara"), "a@b.com"), 'Z');
    }

    #[test]
    fn derive_avatar_letter_from_address() {
        assert_eq!(derive_avatar_letter(None, "fred@b.com"), 'F');
    }

    #[test]
    fn derive_avatar_letter_non_alpha_fallback() {
        assert_eq!(derive_avatar_letter(Some("42"), "99@b.com"), '#');
    }
```

### Step 3: Verify

```bash
cargo test -p inboxly-core
cargo clippy -p inboxly-core -- -D warnings
```

**Commit:** `feat(core): add Contact struct with avatar letter derivation (M11)`

---

## Task 3: Header Parsing — Extract Contacts from RFC 2822 Address Fields

**Files:**
- Modify: `inboxly-core/src/contact.rs` (add header parsing functions)

### Step 1: Add address parsing functions

These functions parse `From`, `To`, `Cc` header values into `(Option<display_name>, email_address)` tuples. They handle the three common formats:

```rust
/// A raw parsed address from an email header — before Contact resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAddress {
    /// Display name if present (e.g., "Alice Smith").
    pub name: Option<String>,
    /// Email address, lowercased (e.g., "alice@example.com").
    pub address: String,
}

/// Parse a single address field value (e.g., a From header).
///
/// Handles:
/// - `"Display Name" <addr@example.com>`
/// - `Display Name <addr@example.com>`
/// - `<addr@example.com>`
/// - `addr@example.com`
pub fn parse_address(raw: &str) -> Option<ParsedAddress> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    if let Some(angle_start) = raw.rfind('<') {
        let angle_end = raw.rfind('>')?;
        if angle_end <= angle_start {
            return None;
        }

        let address = raw[angle_start + 1..angle_end].trim().to_lowercase();
        if address.is_empty() || !address.contains('@') {
            return None;
        }

        let name_part = raw[..angle_start].trim();
        let name = if name_part.is_empty() {
            None
        } else {
            // Strip surrounding quotes if present
            let unquoted = name_part
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(name_part)
                .trim();
            if unquoted.is_empty() {
                None
            } else {
                Some(unquoted.to_string())
            }
        };

        Some(ParsedAddress { name, address })
    } else if raw.contains('@') {
        // Bare address
        Some(ParsedAddress {
            name: None,
            address: raw.to_lowercase(),
        })
    } else {
        None
    }
}

/// Parse a comma-separated address list (e.g., To or Cc headers).
///
/// Handles quoted display names that may contain commas:
/// `"Last, First" <a@b.com>, Other <c@d.com>`
pub fn parse_address_list(raw: &str) -> Vec<ParsedAddress> {
    let mut results = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut in_angle = false;

    for ch in raw.chars() {
        match ch {
            '"' if !in_angle => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            '<' if !in_quotes => {
                in_angle = true;
                current.push(ch);
            }
            '>' if !in_quotes => {
                in_angle = false;
                current.push(ch);
            }
            ',' if !in_quotes && !in_angle => {
                if let Some(addr) = parse_address(&current) {
                    results.push(addr);
                }
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }

    // Don't forget the last entry
    if let Some(addr) = parse_address(&current) {
        results.push(addr);
    }

    results
}
```

### Step 2: Add tests for header parsing

Add to the tests module:

```rust
    // --- Address parsing tests ---

    #[test]
    fn parse_display_name_and_angle_address() {
        let parsed = parse_address("Alice Smith <alice@example.com>").unwrap();
        assert_eq!(parsed.name, Some("Alice Smith".to_string()));
        assert_eq!(parsed.address, "alice@example.com");
    }

    #[test]
    fn parse_quoted_display_name() {
        let parsed = parse_address("\"Smith, Alice\" <alice@example.com>").unwrap();
        assert_eq!(parsed.name, Some("Smith, Alice".to_string()));
        assert_eq!(parsed.address, "alice@example.com");
    }

    #[test]
    fn parse_angle_only() {
        let parsed = parse_address("<alice@example.com>").unwrap();
        assert_eq!(parsed.name, None);
        assert_eq!(parsed.address, "alice@example.com");
    }

    #[test]
    fn parse_bare_address() {
        let parsed = parse_address("alice@example.com").unwrap();
        assert_eq!(parsed.name, None);
        assert_eq!(parsed.address, "alice@example.com");
    }

    #[test]
    fn parse_address_normalises_case() {
        let parsed = parse_address("Alice@EXAMPLE.COM").unwrap();
        assert_eq!(parsed.address, "alice@example.com");
    }

    #[test]
    fn parse_address_empty_returns_none() {
        assert!(parse_address("").is_none());
        assert!(parse_address("   ").is_none());
    }

    #[test]
    fn parse_address_no_at_sign_returns_none() {
        assert!(parse_address("not-an-email").is_none());
    }

    #[test]
    fn parse_address_list_single() {
        let list = parse_address_list("Bob <bob@example.com>");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].address, "bob@example.com");
    }

    #[test]
    fn parse_address_list_multiple() {
        let list = parse_address_list(
            "Alice <alice@a.com>, Bob <bob@b.com>, charlie@c.com"
        );
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].address, "alice@a.com");
        assert_eq!(list[1].address, "bob@b.com");
        assert_eq!(list[2].address, "charlie@c.com");
    }

    #[test]
    fn parse_address_list_with_quoted_commas() {
        let list = parse_address_list(
            "\"Last, First\" <a@b.com>, Other <c@d.com>"
        );
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, Some("Last, First".to_string()));
        assert_eq!(list[0].address, "a@b.com");
        assert_eq!(list[1].address, "c@d.com");
    }

    #[test]
    fn parse_address_list_empty() {
        let list = parse_address_list("");
        assert!(list.is_empty());
    }
```

### Step 3: Verify

```bash
cargo test -p inboxly-core
cargo clippy -p inboxly-core -- -D warnings
```

**Commit:** `feat(core): add RFC 2822 address header parsing (M11)`

---

## Task 4: Create `ContactStore` with Upsert and Lookup

**Files:**
- Create: `inboxly-store/src/contacts.rs`
- Modify: `inboxly-store/src/lib.rs` (add module declaration)

### Step 1: Add module declaration

In `inboxly-store/src/lib.rs`, add:

```rust
pub mod contacts;
```

### Step 2: Implement `ContactStore`

Create `inboxly-store/src/contacts.rs`:

```rust
//! Contact storage and extraction.
//!
//! Manages the `contacts` table — upserting contacts extracted from email
//! headers, looking up contacts by address, and assigning avatar letters/colours.

use rusqlite::{params, Connection, OptionalExtension};

use inboxly_core::contact::{
    avatar_color_index, Contact, ParsedAddress,
};

/// Provides contact read/write operations against the SQLite `contacts` table.
pub struct ContactStore<'a> {
    conn: &'a Connection,
}

impl<'a> ContactStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Upsert a single contact.
    ///
    /// If the address already exists:
    /// - `display_name` is updated only if the new record is more recent (`last_seen` is newer)
    ///   AND the new display name is not empty.
    /// - `last_seen` is updated to the maximum of old and new.
    /// - `avatar_letter` and `avatar_color_index` are recomputed if the display name changes.
    pub fn upsert(&self, contact: &Contact) -> rusqlite::Result<()> {
        // Check if contact exists
        let existing: Option<(Option<String>, i64)> = self
            .conn
            .query_row(
                "SELECT display_name, last_seen FROM contacts WHERE address = ?1",
                params![contact.address],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        match existing {
            Some((existing_name, existing_last_seen)) => {
                // Merge: prefer newer display name, keep max last_seen
                let new_last_seen = contact.last_seen.max(existing_last_seen);

                let resolved_name = if contact.last_seen >= existing_last_seen
                    && contact.display_name.is_some()
                {
                    // Newer record has a name — use it
                    contact.display_name.as_deref()
                } else {
                    // Keep existing name
                    existing_name.as_deref()
                };

                // Recompute avatar from resolved name
                let resolved_contact =
                    Contact::new(&contact.address, resolved_name, new_last_seen);

                self.conn.execute(
                    "UPDATE contacts SET display_name = ?1, avatar_letter = ?2, \
                     avatar_color_index = ?3, last_seen = ?4 WHERE address = ?5",
                    params![
                        resolved_contact.display_name,
                        resolved_contact.avatar_letter.to_string(),
                        resolved_contact.avatar_color_index.map(|i| i as i64).unwrap_or(-1),
                        resolved_contact.last_seen,
                        resolved_contact.address,
                    ],
                )?;
            }
            None => {
                // Insert new contact
                self.conn.execute(
                    "INSERT INTO contacts (address, display_name, avatar_letter, \
                     avatar_color_index, last_seen) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        contact.address,
                        contact.display_name,
                        contact.avatar_letter.to_string(),
                        contact.avatar_color_index.map(|i| i as i64).unwrap_or(-1),
                        contact.last_seen,
                    ],
                )?;
            }
        }

        Ok(())
    }

    /// Look up a contact by email address (case-insensitive).
    pub fn get_by_address(&self, address: &str) -> rusqlite::Result<Option<Contact>> {
        let address = address.trim().to_lowercase();
        self.conn
            .query_row(
                "SELECT address, display_name, avatar_letter, avatar_color_index, last_seen \
                 FROM contacts WHERE address = ?1",
                params![address],
                |row| {
                    let address: String = row.get(0)?;
                    let display_name: Option<String> = row.get(1)?;
                    let avatar_letter_str: String = row.get(2)?;
                    let avatar_color_idx: i64 = row.get(3)?;
                    let last_seen: i64 = row.get(4)?;

                    let avatar_letter = avatar_letter_str
                        .chars()
                        .next()
                        .unwrap_or('#');
                    let avatar_color_index = if avatar_color_idx >= 0 && avatar_color_idx <= 25 {
                        Some(avatar_color_idx as usize)
                    } else {
                        None
                    };

                    Ok(Contact {
                        address,
                        display_name,
                        avatar_letter,
                        avatar_color_index,
                        last_seen,
                    })
                },
            )
            .optional()
    }

    /// Returns all contacts, ordered by most recently seen first.
    pub fn list_all(&self) -> rusqlite::Result<Vec<Contact>> {
        let mut stmt = self.conn.prepare(
            "SELECT address, display_name, avatar_letter, avatar_color_index, last_seen \
             FROM contacts ORDER BY last_seen DESC",
        )?;

        let contacts = stmt
            .query_map([], |row| {
                let address: String = row.get(0)?;
                let display_name: Option<String> = row.get(1)?;
                let avatar_letter_str: String = row.get(2)?;
                let avatar_color_idx: i64 = row.get(3)?;
                let last_seen: i64 = row.get(4)?;

                let avatar_letter = avatar_letter_str.chars().next().unwrap_or('#');
                let avatar_color_index = if avatar_color_idx >= 0 && avatar_color_idx <= 25 {
                    Some(avatar_color_idx as usize)
                } else {
                    None
                };

                Ok(Contact {
                    address,
                    display_name,
                    avatar_letter,
                    avatar_color_index,
                    last_seen,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(contacts)
    }

    /// Upsert multiple contacts from parsed addresses (batch operation).
    ///
    /// `email_date` is the unix epoch of the email these addresses came from.
    pub fn upsert_from_parsed(
        &self,
        addresses: &[ParsedAddress],
        email_date: i64,
    ) -> rusqlite::Result<()> {
        for parsed in addresses {
            let contact = Contact::new(
                &parsed.address,
                parsed.name.as_deref(),
                email_date,
            );
            self.upsert(&contact)?;
        }
        Ok(())
    }
}
```

### Step 3: Verify

```bash
cargo test -p inboxly-store
cargo clippy -p inboxly-store -- -D warnings
```

**Commit:** `feat(store): add ContactStore with upsert and lookup (M11)`

---

## Task 5: Add Unit Tests for `ContactStore`

**Files:**
- Modify: `inboxly-store/src/contacts.rs` (add tests module)

### Step 1: Add tests

Add at the bottom of `inboxly-store/src/contacts.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE contacts (
                address TEXT PRIMARY KEY,
                display_name TEXT,
                avatar_letter TEXT NOT NULL,
                avatar_color_index INTEGER NOT NULL,
                last_seen INTEGER NOT NULL
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn insert_and_retrieve_contact() {
        let conn = setup_db();
        let store = ContactStore::new(&conn);

        let contact = Contact::new("alice@example.com", Some("Alice"), 1700000000);
        store.upsert(&contact).unwrap();

        let retrieved = store.get_by_address("alice@example.com").unwrap().unwrap();
        assert_eq!(retrieved.address, "alice@example.com");
        assert_eq!(retrieved.display_name, Some("Alice".to_string()));
        assert_eq!(retrieved.avatar_letter, 'A');
        assert_eq!(retrieved.avatar_color_index, Some(0));
        assert_eq!(retrieved.last_seen, 1700000000);
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let conn = setup_db();
        let store = ContactStore::new(&conn);

        let contact = Contact::new("alice@example.com", Some("Alice"), 1700000000);
        store.upsert(&contact).unwrap();

        let retrieved = store.get_by_address("ALICE@EXAMPLE.COM").unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let conn = setup_db();
        let store = ContactStore::new(&conn);

        assert!(store.get_by_address("nobody@x.com").unwrap().is_none());
    }

    #[test]
    fn upsert_updates_name_when_newer() {
        let conn = setup_db();
        let store = ContactStore::new(&conn);

        // First insert with name
        let c1 = Contact::new("bob@b.com", Some("Bob"), 1000);
        store.upsert(&c1).unwrap();

        // Newer insert with different name
        let c2 = Contact::new("bob@b.com", Some("Robert"), 2000);
        store.upsert(&c2).unwrap();

        let retrieved = store.get_by_address("bob@b.com").unwrap().unwrap();
        assert_eq!(retrieved.display_name, Some("Robert".to_string()));
        assert_eq!(retrieved.last_seen, 2000);
        // Avatar letter now 'R' from "Robert"
        assert_eq!(retrieved.avatar_letter, 'R');
    }

    #[test]
    fn upsert_keeps_name_when_older() {
        let conn = setup_db();
        let store = ContactStore::new(&conn);

        // First insert with recent name
        let c1 = Contact::new("bob@b.com", Some("Robert"), 2000);
        store.upsert(&c1).unwrap();

        // Older insert with different name
        let c2 = Contact::new("bob@b.com", Some("Bob"), 1000);
        store.upsert(&c2).unwrap();

        let retrieved = store.get_by_address("bob@b.com").unwrap().unwrap();
        assert_eq!(retrieved.display_name, Some("Robert".to_string()));
        assert_eq!(retrieved.last_seen, 2000); // keeps max
    }

    #[test]
    fn upsert_does_not_overwrite_name_with_none() {
        let conn = setup_db();
        let store = ContactStore::new(&conn);

        let c1 = Contact::new("x@y.com", Some("Xavier"), 1000);
        store.upsert(&c1).unwrap();

        // Newer but no display name — should keep "Xavier"
        let c2 = Contact::new("x@y.com", None, 2000);
        store.upsert(&c2).unwrap();

        let retrieved = store.get_by_address("x@y.com").unwrap().unwrap();
        assert_eq!(retrieved.display_name, Some("Xavier".to_string()));
        assert_eq!(retrieved.last_seen, 2000);
    }

    #[test]
    fn list_all_ordered_by_last_seen_desc() {
        let conn = setup_db();
        let store = ContactStore::new(&conn);

        store.upsert(&Contact::new("a@x.com", Some("Alpha"), 100)).unwrap();
        store.upsert(&Contact::new("b@x.com", Some("Beta"), 300)).unwrap();
        store.upsert(&Contact::new("c@x.com", Some("Charlie"), 200)).unwrap();

        let all = store.list_all().unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].address, "b@x.com");  // 300
        assert_eq!(all[1].address, "c@x.com");  // 200
        assert_eq!(all[2].address, "a@x.com");  // 100
    }

    #[test]
    fn upsert_from_parsed_batch() {
        let conn = setup_db();
        let store = ContactStore::new(&conn);

        let parsed = vec![
            ParsedAddress {
                name: Some("Alice".to_string()),
                address: "alice@a.com".to_string(),
            },
            ParsedAddress {
                name: None,
                address: "bob@b.com".to_string(),
            },
        ];

        store.upsert_from_parsed(&parsed, 5000).unwrap();

        let alice = store.get_by_address("alice@a.com").unwrap().unwrap();
        assert_eq!(alice.display_name, Some("Alice".to_string()));
        assert_eq!(alice.last_seen, 5000);

        let bob = store.get_by_address("bob@b.com").unwrap().unwrap();
        assert_eq!(bob.display_name, None);
        assert_eq!(bob.avatar_letter, 'B');
    }

    #[test]
    fn contact_with_non_alpha_name_gets_default_colour() {
        let conn = setup_db();
        let store = ContactStore::new(&conn);

        let contact = Contact::new("123@example.com", Some("42 Service"), 1000);
        store.upsert(&contact).unwrap();

        let retrieved = store.get_by_address("123@example.com").unwrap().unwrap();
        assert_eq!(retrieved.avatar_letter, '#');
        assert_eq!(retrieved.avatar_color_index, None);
    }
}
```

### Step 2: Verify

```bash
cargo test -p inboxly-store
cargo clippy -p inboxly-store -- -D warnings
```

**Commit:** `test(store): add ContactStore unit tests (M11)`

---

## Task 6: Email Ingest Hook — Extract Contacts on Email Insert

**Files:**
- Modify: `inboxly-store/src/contacts.rs` (add `extract_contacts_from_email` function)
- Modify: `inboxly-store/src/lib.rs` or email insert path (call extraction after insert)

### Step 1: Add the extraction function

Add to `inboxly-store/src/contacts.rs`:

```rust
use inboxly_core::contact::{parse_address, parse_address_list};

impl<'a> ContactStore<'a> {
    /// Extract and upsert contacts from a single email's headers.
    ///
    /// Parses From, To, and Cc header values, creates `Contact` entries, and
    /// upserts them into the contacts table. This should be called as part of
    /// the email ingest pipeline (after inserting the email into the `emails` table).
    ///
    /// # Arguments
    /// - `from` — the raw `From` header value (single address)
    /// - `to` — the raw `To` header value (comma-separated list), or `None`
    /// - `cc` — the raw `Cc` header value (comma-separated list), or `None`
    /// - `email_date` — unix epoch of the email's Date header
    pub fn extract_from_headers(
        &self,
        from: &str,
        to: Option<&str>,
        cc: Option<&str>,
        email_date: i64,
    ) -> rusqlite::Result<()> {
        // Parse From (single address)
        if let Some(parsed) = parse_address(from) {
            let contact = Contact::new(
                &parsed.address,
                parsed.name.as_deref(),
                email_date,
            );
            self.upsert(&contact)?;
        }

        // Parse To (address list)
        if let Some(to_raw) = to {
            let to_addrs = parse_address_list(to_raw);
            self.upsert_from_parsed(&to_addrs, email_date)?;
        }

        // Parse Cc (address list)
        if let Some(cc_raw) = cc {
            let cc_addrs = parse_address_list(cc_raw);
            self.upsert_from_parsed(&cc_addrs, email_date)?;
        }

        Ok(())
    }
}
```

### Step 2: Wire into the email ingest pipeline

In the store's email insert function (likely `inboxly-store/src/email.rs` or wherever `insert_email()` lives), add a call after each email is inserted:

```rust
// After inserting the email into the `emails` table:
let contact_store = ContactStore::new(&self.conn);
contact_store.extract_from_headers(
    &email.from_header_raw,    // raw From: header value
    email.to_header_raw.as_deref(),
    email.cc_header_raw.as_deref(),
    email.date_epoch,          // unix epoch from Date header
)?;
```

The exact field names depend on how `EmailMeta` stores header values. The spec shows:
- `from_name` + `from_address` (pre-parsed)
- `to_json`, `cc_json` (JSON arrays)

If headers are already parsed into structured fields, reconstruct the raw format or pass the parsed data directly:

```rust
// Alternative if headers are pre-parsed:
let from_contact = Contact::new(
    &email.from_address,
    Some(&email.from_name).filter(|n| !n.is_empty()),
    email.date,
);
contact_store.upsert(&from_contact)?;

// For To/Cc stored as JSON arrays of {name, address} objects:
if let Ok(to_contacts) = serde_json::from_str::<Vec<ContactJson>>(&email.to_json) {
    for tc in &to_contacts {
        let contact = Contact::new(&tc.address, tc.name.as_deref(), email.date);
        contact_store.upsert(&contact)?;
    }
}
```

### Step 3: Add an integration test for the ingest hook

Add to the tests module in `contacts.rs`:

```rust
    #[test]
    fn extract_from_headers_full_email() {
        let conn = setup_db();
        let store = ContactStore::new(&conn);

        store
            .extract_from_headers(
                "Alice Smith <alice@example.com>",
                Some("Bob <bob@b.com>, charlie@c.com"),
                Some("\"Davis, Eve\" <eve@d.com>"),
                1700000000,
            )
            .unwrap();

        // Should have 4 contacts
        let all = store.list_all().unwrap();
        assert_eq!(all.len(), 4);

        let alice = store.get_by_address("alice@example.com").unwrap().unwrap();
        assert_eq!(alice.display_name, Some("Alice Smith".to_string()));
        assert_eq!(alice.avatar_letter, 'A');

        let bob = store.get_by_address("bob@b.com").unwrap().unwrap();
        assert_eq!(bob.display_name, Some("Bob".to_string()));

        let charlie = store.get_by_address("charlie@c.com").unwrap().unwrap();
        assert_eq!(charlie.display_name, None);
        assert_eq!(charlie.avatar_letter, 'C');

        let eve = store.get_by_address("eve@d.com").unwrap().unwrap();
        assert_eq!(eve.display_name, Some("Davis, Eve".to_string()));
    }

    #[test]
    fn extract_from_headers_updates_existing_contact() {
        let conn = setup_db();
        let store = ContactStore::new(&conn);

        // First email — Alice with no display name
        store
            .extract_from_headers("alice@a.com", None, None, 1000)
            .unwrap();

        let alice = store.get_by_address("alice@a.com").unwrap().unwrap();
        assert_eq!(alice.display_name, None);
        assert_eq!(alice.avatar_letter, 'A'); // from address

        // Second email — Alice now has a display name
        store
            .extract_from_headers("Alice Smith <alice@a.com>", None, None, 2000)
            .unwrap();

        let alice = store.get_by_address("alice@a.com").unwrap().unwrap();
        assert_eq!(alice.display_name, Some("Alice Smith".to_string()));
        assert_eq!(alice.last_seen, 2000);
    }
```

### Step 4: Verify

```bash
cargo test -p inboxly-store
cargo clippy -p inboxly-store -- -D warnings
```

**Commit:** `feat(store): wire contact extraction into email ingest pipeline (M11)`

---

## Task 7: Batch Contact Extraction — Backfill from Existing Emails

**Files:**
- Modify: `inboxly-store/src/contacts.rs` (add batch extraction method)

### Step 1: Add batch extraction method

This method scans all existing emails and extracts contacts. It is used when M11 is applied to a database that already has emails (from M7 initial sync), or for database rebuilds.

Add to `impl ContactStore`:

```rust
    /// Batch-extract contacts from all existing emails in the database.
    ///
    /// Scans the `emails` table and extracts contacts from the `from_name`,
    /// `from_address`, `to_json`, and `cc_json` columns. This is idempotent —
    /// running it multiple times produces the same result due to upsert semantics.
    ///
    /// Returns the number of contacts upserted.
    pub fn backfill_from_emails(&self) -> rusqlite::Result<usize> {
        let mut stmt = self.conn.prepare(
            "SELECT from_name, from_address, to_json, cc_json, date FROM emails",
        )?;

        let mut count = 0;

        let rows = stmt.query_map([], |row| {
            let from_name: Option<String> = row.get(0)?;
            let from_address: String = row.get(1)?;
            let to_json: Option<String> = row.get(2)?;
            let cc_json: Option<String> = row.get(3)?;
            let date: i64 = row.get(4)?;
            Ok((from_name, from_address, to_json, cc_json, date))
        })?;

        for row in rows {
            let (from_name, from_address, to_json, cc_json, date) = row?;

            // Upsert the From contact
            let from_contact = Contact::new(
                &from_address,
                from_name.as_deref(),
                date,
            );
            self.upsert(&from_contact)?;
            count += 1;

            // Parse To contacts from JSON array
            if let Some(ref json) = to_json {
                count += self.upsert_contacts_from_json(json, date)?;
            }

            // Parse Cc contacts from JSON array
            if let Some(ref json) = cc_json {
                count += self.upsert_contacts_from_json(json, date)?;
            }
        }

        Ok(count)
    }

    /// Parse a JSON array of contact objects and upsert them.
    ///
    /// Expected format: `[{"name": "Alice", "address": "a@b.com"}, ...]`
    /// or `[{"address": "a@b.com"}, ...]` (name optional).
    fn upsert_contacts_from_json(
        &self,
        json: &str,
        date: i64,
    ) -> rusqlite::Result<usize> {
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
            let contact = Contact::new(address, name, date);
            self.upsert(&contact)?;
            count += 1;
        }

        Ok(count)
    }
```

### Step 2: Add `serde_json` dependency to `inboxly-store/Cargo.toml`

```toml
[dependencies]
serde_json = "1"
```

### Step 3: Add tests for batch extraction

Add to the tests module:

```rust
    fn setup_db_with_emails() -> Connection {
        let conn = setup_db();
        conn.execute_batch(
            "CREATE TABLE emails (
                id TEXT PRIMARY KEY,
                from_name TEXT,
                from_address TEXT NOT NULL,
                to_json TEXT,
                cc_json TEXT,
                date INTEGER NOT NULL
            );

            INSERT INTO emails VALUES (
                'msg1', 'Alice Smith', 'alice@a.com',
                '[{\"name\": \"Bob\", \"address\": \"bob@b.com\"}]',
                NULL,
                1000
            );
            INSERT INTO emails VALUES (
                'msg2', NULL, 'charlie@c.com',
                '[{\"address\": \"alice@a.com\"}]',
                '[{\"name\": \"Dave\", \"address\": \"dave@d.com\"}]',
                2000
            );
            INSERT INTO emails VALUES (
                'msg3', 'Alice S.', 'alice@a.com',
                NULL, NULL,
                3000
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn backfill_extracts_all_contacts() {
        let conn = setup_db_with_emails();
        let store = ContactStore::new(&conn);

        let count = store.backfill_from_emails().unwrap();
        assert!(count > 0);

        // Should have 4 unique contacts: alice, bob, charlie, dave
        let all = store.list_all().unwrap();
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn backfill_resolves_display_name_from_most_recent() {
        let conn = setup_db_with_emails();
        let store = ContactStore::new(&conn);
        store.backfill_from_emails().unwrap();

        // Alice appears in msg1 (date=1000, name="Alice Smith"),
        // msg2 To (date=2000, no name), msg3 (date=3000, name="Alice S.")
        // Most recent is "Alice S." at date 3000
        let alice = store.get_by_address("alice@a.com").unwrap().unwrap();
        assert_eq!(alice.display_name, Some("Alice S.".to_string()));
        assert_eq!(alice.last_seen, 3000);
    }

    #[test]
    fn backfill_is_idempotent() {
        let conn = setup_db_with_emails();
        let store = ContactStore::new(&conn);

        store.backfill_from_emails().unwrap();
        let first_run = store.list_all().unwrap();

        store.backfill_from_emails().unwrap();
        let second_run = store.list_all().unwrap();

        assert_eq!(first_run.len(), second_run.len());
        for (a, b) in first_run.iter().zip(second_run.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn upsert_contacts_from_json_handles_invalid_json() {
        let conn = setup_db();
        let store = ContactStore::new(&conn);

        // Should not panic or error
        let count = store.upsert_contacts_from_json("not json", 1000).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn upsert_contacts_from_json_skips_entries_without_address() {
        let conn = setup_db();
        let store = ContactStore::new(&conn);

        let json = r#"[{"name": "Ghost"}, {"address": "valid@x.com"}]"#;
        let count = store.upsert_contacts_from_json(json, 1000).unwrap();
        assert_eq!(count, 1);
    }
```

### Step 4: Verify

```bash
cargo test -p inboxly-store
cargo clippy -p inboxly-store -- -D warnings
```

**Commit:** `feat(store): add batch contact extraction from existing emails (M11)`

---

## Task 8: Public API — Re-export and Documentation

**Files:**
- Modify: `inboxly-core/src/lib.rs` (re-export key types)
- Modify: `inboxly-store/src/lib.rs` (re-export ContactStore)

### Step 1: Re-export contact types from `inboxly-core`

In `inboxly-core/src/lib.rs`, add public re-exports so downstream crates can use short paths:

```rust
pub use contact::{
    AvatarColor, Contact, ParsedAddress,
    AVATAR_COLOR_DEFAULT, AVATAR_PALETTE,
    avatar_color_for_letter, avatar_color_index,
    parse_address, parse_address_list,
};
```

### Step 2: Re-export `ContactStore` from `inboxly-store`

In `inboxly-store/src/lib.rs`:

```rust
pub use contacts::ContactStore;
```

### Step 3: Add module-level documentation

Ensure `inboxly-core/src/contact.rs` has a complete module doc comment at the top:

```rust
//! Contact types, avatar colour palette, and RFC 2822 address parsing.
//!
//! # Avatar System
//!
//! Each contact is assigned a single-character avatar letter (A-Z) derived from
//! their display name, and a corresponding colour from the 26-colour BigTop
//! palette. This provides consistent visual identification without profile photos.
//!
//! # Address Parsing
//!
//! Functions [`parse_address`] and [`parse_address_list`] handle the common
//! RFC 2822 address formats found in From, To, and Cc headers.
//!
//! # Contact Resolution
//!
//! When the same email address appears across multiple emails, the display name
//! from the most recent email is preferred. The [`Contact::new`] constructor
//! handles avatar letter and colour derivation automatically.
```

### Step 4: Verify

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo doc -p inboxly-core -p inboxly-store --no-deps
```

**Commit:** `feat: public API re-exports and documentation for contacts (M11)`

---

## Task 9: Integration Test — Full Pipeline

**Files:**
- Create: `inboxly-store/tests/contact_integration.rs`

### Step 1: Write the integration test

This test simulates the full pipeline: creating a database, inserting emails, running contact extraction, and verifying avatar assignments.

```rust
//! Integration test: contact extraction pipeline.

use rusqlite::Connection;

use inboxly_core::contact::{
    avatar_color_for_letter, Contact, AvatarColor, AVATAR_PALETTE,
};
use inboxly_store::ContactStore;

fn setup_full_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE contacts (
            address TEXT PRIMARY KEY,
            display_name TEXT,
            avatar_letter TEXT NOT NULL,
            avatar_color_index INTEGER NOT NULL,
            last_seen INTEGER NOT NULL
        );
        CREATE TABLE emails (
            id TEXT PRIMARY KEY,
            from_name TEXT,
            from_address TEXT NOT NULL,
            to_json TEXT,
            cc_json TEXT,
            date INTEGER NOT NULL
        );",
    )
    .unwrap();
    conn
}

#[test]
fn full_pipeline_extract_and_query() {
    let conn = setup_full_db();

    // Simulate email ingest: 3 emails
    conn.execute_batch(r#"
        INSERT INTO emails VALUES (
            'msg1', 'Sarah Connor', 'sarah@skynet.com',
            '[{"name": "John Connor", "address": "john@resistance.net"}]',
            NULL,
            1000
        );
        INSERT INTO emails VALUES (
            'msg2', 'Kyle Reese', 'kyle@resistance.net',
            '[{"address": "sarah@skynet.com"}]',
            '[{"name": "Sarah C.", "address": "sarah@skynet.com"}]',
            2000
        );
        INSERT INTO emails VALUES (
            'msg3', 'Sarah C.', 'sarah@skynet.com',
            '[{"name": "Kyle Reese", "address": "kyle@resistance.net"}]',
            NULL,
            3000
        );
    "#).unwrap();

    let store = ContactStore::new(&conn);
    store.backfill_from_emails().unwrap();

    // Verify contact count
    let all = store.list_all().unwrap();
    assert_eq!(all.len(), 3); // sarah, john, kyle

    // Sarah: most recent name is "Sarah C." from msg3 (date=3000)
    let sarah = store.get_by_address("sarah@skynet.com").unwrap().unwrap();
    assert_eq!(sarah.display_name, Some("Sarah C.".to_string()));
    assert_eq!(sarah.avatar_letter, 'S');
    assert_eq!(sarah.avatar_color_index, Some(18)); // S = index 18
    assert_eq!(sarah.last_seen, 3000);
    assert_eq!(sarah.avatar_color(), AVATAR_PALETTE[18]); // #a1887f

    // John: only appeared in msg1
    let john = store.get_by_address("john@resistance.net").unwrap().unwrap();
    assert_eq!(john.display_name, Some("John Connor".to_string()));
    assert_eq!(john.avatar_letter, 'J');
    assert_eq!(john.avatar_color_index, Some(9)); // J = index 9

    // Kyle: appeared in msg2 (from) and msg3 (to)
    let kyle = store.get_by_address("kyle@resistance.net").unwrap().unwrap();
    assert_eq!(kyle.display_name, Some("Kyle Reese".to_string()));
    assert_eq!(kyle.avatar_letter, 'K');
}

#[test]
fn every_letter_maps_to_unique_colour() {
    // Verify that A-Z all produce valid palette entries
    for (i, ch) in ('A'..='Z').enumerate() {
        let color = avatar_color_for_letter(ch);
        assert_eq!(color, AVATAR_PALETTE[i], "Mismatch for letter {ch}");
        // Ensure not the default colour (each letter is distinct)
        assert_ne!(
            color,
            AvatarColor::new(0xef, 0xef, 0xef),
            "Letter {ch} should not map to default"
        );
    }
}

#[test]
fn contact_header_extraction_deduplicates() {
    let conn = setup_full_db();
    let store = ContactStore::new(&conn);

    // Same person in From and To of different emails
    store
        .extract_from_headers(
            "Alice <alice@a.com>",
            Some("alice@a.com, bob@b.com"),
            None,
            1000,
        )
        .unwrap();

    let all = store.list_all().unwrap();
    // alice appears twice (from + to) but should only have 1 entry
    assert_eq!(all.len(), 2); // alice + bob

    let alice = store.get_by_address("alice@a.com").unwrap().unwrap();
    // The From header had the name "Alice", the To was bare — should keep "Alice"
    assert_eq!(alice.display_name, Some("Alice".to_string()));
}
```

### Step 2: Verify

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

**Commit:** `test: add contact pipeline integration tests (M11)`

---

## Summary

| Task | Description | Files | Tests |
|------|-------------|-------|-------|
| 1 | Avatar colour palette (26 colours + default) | `inboxly-core/src/contact.rs`, `lib.rs` | 7 |
| 2 | `Contact` struct + display name derivation | `inboxly-core/src/contact.rs` | 10 |
| 3 | RFC 2822 address header parsing | `inboxly-core/src/contact.rs` | 9 |
| 4 | `ContactStore` (upsert, lookup, list) | `inboxly-store/src/contacts.rs`, `lib.rs` | 0 (task 5) |
| 5 | `ContactStore` unit tests | `inboxly-store/src/contacts.rs` | 10 |
| 6 | Email ingest hook + `extract_from_headers` | `inboxly-store/src/contacts.rs`, email ingest path | 2 |
| 7 | Batch backfill from existing emails | `inboxly-store/src/contacts.rs`, `Cargo.toml` | 5 |
| 8 | Public API re-exports + documentation | `inboxly-core/src/lib.rs`, `inboxly-store/src/lib.rs` | 0 |
| 9 | Integration test — full pipeline | `inboxly-store/tests/contact_integration.rs` | 3 |
| **Total** | | | **46** |

### Verification Checklist

After all tasks, confirm:

- [ ] `cargo test --workspace` — all 46+ tests pass
- [ ] `cargo clippy --workspace -- -D warnings` — zero warnings
- [ ] `cargo doc -p inboxly-core -p inboxly-store --no-deps` — docs build clean
- [ ] Contact extraction runs on every email ingest (From/To/Cc)
- [ ] Batch backfill populates contacts from all existing emails
- [ ] Display names resolve to the most recent occurrence
- [ ] Avatar letters are uppercase A-Z (or '#' for non-alpha)
- [ ] All 26 palette colours are correct against the spec
- [ ] Deduplication by lowercase email address works
- [ ] `ContactStore::get_by_address` is case-insensitive
