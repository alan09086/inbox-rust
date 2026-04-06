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

    // Derived signals scoped to just the undo state.
    // use_memo re-computes only when its reactive reads actually change,
    // so the effect below re-runs only on undo state transitions — not
    // on every app-state mutation.
    let active = use_memo(move || app_state.read().undo_state.is_active());
    let generation = use_memo(move || app_state.read().undo_state.generation());
    let description_sig = use_memo(move || app_state.read().undo_state.description());

    // Spawn a fresh timer each time a new undo action is pushed.
    // The captured generation lets the timer no-op if a newer action
    // has replaced it — otherwise stale timers from rapid successive
    // actions would prematurely expire the newest snackbar.
    use_effect(move || {
        if *active.read() {
            let gen_at_spawn = *generation.read();
            spawn(async move {
                tokio::time::sleep(UNDO_TIMEOUT).await;
                // peek() reads without subscribing -- safe inside async.
                let current_gen = app_state.peek().undo_state.generation();
                if current_gen == gen_at_spawn {
                    app_state.write().update(Message::UndoExpired);
                }
            });
        }
    });

    // Early return when no active undo.
    let Some(description) = description_sig.read().clone() else {
        return rsx! {};
    };

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
