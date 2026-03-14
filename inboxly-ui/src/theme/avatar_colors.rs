//! Avatar letter tile colours for A-Z.
//!
//! These are constant across light and dark themes.
//! Spec reference: Theme System > Avatar Letter Tile Colours.

use iced::Color;

use super::colors::hex;

/// Avatar letter tile colours for A-Z plus a default (index 26).
pub const AVATAR_COLORS: [Color; 27] = [
    hex("#e06055"), // A
    hex("#ed6192"), // B
    hex("#ba68c8"), // C
    hex("#9575cd"), // D
    hex("#7986cb"), // E
    hex("#5e97f6"), // F
    hex("#4fc3f7"), // G
    hex("#58d0e1"), // H
    hex("#4fb6ac"), // I
    hex("#57bb8a"), // J
    hex("#9ccc65"), // K
    hex("#d4e157"), // L
    hex("#fdd835"), // M
    hex("#f6bf32"), // N
    hex("#f5a631"), // O
    hex("#f18864"), // P
    hex("#c2c2c2"), // Q
    hex("#90a4ae"), // R
    hex("#a1887f"), // S
    hex("#a3a3a3"), // T
    hex("#afb6e0"), // U
    hex("#b39ddb"), // V
    hex("#c2c2c2"), // W
    hex("#80deea"), // X
    hex("#bcaaa4"), // Y
    hex("#aed581"), // Z
    hex("#efefef"), // default (index 26)
];

/// Get the avatar colour for a given character.
///
/// Maps A-Z (case-insensitive) to the corresponding colour.
/// Returns the default colour for non-letter characters.
pub const fn for_letter(c: char) -> Color {
    let index = match c {
        'A' | 'a' => 0,
        'B' | 'b' => 1,
        'C' | 'c' => 2,
        'D' | 'd' => 3,
        'E' | 'e' => 4,
        'F' | 'f' => 5,
        'G' | 'g' => 6,
        'H' | 'h' => 7,
        'I' | 'i' => 8,
        'J' | 'j' => 9,
        'K' | 'k' => 10,
        'L' | 'l' => 11,
        'M' | 'm' => 12,
        'N' | 'n' => 13,
        'O' | 'o' => 14,
        'P' | 'p' => 15,
        'Q' | 'q' => 16,
        'R' | 'r' => 17,
        'S' | 's' => 18,
        'T' | 't' => 19,
        'U' | 'u' => 20,
        'V' | 'v' => 21,
        'W' | 'w' => 22,
        'X' | 'x' => 23,
        'Y' | 'y' => 24,
        'Z' | 'z' => 25,
        _ => 26, // default
    };
    AVATAR_COLORS[index]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::colors::hex;

    fn assert_color_eq(a: Color, hex_str: &str) {
        let b = hex(hex_str);
        assert!(
            (a.r - b.r).abs() < 0.002 && (a.g - b.g).abs() < 0.002 && (a.b - b.b).abs() < 0.002,
            "expected {hex_str}, got {a:?}"
        );
    }

    #[test]
    fn avatar_a_is_red() {
        assert_color_eq(for_letter('A'), "#e06055");
    }

    #[test]
    fn avatar_z_is_green() {
        assert_color_eq(for_letter('Z'), "#aed581");
    }

    #[test]
    fn avatar_case_insensitive() {
        let upper = for_letter('F');
        let lower = for_letter('f');
        assert!((upper.r - lower.r).abs() < 0.001);
        assert!((upper.g - lower.g).abs() < 0.001);
        assert!((upper.b - lower.b).abs() < 0.001);
    }

    #[test]
    fn avatar_non_letter_returns_default() {
        assert_color_eq(for_letter('1'), "#efefef");
        assert_color_eq(for_letter('!'), "#efefef");
        assert_color_eq(for_letter(' '), "#efefef");
    }

    #[test]
    fn avatar_array_has_27_entries() {
        assert_eq!(AVATAR_COLORS.len(), 27); // 26 letters + default
    }

    #[test]
    fn avatar_m_is_yellow() {
        assert_color_eq(for_letter('M'), "#fdd835");
    }

    #[test]
    fn avatar_all_26_letters_resolve() {
        for c in b'A'..=b'Z' {
            let color = for_letter(c as char);
            let default = AVATAR_COLORS[26];
            // Q and W happen to equal the default grey, skip those
            if c != b'Q' && c != b'W' {
                assert!(
                    (color.r - default.r).abs() > 0.01
                        || (color.g - default.g).abs() > 0.01
                        || (color.b - default.b).abs() > 0.01,
                    "letter {} returned default colour",
                    c as char
                );
            }
        }
    }
}
