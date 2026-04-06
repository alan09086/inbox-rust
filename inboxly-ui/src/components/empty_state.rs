//! Generic empty state placeholder.
//!
//! Used by the Snoozed and Done views when they have no content to show.
//! Unlike `InboxZero` (which celebrates a cleared inbox), this is a neutral
//! "nothing here" placeholder.

use dioxus::prelude::*;

/// Generic "nothing here" placeholder for views without content.
///
/// Takes an `icon` (Unicode glyph) and `text` message to display.
#[component]
pub fn EmptyState(icon: String, text: String) -> Element {
    rsx! {
        div {
            class: "empty-state",
            div {
                class: "empty-state-icon",
                "{icon}"
            }
            div {
                class: "empty-state-text",
                "{text}"
            }
        }
    }
}
