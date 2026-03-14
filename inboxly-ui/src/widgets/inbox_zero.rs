//! Inbox Zero Sun illustration -- the signature "all done" view.
//!
//! Shows a sunny landscape illustration when the inbox is empty,
//! matching Google Inbox's celebratory Inbox Zero state.
//! For v1, uses a text-based placeholder with emoji art.

use iced::widget::{Space, center, column, text};
use iced::{Color, Element, Length};

/// Build the Inbox Zero celebration view.
///
/// Displays a large sun emoji, "You're all done!" heading, and
/// a cheerful subtext. Full SVG illustration deferred to post-v1.
pub fn inbox_zero<'a, Message: 'a>(
    primary_text: Color,
    secondary_text: Color,
) -> Element<'a, Message> {
    let sun = text("\u{2600}\u{FE0F}") // sun emoji
        .size(64.0)
        .color(Color::from_rgb(1.0, 0.84, 0.0)); // gold

    let heading = text("You're all done!")
        .size(28.0)
        .color(primary_text)
        .font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..Default::default()
        });

    let subtext = text("Enjoy your day.").size(16.0).color(secondary_text);

    center(
        column![
            sun,
            Space::new().height(16.0),
            heading,
            Space::new().height(8.0),
            subtext,
        ]
        .align_x(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
