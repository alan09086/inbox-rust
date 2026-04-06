//! Content area component -- placeholder for each view.

use dioxus::prelude::*;

use crate::app::{Inboxly, Message};

/// Content area placeholder.
///
/// Shows the name of the currently active view. Clicking anywhere
/// dismisses the account switcher if it's open.
#[component]
pub fn ContentArea() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    let state = app_state.read();

    let view_name = state.active_view.title();
    let switcher_open = state.account_switcher_open;

    drop(state);

    rsx! {
        div {
            class: "content-area",
            onclick: move |_| {
                if switcher_open {
                    app_state.write().update(Message::ToggleAccountSwitcher);
                }
            },
            "{view_name} -- content area placeholder"
        }
    }
}
