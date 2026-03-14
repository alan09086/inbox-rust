use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique identifier for an email account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub Uuid);

impl AccountId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AccountId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Email identifier — corresponds to the Message-ID header.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EmailId(pub String);

impl EmailId {
    pub fn new(message_id: impl Into<String>) -> Self {
        Self(message_id.into())
    }
}

impl fmt::Display for EmailId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Locally generated thread identifier — groups emails by References/In-Reply-To.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadId(pub Uuid);

impl ThreadId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ThreadId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ThreadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a bundle (category grouping).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BundleId(pub Uuid);

impl BundleId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for BundleId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for BundleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_id_unique() {
        let a = AccountId::new();
        let b = AccountId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn email_id_from_string() {
        let id = EmailId::new("<abc@example.com>");
        assert_eq!(id.0, "<abc@example.com>");
    }

    #[test]
    fn thread_id_unique() {
        let a = ThreadId::new();
        let b = ThreadId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn bundle_id_unique() {
        let a = BundleId::new();
        let b = BundleId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn id_display() {
        let account = AccountId::new();
        let display = format!("{account}");
        assert!(!display.is_empty());

        let email = EmailId::new("test@example.com");
        assert_eq!(format!("{email}"), "test@example.com");
    }

    #[test]
    fn id_serde_roundtrip() {
        let id = AccountId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: AccountId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);

        let eid = EmailId::new("<test@mail.com>");
        let json = serde_json::to_string(&eid).unwrap();
        let back: EmailId = serde_json::from_str(&json).unwrap();
        assert_eq!(eid, back);
    }
}
