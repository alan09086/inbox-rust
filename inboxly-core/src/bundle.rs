use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::{BundleId, ThreadId};

/// Bundle category — determines default colour, icon, and heuristic rules.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BundleCategory {
    Social,
    Promos,
    Updates,
    Finance,
    Purchases,
    Travel,
    Forums,
    LowPriority,
    /// User-saved items (pinned/kept).
    Saved,
    /// User-defined custom category.
    Custom(String),
}

impl BundleCategory {
    /// Stable string key for storage and settings (lowercase, underscore-separated).
    ///
    /// Use this for database keys and configuration, not display.
    /// For display purposes, use [`label()`](Self::label).
    pub fn as_str(&self) -> &str {
        match self {
            Self::Social => "social",
            Self::Promos => "promos",
            Self::Updates => "updates",
            Self::Finance => "finance",
            Self::Purchases => "purchases",
            Self::Travel => "travel",
            Self::Forums => "forums",
            Self::LowPriority => "low_priority",
            Self::Saved => "saved",
            Self::Custom(name) => name.as_str(),
        }
    }

    /// Human-readable label for display.
    pub fn label(&self) -> &str {
        match self {
            Self::Social => "Social",
            Self::Promos => "Promos",
            Self::Updates => "Updates",
            Self::Finance => "Finance",
            Self::Purchases => "Purchases",
            Self::Travel => "Travel",
            Self::Forums => "Forums",
            Self::LowPriority => "Low Priority",
            Self::Saved => "Saved",
            Self::Custom(name) => name.as_str(),
        }
    }
}

/// Controls how a bundle appears in the inbox feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BundleVisibility {
    /// Emails grouped and shown as collapsed bundle in inbox.
    Bundled,
    /// Emails shown individually in inbox (not grouped).
    Unbundled,
    /// Emails skip the inbox entirely (only in bundle view).
    SkipInbox,
}

// BundleThrottle is now defined in crate::throttle (M14).
pub use crate::throttle::BundleThrottle;

/// Icon identifier for bundle display.
/// Uses named icons rather than embedding actual icon data.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BundleIcon {
    Social,
    Promos,
    Updates,
    Finance,
    Purchases,
    Travel,
    Forums,
    LowPriority,
    Saved,
    /// Custom icon name for user-defined bundles.
    Custom(String),
}

/// RGBA colour representation for bundle title and badge colours.
/// Stored as `[r, g, b, a]` with each component in 0.0..=1.0 range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Create a colour from RGB hex (e.g., 0xd23f31).
    pub fn from_rgb_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as f32 / 255.0,
            g: ((hex >> 8) & 0xFF) as f32 / 255.0,
            b: (hex & 0xFF) as f32 / 255.0,
            a: 1.0,
        }
    }
}

/// A group of related threads displayed as a collapsible unit in the inbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bundle {
    /// Unique bundle identifier.
    pub id: BundleId,
    /// Category (Social, Promos, etc.).
    pub category: BundleCategory,
    /// Display name.
    pub name: String,
    /// Title colour from BigTop palette.
    pub color: Color,
    /// Pastel badge background colour.
    pub badge_color: Color,
    /// Icon for bundle row.
    pub icon: BundleIcon,
    /// Thread IDs contained in this bundle.
    pub threads: Vec<ThreadId>,
    /// Number of unread threads.
    pub unread_count: u32,
    /// Timestamp of the newest thread in the bundle.
    pub newest_date: DateTime<Utc>,
    /// How this bundle appears in the inbox.
    pub visibility: BundleVisibility,
    /// Delivery frequency.
    pub throttle: BundleThrottle,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_category_labels() {
        assert_eq!(BundleCategory::Social.label(), "Social");
        assert_eq!(BundleCategory::LowPriority.label(), "Low Priority");
        assert_eq!(BundleCategory::Custom("Work".into()).label(), "Work");
    }

    #[test]
    fn color_from_hex() {
        let red = Color::from_rgb_hex(0xFF0000);
        assert!((red.r - 1.0).abs() < f32::EPSILON);
        assert!(red.g.abs() < f32::EPSILON);
        assert!(red.b.abs() < f32::EPSILON);

        let social = Color::from_rgb_hex(0xd23f31);
        assert!((social.r - 0.8235294).abs() < 0.001);
    }

    #[test]
    fn bundle_creation() {
        let bundle = Bundle {
            id: BundleId::new(),
            category: BundleCategory::Social,
            name: "Social".into(),
            color: Color::from_rgb_hex(0xd23f31),
            badge_color: Color::from_rgb_hex(0xfaebea),
            icon: BundleIcon::Social,
            threads: vec![ThreadId::new(), ThreadId::new()],
            unread_count: 2,
            newest_date: Utc::now(),
            visibility: BundleVisibility::Bundled,
            throttle: BundleThrottle::Immediate,
        };
        assert_eq!(bundle.threads.len(), 2);
        assert_eq!(bundle.category.label(), "Social");
    }

    #[test]
    fn bundle_serde_roundtrip() {
        let bundle = Bundle {
            id: BundleId::new(),
            category: BundleCategory::Purchases,
            name: "Purchases".into(),
            color: Color::from_rgb_hex(0x6d4c41),
            badge_color: Color::from_rgb_hex(0xf0edec),
            icon: BundleIcon::Purchases,
            threads: vec![],
            unread_count: 0,
            newest_date: Utc::now(),
            visibility: BundleVisibility::Bundled,
            throttle: BundleThrottle::Daily {
                delivery_time: chrono::NaiveTime::from_hms_opt(17, 0, 0).expect("valid time"),
            },
        };
        let json = serde_json::to_string(&bundle).unwrap();
        let back: Bundle = serde_json::from_str(&json).unwrap();
        assert_eq!(bundle.id, back.id);
        assert_eq!(bundle.category, back.category);
    }
}
