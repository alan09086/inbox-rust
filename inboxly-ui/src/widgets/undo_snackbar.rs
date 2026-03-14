//! Undo snackbar -- bottom-of-screen notification with undo button.

use iced::widget::{button, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::theme::dimensions::DEFAULT_PADDING;

/// Build the undo snackbar element.
///
/// Displays the action description text and an "Undo" button.
/// The snackbar appears at the bottom of the content area.
pub fn undo_snackbar<'a, Message: 'a + Clone>(
    description: &str,
    on_undo: Message,
    surface_color: Color,
    text_color: Color,
    accent_color: Color,
) -> Element<'a, Message> {
    let desc = text(description.to_owned()).size(14.0).color(text_color);

    let undo_btn = button(text("Undo").size(14.0).color(accent_color))
        .on_press(on_undo)
        .padding([4.0, 12.0])
        .style(move |_theme, _status| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: accent_color,
            border: Border::default(),
            ..Default::default()
        });

    let content = row![desc, undo_btn]
        .spacing(16.0)
        .align_y(Alignment::Center)
        .padding([8.0, DEFAULT_PADDING]);

    container(content)
        .width(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(surface_color)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}
