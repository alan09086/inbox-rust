//! Scrollable inbox feed view -- section headers + email rows + bundle rows.

use iced::widget::{Column, scrollable};
use iced::{Color, Element, Length};

use crate::feed::{FeedEntry, FeedSection};
use crate::widgets::bundle_row::bundle_row_collapsed;
use crate::widgets::email_row::email_row;
use crate::widgets::empty_state::empty_inbox;
use crate::widgets::section_header::section_header;

/// Messages from the inbox view.
#[derive(Debug, Clone)]
pub enum InboxViewMessage {
    /// User clicked a bundle row to expand/collapse it.
    ToggleBundle(String),
}

/// Build the scrollable inbox feed view.
///
/// Renders section headers, email rows, and bundle rows from pre-built
/// feed sections. Shows an empty state when there are no sections.
///
/// Theme colours are passed in from the app-level theme.
pub fn inbox_view<'a>(
    sections: &[FeedSection],
    primary_text_color: Color,
    secondary_text_color: Color,
    surface_color: Color,
    divider_color: Color,
) -> Element<'a, InboxViewMessage> {
    // Empty state.
    if sections.is_empty() {
        return empty_inbox(secondary_text_color);
    }

    // Build the feed column: alternating section headers and rows.
    let mut feed_column = Column::new().width(Length::Fill).spacing(0.0);

    for section in sections {
        // Section header.
        feed_column = feed_column.push(section_header(section.group, secondary_text_color));

        // Entries within this section.
        for entry in &section.items {
            match entry {
                FeedEntry::Thread(item) => {
                    feed_column = feed_column.push(email_row(
                        item,
                        primary_text_color,
                        secondary_text_color,
                        surface_color,
                        divider_color,
                    ));
                }
                FeedEntry::Bundle(summary) => {
                    let bundle_id = summary.bundle_id.clone();
                    feed_column = feed_column.push(bundle_row_collapsed(
                        summary,
                        InboxViewMessage::ToggleBundle(bundle_id),
                        secondary_text_color,
                        surface_color,
                        divider_color,
                    ));
                }
            }
        }
    }

    // Wrap in a scrollable container.
    scrollable(feed_column)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
