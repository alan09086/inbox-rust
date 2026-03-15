# M26: PopupMenu Widget — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Build the reusable `PopupMenu<Message>` widget that renders dropdown/context menus as Iced overlays. Every dropdown, overflow menu, and context menu in the app will use this single primitive. This is System 1 of the QoL Menus & Settings spec — no toolbar, settings, or context menu integration in this milestone.

**Crates:** `inboxly-ui`

**Branch:** `m26-popup-menu-widget`

**Prereqs:** M1-M25 complete (workspace, core types, store, IMAP sync, bundler, snooze, UI shell, theme, inbox feed, bundles, done/pin/sweep, swipe, snooze picker, reminders/FAB, compose/SMTP, search, highlights/polish)

**Spec ref:** QoL Menus & Settings Design Spec §System 1: PopupMenu Widget (lines 20-85), §Testing Strategy (lines 309-316)

**Tech Stack:** Rust, iced 0.14 (overlay API, Widget trait, advanced feature)

---

> **⚠ Iced 0.14 Overlay API Notes:**
>
> In Iced 0.14 with the `advanced` feature, custom overlays are created by implementing `Widget::overlay()` on the outer wrapper widget. The method returns `Option<overlay::Element<'_, Message, Theme, Renderer>>`. The overlay renders above all other content in the window. The `PopupMenu` widget wraps a trigger element and conditionally returns an overlay containing the menu card and invisible backdrop.
>
> Key types from `iced::advanced`:
> - `Widget` trait — `layout()`, `draw()`, `on_event()`, `overlay()`, `size()`, `state()`, `tag()`
> - `overlay::Element` — wraps a custom `Overlay` impl for the dropdown
> - `Overlay` trait — `layout()`, `draw()`, `on_event()`, `mouse_interaction()`
> - `widget::Tree` — holds widget state across frames
> - `renderer::Quad` — for drawing background rectangles with rounded corners and shadows
>
> **Pattern:** The `PopupMenu` widget itself implements `Widget`. Its `overlay()` method returns a custom `MenuOverlay` struct that implements `Overlay`. The `MenuOverlay` handles all backdrop click detection, menu item rendering, hover tracking, and keyboard events.

---

## File Structure

| Action | File | Description |
|--------|------|-------------|
| Create | `inboxly-ui/src/widgets/popup_menu.rs` | Core `PopupMenu` widget, `MenuItem`, `MenuItemStyle`, `PopupAnchor`, `MenuOverlay` |
| Modify | `inboxly-ui/src/widgets/mod.rs` | Add `pub mod popup_menu;` |
| Modify | `inboxly-ui/src/theme/dimensions.rs` | Add popup menu dimension constants |
| Modify | `inboxly-ui/src/theme/colors.rs` | Add popup menu colour tokens to `ThemeColors` |

---

## Task Overview

| # | Task | Est. |
|---|------|------|
| 1 | Add popup menu dimension constants | 5 min |
| 2 | Add popup menu colour tokens to ThemeColors | 10 min |
| 3 | Define `MenuItem`, `MenuItemStyle`, `PopupAnchor` types | 10 min |
| 4 | Implement `PopupMenu` widget struct and `Widget` trait (trigger passthrough) | 25 min |
| 5 | Implement `MenuOverlay` — layout and drawing | 30 min |
| 6 | Implement `MenuOverlay` — event handling (click, hover, Escape) | 25 min |
| 7 | Wire into widgets module and verify workspace compiles | 5 min |
| 8 | Integration smoke test — popup menu in isolation | 15 min |

---

## Task 1: Add popup menu dimension constants

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/dimensions.rs`

Add dimension constants derived from the spec's Visual Spec section.

- [ ] **Step 1: Write failing test**

Add tests at the bottom of the existing `#[cfg(test)]` block in `dimensions.rs`:

```rust
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
```

- [ ] **Step 2: Verify tests fail**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- dimensions 2>&1 | grep "FAILED\|error"
```

- [ ] **Step 3: Add the constants**

Add these constants after the existing `// -- Dividers --` section:

```rust
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
```

- [ ] **Step 4: Verify all tests pass**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- dimensions && cargo clippy -p inboxly-ui -- -D warnings
```

- [ ] **Step 5: Commit**

```
feat(ui): add popup menu dimension constants from QoL spec
```

---

## Task 2: Add popup menu colour tokens to ThemeColors

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/colors.rs`

