//! Content area component -- shows the active view's content.

use dioxus::prelude::*;

use crate::app::{Inboxly, Message};
use crate::components::inbox_feed::InboxFeed;
use crate::theme::ActiveView;

/// Content area that renders the active view.
///
/// Shows the inbox feed for the Inbox view, or a placeholder for other views.
/// Clicking anywhere dismisses the account switcher if it's open.
#[component]
pub fn ContentArea() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    let state = app_state.read();

    let view = state.active_view;
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
            match view {
                ActiveView::Inbox => rsx! { InboxFeed {} },
                _ => rsx! {
                    div {
                        style: "display: flex; align-items: center; justify-content: center; flex: 1; color: var(--text-secondary);",
                        "{view.title()} \u{2014} coming soon"
                    }
                },
            }
        }
    }
}
