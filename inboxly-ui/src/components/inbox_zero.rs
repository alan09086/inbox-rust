//! Inbox Zero celebration view.
//!
//! Shown inside the ContentArea when `feed_sections` is empty. A large
//! illustration + celebratory text. This is the "you've done it!" state
//! that rewards the user for clearing their inbox.

use dioxus::prelude::*;

/// Celebration state shown when the inbox feed is empty.
#[component]
pub fn InboxZero() -> Element {
    rsx! {
        div {
            class: "inbox-zero",
            div {
                class: "inbox-zero-icon",
                "\u{1F389}"  // 🎉 party popper
            }
            div {
                class: "inbox-zero-title",
                "You're all caught up"
            }
            div {
                class: "inbox-zero-subtitle",
                "Take a break, or start something new."
            }
        }
    }
}
