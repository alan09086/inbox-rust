//! Email thread row component with avatar, sender, subject, snippet, and badges.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::app::{Inboxly, Message, Point};
use crate::feed::FeedItem;
use crate::theme::avatar_colors;

/// A single email thread row in the inbox feed.
///
/// Shows an avatar letter tile, sender name, subject + snippet, timestamp,
/// attachment icon, message count badge, and overflow button.
#[component]
pub fn EmailRow(item: FeedItem) -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();

    // Compute avatar background colour from sender's first letter.
    let avatar_color = avatar_colors::for_letter(item.avatar_letter).to_css();
    let is_unread = item.is_unread;
    let thread_id: Arc<str> = Arc::from(item.thread_id.as_str());
    let tid_ctx = Arc::clone(&thread_id);
    let tid_done = Arc::clone(&thread_id);
    let tid_pin = Arc::clone(&thread_id);
    let tid_snooze = Arc::clone(&thread_id);
    // thread_id itself is moved into the overflow closure below.
    let sender_arc: Arc<str> = Arc::from(item.sender_address.as_str());
    let sender_ctx = Arc::clone(&sender_arc);
    // sender_arc itself is moved into the overflow closure below.

    rsx! {
        div {
            class: if is_unread { "email-row unread" } else { "email-row" },
            oncontextmenu: move |evt: Event<MouseData>| {
                evt.prevent_default();
                let coords = evt.client_coordinates();
                app_state.write().update(Message::OpenContextMenu {
                    thread_id: tid_ctx.to_string(),
                    sender_address: sender_ctx.to_string(),
                    position: Point::new(coords.x as f32, coords.y as f32),
                });
            },
            // Avatar letter tile
            div {
                class: "avatar",
                style: "background: {avatar_color};",
                "{item.avatar_letter}"
            }
            // Content: sender + timestamp on top row, subject + snippet on bottom
            div {
                class: "email-content",
                div {
                    class: "email-top-row",
                    span { class: "email-sender", "{item.sender_name}" }
                    span { class: "email-timestamp", "{item.timestamp_display}" }
                }
                div {
                    class: "email-bottom-row",
                    span { class: "email-subject", "{item.subject}" }
                    if !item.snippet.is_empty() {
                        span { class: "email-subject-separator", " — " }
                        span { class: "email-snippet", "{item.snippet}" }
                    }
                }
            }
            // Badges: attachment icon + message count
            div {
                class: "email-badges",
                if item.has_attachments {
                    span { class: "email-attachment-icon", "\u{1F4CE}" }
                }
                if item.email_count > 1 {
                    span { class: "email-count-badge", "{item.email_count}" }
                }
            }
            // Hover actions: Done, Pin, Snooze
            div {
                class: "hover-actions",
                oncontextmenu: move |evt: Event<MouseData>| {
                    evt.prevent_default();
                    evt.stop_propagation();
                },
                button {
                    class: "hover-action-btn",
                    aria_label: "Mark done",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::MarkDone(tid_done.to_string()));
                    },
                    "\u{2713}"
                }
                button {
                    class: "hover-action-btn",
                    aria_label: "Toggle pin",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::TogglePin(tid_pin.to_string()));
                    },
                    "\u{1F4CC}"
                }
                button {
                    class: "hover-action-btn",
                    aria_label: "Snooze",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        let coords = evt.client_coordinates();
                        app_state.write().update(Message::OpenSnoozePicker {
                            thread_id: tid_snooze.to_string(),
                            position: Point::new(coords.x as f32, coords.y as f32),
                        });
                    },
                    "\u{23F0}"
                }
            }
            // Overflow (three-dot) menu button
            button {
                class: "overflow-btn",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    let coords = evt.client_coordinates();
                    let position = Point::new(coords.x as f32, coords.y as f32);
                    app_state.write().update(Message::OpenOverflowMenu {
                        thread_id: thread_id.to_string(),
                        sender_address: sender_arc.to_string(),
                        position,
                    });
                },
                "\u{22EE}"
            }
        }
    }
}
