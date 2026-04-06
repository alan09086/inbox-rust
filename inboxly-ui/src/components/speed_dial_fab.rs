//! Compose floating action button (bottom-right).
//!
//! M33 scope: this is a visual placeholder. The Compose view has no
//! corresponding Message variant yet — adding one is out of M33 scope.
//! The onclick is a no-op for now; a future milestone will wire it
//! to dispatch a Compose action (likely Message::OpenCompose or
//! Message::Navigate(ActiveView::Compose)).

use dioxus::prelude::*;

/// Floating "+" button fixed to the bottom-right corner.
#[component]
pub fn SpeedDialFab() -> Element {
    rsx! {
        button {
            class: "fab",
            // No app_state needed — onclick is currently a no-op.
            // Eventually this will dispatch a Compose action.
            onclick: move |evt: Event<MouseData>| {
                evt.stop_propagation();
                tracing::debug!("SpeedDialFab clicked (compose not yet implemented)");
            },
            "+"
        }
    }
}
