//! Inboxly theme system -- BigTop design tokens.
//!
//! # Architecture
//!
//! ```text
//! ThemePreference (config)     SystemColorScheme (D-Bus)
//!        \                        /
//!         +--- InboxlyTheme -----+
//!              |           |
//!         ThemeColors    iced::Theme::Custom
//!              |
//!   light() / dark() colour tokens
//! ```
//!
//! # Modules
//!
//! - [`colors`] -- Light/dark colour tokens (`ThemeColors`)
//! - [`bundle_colors`] -- Bundle category colours (constant across themes)
//! - [`avatar_colors`] -- Avatar letter tile A-Z palette (constant across themes)
//! - [`dimensions`] -- Layout dimension constants from BigTop APK
//! - [`typography`] -- Font size and weight constants from BigTop APK
//! - [`system`] -- System theme detection via freedesktop portal D-Bus

pub mod avatar_colors;
pub mod bundle_colors;
pub mod colors;
pub mod dimensions;
pub mod system;
pub mod typography;

pub use colors::ThemeColors;
pub use system::{SystemColorScheme, SystemThemeError, query_system_color_scheme};

use iced::theme::Palette;
use iced::{Color, Theme};

use inboxly_core::config::ThemePreference;

// ============================================================================
// Backward-compatible re-exports from M15
// ============================================================================
//
// These items were originally in the flat `theme.rs` file. They are kept here
// so that `nav.rs`, `toolbar.rs`, and `app.rs` continue to compile without
// changes to their import paths.

/// The three primary views that drive toolbar colour and content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ActiveView {
    #[default]
    Inbox,
    Snoozed,
    Done,
    /// Full-screen settings view. Toolbar turns grey, hamburger becomes back arrow.
    Settings,
}

impl ActiveView {
    /// Display name shown in the toolbar title.
    pub fn title(&self) -> &'static str {
        match self {
            Self::Inbox => "Inbox",
            Self::Snoozed => "Snoozed",
            Self::Done => "Done",
            Self::Settings => "Settings",
        }
    }

    /// Toolbar background colour for this view (light theme).
    pub fn toolbar_color(&self) -> Color {
        match self {
            Self::Inbox => color_from_hex(0x42, 0x85, 0xf4), // #4285f4
            Self::Snoozed => color_from_hex(0xef, 0x6c, 0x00), // #ef6c00
            Self::Done => color_from_hex(0x0f, 0x9d, 0x58),  // #0f9d58
            Self::Settings => color_from_hex(0x45, 0x5a, 0x64),
        }
    }

    /// Toolbar background colour for this view, theme-aware.
    pub fn toolbar_color_themed(&self, theme: &InboxlyTheme) -> Color {
        match self {
            Self::Inbox => theme.colors.toolbar_inbox,
            Self::Snoozed => theme.colors.toolbar_snoozed,
            Self::Done => theme.colors.toolbar_done,
            Self::Settings => theme.colors.toolbar_settings,
        }
    }
}

/// Convert RGB bytes to `iced::Color` (0.0..1.0 range).
pub fn color_from_hex(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    )
}

// Layout constants (re-exported from dimensions for backward compat).
pub use dimensions::{
    AVATAR_DIAMETER, DEFAULT_PADDING, DIVIDER_THICKNESS, NAV_DRAWER_WIDTH, TOOLBAR_HEIGHT,
};

/// Nav drawer item height (alias for backward compat with M15 name).
pub const NAV_ITEM_HEIGHT: f32 = dimensions::NAV_DRAWER_ITEM_HEIGHT;

// Typography (re-exported from typography for backward compat).
pub use typography::{NAV_ITEM_SIZE, TOOLBAR_TITLE_SIZE};

// Light theme colour convenience functions (backward compat with M15).

/// Background colour (light theme).
pub fn bg_color() -> Color {
    color_from_hex(0xec, 0xec, 0xec) // #ececec
}

