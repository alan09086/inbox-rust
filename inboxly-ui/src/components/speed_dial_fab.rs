//! Compose floating action button (bottom-right).
//!
//! Wired in M35 Phase 9 to dispatch `Message::OpenCompose`. Phase 6 added
//! the state machine and the `OpenCompose` Message variant; Phase 8 added
//! the `ComposeView` component; this phase wires the entry point.

use dioxus::prelude::*;

use crate::app::{Inboxly, Message};

/// Floating "+" button fixed to the bottom-right corner. Opens the
/// compose view when clicked.
#[component]
pub fn SpeedDialFab() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    rsx! {
        button {
            class: "fab",
            aria_label: "Compose new email",
            onclick: move |evt: Event<MouseData>| {
                evt.stop_propagation();
                app_state.write().update(Message::OpenCompose);
            },
            "+"
        }
    }
}
