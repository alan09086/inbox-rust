//! Thread detail view — header bar + scrollable list of messages.
//!
//! Reads from the `Signal<Option<Arc<LoadedThread>>>` context that
//! the App component provides. The body data lives in this separate
//! signal (per eng review Issue 1.4) so per-write `Clone` of Inboxly
//! doesn't drag thread bodies around. The back button dispatches
//! `Message::CloseThread`, which clears `Inboxly::open_thread_id`;
//! the App-level use_effect bridge then clears the body signal.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::app::{Inboxly, Message};
use crate::components::thread_message::ThreadMessage;
use crate::loaded_thread::LoadedThread;

#[component]
pub fn ThreadDetailView() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    let open_thread = use_context::<Signal<Option<Arc<LoadedThread>>>>();

    // Read the body signal — this clone is cheap (Arc bump).
    let thread_arc = open_thread.read().clone();
    let Some(thread) = thread_arc else {
        return rsx! {};
    };
    // From here on we work with `&LoadedThread` (via the Arc).

    rsx! {
        div {
            class: "thread-detail-view",
            div {
                class: "thread-detail-header",
                button {
                    class: "thread-detail-back",
                    aria_label: "Back to inbox",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::CloseThread);
                    },
                    "\u{2190}"  // ← left arrow
                }
                span { class: "thread-detail-subject", "{thread.subject}" }
            }
            // Error banner — only rendered when the loader failed.
            // Eng review Issue 2.1: surface load failures to the user
            // instead of silently showing demo/empty content.
            if let Some(ref err) = thread.error_message {
                div {
                    class: "thread-detail-error-banner",
                    role: "alert",
                    span { class: "thread-detail-error-icon", "\u{26A0}\u{FE0F}" }  // ⚠️
                    span { class: "thread-detail-error-text", "{err}" }
                }
            }
            if thread.messages.is_empty() {
                div { class: "thread-detail-empty", "No messages in this thread." }
            } else {
                for message in thread.messages.iter() {
                    // Issue 2.8: messages are `Vec<Arc<LoadedMessage>>`,
                    // so this clone is a refcount bump (one atomic
                    // increment), not a deep clone of the body bytes.
                    // The Arc::clone makes the cheapness explicit at
                    // the call site so future readers don't worry.
                    ThreadMessage { message: Arc::clone(message) }
                }
            }
        }
    }
}
