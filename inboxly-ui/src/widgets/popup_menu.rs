//! Popup menu widget — reusable dropdown/context menu overlay.
//!
//! Renders a positioned menu card as an Iced overlay. Every dropdown,
//! overflow menu, and context menu in the app uses this single primitive.
//!
//! # Architecture
//!
//! ```text
//! PopupMenu<Message>
//! ├── trigger: Element<Message>      -- the button/area that opens the menu
//! ├── items: Vec<MenuItem<Message>>  -- menu entries
//! ├── is_open: bool                  -- controlled by parent state
//! ├── anchor: PopupAnchor            -- where the menu positions relative to trigger
//! └── on_dismiss: Message            -- sent when click-away or item selected
//! ```
//!
//! Spec reference: QoL Menus & Settings §System 1: PopupMenu Widget.

/// Style variant for a menu action item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuItemStyle {
    /// Standard menu item — normal text colour, neutral hover.
    #[default]
    Normal,
    /// Destructive action — red text, red-tinted hover.
    /// Used for Report Spam, Block Sender, etc.
    Destructive,
}

/// Where the popup menu positions itself relative to the trigger element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopupAnchor {
    /// Opens below trigger, right-aligned to the trigger's right edge.
    /// Default for overflow menus.
    #[default]
    BelowRight,
    /// Opens below trigger, left-aligned to the trigger's left edge.
    BelowLeft,
    /// Opens at the mouse cursor position.
    /// Used for right-click context menus.
    AtCursor,
}

/// A single entry in a popup menu.
///
/// Menu items are generic over the application `Message` type so that
/// clicking an action item can produce any message the app expects.
#[derive(Debug, Clone)]
pub enum MenuItem<Message> {
    /// A clickable action item with an optional icon.
    Action {
        /// Display label text.
        label: String,
        /// Optional Unicode icon character displayed left of the label.
        icon: Option<char>,
        /// Message produced when this item is clicked.
        message: Message,
        /// Visual style (Normal or Destructive).
        style: MenuItemStyle,
    },
    /// A horizontal separator line between groups of items.
    Separator,
    /// A submenu that expands on hover (max 1 level deep).
    Submenu {
        /// Display label text.
        label: String,
        /// Optional Unicode icon character displayed left of the label.
        icon: Option<char>,
        /// Child menu items (no further nesting).
        items: Vec<MenuItem<Message>>,
    },
}

impl<Message> MenuItem<Message> {
    /// Create a normal action item with just a label and message.
    pub fn action(label: impl Into<String>, message: Message) -> Self {
        Self::Action {
            label: label.into(),
            icon: None,
            message,
            style: MenuItemStyle::Normal,
        }
    }

    /// Create a normal action item with an icon.
    pub fn action_with_icon(
        label: impl Into<String>,
        icon: char,
        message: Message,
    ) -> Self {
        Self::Action {
            label: label.into(),
            icon: Some(icon),
            message,
            style: MenuItemStyle::Normal,
        }
    }

    /// Create a destructive action item (red text).
    pub fn destructive(label: impl Into<String>, message: Message) -> Self {
        Self::Action {
            label: label.into(),
            icon: None,
            message,
            style: MenuItemStyle::Destructive,
        }
    }

    /// Create a destructive action item with an icon.
    pub fn destructive_with_icon(
        label: impl Into<String>,
        icon: char,
        message: Message,
    ) -> Self {
        Self::Action {
            label: label.into(),
            icon: Some(icon),
            message,
            style: MenuItemStyle::Destructive,
        }
    }

    /// Create a separator.
    pub fn separator() -> Self {
        Self::Separator
    }

    /// Create a submenu.
    pub fn submenu(
        label: impl Into<String>,
        icon: Option<char>,
        items: Vec<MenuItem<Message>>,
    ) -> Self {
        Self::Submenu {
            label: label.into(),
            icon,
            items,
        }
    }

    /// Returns `true` if this item is a separator.
    pub fn is_separator(&self) -> bool {
        matches!(self, Self::Separator)
    }

