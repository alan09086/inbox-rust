//! Renders a single message inside the thread detail view.
//!
//! Avatar tile + sender + date in the header, sanitised HTML or
//! plain-text body, and an optional attachment list at the bottom.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::loaded_thread::LoadedMessage;
use crate::sanitize::sanitize_html;
use crate::theme::avatar_colors;

/// `Arc<LoadedMessage>` instead of owned `LoadedMessage` per eng
/// review Issue 2.8: per-render clones in `ThreadDetailView`'s
/// `for` loop become refcount bumps instead of deep clones of
/// the body bytes. The `Arc<T>` impls give us `Clone + PartialEq`
/// for free as long as `T: PartialEq` (which `LoadedMessage` is).
#[component]
pub fn ThreadMessage(message: Arc<LoadedMessage>) -> Element {
    let avatar_letter = message
        .from_name
        .chars()
        .next()
        .unwrap_or('?')
        .to_ascii_uppercase();
    let avatar_color = avatar_colors::for_letter(avatar_letter).to_css();
    // Issue 2.7: date is Option — display "(unknown time)" for None
    // (corrupt or out-of-range timestamps in the store) instead of
    // the previous misleading Utc::now() fallback.
    let date_display = match message.date {
        Some(dt) => dt.format("%b %-d, %Y at %-I:%M %p").to_string(),
        None => "(unknown time)".to_string(),
    };

    // Choose a body renderer: sanitised HTML if available, else plain text in a <pre>.
    // Field accesses below go through Arc's Deref to &LoadedMessage.
    let body = match (&message.body_html, &message.body_text) {
        (Some(html), _) => {
            let sanitised = sanitize_html(html);
            rsx! { div { class: "thread-message-body", dangerous_inner_html: "{sanitised}" } }
        }
        (None, Some(text)) => {
            let owned = text.clone();
            rsx! { div { class: "thread-message-body", pre { "{owned}" } } }
        }
        (None, None) => {
            rsx! { div { class: "thread-message-body", "(no content)" } }
        }
    };

    rsx! {
        div {
            class: "thread-message",
            div {
                class: "thread-message-header",
                div {
                    class: "avatar",
                    style: "background: {avatar_color};",
                    "{avatar_letter}"
                }
                div {
                    class: "thread-message-from",
                    span { class: "thread-message-sender", "{message.from_name}" }
                    span { class: "thread-message-address", "{message.from_address}" }
                }
                span { class: "thread-message-date", "{date_display}" }
            }
            {body}
            if !message.attachments.is_empty() {
                div {
                    class: "thread-message-attachments",
                    for att in message.attachments.iter() {
                        div {
                            class: "thread-message-attachment",
                            span { class: "thread-message-attachment-icon", "\u{1F4CE}" }
                            span { class: "thread-message-attachment-name", "{att.filename}" }
                            span { class: "thread-message-attachment-size", "{att.size_bytes} bytes" }
                        }
                    }
                }
            }
        }
    }
}
