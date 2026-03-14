//! Bundle category icon circle -- 40dp tinted circle with Unicode symbol.

use iced::widget::{container, text};
use iced::{Background, Border, Color, Element};

use crate::theme::dimensions::AVATAR_DIAMETER;

/// Returns the icon character for a bundle category key.
///
/// These are placeholder Unicode symbols; replace with SVG icons in M25 polish.
pub fn category_icon_char(category: &str) -> &'static str {
    match category {
        "social" => "\u{1F464}",      // person silhouette
        "promos" => "\u{1F3F7}",      // label/tag
        "updates" => "\u{1F514}",     // bell
        "finance" => "\u{1F4B0}",     // money bag
        "purchases" => "\u{1F6D2}",   // shopping cart
        "travel" => "\u{2708}",       // airplane
        "forums" => "\u{1F4AC}",      // speech bubble
        "low_priority" => "\u{2B07}", // down arrow
        "saved" => "\u{2B50}",        // star
        _ => "\u{1F4C1}",             // folder
    }
}

/// Renders a 40dp circle with tinted background and category icon centered inside.
pub fn category_icon_circle<'a, Message: 'a>(
    category: &str,
    title_color: Color,
    badge_bg: Color,
) -> Element<'a, Message> {
    let icon_char = category_icon_char(category);

    container(
        text(icon_char)
            .size(20.0)
            .color(title_color)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
    )
    .width(AVATAR_DIAMETER)
    .height(AVATAR_DIAMETER)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(badge_bg)),
        border: Border {
            radius: (AVATAR_DIAMETER / 2.0).into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}
