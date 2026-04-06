//! Email thread row component with avatar, sender, subject, snippet, and badges.

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
    let thread_id_overflow = item.thread_id.clone();
    let thread_id_ctx = item.thread_id.clone();
    let thread_id_done = item.thread_id.clone();
    let thread_id_pin = item.thread_id.clone();
    let thread_id_snooze = item.thread_id.clone();
    let sender_address = item.sender_address.clone();
    // Suppress unused variable warning -- sender_address will be used by
    // future block/rule actions.
    let _ = sender_address;

    rsx! {
        div {
            class: if is_unread { "email-row unread" } else { "email-row" },
            oncontextmenu: move |evt: Event<MouseData>| {
                evt.prevent_default();
                let coords = evt.client_coordinates();
                app_state.write().update(Message::OpenContextMenu {
                    thread_id: thread_id_ctx.clone(),
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
                button {
                    class: "hover-action-btn",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::MarkDone(thread_id_done.clone()));
                    },
                    "\u{2713}"
                }
                button {
                    class: "hover-action-btn",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::TogglePin(thread_id_pin.clone()));
                    },
                    "\u{1F4CC}"
                }
                button {
                    class: "hover-action-btn",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        let coords = evt.client_coordinates();
                        app_state.write().update(Message::OpenSnoozePicker {
                            thread_id: thread_id_snooze.clone(),
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
                    app_state.write().update(Message::OpenOverflowMenu(thread_id_overflow.clone()));
                },
                "\u{22EE}"
            }
        }
    }
}
