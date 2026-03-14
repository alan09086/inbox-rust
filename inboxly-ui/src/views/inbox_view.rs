//! Scrollable inbox feed view -- section headers + email rows.

use iced::widget::{Column, scrollable};
use iced::{Color, Element, Length};

use crate::feed::FeedSection;
use crate::widgets::email_row::email_row;
use crate::widgets::empty_state::empty_inbox;
use crate::widgets::section_header::section_header;

/// Build the scrollable inbox feed view.
///
/// Renders section headers and email rows from pre-built feed sections.
/// Shows an empty state when there are no sections.
///
/// Theme colours are passed in from the app-level theme.
pub fn inbox_view<'a, Message: 'a + Clone>(
    sections: &[FeedSection],
    primary_text_color: Color,
    secondary_text_color: Color,
    surface_color: Color,
    divider_color: Color,
) -> Element<'a, Message> {
    // Empty state.
    if sections.is_empty() {
        return empty_inbox(secondary_text_color);
    }

    // Build the feed column: alternating section headers and email rows.
    let mut feed_column = Column::new().width(Length::Fill).spacing(0.0);

    for section in sections {
        // Section header.
        feed_column = feed_column.push(section_header(section.group, secondary_text_color));

        // Email rows within this section.
        for item in &section.items {
            feed_column = feed_column.push(email_row(
                item,
                primary_text_color,
                secondary_text_color,
                surface_color,
                divider_color,
            ));
        }
    }

    // Wrap in a scrollable container.
    scrollable(feed_column)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
