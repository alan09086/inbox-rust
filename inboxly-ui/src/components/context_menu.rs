//! Right-click context menu for thread actions.
//!
//! Thin wrapper around [`menu_actions::render_menu_body`] that reads
//! the context-menu state fields.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::app::{Inboxly, Message};
use crate::components::menu_actions::render_menu_body;

/// Right-click context menu.
///
/// Checks `context_menu_thread` and renders nothing when the menu is closed.
/// When open, delegates to [`render_menu_body`] with `CloseContextMenu`.
#[component]
pub fn ContextMenu() -> Element {
    let app_state = use_context::<Signal<Inboxly>>();
    let state = app_state.read();
    let Some(thread_id_str) = state.context_menu_thread.clone() else {
        return rsx! {};
    };
    let position = state.context_menu_position;
    let sender_str = state.menu_thread_sender.clone().unwrap_or_default();
    let categories: Vec<String> = state.bundle_categories.iter().map(|c| c.name.clone()).collect();
    drop(state);

    let thread_id: Arc<str> = Arc::from(thread_id_str.as_str());
    let sender: Arc<str> = Arc::from(sender_str.as_str());

    render_menu_body(
        thread_id,
        sender,
        position,
        categories,
        Message::CloseContextMenu,
        app_state,
    )
}
