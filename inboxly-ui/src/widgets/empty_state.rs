//! Empty inbox placeholder for when there are zero items.

use iced::widget::{Space, center, column, text};
use iced::{Color, Element, Length};

/// Build an empty state element for when the inbox has no items.
///
/// Displays a centered "You're all done!" message. The full "Inbox Zero Sun"
/// illustration is deferred to M25 -- this is a text placeholder.
pub fn empty_inbox<'a, Message: 'a>(secondary_text_color: Color) -> Element<'a, Message> {
    let heading = text("You're all done!")
        .size(24.0)
        .color(secondary_text_color)
        .font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..Default::default()
        });

    let subtext = text("Nothing in your inbox. Enjoy your day.")
        .size(16.0)
        .color(secondary_text_color);

    center(column![heading, Space::new().height(8.0), subtext].align_x(iced::Alignment::Center))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
