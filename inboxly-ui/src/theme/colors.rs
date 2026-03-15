//! Colour tokens for light and dark themes.
//!
//! All values derived from the BigTop APK colour palette.
//! Spec reference: Theme System > Light Theme / Dark Theme tables.

use iced::Color;

/// All colour tokens that vary between light and dark themes.
///
/// Values come from Google Inbox's BigTop APK design tokens.
/// See spec: Theme System > Light Theme / Dark Theme tables.
#[derive(Debug, Clone, Copy)]
pub struct ThemeColors {
    /// Main window background (`#ececec` light, `#121212` dark).
    pub background: Color,
    /// Card/surface background (`#ffffff` light, `#1e1e1e` dark).
    pub surface: Color,
    /// Selected item background (`#ebf2ff` light, `#1a2744` dark).
    pub surface_selected: Color,
    /// Primary text colour (`#212121` light, `#e0e0e0` dark).
    pub text_primary: Color,
    /// Secondary/muted text colour (`#757575` light, `#9e9e9e` dark).
    pub text_secondary: Color,
    /// Divider/stroke colour (`#e0e0e0` light, `#2c2c2c` dark).
    pub divider: Color,
    /// Toolbar colour for Inbox view (`#4285f4` light, `#1a3a6e` dark).
    pub toolbar_inbox: Color,
    /// Toolbar colour for Done view (`#0f9d58` light, `#0b5e35` dark).
    pub toolbar_done: Color,
    /// Toolbar colour for Snoozed view (`#ef6c00` light, `#8f4100` dark).
    pub toolbar_snoozed: Color,
    /// Toolbar text/icon colour (white on coloured toolbars).
    pub toolbar_text: Color,
    /// Whether this is a dark theme.
    pub is_dark: bool,

    // -- Popup menu colours --

    /// Menu item hover background (`#f5f5f5` light, `#2a2a2a` dark).
    pub menu_hover: Color,
    /// Destructive menu item hover background (`#fbe9e7` light, `#3d1a14` dark).
    pub menu_destructive_hover: Color,
    /// Destructive menu item text colour (`#ef5350` both themes).
    pub menu_destructive_text: Color,
    /// Menu separator line colour (`#e8e8e8` light, `#333333` dark).
    pub menu_separator: Color,
    /// Menu card shadow colour (rgba black, 18% light, 30% dark).
    pub menu_shadow: Color,
}

impl ThemeColors {
    /// BigTop light theme -- the Google Inbox baseline.
    ///
    /// Spec reference: Theme System > Light Theme table.
    pub const fn light() -> Self {
        Self {
            background: hex("#ececec"),
            surface: hex("#ffffff"),
            surface_selected: hex("#ebf2ff"),
            text_primary: hex("#212121"),
            text_secondary: hex("#757575"),
            divider: hex("#e0e0e0"),
            toolbar_inbox: hex("#4285f4"),
            toolbar_done: hex("#0f9d58"),
            toolbar_snoozed: hex("#ef6c00"),
            toolbar_text: hex("#ffffff"),
            is_dark: false,
            menu_hover: hex("#f5f5f5"),
            menu_destructive_hover: hex("#fbe9e7"),
            menu_destructive_text: hex("#ef5350"),
            menu_separator: hex("#e8e8e8"),
            menu_shadow: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.18 },
        }
    }

    /// Dark theme -- desaturated variants of the BigTop palette.
    ///
    /// Spec reference: Theme System > Dark Theme table.
    pub const fn dark() -> Self {
        Self {
            background: hex("#121212"),
            surface: hex("#1e1e1e"),
            surface_selected: hex("#1a2744"),
            text_primary: hex("#e0e0e0"),
            text_secondary: hex("#9e9e9e"),
            divider: hex("#2c2c2c"),
            toolbar_inbox: hex("#1a3a6e"),
            toolbar_done: hex("#0b5e35"),
            toolbar_snoozed: hex("#8f4100"),
            toolbar_text: hex("#ffffff"),
            is_dark: true,
            menu_hover: hex("#2a2a2a"),
            menu_destructive_hover: hex("#3d1a14"),
            menu_destructive_text: hex("#ef5350"),
            menu_separator: hex("#333333"),
            menu_shadow: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.30 },
        }
    }
}

