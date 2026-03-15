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

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::text;
use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::keyboard;
use iced::mouse;
use iced::{Border, Element, Event, Length, Point, Rectangle, Shadow, Size, Vector};

use crate::theme::colors::ThemeColors;
use crate::theme::dimensions::{
    DIVIDER_THICKNESS, POPUP_MENU_CORNER_RADIUS, POPUP_MENU_ICON_WIDTH, POPUP_MENU_ITEM_FONT_SIZE,
    POPUP_MENU_ITEM_PADDING_H, POPUP_MENU_ITEM_PADDING_V, POPUP_MENU_SEPARATOR_MARGIN,
    POPUP_MENU_SHADOW_BLUR, POPUP_MENU_SHADOW_OFFSET_Y, POPUP_MENU_WIDTH,
};

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
    pub fn action_with_icon(label: impl Into<String>, icon: char, message: Message) -> Self {
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
    pub fn destructive_with_icon(label: impl Into<String>, icon: char, message: Message) -> Self {
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

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

/// Internal state stored in the widget tree.
#[derive(Debug, Default)]
pub(crate) struct PopupMenuState {
    /// Reserved for future keyboard arrow-key navigation. Currently unused.
    #[allow(dead_code)]
    pub(crate) hovered_index: Option<usize>,
    /// Cursor position for AtCursor anchor mode.
    pub(crate) cursor_position: Point,
}

// ---------------------------------------------------------------------------
// PopupMenu widget
// ---------------------------------------------------------------------------

/// A popup menu widget that wraps a trigger element and optionally
/// renders a dropdown overlay.
pub struct PopupMenu<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: iced::advanced::renderer::Renderer,
{
    trigger: Element<'a, Message, Theme, Renderer>,
    items: Vec<MenuItem<Message>>,
    is_open: bool,
    anchor: PopupAnchor,
    on_dismiss: Message,
    theme_colors: ThemeColors,
}

impl<'a, Message, Theme, Renderer> PopupMenu<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: iced::advanced::renderer::Renderer,
{
    /// Create a new popup menu wrapping the given trigger element.
    ///
    /// The menu starts closed. Call [`open`](Self::open) to display it.
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

    /// Set whether the menu is currently open.
    pub fn open(mut self, is_open: bool) -> Self {
        self.is_open = is_open;
        self
    }

    /// Set the anchor positioning mode.
    pub fn anchor(mut self, anchor: PopupAnchor) -> Self {
        self.anchor = anchor;
        self
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for PopupMenu<'_, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: iced::advanced::renderer::Renderer + text::Renderer<Font = iced::Font>,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<PopupMenuState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(PopupMenuState::default())
    }

    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.trigger)]
    }

    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(std::slice::from_ref(&self.trigger));
    }

    fn size(&self) -> Size<Length> {
        self.trigger.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.trigger
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.trigger.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        // Capture cursor position for AtCursor anchor mode.
        if let Event::Mouse(mouse::Event::CursorMoved { position }) = event {
            let state = tree.state.downcast_mut::<PopupMenuState>();
            state.cursor_position = *position;
        }

        self.trigger.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.trigger.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        if self.is_open {
            let state = tree.state.downcast_ref::<PopupMenuState>();
            let trigger_bounds = layout.bounds();

            let menu_overlay = MenuOverlay {
                items: &self.items,
                anchor: self.anchor,
                on_dismiss: &self.on_dismiss,
                theme_colors: self.theme_colors,
                trigger_bounds,
                cursor_position: state.cursor_position,
            };

            Some(overlay::Element::new(Box::new(menu_overlay)))
        } else {
            // When closed, delegate to trigger's overlay if any.
            self.trigger.as_widget_mut().overlay(
                &mut tree.children[0],
                layout,
                renderer,
                viewport,
                translation,
            )
        }
    }
}

impl<'a, Message, Theme, Renderer> From<PopupMenu<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: 'a,
    Renderer: iced::advanced::renderer::Renderer + text::Renderer<Font = iced::Font> + 'a,
{
    fn from(menu: PopupMenu<'a, Message, Theme, Renderer>) -> Self {
        Self::new(menu)
    }
}

// ---------------------------------------------------------------------------
// MenuOverlay
// ---------------------------------------------------------------------------

/// Overlay that renders the popup menu card and handles interaction.
struct MenuOverlay<'a, Message> {
    items: &'a [MenuItem<Message>],
    anchor: PopupAnchor,
    on_dismiss: &'a Message,
    theme_colors: ThemeColors,
    trigger_bounds: Rectangle,
    cursor_position: Point,
}

