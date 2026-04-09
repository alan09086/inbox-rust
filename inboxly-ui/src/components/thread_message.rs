//! Renders a single message inside the thread detail view.
//!
//! Avatar tile + sender + date in the header, sanitised HTML or
//! plain-text body, an optional attachment list, and (M36 phase 10)
//! a footer row of Reply / Reply All / Forward action buttons that
//! dispatch [`Message::OpenComposeReply`] with the appropriate
//! [`ComposeMode`] variant. The actual prefill work happens in the
//! Phase 8 reply-prefill bridge — these buttons are pure dispatch.

use std::sync::Arc;

use dioxus::prelude::*;
use inboxly_core::id::{EmailId, ThreadId};
use inboxly_core::ComposeMode;

use crate::app::{Inboxly, Message};
use crate::loaded_thread::LoadedMessage;
use crate::sanitize::sanitize_html;
use crate::theme::avatar_colors;

/// `Arc<LoadedMessage>` instead of owned `LoadedMessage` per eng
/// review Issue 2.8: per-render clones in `ThreadDetailView`'s
/// `for` loop become refcount bumps instead of deep clones of
/// the body bytes. The `Arc<T>` impls give us `Clone + PartialEq`
/// for free as long as `T: PartialEq` (which `LoadedMessage` is).
///
/// `thread_id` is the parent `LoadedThread`'s id (M36 phase 10).
/// We need it as a separate prop because `LoadedMessage` only
/// carries the `email_id`, but `Message::OpenComposeReply`'s outer
/// `thread_id: String` field — which the prefill bridge actually
/// reads — has to come from the parent thread context. Passed as
/// a plain `String` (not `Arc<String>`): a single thread id is
/// short, the per-render clone is cheap, and avoiding an Arc layer
/// keeps every `ThreadMessage { ... }` call site simple.
#[component]
pub fn ThreadMessage(thread_id: String, message: Arc<LoadedMessage>) -> Element {
    // Pull the app state signal so the button onclick handlers can
    // dispatch into the state machine. `Signal` is `Copy`, so each
    // closure capture is a cheap reference, not a clone.
    let mut app_state = use_context::<Signal<Inboxly>>();
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
            // M36 phase 10: Reply / Reply All / Forward action row.
            //
            // Each button dispatches `Message::OpenComposeReply` with
            // the matching `ComposeMode` variant. The Phase 8 handler
            // sets the `pending_reply` sentinel; the bridge in
            // `components::app` then loads the original via
            // `ThreadReader::load_email`, builds a prefilled
            // `ComposeState` via `compose_state_from_original`, and
            // dispatches `ComposeReplyReady` to commit it.
            //
            // The inner `ThreadId` field on every reply variant is
            // currently a placeholder (`ThreadId::new()` — a fresh
            // UUID). The prefill bridge ignores it: it uses the outer
            // `thread_id: String` from the `OpenComposeReply` envelope
            // (which we pass through faithfully). The newtype exists in
            // `inboxly-core` for a future migration when storage moves
            // from string thread ids to typed ones; for now the
            // placeholder satisfies the type system without changing
            // runtime behaviour.
            div {
                class: "thread-message-footer",
                button {
                    class: "thread-message-action-btn",
                    aria_label: "Reply",
                    onclick: {
                        let thread_id = thread_id.clone();
                        let original_email_id = message.email_id.clone();
                        move |_| {
                            app_state.write().update(Message::OpenComposeReply {
                                thread_id: thread_id.clone(),
                                mode: ComposeMode::Reply {
                                    thread_id: ThreadId::new(),
                                    original_email_id: EmailId(original_email_id.clone()),
                                },
                            });
                        }
                    },
                    span { class: "thread-message-action-icon", "\u{21A9}" }  // ↩
                    span { "Reply" }
                }
                button {
                    class: "thread-message-action-btn",
                    aria_label: "Reply all",
                    onclick: {
                        let thread_id = thread_id.clone();
                        let original_email_id = message.email_id.clone();
                        move |_| {
                            app_state.write().update(Message::OpenComposeReply {
                                thread_id: thread_id.clone(),
                                mode: ComposeMode::ReplyAll {
                                    thread_id: ThreadId::new(),
                                    original_email_id: EmailId(original_email_id.clone()),
                                },
                            });
                        }
                    },
                    span { class: "thread-message-action-icon", "\u{21AA}" }  // ↪
                    span { "Reply All" }
                }
                button {
                    class: "thread-message-action-btn",
                    aria_label: "Forward",
                    onclick: {
                        let thread_id = thread_id.clone();
                        let original_email_id = message.email_id.clone();
                        move |_| {
                            app_state.write().update(Message::OpenComposeReply {
                                thread_id: thread_id.clone(),
                                mode: ComposeMode::Forward {
                                    thread_id: ThreadId::new(),
                                    original_email_id: EmailId(original_email_id.clone()),
                                },
                            });
                        }
                    },
                    span { class: "thread-message-action-icon", "\u{2192}" }  // →
                    span { "Forward" }
                }
            }
        }
    }
}