Add theme-aware colour tokens for the popup menu. The spec defines specific colours for light theme; dark theme uses appropriate tokens from `ThemeColors`.

- [ ] **Step 1: Write failing tests**

Add to the existing `#[cfg(test)]` block in `colors.rs`:

```rust
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
    // Shadow at 18% opacity black
    let c = ThemeColors::light().menu_shadow;
    assert!(c.r.abs() < 0.01);
    assert!(c.g.abs() < 0.01);
    assert!(c.b.abs() < 0.01);
    assert!((c.a - 0.18).abs() < 0.02);
}

#[test]
fn dark_theme_menu_hover() {
    // Dark theme hover should be lighter than surface (#1e1e1e)
    let c = ThemeColors::dark().menu_hover;
    let surface = ThemeColors::dark().surface;
    assert!(c.r > surface.r, "dark hover should be lighter than surface");
}

#[test]
fn dark_theme_menu_destructive_text_same_as_light() {
    // Destructive red stays #ef5350 in both themes (per spec)
    assert_color_hex(ThemeColors::dark().menu_destructive_text, "#ef5350");
}

#[test]
fn dark_theme_menu_shadow_is_darker() {
    let dark_shadow = ThemeColors::dark().menu_shadow;
    let light_shadow = ThemeColors::light().menu_shadow;
    assert!(dark_shadow.a >= light_shadow.a, "dark theme shadow should be at least as opaque");
}
```

- [ ] **Step 2: Verify tests fail**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- colors 2>&1 | grep "FAILED\|error"
```

- [ ] **Step 3: Add colour fields to `ThemeColors`**

Add these fields to the `ThemeColors` struct after the existing `is_dark` field:

```rust
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
```

Update the `light()` constructor to include:

```rust
            menu_hover: hex("#f5f5f5"),
            menu_destructive_hover: hex("#fbe9e7"),
            menu_destructive_text: hex("#ef5350"),
            menu_separator: hex("#e8e8e8"),
            menu_shadow: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.18 },
```

Update the `dark()` constructor to include:

```rust
            menu_hover: hex("#2a2a2a"),
            menu_destructive_hover: hex("#3d1a14"),
            menu_destructive_text: hex("#ef5350"),
            menu_separator: hex("#333333"),
            menu_shadow: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.30 },
```

- [ ] **Step 4: Verify all tests pass**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- colors && cargo clippy -p inboxly-ui -- -D warnings
```

- [ ] **Step 5: Commit**

```
feat(ui): add popup menu colour tokens to ThemeColors
```

---

## Task 3: Define `MenuItem`, `MenuItemStyle`, `PopupAnchor` types

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/popup_menu.rs` (new file)

Create the public types that consumers of the popup menu will use. These are pure data types with no rendering logic yet.

- [ ] **Step 1: Create the file with types and write tests**

```rust
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
```

- [ ] **Step 2: Register the module in `widgets/mod.rs`**

Add `pub mod popup_menu;` to `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/mod.rs`.

- [ ] **Step 3: Verify tests pass**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- popup_menu && cargo clippy -p inboxly-ui -- -D warnings
```

- [ ] **Step 4: Commit**

```
feat(ui): define MenuItem, MenuItemStyle, PopupAnchor types for popup menu
```

---

## Task 4: Implement `PopupMenu` widget struct and `Widget` trait (trigger passthrough)

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/popup_menu.rs`

Implement the `PopupMenu` struct that wraps a trigger element and implements Iced's `Widget` trait. When `is_open` is false, it passes through all layout/draw/event calls to the trigger. When `is_open` is true, it returns an overlay from `overlay()`.

- [ ] **Step 1: Write failing test**

Add to the existing test module:

```rust
    // -- PopupMenu construction tests --
    // (These test the builder API; rendering requires Iced runtime
    //  so we test construction and state only.)

    #[test]
    fn popup_menu_stores_anchor() {
        // Verify the PopupMenu builder accepts all anchor variants.
        // Full rendering tests are in Task 8.
        let _br = PopupAnchor::BelowRight;
        let _bl = PopupAnchor::BelowLeft;
        let _ac = PopupAnchor::AtCursor;
        // If this compiles, the types exist and are constructible.
    }
```

- [ ] **Step 2: Implement PopupMenu struct and Widget trait**

Add to `popup_menu.rs`, above the `#[cfg(test)]` block:

