//! Inboxly theme system -- BigTop design tokens.
//!
//! # Architecture
//!
//! ```text
//! ThemePreference (config)     SystemColorScheme (D-Bus)
//!        \                        /
//!         +--- ThemeConfig ------+
//!              |
//!         ThemeColors
//!              |
//!   light() / dark() colour tokens
//! ```
//!
//! # Modules
//!
//! - [`color_type`] -- Framework-agnostic RGBA colour type
//! - [`colors`] -- Light/dark colour tokens (`ThemeColors`)
//! - [`bundle_colors`] -- Bundle category colours (constant across themes)
//! - [`avatar_colors`] -- Avatar letter tile A-Z palette (constant across themes)
//! - [`dimensions`] -- Layout dimension constants from BigTop APK
//! - [`typography`] -- Font size and weight constants from BigTop APK
//! - [`system`] -- System theme detection via freedesktop portal D-Bus

pub mod avatar_colors;
pub mod bundle_colors;
pub mod color_type;
pub mod colors;
pub mod dimensions;
pub mod system;
pub mod typography;

pub use color_type::Color;
pub use colors::ThemeColors;
pub use system::{SystemColorScheme, SystemThemeError, query_system_color_scheme};

use inboxly_core::config::ThemePreference;

// ============================================================================
// Backward-compatible re-exports from M15
// ============================================================================
//
// These items were originally in the flat `theme.rs` file. They are kept here
// so that `nav.rs`, `toolbar.rs`, and `app.rs` continue to compile without
// changes to their import paths.

/// The primary views that drive toolbar colour and content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ActiveView {
    #[default]
    Inbox,
    Snoozed,
    Done,
    /// Full-screen compose view (M35). Toolbar shows the compose colour
    /// and the back arrow returns to `previous_view`.
    Compose,
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
            Self::Compose => "Compose",
            Self::Settings => "Settings",
        }
    }

    /// Toolbar background colour for this view (light theme).
    pub fn toolbar_color(&self) -> Color {
        match self {
            Self::Inbox => color_from_hex(0x42, 0x85, 0xf4), // #4285f4
            Self::Snoozed => color_from_hex(0xef, 0x6c, 0x00), // #ef6c00
            Self::Done => color_from_hex(0x0f, 0x9d, 0x58),  // #0f9d58
            // Muted purple for compose — distinct from the inbox blue and
            // the settings grey so the user can tell at a glance which
            // mode they are in. Phase 7 adds a themed `toolbar_compose`
            // colour token; until then we reuse `toolbar_settings`.
            Self::Compose => color_from_hex(0x55, 0x4e, 0x91),
            Self::Settings => color_from_hex(0x45, 0x5a, 0x64),
        }
    }

    /// Toolbar background colour for this view, theme-aware.
    pub fn toolbar_color_themed(&self, theme: &ThemeConfig) -> Color {
        let colors = theme.colors();
        match self {
            Self::Inbox => colors.toolbar_inbox,
            Self::Snoozed => colors.toolbar_snoozed,
            Self::Done => colors.toolbar_done,
            // Phase 7 will introduce a dedicated `toolbar_compose`
            // colour token. Until then, reuse the settings toolbar
            // colour so the themed path stays consistent across light
            // and dark mode.
            Self::Compose | Self::Settings => colors.toolbar_settings,
        }
    }

    /// CSS colour string for the toolbar background (theme-aware).
    pub fn toolbar_css(&self, theme: &ThemeConfig) -> String {
        self.toolbar_color_themed(theme).to_css()
    }
}

/// Convert RGB bytes to `Color` (0.0..1.0 range).
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
// ThemeConfig -- framework-agnostic theme configuration
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

/// Framework-agnostic theme configuration.
///
/// Wraps `ThemeColors` and provides theme resolution from system/user
/// preferences. Replaces the Iced-coupled `InboxlyTheme`.
#[derive(Debug, Clone)]
pub struct ThemeConfig {
    /// The colour tokens for this theme variant.
    pub colors: ThemeColors,
}

impl ThemeConfig {
    /// Create a theme from the given colour tokens.
    pub fn new(colors: ThemeColors) -> Self {
        Self { colors }
    }

    /// BigTop light theme.
    pub fn light() -> Self {
        Self::new(ThemeColors::light())
    }

    /// BigTop dark theme.
    pub fn dark() -> Self {
        Self::new(ThemeColors::dark())
    }

    /// Get the colour tokens.
    pub fn colors(&self) -> &ThemeColors {
        &self.colors
    }

    /// Whether this is a dark theme.
    pub fn is_dark(&self) -> bool {
        self.colors.is_dark
    }

