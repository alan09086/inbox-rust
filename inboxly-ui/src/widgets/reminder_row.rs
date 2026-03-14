//! Reminder row widget -- displays a user-created reminder in the feed.
//!
//! Layout:
//! ```text
//! +------+--------------------------------------+--------+
//! | 72dp | Reminder title                       | Due    |
//! | icon | Description or "No due date"          |        |
//! +------+--------------------------------------+--------+
//! ```

use iced::widget::{Space, column, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::theme::dimensions::{AVATAR_COLUMN_WIDTH, DEFAULT_PADDING, DIVIDER_THICKNESS};

/// A reminder item for display in the feed.
#[derive(Debug, Clone)]
pub struct ReminderFeedItem {
    /// Unique reminder ID.
    pub id: String,
    /// Reminder title.
    pub title: String,
    /// Due date display string (e.g., "Tomorrow 8:00 AM", or empty).
    pub due_display: String,
    /// Whether the reminder is overdue.
    pub is_overdue: bool,
}

/// Build a reminder row element.
pub fn reminder_row<'a, Message: 'a + Clone>(
    item: &ReminderFeedItem,
    on_done: Message,
    primary_text_color: Color,
    secondary_text_color: Color,
    surface_color: Color,
    divider_color: Color,
    accent_color: Color,
) -> Element<'a, Message> {
    // Left: reminder icon in 72dp column.
    let icon = container(
        text("\u{1F4CB}") // clipboard emoji
            .size(20.0)
            .color(accent_color)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
    )
    .width(AVATAR_COLUMN_WIDTH)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center);

    // Middle: title + due date.
    let title = text(item.title.clone())
        .size(16.0)
        .color(primary_text_color)
        .font(iced::Font {
            weight: iced::font::Weight::Medium,
            ..Default::default()
        });

    let due = if item.due_display.is_empty() {
        text("No due date".to_owned())
            .size(14.0)
            .color(secondary_text_color)
    } else {
        let color = if item.is_overdue {
            Color::from_rgb(0.82, 0.25, 0.19) // red for overdue
        } else {
            secondary_text_color
        };
        text(item.due_display.clone()).size(14.0).color(color)
    };

    let middle = column![title, due].spacing(2.0).width(Length::Fill);

    // Right: done checkmark button.
    let done_btn = iced::widget::button(text("\u{2713}").size(14.0).color(accent_color))
        .on_press(on_done)
        .padding([4.0, 8.0])
        .style(move |_theme, _status| iced::widget::button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border::default(),
            ..Default::default()
        });

    // Full row.
    let row_content = row![icon, middle, done_btn]
        .spacing(0.0)
        .padding([12.0, DEFAULT_PADDING])
        .align_y(Alignment::Center);

    let row_bg = container(row_content)
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

    column![row_bg, divider].width(Length::Fill).into()
}