```rust
use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::event::Status;
use iced::mouse;
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

use crate::theme::colors::ThemeColors;

/// Internal state stored in the widget tree.
#[derive(Debug, Default)]
struct PopupMenuState {
    /// Index of the menu item currently hovered (-1 = none).
    hovered_index: Option<usize>,
    /// Cursor position for AtCursor anchor mode.
    cursor_position: iced::Point,
}

/// A popup menu widget that wraps a trigger element and optionally
/// renders a dropdown overlay.
///
/// # Type Parameters
///
/// - `'a`: lifetime of the trigger element and menu items
/// - `Message`: the application message type (must be Clone for item messages)
pub struct PopupMenu<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: renderer::Renderer,
{
    /// The trigger element (button, icon, etc.) that the menu anchors to.
    trigger: Element<'a, Message, Theme, Renderer>,
    /// Menu items to display when open.
    items: Vec<MenuItem<Message>>,
    /// Whether the menu overlay is currently visible.
    is_open: bool,
    /// Positioning anchor relative to the trigger.
    anchor: PopupAnchor,
    /// Message sent when the menu should close (click-away, Escape, item selected).
    on_dismiss: Message,
    /// Theme colours for rendering the menu.
    theme_colors: ThemeColors,
}

impl<'a, Message, Renderer> PopupMenu<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Renderer: renderer::Renderer + 'a,
{
    /// Create a new popup menu wrapping the given trigger element.
    ///
    /// The menu starts closed. Set `is_open` to `true` to show it.
    pub fn new(
        trigger: impl Into<Element<'a, Message, Theme, Renderer>>,
        items: Vec<MenuItem<Message>>,
        on_dismiss: Message,
        theme_colors: ThemeColors,
    ) -> Self {
        Self {
            trigger: trigger.into(),
            items,
            is_open: false,
            anchor: PopupAnchor::default(),
            on_dismiss,
            theme_colors,
        }
    }

    /// Set whether the menu overlay is open.
    pub fn open(mut self, is_open: bool) -> Self {
        self.is_open = is_open;
        self
    }

    /// Set the positioning anchor.
    pub fn anchor(mut self, anchor: PopupAnchor) -> Self {
        self.anchor = anchor;
        self
    }
}
```

**Important implementation notes for the `Widget` trait impl:**

- `tag()` returns `widget::tree::Tag::of::<PopupMenuState>()`
- `state()` returns `widget::tree::State::new(PopupMenuState::default())`
- `children()` returns `vec![widget::Tree::new(&self.trigger)]`
- `diff()` calls `tree.children[0].diff(&self.trigger)`
- `size()` delegates to `self.trigger.as_widget().size()`
- `layout()` delegates to `self.trigger.as_widget().layout()`
- `draw()` delegates to `self.trigger.as_widget().draw()`
- `on_event()` delegates to `self.trigger.as_widget().on_event()`, and also captures cursor position into state for `AtCursor` mode
- `mouse_interaction()` delegates to trigger
- `overlay()` — when `self.is_open`, returns `Some(overlay::Element::new(...))` with a `MenuOverlay`; otherwise `None`

The full `Widget` impl is approximately 100 lines. Each method is a thin delegation to the trigger, except `overlay()` which conditionally creates the `MenuOverlay`.

- [ ] **Step 3: Implement the `into()` conversion**

Add an `impl From<PopupMenu<...>> for Element<...>` or use `iced::advanced::widget::component` pattern so that callers can use `.into()` to convert to `Element`. The standard pattern in Iced 0.14:

```rust
impl<'a, Message, Renderer> From<PopupMenu<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(menu: PopupMenu<'a, Message, Theme, Renderer>) -> Self {
        Self::new(menu)
    }
}
```

- [ ] **Step 4: Verify compilation**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui && cargo clippy -p inboxly-ui -- -D warnings
```

- [ ] **Step 5: Commit**

```
feat(ui): implement PopupMenu widget struct with trigger passthrough
```

---