impl<'a, Message> MenuOverlay<'a, Message> {
    /// Compute the height of a single menu item.
    fn item_height(item: &MenuItem<Message>) -> f32 {
        match item {
            MenuItem::Separator => POPUP_MENU_SEPARATOR_MARGIN * 2.0 + DIVIDER_THICKNESS,
            _ => POPUP_MENU_ITEM_PADDING_V * 2.0 + POPUP_MENU_ITEM_FONT_SIZE,
        }
    }

    /// Compute total menu card height from all items.
    fn total_height(items: &[MenuItem<Message>]) -> f32 {
        items.iter().map(Self::item_height).sum()
    }

    /// Compute the position of the menu card top-left corner.
    fn menu_position(&self, menu_size: Size, viewport: Size) -> Point {
        let (mut x, mut y) = match self.anchor {
            PopupAnchor::BelowRight => {
                // Right-align to trigger's right edge.
                let x = self.trigger_bounds.x + self.trigger_bounds.width - menu_size.width;
                let y = self.trigger_bounds.y + self.trigger_bounds.height;
                (x, y)
            }
            PopupAnchor::BelowLeft => {
                // Left-align to trigger's left edge.
                let x = self.trigger_bounds.x;
                let y = self.trigger_bounds.y + self.trigger_bounds.height;
                (x, y)
            }
            PopupAnchor::AtCursor => (self.cursor_position.x, self.cursor_position.y),
        };

        // Clamp to viewport bounds.
        if x + menu_size.width > viewport.width {
            x = viewport.width - menu_size.width;
        }
        if x < 0.0 {
            x = 0.0;
        }
        if y + menu_size.height > viewport.height {
            // Try placing above the trigger instead.
            let above = self.trigger_bounds.y - menu_size.height;
            if above >= 0.0 {
                y = above;
            } else {
                y = viewport.height - menu_size.height;
            }
        }
        if y < 0.0 {
            y = 0.0;
        }

        Point::new(x, y)
    }

    /// Return the index of the clickable item at the given cursor Y
    /// relative to the menu card top, or `None` if over a separator
    /// or outside.
    fn item_index_at(&self, cursor_y: f32) -> Option<usize> {
        let mut y = 0.0;
        for (i, item) in self.items.iter().enumerate() {
            let h = Self::item_height(item);
            if cursor_y >= y && cursor_y < y + h {
                return match item {
                    MenuItem::Separator => None,
                    _ => Some(i),
                };
            }
            y += h;
        }
        None
    }
}

impl<Message, Theme, Renderer> overlay::Overlay<Message, Theme, Renderer>
    for MenuOverlay<'_, Message>
