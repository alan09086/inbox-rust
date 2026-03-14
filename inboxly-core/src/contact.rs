use serde::{Deserialize, Serialize};

/// An email address with optional display name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Contact {
    /// Display name (e.g., "Alan Gaudet"). May be empty.
    pub name: String,
    /// Email address (e.g., "alan@example.com").
    pub address: String,
}

impl Contact {
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
