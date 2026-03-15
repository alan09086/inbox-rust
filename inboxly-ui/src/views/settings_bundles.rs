//! Bundles settings tab -- reorder, throttle, and toggle visibility.

use iced::widget::{Space, button, checkbox, column, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

use inboxly_core::throttle::BundleThrottle;

use crate::app::{Inboxly, Message};

/// Google blue accent for interactive elements.
const ACCENT_BLUE: Color = crate::theme::colors::hex("#4285f4");

/// Render the Bundles settings tab.
pub fn bundles_settings_tab(app: &Inboxly) -> Element<'_, Message> {
    let colors = &app.theme.colors;

    let header = text("Bundles").size(16.0).color(colors.text_primary);

    let description = text("Reorder, show/hide, and configure delivery throttle for each bundle.")
        .size(13.0)
        .color(colors.text_secondary);

    let mut content = column![header, description, Space::new().height(12.0)].spacing(8);

    if app.settings_bundles.is_empty() {
        content = content.push(
            text("No bundles configured.")
                .size(14.0)
                .color(colors.text_secondary),
        );
    } else {
        for (i, bundle) in app.settings_bundles.iter().enumerate() {
            let is_first = i == 0;
            let is_last = i == app.settings_bundles.len() - 1;

            // Up button
            let up_btn: Element<'_, Message> = if is_first {
                text("\u{25b2}")
                    .size(14.0)
                    .color(colors.text_secondary)
                    .into()
            } else {
                let mut new_order: Vec<String> =
                    app.settings_bundles.iter().map(|b| b.id.clone()).collect();
                new_order.swap(i, i - 1);
                let text_primary = colors.text_primary;
                button(text("\u{25b2}").size(14.0).color(text_primary))
                    .on_press(Message::ReorderBundles(new_order))
                    .padding([2, 6])
                    .style(move |_theme, _status| button::Style {
                        background: Some(Background::Color(Color::TRANSPARENT)),
                        text_color: text_primary,
                        border: Border::default(),
                        ..Default::default()
                    })
                    .into()
            };

            // Down button
            let down_btn: Element<'_, Message> = if is_last {
                text("\u{25bc}")
                    .size(14.0)
                    .color(colors.text_secondary)
                    .into()
            } else {
                let mut new_order: Vec<String> =
                    app.settings_bundles.iter().map(|b| b.id.clone()).collect();
                new_order.swap(i, i + 1);
                let text_primary = colors.text_primary;
                button(text("\u{25bc}").size(14.0).color(text_primary))
                    .on_press(Message::ReorderBundles(new_order))
                    .padding([2, 6])
                    .style(move |_theme, _status| button::Style {
                        background: Some(Background::Color(Color::TRANSPARENT)),
                        text_color: text_primary,
                        border: Border::default(),
                        ..Default::default()
                    })
                    .into()
            };

            let arrows = row![up_btn, down_btn].spacing(2).align_y(Alignment::Center);

            // Category name
            let name = text(&bundle.category).size(14.0).color(colors.text_primary);

            // Throttle badge -- parse throttle JSON, show mode label
            let throttle_label = match serde_json::from_str::<BundleThrottle>(&bundle.throttle) {
                Ok(BundleThrottle::Immediate) => "Immediate",
                Ok(BundleThrottle::Daily { .. }) => "Daily",
                Ok(BundleThrottle::Weekly { .. }) => "Weekly",
                Err(_) => "Immediate",
            };

            let badge_bg = match throttle_label {
                "Daily" => crate::theme::colors::hex("#fb8c00"), // orange
                "Weekly" => crate::theme::colors::hex("#8e24aa"), // purple
                _ => crate::theme::colors::hex("#43a047"),       // green for Immediate
            };

            let bundle_id = bundle.id.clone();
            let throttle_badge = button(text(throttle_label).size(12.0).color(Color::WHITE))
                .on_press(Message::ToggleThrottlePopup(Some(bundle_id)))
                .padding([3, 10])
                .style(move |_theme, _status| button::Style {
                    background: Some(Background::Color(badge_bg)),
                    text_color: Color::WHITE,
                    border: Border {
                        radius: 12.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                });

            // Visibility toggle
            let is_visible = bundle.visibility == "visible";
            let bundle_id_for_toggle = bundle.id.clone();
            let vis_checkbox = checkbox(is_visible)
                .label("Visible")
                .on_toggle(move |_| Message::ToggleBundleVisibility(bundle_id_for_toggle.clone()));

            let surface = colors.surface;
            let divider = colors.divider;

            let bundle_row = container(
                row![
                    arrows,
                    Space::new().width(8.0),
                    name,
                    Space::new().width(Length::Fill),
                    throttle_badge,
                    Space::new().width(12.0),
                    vis_checkbox,
                ]
                .align_y(Alignment::Center)
                .padding(10),
            )
            .width(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(Background::Color(surface)),
                border: Border {
                    color: divider,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            });

            content = content.push(bundle_row);
        }
    }

    // Accent-coloured note at the bottom
    content = content.push(Space::new().height(8.0));
    content = content.push(
        text("Click a throttle badge to cycle through Immediate / Daily / Weekly modes.")
            .size(12.0)
            .color(ACCENT_BLUE),
    );

    content.spacing(6).width(Length::Fill).into()
}