where
    Message: Clone,
    Renderer: iced::advanced::renderer::Renderer + text::Renderer<Font = iced::Font>,
{
    fn layout(&mut self, _renderer: &Renderer, bounds: Size) -> layout::Node {
        let menu_height = Self::total_height(self.items);
        let menu_size = Size::new(POPUP_MENU_WIDTH, menu_height);
        let position = self.menu_position(menu_size, bounds);

        // The overlay node covers the full viewport (backdrop).
        // The first child is the menu card itself.
        let menu_node = layout::Node::new(menu_size).move_to(position);
        layout::Node::with_children(bounds, vec![menu_node])
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let menu_layout = layout.children().next().expect("overlay layout always has menu card child");
        let menu_bounds = menu_layout.bounds();

        // -- Shadow --
        renderer.fill_quad(
            renderer::Quad {
                bounds: menu_bounds,
                border: Border {
                    radius: POPUP_MENU_CORNER_RADIUS.into(),
                    ..Border::default()
                },
                shadow: Shadow {
                    color: self.theme_colors.menu_shadow,
                    offset: Vector::new(0.0, POPUP_MENU_SHADOW_OFFSET_Y),
                    blur_radius: POPUP_MENU_SHADOW_BLUR,
                },
                ..renderer::Quad::default()
            },
            self.theme_colors.surface,
        );

        // -- Menu card background --
        renderer.fill_quad(
            renderer::Quad {
                bounds: menu_bounds,
                border: Border {
                    radius: POPUP_MENU_CORNER_RADIUS.into(),
                    ..Border::default()
                },
                ..renderer::Quad::default()
            },
            self.theme_colors.surface,
        );

        // -- Draw each item --
        let cursor_pos = cursor.position();
        let mut y = menu_bounds.y;

        for item in self.items {
            let item_h = Self::item_height(item);
            let item_bounds = Rectangle {
                x: menu_bounds.x,
                y,
                width: menu_bounds.width,
                height: item_h,
            };

            match item {
                MenuItem::Separator => {
                    // Draw a horizontal separator line.
                    let line_y = y + POPUP_MENU_SEPARATOR_MARGIN;
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle {
                                x: menu_bounds.x,
                                y: line_y,
                                width: menu_bounds.width,
                                height: DIVIDER_THICKNESS,
                            },
                            ..renderer::Quad::default()
                        },
                        self.theme_colors.menu_separator,
                    );
                }
                MenuItem::Action {
                    label,
                    icon,
                    style: item_style,
                    ..
                } => {
                    let is_destructive = *item_style == MenuItemStyle::Destructive;

                    // Hover highlight.
                    let is_hovered = cursor_pos.is_some_and(|p| item_bounds.contains(p));
                    if is_hovered {
                        let hover_bg = if is_destructive {
                            self.theme_colors.menu_destructive_hover
                        } else {
                            self.theme_colors.menu_hover
                        };
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: item_bounds,
                                ..renderer::Quad::default()
                            },
                            hover_bg,
                        );
                    }

                    // Text colour.
                    let text_color = if is_destructive {
                        self.theme_colors.menu_destructive_text
                    } else {
                        self.theme_colors.text_primary
                    };

                    let text_x = menu_bounds.x + POPUP_MENU_ITEM_PADDING_H;
                    let text_y = y + POPUP_MENU_ITEM_PADDING_V;
                    let mut label_x = text_x;

                    // Draw icon if present.
                    if let Some(icon_char) = icon {
                        let icon_text = iced::advanced::text::Text {
                            content: String::from(*icon_char),
                            bounds: Size::new(POPUP_MENU_ICON_WIDTH, POPUP_MENU_ITEM_FONT_SIZE),
                            size: POPUP_MENU_ITEM_FONT_SIZE.into(),
                            line_height: text::LineHeight::default(),
                            font: renderer.default_font(),
                            align_x: text::Alignment::Default,
                            align_y: iced::alignment::Vertical::Top,
                            shaping: text::Shaping::Advanced,
                            wrapping: text::Wrapping::None,
                        };
                        renderer.fill_text(
                            icon_text,
                            Point::new(text_x, text_y),
                            text_color,
                            menu_bounds,
                        );
                        label_x += POPUP_MENU_ICON_WIDTH;
                    }

                    // Draw label text.
                    let label_text = iced::advanced::text::Text {
                        content: label.clone(),
                        bounds: Size::new(
                            menu_bounds.width
                                - POPUP_MENU_ITEM_PADDING_H * 2.0
                                - if icon.is_some() {
                                    POPUP_MENU_ICON_WIDTH
                                } else {
                                    0.0
                                },
                            POPUP_MENU_ITEM_FONT_SIZE,
                        ),
                        size: POPUP_MENU_ITEM_FONT_SIZE.into(),
                        line_height: text::LineHeight::default(),
                        font: renderer.default_font(),
                        align_x: text::Alignment::Default,
                        align_y: iced::alignment::Vertical::Top,
                        shaping: text::Shaping::Advanced,
                        wrapping: text::Wrapping::None,
                    };
                    renderer.fill_text(
                        label_text,
                        Point::new(label_x, text_y),
                        text_color,
                        menu_bounds,
                    );
                }
                MenuItem::Submenu { label, icon, .. } => {
                    // Hover highlight.
                    let is_hovered = cursor_pos.is_some_and(|p| item_bounds.contains(p));
                    if is_hovered {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: item_bounds,
                                ..renderer::Quad::default()
                            },
                            self.theme_colors.menu_hover,
                        );
                    }

                    let text_x = menu_bounds.x + POPUP_MENU_ITEM_PADDING_H;
                    let text_y = y + POPUP_MENU_ITEM_PADDING_V;
                    let mut label_x = text_x;

                    // Draw icon if present.
                    if let Some(icon_char) = icon {
                        let icon_text = iced::advanced::text::Text {
                            content: String::from(*icon_char),
                            bounds: Size::new(POPUP_MENU_ICON_WIDTH, POPUP_MENU_ITEM_FONT_SIZE),
                            size: POPUP_MENU_ITEM_FONT_SIZE.into(),
                            line_height: text::LineHeight::default(),
                            font: renderer.default_font(),
                            align_x: text::Alignment::Default,
                            align_y: iced::alignment::Vertical::Top,
                            shaping: text::Shaping::Advanced,
                            wrapping: text::Wrapping::None,
                        };
                        renderer.fill_text(
                            icon_text,
                            Point::new(text_x, text_y),
                            self.theme_colors.text_primary,
                            menu_bounds,
                        );
                        label_x += POPUP_MENU_ICON_WIDTH;
                    }

                    // Draw label.
                    let label_text = iced::advanced::text::Text {
                        content: label.clone(),
                        bounds: Size::new(
                            menu_bounds.width
                                - POPUP_MENU_ITEM_PADDING_H * 2.0
                                - if icon.is_some() {
                                    POPUP_MENU_ICON_WIDTH
                                } else {
                                    0.0
                                },
                            POPUP_MENU_ITEM_FONT_SIZE,
                        ),
                        size: POPUP_MENU_ITEM_FONT_SIZE.into(),
                        line_height: text::LineHeight::default(),
                        font: renderer.default_font(),
                        align_x: text::Alignment::Default,
                        align_y: iced::alignment::Vertical::Top,
                        shaping: text::Shaping::Advanced,
                        wrapping: text::Wrapping::None,
                    };
                    renderer.fill_text(
                        label_text,
                        Point::new(label_x, text_y),
                        self.theme_colors.text_primary,
                        menu_bounds,
                    );

                    // Draw chevron (right arrow) for submenu indicator.
                    let chevron_text = iced::advanced::text::Text {
                        content: String::from('\u{203A}'), // single right-pointing angle
                        bounds: Size::new(POPUP_MENU_ITEM_FONT_SIZE, POPUP_MENU_ITEM_FONT_SIZE),
                        size: POPUP_MENU_ITEM_FONT_SIZE.into(),
                        line_height: text::LineHeight::default(),
                        font: renderer.default_font(),
                        align_x: text::Alignment::Right,
                        align_y: iced::alignment::Vertical::Top,
                        shaping: text::Shaping::Advanced,
                        wrapping: text::Wrapping::None,
                    };
                    renderer.fill_text(
                        chevron_text,
                        Point::new(
                            menu_bounds.x + menu_bounds.width - POPUP_MENU_ITEM_PADDING_H,
                            text_y,
                        ),
                        self.theme_colors.text_secondary,
                        menu_bounds,
                    );
                }
            }
            y += item_h;
        }
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let menu_layout = layout.children().next().expect("overlay layout always has menu card child");
        let menu_bounds = menu_layout.bounds();

        match event {
            // Escape key dismisses the menu.
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            }) => {
                shell.publish(self.on_dismiss.clone());
                shell.capture_event();
            }

            // Mouse button press.
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = cursor.position() {
                    if menu_bounds.contains(pos) {
                        // Click inside menu — find which item.
                        let relative_y = pos.y - menu_bounds.y;
                        if let Some(idx) = self.item_index_at(relative_y) {
                            match &self.items[idx] {
                                MenuItem::Action { message, .. } => {
                                    shell.publish(message.clone());
                                    shell.publish(self.on_dismiss.clone());
                                    shell.capture_event();
                                }
                                MenuItem::Submenu { .. } => {
                                    // Submenu expansion is a future enhancement.
                                    shell.capture_event();
                                }
                                MenuItem::Separator => {}
                            }
                        } else {
                            // Clicked on separator — just capture event.
                            shell.capture_event();
                        }
                    } else {
                        // Click outside menu — dismiss (backdrop).
                        shell.publish(self.on_dismiss.clone());
                        shell.capture_event();
                    }
                }
            }

            // Mouse move events are not captured — other widgets may need them.
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {}

            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let menu_layout = layout.children().next().expect("overlay layout always has menu card child");
        let menu_bounds = menu_layout.bounds();

        if let Some(pos) = cursor.position()
            && menu_bounds.contains(pos)
        {
            let relative_y = pos.y - menu_bounds.y;
            if self.item_index_at(relative_y).is_some() {
                return mouse::Interaction::Pointer;
            }
        }

        mouse::Interaction::default()
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
        let item: MenuItem<&str> = MenuItem::action_with_icon("Reply", '\u{21A9}', "reply");
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
            MenuItem::Action { icon, style, .. } => {
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
        let item: MenuItem<&str> = MenuItem::submenu("Move to...", Some('\u{1F4C1}'), children);
        match item {
            MenuItem::Submenu { label, icon, items } => {
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

    // -- PopupMenuState tests --

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

    // -- Integration-level construction tests --

    #[test]
    fn build_realistic_overflow_menu() {
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
            MenuItem::destructive_with_icon("Block sender", '\u{1F6AB}', "block".into()),
            MenuItem::destructive_with_icon("Report spam", '\u{26A0}', "spam".into()),
        ];

        assert_eq!(items.len(), 13);
        assert!(items[3].is_separator());
        assert!(items[7].is_separator());
        assert!(items[10].is_separator());
        assert!(items[11].is_destructive());
        assert!(items[12].is_destructive());
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
        let item: MenuItem<String> =
            MenuItem::submenu("Move to...", Some('\u{1F4C1}'), submenu_items);

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
}