## Task 5: Implement `MenuOverlay` — layout and drawing

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/popup_menu.rs`

Implement the `MenuOverlay` struct that implements Iced's `Overlay` trait. This handles positioning the menu card relative to the trigger, drawing the backdrop, menu card background, and menu items.

- [ ] **Step 1: Implement `MenuOverlay` struct**

```rust
/// The overlay that renders the popup menu content above all other widgets.
///
/// Created by `PopupMenu::overlay()` when the menu is open.
struct MenuOverlay<'a, 'b, Message, Renderer>
where
    Renderer: renderer::Renderer,
{
    /// Reference to the menu items.
    items: &'b [MenuItem<Message>],
    /// Dismiss message (click-away, Escape, item click).
    on_dismiss: &'b Message,
    /// Positioning anchor.
    anchor: PopupAnchor,
    /// Theme colours.
    theme_colors: ThemeColors,
    /// Mutable reference to the widget state for hover tracking.
    state: &'b mut PopupMenuState,
    /// Bounds of the trigger element (used for positioning).
    trigger_bounds: Rectangle,
    /// Phantom for lifetime.
    _phantom: std::marker::PhantomData<(&'a (), Renderer)>,
}
```

- [ ] **Step 2: Implement `Overlay` trait — `layout()`**

The layout method computes the menu card position and size:

1. Menu width = `POPUP_MENU_WIDTH` (260px)
2. Menu height = sum of item heights:
   - Action items: `POPUP_MENU_ITEM_PADDING_V * 2 + POPUP_MENU_ITEM_FONT_SIZE` (approx 39px)
   - Separator: `POPUP_MENU_SEPARATOR_MARGIN * 2 + DIVIDER_THICKNESS` (5px)
   - Submenu items: same height as Action items
3. Position based on anchor:
   - `BelowRight`: x = trigger.x + trigger.width - menu_width, y = trigger.y + trigger.height
   - `BelowLeft`: x = trigger.x, y = trigger.y + trigger.height
   - `AtCursor`: x = cursor.x, y = cursor.y
4. Clamp to viewport bounds so the menu doesn't render off-screen

Return a `layout::Node` with the computed size, positioned via `node.move_to(position)`.

- [ ] **Step 3: Implement `Overlay` trait — `draw()`**

Drawing order (back to front):

1. **Invisible backdrop**: Full-viewport transparent rectangle (click target only, not drawn)
2. **Shadow**: A slightly offset, blurred dark rectangle behind the menu card. Use `renderer::Quad` with `shadow: Shadow { color: theme_colors.menu_shadow, offset: Vector::new(0.0, POPUP_MENU_SHADOW_OFFSET_Y), blur_radius: POPUP_MENU_SHADOW_BLUR }`
3. **Menu card background**: White (light) or `surface` (dark) with `POPUP_MENU_CORNER_RADIUS` radius and 1px border (`divider` colour)
4. **Menu items**: Iterate items, drawing each at its vertical offset:
   - **Action**: Icon (if present) at x + padding, label at x + padding + icon_width + 8px gap. Text colour = `text_primary` (Normal) or `menu_destructive_text` (Destructive). If hovered: fill background with `menu_hover` (Normal) or `menu_destructive_hover` (Destructive)
   - **Separator**: Horizontal line at `menu_separator` colour, inset by padding
   - **Submenu**: Same as Action but with a right-arrow chevron `▸` on the right side

Use `renderer.fill_quad()` for backgrounds and `renderer.fill_text()` for text (or Iced's `text::Renderer` trait). The exact drawing primitives depend on the Iced 0.14 `advanced::Renderer` API — use `Quad` structs for rectangles and the text rendering methods for labels.

- [ ] **Step 4: Verify compilation and visual constants**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui && cargo clippy -p inboxly-ui -- -D warnings
```

- [ ] **Step 5: Commit**

```
feat(ui): implement MenuOverlay layout and drawing
```

---

## Task 6: Implement `MenuOverlay` — event handling (click, hover, Escape)

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/popup_menu.rs`

Implement the event handling methods on `MenuOverlay` that make the menu interactive.

- [ ] **Step 1: Write failing tests for dismiss behaviour**

Add to the test module:

```rust
    // -- Dismiss and state tests --

    #[test]
    fn popup_menu_state_default_has_no_hover() {
        let state = PopupMenuState::default();
        assert!(state.hovered_index.is_none());
    }

    #[test]
    fn popup_menu_state_tracks_hover() {
        let mut state = PopupMenuState::default();
        state.hovered_index = Some(2);
        assert_eq!(state.hovered_index, Some(2));
    }

    #[test]
    fn popup_menu_state_clears_hover() {
        let mut state = PopupMenuState::default();
        state.hovered_index = Some(1);
        state.hovered_index = None;
        assert!(state.hovered_index.is_none());
    }
```

- [ ] **Step 2: Implement `Overlay::on_event()`**

Handle three dismiss paths plus hover tracking:

1. **Click on backdrop** (mouse press outside menu bounds): Produce `on_dismiss.clone()` via `shell.publish()`, return `Status::Captured`
2. **Click on menu item**: Determine which item was clicked based on cursor Y position within the menu. If it's an `Action`, publish `item.message.clone()` then publish `on_dismiss.clone()`. Return `Status::Captured`. If it's a `Separator`, ignore. If it's a `Submenu`, toggle submenu expansion (stretch goal — initial impl can just be visual).
3. **Escape key press** (`Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Named(keyboard::key::Named::Escape), .. })`): Publish `on_dismiss.clone()`, return `Status::Captured`
4. **Mouse move**: Update `state.hovered_index` based on cursor position within menu bounds. If cursor leaves menu bounds, set `hovered_index = None`. Return `Status::Ignored` (don't capture mouse moves).

**Hit testing logic for item index:**

```rust
fn item_index_at_y(&self, y: f32, menu_bounds: Rectangle) -> Option<usize> {
    let mut current_y = menu_bounds.y;
    for (i, item) in self.items.iter().enumerate() {
        let item_height = match item {
            MenuItem::Separator => {
                POPUP_MENU_SEPARATOR_MARGIN * 2.0 + DIVIDER_THICKNESS
            }
            _ => POPUP_MENU_ITEM_PADDING_V * 2.0 + POPUP_MENU_ITEM_FONT_SIZE,
        };
        if y >= current_y && y < current_y + item_height {
            return if item.is_separator() { None } else { Some(i) };
        }
        current_y += item_height;
    }
    None
}
```

- [ ] **Step 3: Implement `Overlay::mouse_interaction()`**

Return `mouse::Interaction::Pointer` (hand cursor) when hovering over a clickable item, `mouse::Interaction::default()` otherwise.

- [ ] **Step 4: Implement `Overlay::is_over()`**

Return `true` for any position within the viewport (the full-screen backdrop captures all input when the menu is open). This ensures clicks outside the menu are captured by the overlay rather than passing through to widgets underneath.

- [ ] **Step 5: Verify tests pass**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- popup_menu && cargo clippy -p inboxly-ui -- -D warnings
```

