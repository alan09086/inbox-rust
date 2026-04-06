//! Framework-agnostic colour type replacing `iced::Color`.
//!
//! Provides the same `{ r, g, b, a }` layout as `iced::Color` so existing
//! `const fn hex()` construction and float-based assertions continue to work.
//! Adds `to_css()` for rendering to CSS hex/rgba strings.

use std::fmt;

/// An RGBA colour with components in the 0.0..=1.0 range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// Red component (0.0 = none, 1.0 = full).
    pub r: f32,
    /// Green component.
    pub g: f32,
    /// Blue component.
    pub b: f32,
    /// Alpha component (0.0 = transparent, 1.0 = opaque).
    pub a: f32,
}

impl Color {
    /// Fully opaque black.
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    /// Fully opaque white.
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    /// Create a colour from RGB byte values (0..=255) with full opacity.
    pub const fn from_rgb8(r: u8, g: u8, b: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: 1.0,
        }
    }

    /// Create a colour from floating-point RGB (0.0..=1.0) with full opacity.
    pub fn from_rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Render as a CSS colour string.
    ///
    /// Returns `#rrggbb` for fully opaque colours, or
    /// `rgba(r, g, b, a)` for colours with transparency.
    pub fn to_css(&self) -> String {
        let r = (self.r * 255.0).round() as u8;
        let g = (self.g * 255.0).round() as u8;
        let b = (self.b * 255.0).round() as u8;

        if (self.a - 1.0).abs() < 0.001 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("rgba({r}, {g}, {b}, {:.2})", self.a)
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_css())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_is_zero_rgb() {
        assert_eq!(Color::BLACK.r, 0.0);
        assert_eq!(Color::BLACK.g, 0.0);
        assert_eq!(Color::BLACK.b, 0.0);
        assert_eq!(Color::BLACK.a, 1.0);
    }

    #[test]
    fn white_is_full_rgb() {
        assert_eq!(Color::WHITE.r, 1.0);
        assert_eq!(Color::WHITE.g, 1.0);
        assert_eq!(Color::WHITE.b, 1.0);
        assert_eq!(Color::WHITE.a, 1.0);
    }

    #[test]
    fn from_rgb8_converts_correctly() {
        let c = Color::from_rgb8(66, 133, 244);
        assert!((c.r - 66.0 / 255.0).abs() < 0.002);
        assert!((c.g - 133.0 / 255.0).abs() < 0.002);
        assert!((c.b - 244.0 / 255.0).abs() < 0.002);
    }

    #[test]
    fn to_css_opaque_hex() {
        let c = Color::from_rgb8(66, 133, 244);
        assert_eq!(c.to_css(), "#4285f4");
    }

    #[test]
    fn to_css_transparent_rgba() {
        let c = Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.18,
        };
        assert_eq!(c.to_css(), "rgba(0, 0, 0, 0.18)");
    }

    #[test]
    fn display_uses_to_css() {
        let c = Color::WHITE;
        assert_eq!(format!("{c}"), "#ffffff");
    }
}
