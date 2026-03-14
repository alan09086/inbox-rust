//! Hover action buttons -- revealed when hovering over email rows.
//!
//! Desktop users interact via hover-revealed action buttons rather than swipe.
//! Buttons appear on the right side of the email row: Done (checkmark),
//! Pin (pin icon), and Snooze (clock icon).

use iced::widget::{button, container, row, text};
use iced::{Background, Border, Color, Element, Length};

use crate::theme::dimensions::DEFAULT_PADDING;

/// Build a row of hover action buttons for an email row.
///
/// Returns a right-aligned row containing Done, Pin, and Snooze buttons.
/// Caller should overlay this on the email row when mouse is hovering.
pub fn hover_action_buttons<'a, Message: 'a + Clone>(
    on_done: Message,
    on_pin: Message,
    on_snooze: Message,
    accent_color: Color,
    surface_color: Color,
) -> Element<'a, Message> {
    let done_btn = action_button("\u{2713}", "Done", on_done, accent_color, surface_color);
    let pin_btn = action_button("\u{1F4CC}", "Pin", on_pin, accent_color, surface_color);
    let snooze_btn = action_button(
        "\u{1F552}",
        "Snooze",
        on_snooze,
        accent_color,
        surface_color,
    );

    row![done_btn, pin_btn, snooze_btn]
        .spacing(4.0)
        .padding([0.0, DEFAULT_PADDING])
        .into()
}

/// Single action button with icon and tooltip text.
fn action_button<'a, Message: 'a + Clone>(
    icon: &str,
    _tooltip: &str, // tooltip support deferred to M25
    on_press: Message,
    accent_color: Color,
    surface_color: Color,
) -> Element<'a, Message> {
    let icon_text = text(icon.to_owned())
        .size(16.0)
        .color(accent_color)
        .align_x(iced::alignment::Horizontal::Center);

    button(
        container(icon_text)
            .width(Length::Fixed(32.0))
            .height(Length::Fixed(32.0))
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(on_press)
    .padding(0.0)
    .style(move |_theme, _status| button::Style {
        background: Some(Background::Color(surface_color)),
        border: Border {
            radius: 16.0.into(), // circular
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_buttons_accessible() {
        // Verify the hover_action_buttons function is callable.
        // Visual rendering can't be unit-tested without Iced renderer.
        let _: Element<'_, &str> = hover_action_buttons(
            "done",
            "pin",
            "snooze",
            Color::from_rgb(0.26, 0.52, 0.96), // blue
            Color::WHITE,
        );
    }
}
