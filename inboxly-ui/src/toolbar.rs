//! Toolbar view -- coloured bar at the top of the application.

use iced::widget::{Space, button, container, row, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::app::{Inboxly, Message};
use crate::theme::{
    AVATAR_DIAMETER, DEFAULT_PADDING, TOOLBAR_HEIGHT, TOOLBAR_TITLE_SIZE, color_from_hex,
};

/// Render the toolbar bar.
///
/// 56dp tall, background colour changes by active view. Contains:
/// - Left: hamburger menu toggle (text for now)
/// - Center-left: title text showing the active view name
/// - Center: search bar placeholder
/// - Right: account avatar placeholder (circle with first letter)
pub fn view_toolbar(app: &Inboxly) -> Element<'_, Message> {
    let toolbar_bg = app.active_view.toolbar_color();

    // Hamburger button
    let hamburger = button(text("\u{2630}").size(20.0).color(Color::WHITE))
        .on_press(Message::ToggleDrawer)
        .padding([8, 12])
        .style(move |_theme, _status| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: Color::WHITE,
            border: Border::default(),
            ..Default::default()
        });

    // Title
    let title = text(app.active_view.title())
        .size(TOOLBAR_TITLE_SIZE)
        .color(Color::WHITE);

    // Search placeholder
    let search = text_input("Search mail", "")
        .on_input(Message::SearchChanged)
        .width(Length::FillPortion(3))
        .padding([8, 12]);

    // Account avatar (first letter circle)
    let avatar_letter = app
        .account_email
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let avatar = container(
        text(avatar_letter)
            .size(16.0)
            .color(Color::WHITE)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
    )
    .width(AVATAR_DIAMETER)
    .height(AVATAR_DIAMETER)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(color_from_hex(0x66, 0x66, 0x66))),
        border: Border {
            radius: (AVATAR_DIAMETER / 2.0).into(),
            ..Default::default()
        },
        ..Default::default()
    });

    let toolbar_row = row![
        hamburger,
        title,
        Space::new().width(Length::Fixed(DEFAULT_PADDING)),
        search,
        Space::new().width(Length::Fill),
        avatar,
    ]
    .spacing(12)
    .padding([0.0, DEFAULT_PADDING])
    .align_y(Alignment::Center)
    .height(TOOLBAR_HEIGHT)
    .width(Length::Fill);

    container(toolbar_row)
        .width(Length::Fill)
        .height(TOOLBAR_HEIGHT)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(toolbar_bg)),
            ..Default::default()
        })
        .into()
}
