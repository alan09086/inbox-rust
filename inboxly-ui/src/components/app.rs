//! Root application component -- shell layout with theme context.

use dioxus::prelude::*;

use crate::app::Inboxly;
use crate::components::content_area::ContentArea;
use crate::components::nav_drawer::NavDrawer;
use crate::components::snooze_picker::SnoozePicker;
use crate::components::speed_dial_fab::SpeedDialFab;
use crate::components::toolbar::Toolbar;
use crate::components::undo_snackbar::UndoSnackbar;
use crate::theme::{ActiveView, ThemeConfig};

/// Window title -- updates reactively based on active view.
#[allow(dead_code)]
fn window_title(view: ActiveView) -> String {
    format!("Inboxly \u{2014} {}", view.title())
}

/// CSS asset for the main stylesheet.
static CSS: Asset = asset!("/assets/main.css");

/// Root application component.
///
/// Creates the `Inboxly` state and provides it as context to all children.
/// Sets the `data-theme` attribute for CSS theming, and renders the shell
/// layout: toolbar + body row (nav drawer + content area).
#[component]
pub fn App() -> Element {
    // Initialise app state once on first render.
    let app_state = use_context_provider(|| {
        Signal::new(Inboxly {
            theme: ThemeConfig::from_system(),
            ..Inboxly::default()
        })
    });

    let is_dark = app_state.read().theme.colors.is_dark;
    let drawer_open = app_state.read().drawer_open;
    let active_view = app_state.read().active_view;

    rsx! {
        document::Stylesheet { href: CSS }

        div {
            class: "app-shell",
            "data-theme": if is_dark { "dark" } else { "light" },

            Toolbar {}

            div {
                class: "body-row",

                if drawer_open && active_view != ActiveView::Settings {
                    NavDrawer {}
                }

                ContentArea {}
            }
        }

        UndoSnackbar {}

        if active_view != ActiveView::Settings {
            SpeedDialFab {}
            SnoozePicker {}
        }
    }
}