/// Surface colour (light theme).
pub fn surface_color() -> Color {
    Color::WHITE
}

/// Primary text colour (light theme).
pub fn primary_text() -> Color {
    color_from_hex(0x21, 0x21, 0x21) // #212121
}

/// Secondary text colour (light theme).
pub fn secondary_text() -> Color {
    color_from_hex(0x75, 0x75, 0x75) // #757575
}

/// Divider colour (light theme).
pub fn divider_color() -> Color {
    color_from_hex(0xe0, 0xe0, 0xe0) // #e0e0e0
}

/// Selected item background (light theme).
pub fn selected_bg() -> Color {
    color_from_hex(0xeb, 0xf2, 0xff) // #ebf2ff
}

/// Title and badge colour pair for a bundle category (string-based lookup).
pub struct CategoryColor {
    /// Title text colour.
    pub title: Color,
    /// Badge background colour.
    pub badge: Color,
}

/// Get bundle category colours by name (backward compat with M15).
///
/// Prefer [`bundle_colors::for_category`] for typed lookups.
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

// ============================================================================
// InboxlyTheme -- the app-level theme with Iced integration
// ============================================================================

/// Trait for reading settings -- abstracts over the SQLite store.
///
/// Allows theme resolution without a direct dependency on the concrete
/// store type, enabling testing with mock stores.
pub trait SettingsReader {
    /// Read a setting value by key. Returns `None` if the key doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying store operation fails.
    fn get_setting(&self, key: &str) -> Result<Option<String>, Box<dyn std::error::Error>>;
}

/// Trait for writing settings -- abstracts over the SQLite store.
pub trait SettingsWriter {
    /// Write a setting key-value pair. Upserts (insert or replace).
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying store operation fails.
    fn set_setting(&self, key: &str, value: &str) -> Result<(), Box<dyn std::error::Error>>;
}

/// The main theme struct for the Inboxly application.
///
/// Wraps `ThemeColors` (the BigTop colour tokens) and provides conversion
/// to Iced's `Theme` type for widget styling.
#[derive(Debug, Clone)]
pub struct InboxlyTheme {
    /// The colour tokens for this theme variant.
    pub colors: ThemeColors,
    /// Cached Iced `Theme` for widget styling.
    iced_theme: Theme,
}

impl InboxlyTheme {
    /// Create a theme from the given colour tokens.
    pub fn new(colors: ThemeColors) -> Self {
        let iced_theme = Self::build_iced_theme(&colors);
        Self { colors, iced_theme }
    }

    /// BigTop light theme.
    pub fn light() -> Self {
        Self::new(ThemeColors::light())
    }

    /// BigTop dark theme.
    pub fn dark() -> Self {
        Self::new(ThemeColors::dark())
    }

    /// Get the Iced `Theme` for use in widget styling.
    pub fn iced_theme(&self) -> &Theme {
        &self.iced_theme
    }

    /// Convert to an owned Iced `Theme`.
    pub fn into_iced_theme(self) -> Theme {
        self.iced_theme
    }

    /// Detect system theme preference and return the matching theme.
    ///
    /// Queries `org.freedesktop.portal.Settings` for `color-scheme`.
    /// Falls back to light theme if:
    /// - D-Bus is unavailable
    /// - The portal doesn't support the setting
    /// - The system reports "no preference"
    ///
    /// This is synchronous and safe to call from any context (no Tokio needed).
    pub fn from_system() -> Self {
        match system::query_system_color_scheme() {
            Ok(SystemColorScheme::Dark) => {
                tracing::info!("system theme detected: dark");
                Self::dark()
            }
            Ok(SystemColorScheme::Light | SystemColorScheme::NoPreference) => {
                tracing::info!("system theme detected: light (or no preference)");
                Self::light()
            }
            Err(e) => {
                tracing::warn!("system theme detection failed ({e}), defaulting to light");
                Self::light()
            }
        }
    }

