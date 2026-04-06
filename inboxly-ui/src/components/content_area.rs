//! Content area component -- shows the active view's content.

use dioxus::prelude::*;

use crate::app::{Inboxly, Message};
use crate::components::empty_state::EmptyState;
use crate::components::inbox_feed::InboxFeed;
use crate::components::inbox_zero::InboxZero;
use crate::theme::ActiveView;

/// Content area that renders the active view.
///
/// Shows the inbox feed for the Inbox view (or InboxZero when the feed is
/// empty), EmptyState placeholders for Snoozed and Done, and a placeholder
/// for Settings. Clicking anywhere dismisses the account switcher if open.
#[component]
pub fn ContentArea() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    let state = app_state.read();

    let view = state.active_view;
    let switcher_open = state.account_switcher_open;
    let inbox_empty = state.feed_sections.is_empty();

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
                ActiveView::Inbox => {
                    if inbox_empty {
                        rsx! { InboxZero {} }
                    } else {
                        rsx! { InboxFeed {} }
                    }
                }
                ActiveView::Snoozed => rsx! {
                    EmptyState {
                        icon: "\u{23F0}".to_string(),
                        text: "No snoozed conversations".to_string()
                    }
                },
                ActiveView::Done => rsx! {
                    EmptyState {
                        icon: "\u{2705}".to_string(),
                        text: "No done conversations".to_string()
                    }
                },
                ActiveView::Settings => rsx! {
                    div {
                        style: "display: flex; align-items: center; justify-content: center; flex: 1; color: var(--text-secondary);",
                        "Settings \u{2014} coming soon"
                    }
                },
            }
        }
    }
}
