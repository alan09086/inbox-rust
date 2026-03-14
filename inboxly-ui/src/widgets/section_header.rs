//! Section header widget -- 48dp tall, 14sp bold grey text.

use iced::widget::{container, row, text};
use iced::{Alignment, Color, Element, Length};

use crate::feed::DateGroup;
use crate::theme::dimensions::{DEFAULT_PADDING, SECTION_HEADER_HEIGHT};
use crate::theme::typography::{SECTION_HEADER_SIZE, SECTION_HEADER_WEIGHT};

/// Build a section header element for the given date group.
///
/// Renders as a 48dp tall row with bold grey text (14sp) left-aligned.
pub fn section_header<'a, Message: 'a>(
    group: DateGroup,
    secondary_text_color: Color,
) -> Element<'a, Message> {
    let label = text(group.label())
        .size(SECTION_HEADER_SIZE)
        .color(secondary_text_color)
        .font(iced::Font {
            weight: SECTION_HEADER_WEIGHT,
            ..Default::default()
        });

    container(row![label].align_y(Alignment::Center))
        .height(SECTION_HEADER_HEIGHT)
        .width(Length::Fill)
        .padding([0.0, DEFAULT_PADDING])
        .align_y(iced::alignment::Vertical::Center)
        .into()
}
