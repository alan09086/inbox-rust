//! Bundle row widget -- collapsed summary of bundled emails.
//!
//! Layout:
//! ```text
//! +------+--------------------------------------+--------+
//! | 72dp | Category Name        [3 new]         | 2:34pm |
//! | icon | Alice, Bob, Charlie                  |        |
//! +------+--------------------------------------+--------+
//! ```

use iced::widget::{Row, Space, column, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::feed::format_timestamp;
use crate::theme::bundle_colors;
use crate::theme::dimensions::{AVATAR_COLUMN_WIDTH, DEFAULT_PADDING, DIVIDER_THICKNESS};
use crate::theme::typography::{EMAIL_TITLE_SIZE, SNIPPET_SIZE, TIMESTAMP_SIZE};
use crate::widgets::bundle_icon::category_icon_circle;

use inboxly_store::BundleSummary;

/// Build a collapsed bundle row element.
///
/// Colours are resolved from the bundle's category via the theme's
/// bundle_colors module.
pub fn bundle_row_collapsed<'a, Message: 'a + Clone>(
    summary: &BundleSummary,
    on_click: Message,
    secondary_text_color: Color,
    surface_color: Color,
    divider_color: Color,
) -> Element<'a, Message> {
    let cat_colors = bundle_colors::for_category_str(&summary.category);

    // Left: icon circle in 72dp column.
    let icon = container(category_icon_circle(
        &summary.category,
        cat_colors.title,
        cat_colors.badge,
    ))
    .width(AVATAR_COLUMN_WIDTH)
    .padding([0.0, 16.0])
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center);

    // Middle line 1: category name + unread badge.
    let category_name = text(summary.name.clone())
        .size(EMAIL_TITLE_SIZE)
        .color(cat_colors.title)
        .font(iced::Font {
            weight: iced::font::Weight::Medium,
            ..Default::default()
        });

    let mut line1 = Row::new().spacing(8.0).align_y(Alignment::Center);
    line1 = line1.push(category_name);

    if summary.unread_count > 0 {
        let badge_title = cat_colors.title;
        let badge_bg = cat_colors.badge;
        let badge_text = text(format!("{} new", summary.unread_count))
            .size(12.0)
            .color(badge_title)
            .font(iced::Font {
                weight: iced::font::Weight::Bold,
                ..Default::default()
            });
        let badge = container(badge_text)
            .padding([2.0, 8.0])
            .style(move |_theme| container::Style {
                background: Some(Background::Color(badge_bg)),
                border: Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });
        line1 = line1.push(badge);
    }

    // Middle line 2: sender previews.
    let mut sender_parts = Row::new().spacing(0.0);
    for (i, sender) in summary.sender_previews.iter().take(3).enumerate() {
        if i > 0 {
            sender_parts =
                sender_parts.push(text(", ").size(SNIPPET_SIZE).color(secondary_text_color));
        }
        let weight = if sender.is_unread {
            iced::font::Weight::Bold
        } else {
            iced::font::Weight::Normal
        };
        sender_parts = sender_parts.push(
            text(sender.name.clone())
                .size(SNIPPET_SIZE)
                .color(secondary_text_color)
                .font(iced::Font {
                    weight,
                    ..Default::default()
                }),
        );
    }
    if summary.sender_previews.len() > 3 {
        let remaining = summary.sender_previews.len() - 3;
        sender_parts = sender_parts.push(
            text(format!(", +{remaining}"))
                .size(SNIPPET_SIZE)
                .color(secondary_text_color),
        );
    }

    let middle = column![line1, sender_parts]
        .spacing(2.0)
        .width(Length::Fill);

    // Right: timestamp.
    let ts = format_timestamp(summary.newest_date);
    let timestamp = text(ts).size(TIMESTAMP_SIZE).color(secondary_text_color);

    // Assemble row.
    let row_content = row![icon, middle, timestamp]
        .spacing(0.0)
        .padding([12.0, DEFAULT_PADDING])
        .align_y(Alignment::Center);

    // Wrap in clickable container.
    let row_bg = container(
        iced::widget::button(row_content)
            .on_press(on_click)
            .width(Length::Fill)
            .style(move |_theme, _status| iced::widget::button::Style {
                background: Some(Background::Color(surface_color)),
                border: Border::default(),
                ..Default::default()
            }),
    )
    .width(Length::Fill);

    let divider = container(Space::new().width(Length::Fill).height(DIVIDER_THICKNESS)).style(
        move |_theme| container::Style {
            background: Some(Background::Color(divider_color)),
            ..Default::default()
        },
    );

    column![row_bg, divider].width(Length::Fill).into()
}
