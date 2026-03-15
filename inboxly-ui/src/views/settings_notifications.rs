//! Notifications settings tab -- desktop notifications, sound, and bundle filters.

use iced::widget::{Space, checkbox, column, row, text};
use iced::{Element, Length};

use crate::app::{Inboxly, Message};

/// Render the Notifications settings tab.
pub fn notifications_settings_tab(app: &Inboxly) -> Element<'_, Message> {
    let colors = &app.theme.colors;

    let header = text("Notifications").size(16.0).color(colors.text_primary);

    // Desktop notifications toggle
    let notif_enabled = app.notifications_enabled;
    let desktop_toggle = checkbox(notif_enabled)
        .label("Desktop Notifications")
        .on_toggle(|_| Message::ToggleNotifications);

    // Sound toggle -- disabled when notifications are off
    let sound_toggle: Element<'_, Message> = if notif_enabled {
        checkbox(app.notification_sound)
            .label("Sound")
            .on_toggle(|_| Message::ToggleNotificationSound)
            .into()
    } else {
        // Show a greyed-out checkbox (no on_toggle = disabled)
        checkbox(app.notification_sound).label("Sound").into()
    };

    let sound_hint: Element<'_, Message> = if !notif_enabled {
        text("Enable desktop notifications to configure sound.")
            .size(12.0)
            .color(colors.text_secondary)
            .into()
    } else {
        Space::new().into()
    };

    // "Notify for" section
    let notify_for_header = text("Notify for").size(16.0).color(colors.text_primary);

    let is_all = app.notification_bundles.contains(&"all".to_string());
    let is_primary = app.notification_bundles.contains(&"primary".to_string());

    // "All mail" option
    let all_mail_checkbox = checkbox(is_all)
        .label("All mail")
        .on_toggle(move |checked| {
            if checked {
                Message::SetNotificationBundles(vec!["all".to_string()])
            } else {
                Message::SetNotificationBundles(vec![])
            }
        });

    // "Primary only" option
    let primary_checkbox = checkbox(is_primary && !is_all)
        .label("Primary only")
        .on_toggle(move |checked| {
            if checked {
                Message::SetNotificationBundles(vec!["primary".to_string()])
            } else {
                Message::SetNotificationBundles(vec![])
            }
        });

    let mut content = column![
        header,
        Space::new().height(8.0),
        desktop_toggle,
        row![Space::new().width(24.0), sound_toggle].align_y(iced::Alignment::Center),
        sound_hint,
        Space::new().height(16.0),
        notify_for_header,
        Space::new().height(4.0),
        all_mail_checkbox,
        primary_checkbox,
    ]
    .spacing(6);

    // Per-bundle checkboxes (only shown when not "all")
    if !is_all {
        content = content.push(Space::new().height(4.0));
        content = content.push(
            text("Or select specific bundles:")
                .size(13.0)
                .color(colors.text_secondary),
        );

        let bundle_categories = [
            "Social",
            "Promos",
            "Updates",
            "Finance",
            "Purchases",
            "Travel",
            "Forums",
            "Low Priority",
        ];

        for cat in &bundle_categories {
            let cat_str = cat.to_string();
            let is_checked = app.notification_bundles.contains(&cat_str);
            let current_bundles = app.notification_bundles.clone();
            let cat_owned = cat_str.clone();

            let cat_checkbox = checkbox(is_checked).label(*cat).on_toggle(move |checked| {
                let mut new_bundles = current_bundles.clone();
                // Remove "primary" if present -- we're doing per-bundle selection
                new_bundles.retain(|b| b != "primary" && b != "all");
                if checked {
                    if !new_bundles.contains(&cat_owned) {
                        new_bundles.push(cat_owned.clone());
                    }
                } else {
                    new_bundles.retain(|b| b != &cat_owned);
                }
                Message::SetNotificationBundles(new_bundles)
            });

            content = content.push(
                row![Space::new().width(16.0), cat_checkbox].align_y(iced::Alignment::Center),
            );
        }
    }

    content.width(Length::Fill).into()
}