    /// Returns `true` if this item is a destructive action.
    pub fn is_destructive(&self) -> bool {
        matches!(
            self,
            Self::Action {
                style: MenuItemStyle::Destructive,
                ..
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- MenuItemStyle tests --

    #[test]
    fn menu_item_style_default_is_normal() {
        assert_eq!(MenuItemStyle::default(), MenuItemStyle::Normal);
    }

    #[test]
    fn menu_item_style_eq() {
        assert_eq!(MenuItemStyle::Destructive, MenuItemStyle::Destructive);
        assert_ne!(MenuItemStyle::Normal, MenuItemStyle::Destructive);
    }

    // -- PopupAnchor tests --

    #[test]
    fn popup_anchor_default_is_below_right() {
        assert_eq!(PopupAnchor::default(), PopupAnchor::BelowRight);
    }

    #[test]
    fn popup_anchor_all_variants_distinct() {
        let variants = [
            PopupAnchor::BelowRight,
            PopupAnchor::BelowLeft,
            PopupAnchor::AtCursor,
        ];
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    // -- MenuItem constructor tests --

    #[test]
    fn action_creates_normal_item() {
        let item: MenuItem<&str> = MenuItem::action("Archive", "archive");
        match item {
            MenuItem::Action {
                label,
                icon,
                message,
                style,
            } => {
                assert_eq!(label, "Archive");
                assert_eq!(icon, None);
                assert_eq!(message, "archive");
                assert_eq!(style, MenuItemStyle::Normal);
            }
            _ => panic!("expected Action variant"),
        }
    }

    #[test]
    fn action_with_icon_creates_item_with_icon() {
        let item: MenuItem<&str> =
            MenuItem::action_with_icon("Reply", '\u{21A9}', "reply");
        match item {
            MenuItem::Action {
                label,
                icon,
                message,
                style,
            } => {
                assert_eq!(label, "Reply");
                assert_eq!(icon, Some('\u{21A9}'));
                assert_eq!(message, "reply");
                assert_eq!(style, MenuItemStyle::Normal);
            }
            _ => panic!("expected Action variant"),
        }
    }

    #[test]
    fn destructive_creates_red_item() {
        let item: MenuItem<&str> = MenuItem::destructive("Block sender", "block");
        match item {
            MenuItem::Action { style, .. } => {
                assert_eq!(style, MenuItemStyle::Destructive);
            }
            _ => panic!("expected Action variant"),
        }
    }

    #[test]
    fn destructive_with_icon_creates_red_item_with_icon() {
        let item: MenuItem<&str> =
            MenuItem::destructive_with_icon("Report spam", '\u{26A0}', "spam");
        match item {
            MenuItem::Action {
                icon, style, ..
            } => {
                assert_eq!(icon, Some('\u{26A0}'));
                assert_eq!(style, MenuItemStyle::Destructive);
            }
            _ => panic!("expected Action variant"),
        }
    }

    #[test]
    fn separator_is_separator() {
        let item: MenuItem<&str> = MenuItem::separator();
        assert!(item.is_separator());
    }

    #[test]
    fn action_is_not_separator() {
        let item: MenuItem<&str> = MenuItem::action("test", "msg");
        assert!(!item.is_separator());
    }

    #[test]
    fn destructive_item_is_destructive() {
        let item: MenuItem<&str> = MenuItem::destructive("Delete", "del");
        assert!(item.is_destructive());
    }

    #[test]
    fn normal_item_is_not_destructive() {
        let item: MenuItem<&str> = MenuItem::action("Edit", "edit");
        assert!(!item.is_destructive());
    }

    #[test]
    fn submenu_creates_nested_items() {
        let children = vec![
            MenuItem::action("Inbox", "inbox"),
            MenuItem::action("Trash", "trash"),
        ];
        let item: MenuItem<&str> =
            MenuItem::submenu("Move to...", Some('\u{1F4C1}'), children);
        match item {
            MenuItem::Submenu {
                label,
                icon,
                items,
            } => {
                assert_eq!(label, "Move to...");
                assert_eq!(icon, Some('\u{1F4C1}'));
                assert_eq!(items.len(), 2);
            }
            _ => panic!("expected Submenu variant"),
        }
    }

    #[test]
    fn submenu_is_not_separator() {
        let item: MenuItem<&str> = MenuItem::submenu("Sub", None, vec![]);
        assert!(!item.is_separator());
    }

    #[test]
    fn submenu_is_not_destructive() {
        let item: MenuItem<&str> = MenuItem::submenu("Sub", None, vec![]);
        assert!(!item.is_destructive());
    }
}
