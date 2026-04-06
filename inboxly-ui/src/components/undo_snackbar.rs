//! Timed undo snackbar shown at the bottom of the window.
//!
//! Renders a fixed-position bar when `Inboxly::undo_state` has an active
//! action. Contains the action description and an "Undo" button that
//! dispatches `Message::Undo`. A background timer dispatches
//! `Message::UndoExpired` after `UNDO_TIMEOUT` elapses, clearing the
//! snackbar.

use dioxus::prelude::*;

use crate::app::{Inboxly, Message};
use crate::undo::UNDO_TIMEOUT;

/// Bottom-centre undo snackbar with action description, Undo button,
/// and auto-expire timer.
#[component]
pub fn UndoSnackbar() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();

    let description = app_state.read().undo_state.description();
    let Some(description) = description else {
        return rsx! {};
    };

    // Spawn a 7-second timer that dispatches UndoExpired.
    // use_effect re-runs whenever the reactive reads inside it change,
    // so a fresh timer fires for each new undo action.
    use_effect(move || {
        // Reading is_active() establishes a reactive dependency so this
        // effect re-runs whenever undo_state changes (new action pushed).
        let active = app_state.read().undo_state.is_active();
        if active {
            spawn(async move {
                tokio::time::sleep(UNDO_TIMEOUT).await;
                // By the time the timer fires the state may already be
                // cleared (user clicked Undo, or a new action replaced it).
                // UndoExpired calls undo_state.clear(), which is idempotent.
                app_state.write().update(Message::UndoExpired);
            });
        }
    });

    rsx! {
        div {
            class: "undo-snackbar",
            span {
                class: "undo-snackbar-text",
                "{description}"
            }
            button {
                class: "undo-snackbar-btn",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state.write().update(Message::Undo);
                },
                "Undo"
            }
        }
    }
}
