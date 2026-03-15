//! Right-click area widget -- intercepts right-click events and emits a message
//! with the cursor position, enabling context menus.
//!
//! Delegates all layout, drawing, and non-right-click event handling to the
//! wrapped child element. Only captures `mouse::Button::Right` press events
//! when the cursor is within bounds.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Shell, overlay};
use iced::mouse;
use iced::{Element, Event, Length, Rectangle, Size, Vector};

/// A wrapper widget that intercepts right-click events and emits a message
/// with the cursor position.
///
/// All other events and rendering are delegated to the wrapped content element.
pub struct RightClickArea<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: iced::advanced::renderer::Renderer,
{
    content: Element<'a, Message, Theme, Renderer>,
    on_right_click: Option<Box<dyn Fn(iced::Point) -> Message + 'a>>,
}

impl<'a, Message, Theme, Renderer> RightClickArea<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::renderer::Renderer,
{
    /// Create a new right-click area wrapping the given content element.
    pub fn new(content: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        Self {
            content: content.into(),
            on_right_click: None,
        }
    }

    /// Set the callback for right-click events.
    ///
    /// The callback receives the cursor position (in window coordinates)
    /// where the right-click occurred.
    pub fn on_right_click(mut self, f: impl Fn(iced::Point) -> Message + 'a) -> Self {
        self.on_right_click = Some(Box::new(f));
        self
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for RightClickArea<'_, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: iced::advanced::renderer::Renderer,
{
    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
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
        self.content.as_widget().draw(
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
        // Intercept right-click events within our bounds.
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) = event
            && let Some(ref on_right_click) = self.on_right_click
            && let Some(position) = cursor.position_over(layout.bounds())
        {
            shell.publish(on_right_click(position));
            shell.capture_event();
            return;
        }

        // Delegate all other events to the child.
        self.content.as_widget_mut().update(
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
        self.content.as_widget().mouse_interaction(
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
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<RightClickArea<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: 'a,
    Renderer: iced::advanced::renderer::Renderer + 'a,
{
    fn from(area: RightClickArea<'a, Message, Theme, Renderer>) -> Self {
        Self::new(area)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_click_area_can_be_constructed() {
        // Verify the type can be constructed with a simple element.
        // Visual/event testing requires the Iced runtime.
        let content: Element<'_, String> = iced::widget::text("test").into();
        let _area: RightClickArea<'_, String> =
            RightClickArea::new(content).on_right_click(|pos| format!("click at {pos:?}"));
    }
}
