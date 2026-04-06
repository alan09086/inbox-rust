//! Toolbar component -- coloured bar at the top of the application.

use dioxus::prelude::*;

use crate::app::{Inboxly, Message};
use crate::theme::ActiveView;

/// Toolbar component.
///
/// 56dp tall, background colour changes by active view. Contains:
/// - Left: hamburger/back button
/// - Title text
/// - Search placeholder
/// - Gear icon (settings)
/// - Account avatar
#[component]
pub fn Toolbar() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    let state = app_state.read();

    let toolbar_css = state.active_view.toolbar_css(&state.theme);
    let title = state.active_view.title();
    let is_settings = state.active_view == ActiveView::Settings;
    let nav_icon = if is_settings { "\u{2190}" } else { "\u{2630}" };
    let avatar_letter = state
        .active_email()
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();

    // Drop the read borrow before creating event handlers
    drop(state);

    rsx! {
        div {
            class: "toolbar",
            style: "background: {toolbar_css};",

            // Hamburger / back button
            button {
                class: "toolbar-btn",
                onclick: move |_| {
                    let mut state = app_state.write();
                    if state.active_view == ActiveView::Settings {
                        state.update(Message::NavigateBack);
                    } else {
                        state.update(Message::ToggleDrawer);
                    }
                },
                "{nav_icon}"
            }

            // Title
            span { class: "toolbar-title", "{title}" }

            // Search placeholder
            input {
                class: "toolbar-search",
                r#type: "text",
                placeholder: "Search mail",
                readonly: true,
            }

            // Spacer
            div { class: "toolbar-spacer" }

            // Gear icon (settings)
            button {
                class: "toolbar-btn",
                onclick: move |_| {
                    app_state.write().update(Message::NavigateToSettings);
                },
                "\u{2699}"
            }

            // Account avatar
            div {
                class: "toolbar-avatar",
                "{avatar_letter}"
            }
        }
    }
}
