//! Shared action body for `ContextMenu` and `OverflowMenu`.
//!
//! These two components are structurally identical — they differ only in
//! which state fields they read and which close message they dispatch.
//! This module holds the shared backdrop + action card rendering so that
//! adding a new thread action requires only one file edit.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::app::{Inboxly, Message, MoveDestination, Point};

/// Render the shared menu body: backdrop + positioned action card.
///
/// `close_msg` is dispatched when the backdrop is clicked. Both
/// `CloseContextMenu` and `CloseOverflowMenu` are unit variants, so
/// they clone trivially for the closure capture.
pub(super) fn render_menu_body(
    thread_id: Arc<str>,
    sender: Arc<str>,
    position: Point,
    categories: Vec<String>,
    close_msg: Message,
    app_state: Signal<Inboxly>,
) -> Element {
    // Pre-allocate Arc<str> for thread_id — one per action closure that needs it.
    let tid: Arc<str> = Arc::clone(&thread_id);
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
    // tid is cloned (not moved) into AddToBundle closures; it remains live but unused after the loop.

    // Pre-allocate Arc<str> for sender — used by BlockSender and CreateRuleFromSender.
    let sender_rule = Arc::clone(&sender);
    let sender_block = sender;

    rsx! {
        // Full-screen backdrop: click to dismiss, right-click also dismissed.
        div {
            class: "menu-backdrop",
            onclick: {
                let close_msg = close_msg.clone();
                let mut app_state = app_state;
                move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state.write().update(close_msg.clone());
                }
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
                onclick: {
                    let mut app_state = app_state;
                    move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::Reply(tid_reply.to_string()));
                    }
                },
                span { class: "menu-icon", "\u{21B6}" }
                "Reply"
            }
            button {
                class: "menu-item",
                onclick: {
                    let mut app_state = app_state;
                    move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state
                            .write()
                            .update(Message::ReplyAll(tid_reply_all.to_string()));
                    }
                },
                span { class: "menu-icon", "\u{21B7}" }
                "Reply All"
            }
            button {
                class: "menu-item",
                onclick: {
                    let mut app_state = app_state;
                    move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state
                            .write()
                            .update(Message::Forward(tid_forward.to_string()));
                    }
                },
                span { class: "menu-icon", "\u{2192}" }
                "Forward"
            }

            div { class: "menu-separator" }

            // ── Group 2: Read state + Move ────────────────────────────────
            button {
                class: "menu-item",
                onclick: {
                    let mut app_state = app_state;
                    move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::MarkReadState {
                            thread_id: tid_mark_read.to_string(),
                            read: true,
                        });
                    }
                },
                span { class: "menu-icon", "\u{2713}" }
                "Mark Read"
            }
            button {
                class: "menu-item",
                onclick: {
                    let mut app_state = app_state;
                    move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::MarkReadState {
                            thread_id: tid_mark_unread.to_string(),
                            read: false,
                        });
                    }
                },
                span { class: "menu-icon", "\u{2B55}" }
                "Mark Unread"
            }
            button {
                class: "menu-item",
                onclick: {
                    let mut app_state = app_state;
                    move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::MoveTo {
                            thread_id: tid_move_inbox.to_string(),
                            destination: MoveDestination::Inbox,
                        });
                    }
                },
                span { class: "menu-icon", "\u{1F4E5}" }
                "Move to Inbox"
            }
            button {
                class: "menu-item",
                onclick: {
                    let mut app_state = app_state;
                    move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::MoveTo {
                            thread_id: tid_move_trash.to_string(),
                            destination: MoveDestination::Trash,
                        });
                    }
                },
                span { class: "menu-icon", "\u{1F5D1}" }
                "Move to Trash"
            }
            button {
                class: "menu-item",
                onclick: {
                    let mut app_state = app_state;
                    move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::MoveTo {
                            thread_id: tid_move_spam.to_string(),
                            destination: MoveDestination::Spam,
                        });
                    }
                },
                span { class: "menu-icon", "\u{26A0}" }
                "Move to Spam"
            }

            div { class: "menu-separator" }

            // ── Group 3: Organise ─────────────────────────────────────────
            button {
                class: "menu-item",
                onclick: {
                    let mut app_state = app_state;
                    move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state
                            .write()
                            .update(Message::MuteThread(tid_mute.to_string()));
                    }
                },
                span { class: "menu-icon", "\u{1F515}" }
                "Mute"
            }

            // Flat Add-to-Bundle sub-items — one per category.
            for category in categories {
                {
                    let cat = category.clone();
                    let cat_tid = Arc::clone(&tid);
                    let mut app_state = app_state;
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
                onclick: {
                    let mut app_state = app_state;
                    move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state
                            .write()
                            .update(Message::CreateRuleFromSender(sender_rule.to_string()));
                    }
                },
                span { class: "menu-icon", "\u{2699}" }
                "Create Rule from Sender"
            }

            div { class: "menu-separator" }

            // ── Group 4: Destructive ──────────────────────────────────────
            button {
                class: "menu-item destructive",
                onclick: {
                    let mut app_state = app_state;
                    move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::BlockSender {
                            thread_id: tid_block.to_string(),
                            sender_address: sender_block.to_string(),
                        });
                    }
                },
                span { class: "menu-icon", "\u{1F6AB}" }
                "Block Sender"
            }
            button {
                class: "menu-item destructive",
                onclick: {
                    let mut app_state = app_state;
                    move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state
                            .write()
                            .update(Message::ReportSpam(tid_spam.to_string()));
                    }
                },
                span { class: "menu-icon", "\u{26A0}" }
                "Report Spam"
            }
        }
    }
}