    /// Detect system theme preference and return the matching theme.
    ///
    /// Queries `org.freedesktop.portal.Settings` for `color-scheme`.
    /// Falls back to light theme if detection fails.
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

    /// Resolve theme from user preference.
    pub fn from_preference(pref: ThemePreference) -> Self {
        match pref {
            ThemePreference::Light => Self::light(),
            ThemePreference::Dark => Self::dark(),
            ThemePreference::System => Self::from_system(),
        }
    }

    /// Resolve theme from a preference stored in the settings table.
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
    pub fn toggle(&self) -> Self {
        if self.colors.is_dark {
            Self::light()
        } else {
            Self::dark()
        }
    }

    /// Save the current theme preference to the settings store.
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
}

/// Backward-compatible alias for `ThemeConfig`.
pub type InboxlyTheme = ThemeConfig;

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

    // -- ThemeConfig tests --

    #[test]
    fn light_theme_creates_successfully() {
        let theme = ThemeConfig::light();
        assert!(!theme.is_dark());
    }

    #[test]
    fn dark_theme_creates_successfully() {
        let theme = ThemeConfig::dark();
        assert!(theme.is_dark());
    }

    #[test]
    fn toggle_light_to_dark() {
        let light = ThemeConfig::light();
        let dark = light.toggle();
        assert!(dark.is_dark());
    }

    #[test]
    fn toggle_dark_to_light() {
        let dark = ThemeConfig::dark();
        let light = dark.toggle();
        assert!(!light.is_dark());
    }

    #[test]
    fn toggle_is_involution() {
        let original = ThemeConfig::light();
        let toggled_twice = original.toggle().toggle();
        assert_eq!(original.is_dark(), toggled_twice.is_dark());
    }

    #[test]
    fn colors_returns_correct_tokens() {
        let light = ThemeConfig::light();
        let colors = light.colors();
        let expected = ThemeColors::light().background;
        assert!(
            (colors.background.r - expected.r).abs() < 0.01
                && (colors.background.g - expected.g).abs() < 0.01
                && (colors.background.b - expected.b).abs() < 0.01,
            "ThemeConfig colors background doesn't match light theme"
        );
    }

    #[test]
    fn dark_colors_returns_correct_tokens() {
        let dark = ThemeConfig::dark();
        let colors = dark.colors();
        let expected = ThemeColors::dark().background;
        assert!(
            (colors.background.r - expected.r).abs() < 0.01
                && (colors.background.g - expected.g).abs() < 0.01
                && (colors.background.b - expected.b).abs() < 0.01,
            "ThemeConfig colors background doesn't match dark theme"
        );
    }

    #[test]
    fn themed_toolbar_color_uses_theme_values() {
        let dark = ThemeConfig::dark();
        let inbox_color = ActiveView::Inbox.toolbar_color_themed(&dark);
        let expected = ThemeColors::dark().toolbar_inbox;
        assert!(
            (inbox_color.r - expected.r).abs() < 0.01
                && (inbox_color.g - expected.g).abs() < 0.01
                && (inbox_color.b - expected.b).abs() < 0.01,
        );
    }

    #[test]
    fn toolbar_css_returns_hex_string() {
        let light = ThemeConfig::light();
        let css = ActiveView::Inbox.toolbar_css(&light);
        assert_eq!(css, "#4285f4");
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
        let theme = ThemeConfig::from_preference(ThemePreference::Light);
        assert!(!theme.is_dark());
    }

    #[test]
    fn from_preference_dark() {
        let theme = ThemeConfig::from_preference(ThemePreference::Dark);
        assert!(theme.is_dark());
    }

    #[test]
    fn from_settings_dark() {
        let settings = MockSettings {
            value: Some("dark".to_owned()),
        };
        let theme = ThemeConfig::from_settings(&settings);
        assert!(theme.is_dark());
    }

    #[test]
    fn from_settings_light() {
        let settings = MockSettings {
            value: Some("light".to_owned()),
        };
        let theme = ThemeConfig::from_settings(&settings);
        assert!(!theme.is_dark());
    }

    #[test]
    fn from_settings_missing_key_defaults_to_system() {
        let settings = MockSettings { value: None };
        let _theme = ThemeConfig::from_settings(&settings);
    }

    #[test]
    fn from_settings_invalid_value_defaults_to_system() {
        let settings = MockSettings {
            value: Some("purple".to_owned()),
        };
        let _theme = ThemeConfig::from_settings(&settings);
    }

    #[test]
    fn view_titles() {
        assert_eq!(ActiveView::Inbox.title(), "Inbox");
        assert_eq!(ActiveView::Snoozed.title(), "Snoozed");
        assert_eq!(ActiveView::Done.title(), "Done");
        assert_eq!(ActiveView::Settings.title(), "Settings");
    }
}
