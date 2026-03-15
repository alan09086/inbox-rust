//! Layout dimension constants from the BigTop APK.
//!
//! All values are in logical pixels (1dp = 1 logical pixel at 1x scaling).
//! On HiDPI displays, Iced's built-in scaling applies automatically.
//!
//! Spec reference: Theme System > Dimensions table.

// -- Toolbar --

/// Toolbar height in logical pixels.
pub const TOOLBAR_HEIGHT: f32 = 56.0;
/// Toolbar elevation (shadow depth).
pub const TOOLBAR_ELEVATION: f32 = 2.0;

// -- Navigation Drawer --

/// Nav drawer width when open.
pub const NAV_DRAWER_WIDTH: f32 = 264.0;
/// Nav drawer item height.
pub const NAV_DRAWER_ITEM_HEIGHT: f32 = 48.0;

// -- Spacing --

/// Default margin and padding.
pub const DEFAULT_PADDING: f32 = 16.0;

// -- Avatars --

/// Avatar circle diameter.
pub const AVATAR_DIAMETER: f32 = 40.0;
/// Avatar column width (avatar + surrounding space in list rows).
pub const AVATAR_COLUMN_WIDTH: f32 = 72.0;

// -- List Items --

/// Card elevation for list item cards.
pub const LIST_ITEM_ELEVATION: f32 = 2.0;
/// Corner radius for list item cards (flat in BigTop).
pub const LIST_ITEM_CORNER_RADIUS: f32 = 0.0;

// -- Section Headers --

/// Section header height (Pinned, Today, This Month, etc.).
pub const SECTION_HEADER_HEIGHT: f32 = 48.0;

// -- FAB (Floating Action Button) --

/// Main FAB diameter.
pub const FAB_DIAMETER: f32 = 56.0;
/// Mini FAB diameter (speed dial children).
pub const MINI_FAB_DIAMETER: f32 = 40.0;
/// FAB margin from screen edges.
pub const FAB_EDGE_MARGIN: f32 = 13.0;

// -- Snooze Picker --

/// Snooze picker grid total width.
pub const SNOOZE_GRID_WIDTH: f32 = 288.0;
/// Snooze option cell width.
pub const SNOOZE_CELL_WIDTH: f32 = 142.0;
/// Snooze option cell height.
pub const SNOOZE_CELL_HEIGHT: f32 = 122.0;

// -- Compose --

/// Maximum width of the compose view.
pub const COMPOSE_MAX_WIDTH: f32 = 920.0;

// -- Dividers --

/// Divider line thickness (1 physical pixel; Iced handles DPI).
pub const DIVIDER_THICKNESS: f32 = 1.0;

// -- Popup Menu --

/// Popup menu card width (overflow/context menus).
pub const POPUP_MENU_WIDTH: f32 = 260.0;
/// Popup menu card corner radius.
pub const POPUP_MENU_CORNER_RADIUS: f32 = 10.0;
/// Popup menu item horizontal padding.
pub const POPUP_MENU_ITEM_PADDING_H: f32 = 18.0;
/// Popup menu item vertical padding.
pub const POPUP_MENU_ITEM_PADDING_V: f32 = 12.0;
/// Popup menu icon column width (left-aligned).
pub const POPUP_MENU_ICON_WIDTH: f32 = 22.0;
/// Popup menu item font size.
pub const POPUP_MENU_ITEM_FONT_SIZE: f32 = 15.0;
/// Popup menu separator vertical margin.
pub const POPUP_MENU_SEPARATOR_MARGIN: f32 = 2.0;
/// Popup menu shadow blur radius.
pub const POPUP_MENU_SHADOW_BLUR: f32 = 24.0;
/// Popup menu shadow vertical offset.
pub const POPUP_MENU_SHADOW_OFFSET_Y: f32 = 6.0;

// -- Account Switcher --

/// Avatar diameter in the account switcher header (larger than standard 40dp).
pub const ACCOUNT_SWITCHER_AVATAR: f32 = 44.0;
/// Height of each account row in the expanded account list.
pub const ACCOUNT_ROW_HEIGHT: f32 = 56.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_height_is_56dp() {
        assert_eq!(TOOLBAR_HEIGHT, 56.0);
    }

    #[test]
    fn nav_drawer_width_is_264dp() {
        assert_eq!(NAV_DRAWER_WIDTH, 264.0);
    }

    #[test]
    fn avatar_diameter_is_40dp() {
        assert_eq!(AVATAR_DIAMETER, 40.0);
    }

    #[test]
    fn fab_diameter_is_56dp() {
        assert_eq!(FAB_DIAMETER, 56.0);
    }

    #[test]
    fn default_padding_is_16dp() {
        assert_eq!(DEFAULT_PADDING, 16.0);
    }

    #[test]
    fn snooze_grid_width_is_288dp() {
        assert_eq!(SNOOZE_GRID_WIDTH, 288.0);
    }

    #[test]
    fn compose_max_width_is_920dp() {
        assert_eq!(COMPOSE_MAX_WIDTH, 920.0);
    }

    #[test]
    fn divider_is_1px() {
        assert_eq!(DIVIDER_THICKNESS, 1.0);
    }

    #[test]
    fn flat_cards_have_zero_radius() {
        assert_eq!(LIST_ITEM_CORNER_RADIUS, 0.0);
    }

    #[test]
    fn popup_menu_width_is_260dp() {
        assert_eq!(POPUP_MENU_WIDTH, 260.0);
    }

    #[test]
    fn popup_menu_corner_radius_is_10dp() {
        assert_eq!(POPUP_MENU_CORNER_RADIUS, 10.0);
    }

    #[test]
    fn popup_menu_item_padding_h_is_18dp() {
        assert_eq!(POPUP_MENU_ITEM_PADDING_H, 18.0);
    }

    #[test]
    fn popup_menu_item_padding_v_is_12dp() {
        assert_eq!(POPUP_MENU_ITEM_PADDING_V, 12.0);
    }

    #[test]
    fn popup_menu_icon_width_is_22dp() {
        assert_eq!(POPUP_MENU_ICON_WIDTH, 22.0);
    }

    #[test]
    fn popup_menu_item_font_size_is_15() {
        assert_eq!(POPUP_MENU_ITEM_FONT_SIZE, 15.0);
    }

    #[test]
    fn popup_menu_separator_margin_is_2dp() {
        assert_eq!(POPUP_MENU_SEPARATOR_MARGIN, 2.0);
    }

    #[test]
    fn popup_menu_shadow_blur_is_24dp() {
        assert_eq!(POPUP_MENU_SHADOW_BLUR, 24.0);
    }

    #[test]
    fn popup_menu_shadow_offset_y_is_6dp() {
        assert_eq!(POPUP_MENU_SHADOW_OFFSET_Y, 6.0);
    }

    #[test]
    fn account_switcher_avatar_is_44dp() {
        assert_eq!(ACCOUNT_SWITCHER_AVATAR, 44.0);
    }

    #[test]
    fn account_row_height_is_56dp() {
        assert_eq!(ACCOUNT_ROW_HEIGHT, 56.0);
    }
}
