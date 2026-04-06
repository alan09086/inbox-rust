//! Overflow (three-dot button) menu for thread actions.
//!
//! Thin wrapper around [`menu_actions::render_menu_body`] that reads
//! the overflow-menu state fields.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::app::{Inboxly, Message};
use crate::components::menu_actions::render_menu_body;

/// Overflow (three-dot) menu.
///
/// Checks `overflow_menu_thread` and renders nothing when the menu is closed.
/// When open, delegates to [`render_menu_body`] with `CloseOverflowMenu`.
#[component]
pub fn OverflowMenu() -> Element {
    let app_state = use_context::<Signal<Inboxly>>();
    let state = app_state.read();
    let Some(thread_id_str) = state.overflow_menu_thread.clone() else {
        return rsx! {};
    };
    let position = state.overflow_menu_position;
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
        Message::CloseOverflowMenu,
        app_state,
    )
}