    /// Resolve theme from user preference (blocking).
    ///
    /// - `ThemePreference::Light` -> light theme
    /// - `ThemePreference::Dark` -> dark theme
    /// - `ThemePreference::System` -> queries D-Bus portal (blocking Tokio runtime)
    pub fn from_preference(pref: ThemePreference) -> Self {
        match pref {
            ThemePreference::Light => Self::light(),
            ThemePreference::Dark => Self::dark(),
            ThemePreference::System => Self::from_system(),
        }
    }

    /// Resolve theme from a preference stored in the settings table.
    ///
    /// Reads `"theme"` key from the settings store. If not found or
    /// not parseable, falls back to `ThemePreference::System`.
    pub fn from_settings(settings: &dyn SettingsReader) -> Self {
        let pref = settings
            .get_setting("theme")
            .ok()
            .flatten()
            .and_then(|v| match v.as_str() {
                "light" => Some(ThemePreference::Light),
                "dark" => Some(ThemePreference::Dark),
                "system" => Some(ThemePreference::System),
                _ => None,
            })
            .unwrap_or(ThemePreference::System);

        Self::from_preference(pref)
    }

    /// Toggle between light and dark themes.
    ///
    /// Returns the new theme. Does not persist -- call `save_preference`
    /// afterwards to persist the choice.
    pub fn toggle(&self) -> Self {
        if self.colors.is_dark {
            Self::light()
        } else {
            Self::dark()
        }
    }

    /// Save the current theme preference to the settings store.
    ///
    /// Stores `"light"` or `"dark"` under the `"theme"` key.
    /// When the user explicitly picks a theme, this overrides system detection.
    ///
    /// # Errors
    ///
    /// Returns an error if the settings write fails.
    pub fn save_preference(
        &self,
        settings: &dyn SettingsWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let value = if self.colors.is_dark { "dark" } else { "light" };
        settings.set_setting("theme", value)
    }

    /// Reset to system theme detection (removes manual override).
    ///
    /// # Errors
    ///
    /// Returns an error if the settings write fails.
    pub fn reset_to_system(
        settings: &dyn SettingsWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        settings.set_setting("theme", "system")
    }

