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
//! The [`Contact`] struct is a lightweight address+name pair used throughout
//! the application. For database persistence with avatar metadata, see
//! `ContactRow` in `inboxly-store`.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Avatar colour palette (BigTop APK)
// ---------------------------------------------------------------------------

/// RGB colour for avatar backgrounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AvatarColor {
    /// Red channel (0-255).
    pub r: u8,
    /// Green channel (0-255).
    pub g: u8,
    /// Blue channel (0-255).
    pub b: u8,
}

impl AvatarColor {
    /// Create a new `AvatarColor` from RGB components.
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

// ---------------------------------------------------------------------------
// Contact (lightweight address + name pair)
// ---------------------------------------------------------------------------

/// An email address with optional display name.
///
/// This is the lightweight in-memory representation used throughout the
/// application (in `EmailMeta`, `Thread`, etc.). For database-backed contact
/// records with avatar metadata and timestamps, see `ContactRow` in
/// `inboxly-store`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Contact {
    /// Display name (e.g., "Alan Gaudet"). May be empty.
    pub name: String,
    /// Email address (e.g., "alan@example.com").
    pub address: String,
}

impl Contact {
    /// Create a new `Contact`.
    pub fn new(name: impl Into<String>, address: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            address: address.into(),
        }
    }

    /// Returns the first letter of the display name (for avatar tiles),
    /// falling back to the first letter of the address.
    pub fn avatar_letter(&self) -> char {
        self.name
            .chars()
            .next()
            .or_else(|| self.address.chars().next())
            .unwrap_or('?')
            .to_ascii_uppercase()
    }

    /// Returns the `AvatarColor` for this contact based on its avatar letter.
    pub fn avatar_color(&self) -> AvatarColor {
        avatar_color_for_letter(self.avatar_letter())
    }
}

// ---------------------------------------------------------------------------
// RFC 2822 address parsing
// ---------------------------------------------------------------------------

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

        let address = raw
            .get(angle_start + 1..angle_end)?
            .trim()
            .to_lowercase();
        if address.is_empty() || !address.contains('@') {
            return None;
        }

        let name_part = raw.get(..angle_start)?.trim();
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Existing Contact tests (preserved) ---

    #[test]
    fn contact_avatar_letter_from_name() {
        let c = Contact::new("Sarah", "sarah@example.com");
        assert_eq!(c.avatar_letter(), 'S');
    }

    #[test]
    fn contact_avatar_letter_fallback_to_address() {
        let c = Contact::new("", "bob@example.com");
        assert_eq!(c.avatar_letter(), 'B');
    }

    #[test]
    fn contact_serde_roundtrip() {
        let c = Contact::new("Test User", "test@mail.com");
        let json = serde_json::to_string(&c).unwrap();
        let back: Contact = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    // --- Avatar palette tests ---

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

    // --- Contact avatar_color integration ---

    #[test]
    fn contact_avatar_color_from_name() {
        let c = Contact::new("Alice", "alice@example.com");
        assert_eq!(c.avatar_color(), AvatarColor::new(0xe0, 0x60, 0x55)); // A
    }

    #[test]
    fn contact_avatar_color_from_address() {
        let c = Contact::new("", "bob@example.com");
        assert_eq!(c.avatar_color(), AvatarColor::new(0xed, 0x61, 0x92)); // B
    }

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
        let list =
            parse_address_list("Alice <alice@a.com>, Bob <bob@b.com>, charlie@c.com");
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].address, "alice@a.com");
        assert_eq!(list[1].address, "bob@b.com");
        assert_eq!(list[2].address, "charlie@c.com");
    }

    #[test]
    fn parse_address_list_with_quoted_commas() {
        let list = parse_address_list("\"Last, First\" <a@b.com>, Other <c@d.com>");
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
}
