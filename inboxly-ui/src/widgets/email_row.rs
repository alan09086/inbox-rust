//! Email row widget -- the core visual element of the inbox feed.
//!
//! Layout:
//! ```text
//! +-------------------------------------------------------------+
//! | [Avatar] Sender Name          Timestamp   attachment-icon    |
//! | [40dp  ] Subject -- Snippet preview text...                  |
//! | [72dp  ]                                                     |
//! +-------------------------------------------------------------+
//! ```

use iced::widget::{Space, column, container, row, text};
use iced::{Alignment, Background, Color, Element, Length};

use crate::feed::FeedItem;
use crate::theme::dimensions::{AVATAR_COLUMN_WIDTH, DEFAULT_PADDING, DIVIDER_THICKNESS};
use crate::theme::typography::{
    EMAIL_TITLE_SIZE, EMAIL_TITLE_WEIGHT, EMAIL_TITLE_WEIGHT_UNREAD, SNIPPET_SIZE, TIMESTAMP_SIZE,
};
use crate::widgets::avatar::avatar_circle;

/// Build an email row element from a `FeedItem`.
///
/// Colours are passed in from the theme to keep this widget theme-agnostic.
pub fn email_row<'a, Message: 'a + Clone>(
    item: &FeedItem,
    primary_text_color: Color,
    secondary_text_color: Color,
    surface_color: Color,
    divider_color: Color,
) -> Element<'a, Message> {
    // --- Avatar column (72dp wide) ---
    let avatar = container(avatar_circle(item.avatar_letter, item.avatar_color_index))
        .width(AVATAR_COLUMN_WIDTH)
        .padding([0.0, 16.0])
        .align_y(iced::alignment::Vertical::Center);

    // --- Sender name ---
    let sender_weight = if item.is_unread {
        EMAIL_TITLE_WEIGHT_UNREAD
    } else {
        EMAIL_TITLE_WEIGHT
    };

    let sender_name = item.sender_name.clone();
    let sender = text(sender_name)
        .size(EMAIL_TITLE_SIZE)
        .color(primary_text_color)
        .font(iced::Font {
            weight: sender_weight,
            ..Default::default()
        });

    // --- Subject + Snippet (combined, truncated) ---
    let combined = if item.snippet.is_empty() {
        item.subject.clone()
    } else {
        format!("{} \u{2014} {}", item.subject, item.snippet)
    };

    // Truncate to reasonable display length. Use char boundary for safety.
    let display = if combined.chars().count() > 120 {
        let truncated: String = combined.chars().take(120).collect();
        format!("{truncated}\u{2026}")
    } else {
        combined
    };

    let subject_snippet = text(display).size(SNIPPET_SIZE).color(secondary_text_color);

    // --- Content column ---
    let content = column![sender, subject_snippet]
        .spacing(2.0)
        .width(Length::Fill);

    // --- Right column: timestamp + attachment ---
    let ts_display = item.timestamp_display.clone();
    let timestamp = text(ts_display)
        .size(TIMESTAMP_SIZE)
        .color(secondary_text_color);

    let mut right_col = column![timestamp]
        .align_x(iced::Alignment::End)
        .spacing(4.0);

    if item.has_attachments {
        let attachment_icon = text("\u{1F4CE}") // paperclip emoji
            .size(TIMESTAMP_SIZE)
            .color(secondary_text_color);
        right_col = right_col.push(attachment_icon);
    }

    // --- Full row ---
    let row_content = row![avatar, content, right_col]
        .align_y(Alignment::Center)
        .spacing(0.0)
        .padding([12.0, DEFAULT_PADDING]);

    let row_with_bg = container(row_content)
        .width(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(surface_color)),
            ..Default::default()
        });

    let divider = container(Space::new().width(Length::Fill).height(DIVIDER_THICKNESS)).style(
        move |_theme| container::Style {
            background: Some(Background::Color(divider_color)),
            ..Default::default()
        },
    );

    column![row_with_bg, divider].width(Length::Fill).into()
}