    /// Build an Iced `Theme::Custom` from our colour tokens.
    ///
    /// Maps our tokens to Iced's `Palette`:
    /// - `background` -> our `background`
    /// - `text` -> our `text_primary`
    /// - `primary` -> our `toolbar_inbox` (the app's primary accent)
    /// - `success` -> our `toolbar_done` (green for success actions)
    /// - `warning` -> our `toolbar_snoozed` (orange for snooze/warning)
    /// - `danger` -> Social bundle red (used for delete/error)
    fn build_iced_theme(colors: &ThemeColors) -> Theme {
        let palette = Palette {
            background: colors.background,
            text: colors.text_primary,
            primary: colors.toolbar_inbox,
            success: colors.toolbar_done,
            warning: colors.toolbar_snoozed,
            danger: bundle_colors::SOCIAL.title,
        };

        let name = if colors.is_dark {
            "Inboxly Dark"
        } else {
            "Inboxly Light"
        };

        Theme::custom(name.to_owned(), palette)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- M15 backward compat tests (preserved) --

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

    // -- M16 InboxlyTheme tests --

    #[test]
    fn light_theme_creates_successfully() {
        let theme = InboxlyTheme::light();
        assert!(!theme.colors.is_dark);
    }

    #[test]
    fn dark_theme_creates_successfully() {
        let theme = InboxlyTheme::dark();
        assert!(theme.colors.is_dark);
    }

    #[test]
    fn toggle_light_to_dark() {
        let light = InboxlyTheme::light();
        let dark = light.toggle();
        assert!(dark.colors.is_dark);
    }

    #[test]
    fn toggle_dark_to_light() {
        let dark = InboxlyTheme::dark();
        let light = dark.toggle();
        assert!(!light.colors.is_dark);
    }

    #[test]
    fn toggle_is_involution() {
        let original = InboxlyTheme::light();
        let toggled_twice = original.toggle().toggle();
        assert_eq!(original.colors.is_dark, toggled_twice.colors.is_dark);
    }

    #[test]
    fn iced_theme_is_custom_variant() {
        let theme = InboxlyTheme::light();
        let iced = theme.iced_theme();
        let palette = iced.palette();
        let bg = palette.background;
        let expected = ThemeColors::light().background;
        assert!(
            (bg.r - expected.r).abs() < 0.01
                && (bg.g - expected.g).abs() < 0.01
                && (bg.b - expected.b).abs() < 0.01,
            "Iced theme palette background doesn't match light theme"
        );
    }

    #[test]
    fn iced_theme_dark_palette() {
        let theme = InboxlyTheme::dark();
        let palette = theme.iced_theme().palette();
        let bg = palette.background;
        let expected = ThemeColors::dark().background;
        assert!(
            (bg.r - expected.r).abs() < 0.01
                && (bg.g - expected.g).abs() < 0.01
                && (bg.b - expected.b).abs() < 0.01,
            "Iced theme palette background doesn't match dark theme"
        );
    }

    #[test]
    fn iced_theme_primary_is_toolbar_inbox() {
        let theme = InboxlyTheme::light();
        let palette = theme.iced_theme().palette();
        let primary = palette.primary;
        let expected = ThemeColors::light().toolbar_inbox;
        assert!(
            (primary.r - expected.r).abs() < 0.01
                && (primary.g - expected.g).abs() < 0.01
                && (primary.b - expected.b).abs() < 0.01,
        );
    }

    #[test]
    fn themed_toolbar_color_uses_theme_values() {
        let dark = InboxlyTheme::dark();
        let inbox_color = ActiveView::Inbox.toolbar_color_themed(&dark);
        let expected = ThemeColors::dark().toolbar_inbox;
        assert!(
            (inbox_color.r - expected.r).abs() < 0.01
                && (inbox_color.g - expected.g).abs() < 0.01
                && (inbox_color.b - expected.b).abs() < 0.01,
        );
    }

    // -- Mock settings tests --

    struct MockSettings {
        value: Option<String>,
    }

    impl SettingsReader for MockSettings {
        fn get_setting(&self, _key: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
            Ok(self.value.clone())
        }
    }

    impl SettingsWriter for MockSettings {
        fn set_setting(&self, _key: &str, _value: &str) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
    }

    #[test]
    fn from_preference_light() {
        let theme = InboxlyTheme::from_preference(ThemePreference::Light);
        assert!(!theme.colors.is_dark);
    }

    #[test]
    fn from_preference_dark() {
        let theme = InboxlyTheme::from_preference(ThemePreference::Dark);
        assert!(theme.colors.is_dark);
    }

    #[test]
    fn from_settings_dark() {
        let settings = MockSettings {
            value: Some("dark".to_owned()),
        };
        let theme = InboxlyTheme::from_settings(&settings);
        assert!(theme.colors.is_dark);
    }

    #[test]
    fn from_settings_light() {
        let settings = MockSettings {
            value: Some("light".to_owned()),
        };
        let theme = InboxlyTheme::from_settings(&settings);
        assert!(!theme.colors.is_dark);
    }

    #[test]
    fn from_settings_missing_key_defaults_to_system() {
        let settings = MockSettings { value: None };
        // System detection may or may not work in test, but should not panic.
        let _theme = InboxlyTheme::from_settings(&settings);
    }

    #[test]
    fn from_settings_invalid_value_defaults_to_system() {
        let settings = MockSettings {
            value: Some("purple".to_owned()),
        };
        let _theme = InboxlyTheme::from_settings(&settings);
    }

    #[test]
    fn view_titles() {
        assert_eq!(ActiveView::Inbox.title(), "Inbox");
        assert_eq!(ActiveView::Snoozed.title(), "Snoozed");
        assert_eq!(ActiveView::Done.title(), "Done");
    }
}