/// Parse a hex colour string like `#4285f4` into an `iced::Color`.
///
/// Accepts both `#RRGGBB` and `RRGGBB` formats, case-insensitive.
///
/// # Panics
///
/// Panics if the string contains invalid hex digits. Only call with
/// string literals (compile-time-safe in practice).
pub const fn hex(s: &str) -> Color {
    let bytes = s.as_bytes();
    let offset = if bytes[0] == b'#' { 1 } else { 0 };

    let r = hex_byte(bytes[offset], bytes[offset + 1]);
    let g = hex_byte(bytes[offset + 2], bytes[offset + 3]);
    let b = hex_byte(bytes[offset + 4], bytes[offset + 5]);

    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

const fn hex_byte(hi: u8, lo: u8) -> u8 {
    hex_digit(hi) * 16 + hex_digit(lo)
}

const fn hex_digit(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("invalid hex digit"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: assert a Color matches expected hex RGB values.
    fn assert_color_hex(color: Color, hex_str: &str) {
        let expected = hex(hex_str);
        assert!(
            (color.r - expected.r).abs() < 0.002
                && (color.g - expected.g).abs() < 0.002
                && (color.b - expected.b).abs() < 0.002,
            "expected {hex_str} ({expected:?}), got {color:?}"
        );
    }

    // -- Light theme colour tests --

    #[test]
    fn light_theme_background() {
        assert_color_hex(ThemeColors::light().background, "#ececec");
    }

    #[test]
    fn light_theme_surface() {
        assert_color_hex(ThemeColors::light().surface, "#ffffff");
    }

    #[test]
    fn light_theme_surface_selected() {
        assert_color_hex(ThemeColors::light().surface_selected, "#ebf2ff");
    }

    #[test]
    fn light_theme_text_primary() {
        assert_color_hex(ThemeColors::light().text_primary, "#212121");
    }

    #[test]
    fn light_theme_text_secondary() {
        assert_color_hex(ThemeColors::light().text_secondary, "#757575");
    }

    #[test]
    fn light_theme_divider() {
        assert_color_hex(ThemeColors::light().divider, "#e0e0e0");
    }

    #[test]
    fn light_theme_toolbar_inbox() {
        assert_color_hex(ThemeColors::light().toolbar_inbox, "#4285f4");
    }

    #[test]
    fn light_theme_toolbar_done() {
        assert_color_hex(ThemeColors::light().toolbar_done, "#0f9d58");
    }

    #[test]
    fn light_theme_toolbar_snoozed() {
        assert_color_hex(ThemeColors::light().toolbar_snoozed, "#ef6c00");
    }

    #[test]
    fn light_theme_is_not_dark() {
        assert!(!ThemeColors::light().is_dark);
    }

    // -- Dark theme colour tests --

    #[test]
    fn dark_theme_background() {
        assert_color_hex(ThemeColors::dark().background, "#121212");
    }

    #[test]
    fn dark_theme_surface() {
        assert_color_hex(ThemeColors::dark().surface, "#1e1e1e");
    }

    #[test]
    fn dark_theme_surface_selected() {
        assert_color_hex(ThemeColors::dark().surface_selected, "#1a2744");
    }

    #[test]
    fn dark_theme_text_primary() {
        assert_color_hex(ThemeColors::dark().text_primary, "#e0e0e0");
    }

    #[test]
    fn dark_theme_text_secondary() {
        assert_color_hex(ThemeColors::dark().text_secondary, "#9e9e9e");
    }

    #[test]
    fn dark_theme_divider() {
        assert_color_hex(ThemeColors::dark().divider, "#2c2c2c");
    }

    #[test]
    fn dark_theme_toolbar_inbox() {
        assert_color_hex(ThemeColors::dark().toolbar_inbox, "#1a3a6e");
    }

    #[test]
    fn dark_theme_toolbar_done() {
        assert_color_hex(ThemeColors::dark().toolbar_done, "#0b5e35");
    }

    #[test]
    fn dark_theme_toolbar_snoozed() {
        assert_color_hex(ThemeColors::dark().toolbar_snoozed, "#8f4100");
    }

    #[test]
    fn dark_theme_is_dark() {
        assert!(ThemeColors::dark().is_dark);
    }

    // -- hex parser tests --

    #[test]
    fn hex_parser_lowercase() {
        let c = hex("#ff8000");
        assert!((c.r - 1.0).abs() < 0.002);
        assert!((c.g - 0.502).abs() < 0.002);
        assert!(c.b.abs() < 0.002);
    }

    #[test]
    fn hex_parser_uppercase() {
        let c = hex("#FF8000");
        assert!((c.r - 1.0).abs() < 0.002);
        assert!((c.g - 0.502).abs() < 0.002);
        assert!(c.b.abs() < 0.002);
    }

    #[test]
    fn hex_parser_no_hash() {
        let c = hex("4285f4");
        assert_color_hex(c, "#4285f4");
    }

    // -- Popup menu colour tests --

    #[test]
    fn light_theme_menu_hover() {
        assert_color_hex(ThemeColors::light().menu_hover, "#f5f5f5");
    }

    #[test]
    fn light_theme_menu_destructive_hover() {
        assert_color_hex(ThemeColors::light().menu_destructive_hover, "#fbe9e7");
    }

    #[test]
    fn light_theme_menu_destructive_text() {
        assert_color_hex(ThemeColors::light().menu_destructive_text, "#ef5350");
    }

    #[test]
    fn light_theme_menu_separator() {
        assert_color_hex(ThemeColors::light().menu_separator, "#e8e8e8");
    }

    #[test]
    fn light_theme_menu_shadow() {
        let c = ThemeColors::light().menu_shadow;
        assert!(c.r.abs() < 0.01);
        assert!(c.g.abs() < 0.01);
        assert!(c.b.abs() < 0.01);
        assert!((c.a - 0.18).abs() < 0.02);
    }

    #[test]
    fn dark_theme_menu_hover() {
        let c = ThemeColors::dark().menu_hover;
        let surface = ThemeColors::dark().surface;
        assert!(c.r > surface.r, "dark hover should be lighter than surface");
    }

    #[test]
    fn dark_theme_menu_destructive_text_same_as_light() {
        assert_color_hex(ThemeColors::dark().menu_destructive_text, "#ef5350");
    }

    #[test]
    fn dark_theme_menu_shadow_is_darker() {
        let dark_shadow = ThemeColors::dark().menu_shadow;
        let light_shadow = ThemeColors::light().menu_shadow;
        assert!(dark_shadow.a >= light_shadow.a, "dark theme shadow should be at least as opaque");
    }
}
