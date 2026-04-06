//! Bundle category colours -- constant across light and dark themes.
//!
//! Spec reference: Theme System > Bundle Category Colours table.

use super::color_type::Color;
use super::colors::hex;

/// Bundle category title and badge colour pair.
///
/// These are constant across light and dark themes.
pub struct BundleCategoryColor {
    /// Title text colour for the bundle name.
    pub title: Color,
    /// Pastel background for the unread badge.
    pub badge: Color,
}

/// Social bundle colours: red title, pink badge.
pub const SOCIAL: BundleCategoryColor = BundleCategoryColor {
    title: hex("#d23f31"),
    badge: hex("#faebea"),
};

/// Promos bundle colours: cyan title, light cyan badge.
pub const PROMOS: BundleCategoryColor = BundleCategoryColor {
    title: hex("#00acc1"),
    badge: hex("#e5f6f9"),
};

/// Updates bundle colours: deep orange title, light orange badge.
pub const UPDATES: BundleCategoryColor = BundleCategoryColor {
    title: hex("#f4511e"),
    badge: hex("#feede8"),
};

/// Finance bundle colours: green title, light green badge.
pub const FINANCE: BundleCategoryColor = BundleCategoryColor {
    title: hex("#558b2f"),
    badge: hex("#eef3ea"),
};

/// Purchases bundle colours: brown title, light brown badge.
pub const PURCHASES: BundleCategoryColor = BundleCategoryColor {
    title: hex("#6d4c41"),
    badge: hex("#f0edec"),
};

/// Travel bundle colours: purple title, light purple badge.
pub const TRAVEL: BundleCategoryColor = BundleCategoryColor {
    title: hex("#8e24aa"),
    badge: hex("#f3e9f6"),
};

/// Forums bundle colours: indigo title, light indigo badge.
pub const FORUMS: BundleCategoryColor = BundleCategoryColor {
    title: hex("#3949ab"),
    badge: hex("#ebecf6"),
};

/// Low Priority bundle colours: dark grey title, light grey badge.
pub const LOW_PRIORITY: BundleCategoryColor = BundleCategoryColor {
    title: hex("#212121"),
    badge: hex("#e5e5e5"),
};

/// Look up bundle category colours by `BundleCategory`.
///
/// Returns `LOW_PRIORITY` for `Saved` and `Custom` categories.
pub fn for_category(category: &inboxly_core::BundleCategory) -> &'static BundleCategoryColor {
    use inboxly_core::BundleCategory;
    match category {
        BundleCategory::Social => &SOCIAL,
        BundleCategory::Promos => &PROMOS,
        BundleCategory::Updates => &UPDATES,
        BundleCategory::Finance => &FINANCE,
        BundleCategory::Purchases => &PURCHASES,
        BundleCategory::Travel => &TRAVEL,
        BundleCategory::Forums => &FORUMS,
        BundleCategory::LowPriority => &LOW_PRIORITY,
        BundleCategory::Saved | BundleCategory::Custom(_) => &LOW_PRIORITY,
    }
}

/// Look up bundle category colours by category key string.
///
/// Maps lowercase string keys (as stored in the `bundles` table) to colours.
/// Returns `LOW_PRIORITY` for unknown categories.
pub fn for_category_str(category: &str) -> &'static BundleCategoryColor {
    match category {
        "social" => &SOCIAL,
        "promos" => &PROMOS,
        "updates" => &UPDATES,
        "finance" => &FINANCE,
        "purchases" => &PURCHASES,
        "travel" => &TRAVEL,
        "forums" => &FORUMS,
        "low_priority" => &LOW_PRIORITY,
        _ => &LOW_PRIORITY,
    }
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
    fn social_title_is_red() {
        assert_color_eq(SOCIAL.title, "#d23f31");
    }

    #[test]
    fn social_badge_is_pink() {
        assert_color_eq(SOCIAL.badge, "#faebea");
    }

    #[test]
    fn promos_title_is_cyan() {
        assert_color_eq(PROMOS.title, "#00acc1");
    }

    #[test]
    fn updates_title_is_deep_orange() {
        assert_color_eq(UPDATES.title, "#f4511e");
    }

    #[test]
    fn finance_title_is_green() {
        assert_color_eq(FINANCE.title, "#558b2f");
    }

    #[test]
    fn purchases_title_is_brown() {
        assert_color_eq(PURCHASES.title, "#6d4c41");
    }

    #[test]
    fn travel_title_is_purple() {
        assert_color_eq(TRAVEL.title, "#8e24aa");
    }

    #[test]
    fn forums_title_is_indigo() {
        assert_color_eq(FORUMS.title, "#3949ab");
    }

    #[test]
    fn low_priority_title_is_dark_grey() {
        assert_color_eq(LOW_PRIORITY.title, "#212121");
    }

    #[test]
    fn all_eight_categories_have_distinct_titles() {
        let colors = [
            SOCIAL.title,
            PROMOS.title,
            UPDATES.title,
            FINANCE.title,
            PURCHASES.title,
            TRAVEL.title,
            FORUMS.title,
            LOW_PRIORITY.title,
        ];
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert!(
                    (colors[i].r - colors[j].r).abs() > 0.01
                        || (colors[i].g - colors[j].g).abs() > 0.01
                        || (colors[i].b - colors[j].b).abs() > 0.01,
                    "categories {i} and {j} have identical title colours"
                );
            }
        }
    }

    #[test]
    fn for_category_maps_correctly() {
        use inboxly_core::BundleCategory;

        let social = for_category(&BundleCategory::Social);
        assert_color_eq(social.title, "#d23f31");

        let custom = for_category(&BundleCategory::Custom("MyBundle".into()));
        assert_color_eq(custom.title, "#212121"); // defaults to LOW_PRIORITY
    }
}
