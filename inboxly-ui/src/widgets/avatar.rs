//! Avatar circle widget -- 40dp diameter, coloured background, white letter.
//!
//! Uses container + text styling rather than canvas for simplicity and
//! compatibility. The A-Z palette colours are re-used from the theme module.

use iced::widget::{container, text};
use iced::{Background, Border, Color, Element, Length};

use crate::theme::avatar_colors;
use crate::theme::dimensions::AVATAR_DIAMETER;

/// Create an avatar circle element with the given letter and colour index.
///
/// The avatar is a 40dp circle filled with the palette colour for the given
/// index, with a single white letter centered inside.
pub fn avatar_circle<'a, Message: 'a>(letter: char, color_index: u8) -> Element<'a, Message> {
    let bg_color = avatar_colors::for_letter(letter);
    // Clamp color_index to valid range; prefer letter-based colour.
    let _ = color_index;

    let letter_text = text(letter.to_uppercase().to_string())
        .size(18.0)
        .color(Color::WHITE)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center);

    container(letter_text)
        .width(Length::Fixed(AVATAR_DIAMETER))
        .height(Length::Fixed(AVATAR_DIAMETER))
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(bg_color)),
            border: Border {
                radius: (AVATAR_DIAMETER / 2.0).into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