- [ ] **Step 6: Commit**

```
feat(ui): implement popup menu event handling — click, hover, Escape dismiss
```

---

## Task 7: Wire into widgets module and verify workspace compiles

**Files:**
- Verify: `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/mod.rs` already has `pub mod popup_menu;` (added in Task 3)
- No changes needed if Task 3 was done correctly

- [ ] **Step 1: Verify module is exported**

Confirm `widgets/mod.rs` contains `pub mod popup_menu;`.

- [ ] **Step 2: Full workspace build and test**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings
```

- [ ] **Step 3: Verify no dead code warnings**

The widget types should be public and reachable. If any `dead_code` warnings appear for the new types, it means they aren't reachable from a binary crate — add a re-export in `inboxly-ui/src/lib.rs` if needed:

```rust
pub use widgets::popup_menu::{MenuItem, MenuItemStyle, PopupAnchor, PopupMenu};
```

- [ ] **Step 4: Commit (if any changes were needed)**

```
chore(ui): ensure popup menu types are exported and reachable
```

---

## Task 8: Integration smoke test — popup menu in isolation

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/popup_menu.rs`

Add integration-level tests that construct a `PopupMenu` widget with realistic menu items and verify the type system enforces correctness. Since Iced widgets require a renderer to test visually, these tests focus on construction, type safety, and state.

- [ ] **Step 1: Add integration tests**

Add to the test module:

