//! Right-click context menu for thread actions.
//!
//! Renders a full-viewport backdrop (click to dismiss) plus a positioned
//! card listing all thread actions. Reads state from `Inboxly`:
//! - `context_menu_thread`: `Option<String>` — when `Some`, the menu is visible
//! - `context_menu_position`: `Point` — where the cursor was when opened
//! - `menu_thread_sender`: `Option<String>` — for `BlockSender` and `CreateRuleFromSender`

use std::sync::Arc;

use dioxus::prelude::*;

use crate::app::{Inboxly, Message, MoveDestination};

/// Right-click context menu.
///
/// Checks `context_menu_thread` and renders nothing when the menu is closed.
/// When open, renders a full-screen backdrop (click → dismiss) and a
/// positioned card with all thread actions grouped by function.
#[component]
pub fn ContextMenu() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    let state = app_state.read();
    let Some(thread_id_str) = state.context_menu_thread.clone() else {
        return rsx! {};
    };
    let position = state.context_menu_position;
    let sender_str = state.menu_thread_sender.clone().unwrap_or_default();
    let categories: Vec<String> = state
        .bundle_categories
        .iter()
        .map(|c| c.name.clone())
        .collect();
    drop(state);

    // Pre-allocate Arc<str> for thread_id — one per action closure that needs it.
    let tid: Arc<str> = Arc::from(thread_id_str.as_str());
    let tid_reply = Arc::clone(&tid);
    let tid_reply_all = Arc::clone(&tid);
    let tid_forward = Arc::clone(&tid);
    let tid_mark_read = Arc::clone(&tid);
    let tid_mark_unread = Arc::clone(&tid);
    let tid_move_inbox = Arc::clone(&tid);
    let tid_move_trash = Arc::clone(&tid);
    let tid_move_spam = Arc::clone(&tid);
    let tid_mute = Arc::clone(&tid);
    let tid_block = Arc::clone(&tid);
    let tid_spam = Arc::clone(&tid);
    // tid itself is moved into AddToBundle closures via categories loop.

    // Pre-allocate Arc<str> for sender — used by BlockSender and CreateRuleFromSender.
    let sender: Arc<str> = Arc::from(sender_str.as_str());
    let sender_rule = Arc::clone(&sender);
    let sender_block = sender; // transfer ownership for the last user

    rsx! {
        // Full-screen backdrop: click to dismiss, right-click also dismissed.
        div {
            class: "menu-backdrop",
            onclick: move |evt: Event<MouseData>| {
                evt.stop_propagation();
                app_state.write().update(Message::CloseContextMenu);
            },
            oncontextmenu: move |evt: Event<MouseData>| {
                evt.prevent_default();
                evt.stop_propagation();
            },
        }

        // Positioned action card.
        div {
            class: "context-menu",
            style: "top: {position.y}px; left: {position.x}px;",

            // ── Group 1: Reply actions ────────────────────────────────────
            button {
                class: "menu-item",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state.write().update(Message::Reply(tid_reply.to_string()));
                },
                span { class: "menu-icon", "\u{21B6}" }
                "Reply"
            }
            button {
                class: "menu-item",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state
                        .write()
                        .update(Message::ReplyAll(tid_reply_all.to_string()));
                },
                span { class: "menu-icon", "\u{21B7}" }
                "Reply All"
            }
            button {
                class: "menu-item",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state
                        .write()
                        .update(Message::Forward(tid_forward.to_string()));
                },
                span { class: "menu-icon", "\u{2192}" }
                "Forward"
            }

            div { class: "menu-separator" }

            // ── Group 2: Read state + Move ────────────────────────────────
            button {
                class: "menu-item",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state.write().update(Message::MarkReadState {
                        thread_id: tid_mark_read.to_string(),
                        read: true,
                    });
                },
                span { class: "menu-icon", "\u{2713}" }
                "Mark Read"
            }
            button {
                class: "menu-item",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state.write().update(Message::MarkReadState {
                        thread_id: tid_mark_unread.to_string(),
                        read: false,
                    });
                },
                span { class: "menu-icon", "\u{2B55}" }
                "Mark Unread"
            }
            button {
                class: "menu-item",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state.write().update(Message::MoveTo {
                        thread_id: tid_move_inbox.to_string(),
                        destination: MoveDestination::Inbox,
                    });
                },
                span { class: "menu-icon", "\u{1F4E5}" }
                "Move to Inbox"
            }
            button {
                class: "menu-item",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state.write().update(Message::MoveTo {
                        thread_id: tid_move_trash.to_string(),
                        destination: MoveDestination::Trash,
                    });
                },
                span { class: "menu-icon", "\u{1F5D1}" }
                "Move to Trash"
            }
            button {
                class: "menu-item",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state.write().update(Message::MoveTo {
                        thread_id: tid_move_spam.to_string(),
                        destination: MoveDestination::Spam,
                    });
                },
                span { class: "menu-icon", "\u{26A0}" }
                "Move to Spam"
            }

            div { class: "menu-separator" }

            // ── Group 3: Organise ─────────────────────────────────────────
            button {
                class: "menu-item",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state
                        .write()
                        .update(Message::MuteThread(tid_mute.to_string()));
                },
                span { class: "menu-icon", "\u{1F515}" }
                "Mute"
            }

            // Flat Add-to-Bundle sub-items — one per category.
            for category in categories {
                {
                    let cat = category.clone();
                    let cat_tid = Arc::clone(&tid);
                    rsx! {
                        button {
                            class: "menu-item",
                            onclick: move |evt: Event<MouseData>| {
                                evt.stop_propagation();
                                app_state.write().update(Message::AddToBundle {
                                    thread_id: cat_tid.to_string(),
                                    category: cat.clone(),
                                });
                            },
                            span { class: "menu-icon", "\u{1F4C1}" }
                            "Add to {category}"
                        }
                    }
                }
            }

            button {
                class: "menu-item",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state
                        .write()
                        .update(Message::CreateRuleFromSender(sender_rule.to_string()));
                },
                span { class: "menu-icon", "\u{2699}" }
                "Create Rule from Sender"
            }

            div { class: "menu-separator" }

            // ── Group 4: Destructive ──────────────────────────────────────
            button {
                class: "menu-item destructive",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state.write().update(Message::BlockSender {
                        thread_id: tid_block.to_string(),
                        sender_address: sender_block.to_string(),
                    });
                },
                span { class: "menu-icon", "\u{1F6AB}" }
                "Block Sender"
            }
            button {
                class: "menu-item destructive",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state
                        .write()
                        .update(Message::ReportSpam(tid_spam.to_string()));
                },
                span { class: "menu-icon", "\u{26A0}" }
                "Report Spam"
            }
        }
    }
}
