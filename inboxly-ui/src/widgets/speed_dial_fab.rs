//! Speed Dial FAB (Floating Action Button) -- compose + reminder shortcuts.
//!
//! The FAB floats in the bottom-right corner. Tapping it opens a speed dial
//! with two mini-FABs: "Compose" and "Reminder".

use iced::widget::{button, column, container, text};
use iced::{Background, Border, Color, Element, Length};

use crate::theme::dimensions::{FAB_DIAMETER, FAB_EDGE_MARGIN, MINI_FAB_DIAMETER};

/// Build the main FAB element.
///
/// When not expanded, shows a single large FAB with a "+" icon.
/// When expanded, shows two mini-FABs above it (Compose and Reminder).
pub fn speed_dial_fab<'a, Message: 'a + Clone>(
    is_expanded: bool,
    on_toggle: Message,
    on_compose: Message,
    on_reminder: Message,
    accent_color: Color,
) -> Element<'a, Message> {
    let mut fab_column = column![].spacing(12.0).align_x(iced::Alignment::End);

    if is_expanded {
        // Mini FAB: Compose.
        let compose_fab = mini_fab("\u{270F}", on_compose, accent_color); // pencil
        fab_column = fab_column.push(compose_fab);

        // Mini FAB: Reminder.
        let reminder_fab = mini_fab("\u{1F4CB}", on_reminder, accent_color); // clipboard
        fab_column = fab_column.push(reminder_fab);
    }

    // Main FAB: toggle.
    let icon = if is_expanded { "\u{2715}" } else { "\u{002B}" }; // X or +
    let main_fab = button(
        container(text(icon).size(24.0).color(Color::WHITE))
            .width(FAB_DIAMETER)
            .height(FAB_DIAMETER)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(on_toggle)
    .padding(0.0)
    .style(move |_theme, _status| button::Style {
        background: Some(Background::Color(accent_color)),
        border: Border {
            radius: (FAB_DIAMETER / 2.0).into(),
            ..Default::default()
        },
        ..Default::default()
    });

    fab_column = fab_column.push(main_fab);

    container(fab_column)
        .padding(FAB_EDGE_MARGIN)
        .width(Length::Shrink)
        .into()
}

/// Build a mini FAB (40dp diameter).
fn mini_fab<'a, Message: 'a + Clone>(
    icon: &str,
    on_press: Message,
    accent_color: Color,
) -> Element<'a, Message> {
    button(
        container(text(icon.to_owned()).size(18.0).color(Color::WHITE))
            .width(MINI_FAB_DIAMETER)
            .height(MINI_FAB_DIAMETER)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(on_press)
    .padding(0.0)
    .style(move |_theme, _status| button::Style {
        background: Some(Background::Color(accent_color)),
        border: Border {
            radius: (MINI_FAB_DIAMETER / 2.0).into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn fab_diameter_matches_spec() {
        use crate::theme::dimensions::FAB_DIAMETER;
        assert_eq!(FAB_DIAMETER, 56.0);
    }

    #[test]
    fn mini_fab_diameter_matches_spec() {
        use crate::theme::dimensions::MINI_FAB_DIAMETER;
        assert_eq!(MINI_FAB_DIAMETER, 40.0);
    }
}
