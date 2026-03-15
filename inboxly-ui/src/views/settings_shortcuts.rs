//! Keyboard shortcuts settings tab -- view and remap shortcut bindings.

use iced::widget::{Space, button, column, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::app::{Inboxly, Message};

/// Google blue accent for interactive elements.
const ACCENT_BLUE: Color = crate::theme::colors::hex("#4285f4");
/// Background for the "Press key..." capture state.
const CAPTURE_BG: Color = Color {
    r: 1.0,
    g: 0.96,
    b: 0.76,
    a: 1.0,
};
/// Dark theme capture background.
const CAPTURE_BG_DARK: Color = Color {
    r: 0.30,
    g: 0.27,
    b: 0.15,
    a: 1.0,
};
/// Red for reset buttons.
const RESET_RED: Color = crate::theme::colors::hex("#ef5350");

/// Render the Keyboard Shortcuts settings tab.
pub fn shortcuts_settings_tab(app: &Inboxly) -> Element<'_, Message> {
    let colors = &app.theme.colors;

    let header = text("Keyboard Shortcuts")
        .size(16.0)
        .color(colors.text_primary);

    let description = text("Click a shortcut binding to remap it. Press Escape to cancel capture.")
        .size(13.0)
        .color(colors.text_secondary);

    // Table header row
    let table_header = row![
        text("Action")
            .size(13.0)
            .color(colors.text_secondary)
            .width(Length::FillPortion(3)),
        text("Shortcut")
            .size(13.0)
            .color(colors.text_secondary)
            .width(Length::FillPortion(2)),
    ]
    .padding([8, 0]);

    let divider_color = colors.divider;
    let header_divider = container(Space::new().height(1.0))
        .width(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(divider_color)),
            ..Default::default()
        });

    let mut content = column![
        header,
        description,
        Space::new().height(12.0),
        table_header,
        header_divider,
    ]
    .spacing(4);

    for (action, binding) in app.shortcuts.iter_display_order() {
        let is_capturing = app.capturing_shortcut == Some(action);
        let is_customised = app.shortcuts.is_customised(action);

        // Action label
        let action_label = text(action.label())
            .size(14.0)
            .color(colors.text_primary)
            .width(Length::FillPortion(3));

        // Shortcut cell
        let shortcut_cell: Element<'_, Message> = if is_capturing {
            let capture_bg = if colors.is_dark {
                CAPTURE_BG_DARK
            } else {
                CAPTURE_BG
            };
            container(
                button(text("Press key...").size(14.0).color(colors.text_primary))
                    .on_press(Message::CancelCapture)
                    .padding([4, 10])
                    .style(move |_theme, _status| button::Style {
                        background: Some(Background::Color(capture_bg)),
                        border: Border {
                            color: ACCENT_BLUE,
                            width: 2.0,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    }),
            )
            .width(Length::FillPortion(2))
            .into()
        } else {
            let text_primary = colors.text_primary;
            let surface = colors.surface;
            let divider = colors.divider;
            let binding_owned = binding.to_owned();

            let binding_btn = button(text(binding_owned.clone()).size(14.0).color(text_primary))
                .on_press(Message::StartCapture(action))
                .padding([4, 10])
                .style(move |_theme, _status| button::Style {
                    background: Some(Background::Color(surface)),
                    text_color: text_primary,
                    border: Border {
                        color: divider,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                });

            if is_customised {
                // Show reset button next to the binding
                let reset_btn = button(text("Reset").size(12.0).color(RESET_RED))
                    .on_press(Message::ResetShortcut(action))
                    .padding([2, 6])
                    .style(move |_theme, _status| button::Style {
                        background: Some(Background::Color(Color::TRANSPARENT)),
                        text_color: RESET_RED,
                        border: Border::default(),
                        ..Default::default()
                    });

                container(
                    row![binding_btn, Space::new().width(6.0), reset_btn]
                        .align_y(Alignment::Center),
                )
                .width(Length::FillPortion(2))
                .into()
            } else {
                container(binding_btn).width(Length::FillPortion(2)).into()
            }
        };

        let shortcut_row = row![action_label, shortcut_cell]
            .align_y(Alignment::Center)
            .padding([6, 0]);

        content = content.push(shortcut_row);
    }

    // TODO: Wire keyboard capture via iced subscription.
    // Currently the UI shows "Press key..." but actual key capture requires
    // an iced keyboard subscription to intercept key events and emit
    // SetShortcut or CancelCapture messages.

    content.spacing(2).width(Length::Fill).into()
}
