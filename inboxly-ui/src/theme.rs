//! Theme colours, layout constants, and view definitions for Inboxly.
//!
//! All values derived from the BigTop APK colour palette and the Inbox by
//! Google design spec.

use iced::Color;

/// The three primary views that drive toolbar colour and content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ActiveView {
    #[default]
    Inbox,
    Snoozed,
    Done,
}

impl ActiveView {
    /// Display name shown in the toolbar title.
    pub fn title(&self) -> &'static str {
        match self {
            Self::Inbox => "Inbox",
            Self::Snoozed => "Snoozed",
            Self::Done => "Done",
        }
    }

    /// Toolbar background colour for this view (light theme).
    pub fn toolbar_color(&self) -> Color {
        match self {
            Self::Inbox => color_from_hex(0x42, 0x85, 0xf4), // #4285f4
            Self::Snoozed => color_from_hex(0xef, 0x6c, 0x00), // #ef6c00
            Self::Done => color_from_hex(0x0f, 0x9d, 0x58),  // #0f9d58
        }
    }
}

/// Convert RGB bytes to iced::Color (0.0..1.0 range).
pub fn color_from_hex(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    )
}

// === Layout constants (from BigTop APK, in logical pixels) ===
pub const TOOLBAR_HEIGHT: f32 = 56.0;
pub const NAV_DRAWER_WIDTH: f32 = 264.0;
pub const NAV_ITEM_HEIGHT: f32 = 48.0;
pub const AVATAR_DIAMETER: f32 = 40.0;
pub const DEFAULT_PADDING: f32 = 16.0;
pub const DIVIDER_THICKNESS: f32 = 1.0;

// === Typography (in sp / logical pixels) ===
pub const TOOLBAR_TITLE_SIZE: f32 = 20.0;
pub const NAV_ITEM_SIZE: f32 = 14.0;

// === Theme colours (light) ===
pub fn bg_color() -> Color {
    color_from_hex(0xec, 0xec, 0xec) // #ececec
}

pub fn surface_color() -> Color {
    Color::WHITE
}

pub fn primary_text() -> Color {
    color_from_hex(0x21, 0x21, 0x21) // #212121
}

pub fn secondary_text() -> Color {
    color_from_hex(0x75, 0x75, 0x75) // #757575
}

pub fn divider_color() -> Color {
    color_from_hex(0xe0, 0xe0, 0xe0) // #e0e0e0
}

pub fn selected_bg() -> Color {
    color_from_hex(0xeb, 0xf2, 0xff) // #ebf2ff
}

// === Bundle category colours (title colour from spec) ===

/// Title and badge colour pair for a bundle category.
pub struct CategoryColor {
    pub title: Color,
    pub badge: Color,
}

/// Get the title and badge colours for a bundle category name.
pub fn category_color(category: &str) -> CategoryColor {
    match category {
        "Social" => CategoryColor {
            title: color_from_hex(0xd2, 0x3f, 0x31),
            badge: color_from_hex(0xfa, 0xeb, 0xea),
        },
        "Promos" => CategoryColor {
            title: color_from_hex(0x00, 0xac, 0xc1),
            badge: color_from_hex(0xe5, 0xf6, 0xf9),
        },
        "Updates" => CategoryColor {
            title: color_from_hex(0xf4, 0x51, 0x1e),
            badge: color_from_hex(0xfe, 0xed, 0xe8),
        },
        "Finance" => CategoryColor {
            title: color_from_hex(0x55, 0x8b, 0x2f),
            badge: color_from_hex(0xee, 0xf3, 0xea),
        },
        "Purchases" => CategoryColor {
            title: color_from_hex(0x6d, 0x4c, 0x41),
            badge: color_from_hex(0xf0, 0xed, 0xec),
        },
        "Travel" => CategoryColor {
            title: color_from_hex(0x8e, 0x24, 0xaa),
            badge: color_from_hex(0xf3, 0xe9, 0xf6),
        },
        "Forums" => CategoryColor {
            title: color_from_hex(0x39, 0x49, 0xab),
            badge: color_from_hex(0xeb, 0xec, 0xf6),
        },
        "Low Priority" => CategoryColor {
            title: color_from_hex(0x21, 0x21, 0x21),
            badge: color_from_hex(0xe5, 0xe5, 0xe5),
        },
        _ => CategoryColor {
            title: secondary_text(),
            badge: divider_color(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_from_hex_black() {
        let c = color_from_hex(0, 0, 0);
        assert_eq!(c, Color::BLACK);
    }

    #[test]
    fn color_from_hex_white() {
        let c = color_from_hex(255, 255, 255);
        assert_eq!(c, Color::WHITE);
    }

    #[test]
    fn inbox_toolbar_is_blue() {
        let c = ActiveView::Inbox.toolbar_color();
        assert!((c.r - 66.0 / 255.0).abs() < 0.01);
        assert!((c.g - 133.0 / 255.0).abs() < 0.01);
        assert!((c.b - 244.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn snoozed_toolbar_is_orange() {
        let c = ActiveView::Snoozed.toolbar_color();
        assert!((c.r - 239.0 / 255.0).abs() < 0.01);
        assert!((c.g - 108.0 / 255.0).abs() < 0.01);
        assert!(c.b < 0.01);
    }

    #[test]
    fn done_toolbar_is_green() {
        let c = ActiveView::Done.toolbar_color();
        assert!((c.r - 15.0 / 255.0).abs() < 0.01);
        assert!((c.g - 157.0 / 255.0).abs() < 0.01);
        assert!((c.b - 88.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn social_category_color() {
        let cc = category_color("Social");
        assert!((cc.title.r - f32::from(0xd2_u8) / 255.0).abs() < 0.01);
        assert!((cc.title.g - f32::from(0x3f_u8) / 255.0).abs() < 0.01);
        assert!((cc.title.b - f32::from(0x31_u8) / 255.0).abs() < 0.01);
    }

    #[test]
    fn unknown_category_gets_default() {
        let cc = category_color("UnknownCategory");
        assert_eq!(cc.title, secondary_text());
    }

    #[test]
    fn layout_constants_match_spec() {
        assert!((TOOLBAR_HEIGHT - 56.0).abs() < f32::EPSILON);
        assert!((NAV_DRAWER_WIDTH - 264.0).abs() < f32::EPSILON);
        assert!((NAV_ITEM_HEIGHT - 48.0).abs() < f32::EPSILON);
        assert!((AVATAR_DIAMETER - 40.0).abs() < f32::EPSILON);
        assert!((DEFAULT_PADDING - 16.0).abs() < f32::EPSILON);
        assert!((DIVIDER_THICKNESS - 1.0).abs() < f32::EPSILON);
    }
}
