//! Typography constants from the BigTop APK.
//!
//! Sizes are in logical pixels (sp from APK = 1:1 on desktop).
//! Accessibility font scaling is handled by the window manager on desktop,
//! not by the app (Android's sp scaling is not applicable here).
//!
//! Spec reference: Theme System > Typography table.

/// Font weight for CSS rendering.
///
/// Replaces `iced::font::Weight` — maps to CSS `font-weight` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    /// Normal weight (CSS 400).
    Normal,
    /// Medium weight (CSS 500).
    Medium,
    /// Bold weight (CSS 700).
    Bold,
}

impl Weight {
    /// CSS `font-weight` numeric value.
    pub const fn css_value(&self) -> u16 {
        match self {
            Self::Normal => 400,
            Self::Medium => 500,
            Self::Bold => 700,
        }
    }
}

/// Toolbar title text.
pub const TOOLBAR_TITLE_SIZE: f32 = 20.0;
/// Toolbar title weight.
pub const TOOLBAR_TITLE_WEIGHT: Weight = Weight::Normal;

/// Email title / sender name in list view.
pub const EMAIL_TITLE_SIZE: f32 = 16.0;
/// Email title weight (read messages).
pub const EMAIL_TITLE_WEIGHT: Weight = Weight::Normal;
/// Bold variant for unread emails.
pub const EMAIL_TITLE_WEIGHT_UNREAD: Weight = Weight::Bold;

/// Author name in conversation view.
pub const AUTHOR_NAME_SIZE: f32 = 14.0;
/// Author name weight.
pub const AUTHOR_NAME_WEIGHT: Weight = Weight::Normal;

/// Snippet/preview text in list view.
pub const SNIPPET_SIZE: f32 = 14.0;
/// Snippet weight.
pub const SNIPPET_WEIGHT: Weight = Weight::Normal;

/// Timestamp text.
pub const TIMESTAMP_SIZE: f32 = 12.0;
/// Timestamp weight.
pub const TIMESTAMP_WEIGHT: Weight = Weight::Normal;

/// Section header text (Today, This Month, etc.).
pub const SECTION_HEADER_SIZE: f32 = 14.0;
/// Section header weight.
pub const SECTION_HEADER_WEIGHT: Weight = Weight::Bold;

/// Unread count badge text.
pub const BADGE_SIZE: f32 = 16.0;
/// Badge weight.
pub const BADGE_WEIGHT: Weight = Weight::Bold;

/// Nav drawer item text.
pub const NAV_ITEM_SIZE: f32 = 14.0;
/// Nav drawer item weight.
pub const NAV_ITEM_WEIGHT: Weight = Weight::Medium;

/// Compose view subject line.
pub const COMPOSE_SUBJECT_SIZE: f32 = 18.0;
/// Compose subject weight.
pub const COMPOSE_SUBJECT_WEIGHT: Weight = Weight::Bold;

/// Compose view body text.
pub const COMPOSE_BODY_SIZE: f32 = 16.0;
/// Compose body weight.
pub const COMPOSE_BODY_WEIGHT: Weight = Weight::Normal;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_title_is_20sp() {
        assert_eq!(TOOLBAR_TITLE_SIZE, 20.0);
    }

    #[test]
    fn email_title_is_16sp() {
        assert_eq!(EMAIL_TITLE_SIZE, 16.0);
    }

    #[test]
    fn snippet_is_14sp() {
        assert_eq!(SNIPPET_SIZE, 14.0);
    }

    #[test]
    fn timestamp_is_12sp() {
        assert_eq!(TIMESTAMP_SIZE, 12.0);
    }

    #[test]
    fn section_header_is_bold() {
        assert!(matches!(SECTION_HEADER_WEIGHT, Weight::Bold));
    }

    #[test]
    fn unread_title_is_bold() {
        assert!(matches!(EMAIL_TITLE_WEIGHT_UNREAD, Weight::Bold));
    }

    #[test]
    fn read_title_is_normal() {
        assert!(matches!(EMAIL_TITLE_WEIGHT, Weight::Normal));
    }

    #[test]
    fn nav_item_is_medium_weight() {
        assert!(matches!(NAV_ITEM_WEIGHT, Weight::Medium));
    }

    #[test]
    fn compose_subject_is_18sp_bold() {
        assert_eq!(COMPOSE_SUBJECT_SIZE, 18.0);
        assert!(matches!(COMPOSE_SUBJECT_WEIGHT, Weight::Bold));
    }
}
