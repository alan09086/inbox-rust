use serde::{Deserialize, Serialize};

/// Email status flags, matching IMAP flag semantics.
/// Stored as a bitmask in SQLite for efficient querying.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EmailFlags {
    /// Message has been read (\Seen).
    pub read: bool,
    /// Message is starred/flagged (\Flagged).
    pub starred: bool,
    /// Message has been replied to (\Answered).
    pub answered: bool,
    /// Message is a draft (\Draft).
    pub draft: bool,
}

impl EmailFlags {
    /// All flags unset (new unread message).
    pub fn new() -> Self {
        Self::default()
    }

    /// Convert to bitmask for SQLite storage.
    /// Bit 0 = read, Bit 1 = starred, Bit 2 = answered, Bit 3 = draft.
    pub fn to_bitmask(self) -> u32 {
        let mut mask = 0u32;
        if self.read {
            mask |= 1;
        }
        if self.starred {
            mask |= 2;
        }
        if self.answered {
            mask |= 4;
        }
        if self.draft {
            mask |= 8;
        }
        mask
    }

    /// Construct from SQLite bitmask.
    pub fn from_bitmask(mask: u32) -> Self {
        Self {
            read: mask & 1 != 0,
            starred: mask & 2 != 0,
            answered: mask & 4 != 0,
            draft: mask & 8 != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_flags_all_false() {
        let flags = EmailFlags::new();
        assert!(!flags.read);
        assert!(!flags.starred);
        assert!(!flags.answered);
        assert!(!flags.draft);
    }

    #[test]
    fn bitmask_roundtrip() {
        let flags = EmailFlags {
            read: true,
            starred: false,
            answered: true,
            draft: false,
        };
        let mask = flags.to_bitmask();
        assert_eq!(mask, 0b0101); // read=1, answered=4
        let back = EmailFlags::from_bitmask(mask);
        assert_eq!(flags, back);
    }

    #[test]
    fn bitmask_all_set() {
        let flags = EmailFlags {
            read: true,
            starred: true,
            answered: true,
            draft: true,
        };
        assert_eq!(flags.to_bitmask(), 0b1111);
    }

    #[test]
    fn bitmask_none_set() {
        let flags = EmailFlags::new();
        assert_eq!(flags.to_bitmask(), 0);
        assert_eq!(EmailFlags::from_bitmask(0), flags);
    }
}