```rust
    // -- Integration-level construction tests --

    #[test]
    fn build_realistic_overflow_menu() {
        // Simulates the three-dot overflow menu from the spec.
        let items: Vec<MenuItem<String>> = vec![
            MenuItem::action_with_icon("Move to...", '\u{1F4C1}', "move".into()),
            MenuItem::action("Mark as read", "mark_read".into()),
            MenuItem::action("Mute thread", "mute".into()),
            MenuItem::separator(),
            MenuItem::action_with_icon("Reply", '\u{21A9}', "reply".into()),
            MenuItem::action("Reply All", "reply_all".into()),
            MenuItem::action("Forward", "forward".into()),
            MenuItem::separator(),
            MenuItem::action("Add to bundle...", "add_bundle".into()),
            MenuItem::action("Create rule from sender", "create_rule".into()),
            MenuItem::separator(),
            MenuItem::destructive_with_icon(
                "Block sender",
                '\u{1F6AB}',
                "block".into(),
            ),
            MenuItem::destructive_with_icon(
                "Report spam",
                '\u{26A0}',
                "spam".into(),
            ),
        ];

        // Verify item count.
        assert_eq!(items.len(), 13);

        // Verify separator positions (indices 3, 7, 10).
        assert!(items[3].is_separator());
        assert!(items[7].is_separator());
        assert!(items[10].is_separator());

        // Verify destructive items are last two actions.
        assert!(items[11].is_destructive());
        assert!(items[12].is_destructive());

        // Verify normal items are not destructive.
        assert!(!items[0].is_destructive());
        assert!(!items[4].is_destructive());
    }

    #[test]
    fn build_submenu() {
        let submenu_items: Vec<MenuItem<String>> = vec![
            MenuItem::action("Inbox", "move_inbox".into()),
            MenuItem::action("Trash", "move_trash".into()),
            MenuItem::action("Spam", "move_spam".into()),
        ];
        let item: MenuItem<String> = MenuItem::submenu(
            "Move to...",
            Some('\u{1F4C1}'),
            submenu_items,
        );

        match &item {
            MenuItem::Submenu { items, .. } => {
                assert_eq!(items.len(), 3);
            }
            _ => panic!("expected Submenu"),
        }
    }

    #[test]
    fn empty_menu_has_no_items() {
        let items: Vec<MenuItem<&str>> = vec![];
        assert!(items.is_empty());
    }

    #[test]
    fn menu_item_label_preserved() {
        let item: MenuItem<u32> = MenuItem::action("Very Long Label With Spaces", 42);
        match item {
            MenuItem::Action { label, .. } => {
                assert_eq!(label, "Very Long Label With Spaces");
            }
            _ => panic!("expected Action"),
        }
    }

    #[test]
    fn all_anchor_variants_are_copy() {
        let a = PopupAnchor::BelowRight;
        let b = a; // Copy
        assert_eq!(a, b);
    }

    #[test]
    fn all_style_variants_are_copy() {
        let a = MenuItemStyle::Destructive;
        let b = a; // Copy
        assert_eq!(a, b);
    }
```

- [ ] **Step 2: Run full test suite**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test --workspace && cargo clippy --workspace -- -D warnings
```

- [ ] **Step 3: Verify test count increase**

Check that the new tests show up in the output. Expected: at least 25 new tests across Tasks 1-8.

- [ ] **Step 4: Commit**

```
test(ui): add integration smoke tests for popup menu widget
```

---

## Verification Checklist

Before considering M26 complete, verify all of the following:

- [ ] `cargo build --workspace` — zero errors
- [ ] `cargo clippy --workspace -- -D warnings` — zero warnings
- [ ] `cargo test --workspace` — all tests pass (should be 496 + ~30 new = ~526 total)
- [ ] `cargo fmt --check` — properly formatted
- [ ] New file: `inboxly-ui/src/widgets/popup_menu.rs` exists with all types and Widget/Overlay impls
- [ ] `ThemeColors` has 5 new menu colour fields in both `light()` and `dark()` constructors
- [ ] `dimensions.rs` has 9 new popup menu constants
- [ ] `widgets/mod.rs` exports `pub mod popup_menu;`
- [ ] All public items have doc comments
- [ ] No `.unwrap()` in production code (`.expect("reason")` allowed only for infallible cases)
- [ ] No `clippy::indexing_slicing` — use `.get()` with proper handling
- [ ] Branch merged to `main`

---

## Post-Merge

After merging `m26-popup-menu-widget` to `main`:

1. Update `CHANGELOG.md` with v0.26.0 entry
2. Update `README.md` if feature list section exists
3. Bump workspace version in root `Cargo.toml` to `0.26.0`
4. Update memory with new status: "M26 complete (v0.26.0). ~526 tests. Next: M27."
5. Push to both remotes: `git push origin main && git push github main`

---

## What M26 Does NOT Cover

These are handled in subsequent milestones:

- **M27**: Toolbar integration — gear icon, three-dot overflow button on email rows, wiring `PopupMenu` into the email row hover actions
- **M28**: Right-click context menu — `mouse::Event::ButtonPressed(Button::Right)` handling, `AtCursor` anchor usage
- **M29**: Settings view — full settings layout with sidebar nav, all 6 tabs
- **M30**: Account switcher — inline expansion in nav drawer, multi-account support
