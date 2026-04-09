//! Content area component -- shows the active view's content.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::app::{Inboxly, Message};
use crate::components::compose_view::ComposeView;
use crate::components::empty_state::EmptyState;
use crate::components::inbox_feed::InboxFeed;
use crate::components::inbox_zero::InboxZero;
use crate::components::thread_detail_view::ThreadDetailView;
use crate::loaded_thread::LoadedThread;
use crate::state::ComposeLayout;
use crate::theme::ActiveView;

/// Content area that renders the active view.
///
/// Shows the inbox feed for the Inbox view (or InboxZero when the feed is
/// empty), EmptyState placeholders for Snoozed and Done, and a placeholder
/// for Settings. Clicking anywhere dismisses the account switcher if open.
///
/// **M36 Phase 11**: when `compose.layout == Inline` AND a thread is open
/// AND a draft exists, the Inbox arm renders a 35/65 vertical split with
/// `ThreadDetailView` in the top pane and `ComposeView` in the bottom
/// pane. See [`Message::ComposeToggleLayout`].
//
// TODO(post-M36, eng review D1): split `compose: ComposeState` out of
// the main `Inboxly` signal into its own top-level Dioxus context. As
// of Phase 11 every keystroke in the inline ComposeView re-runs the
// `app_state.read()` below, which forces ContentArea (and therefore
// ThreadDetailView + sanitize_html on the open thread) to re-render
// on every character. The signal split is a 300-LOC refactor across
// 260+ compose dispatch sites, so it was deferred from M36 Phase 11.
// Track via the post-M36 perf milestone.
#[component]
pub fn ContentArea() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    let open_thread = use_context::<Signal<Option<Arc<LoadedThread>>>>();
    let state = app_state.read();

    let view = state.active_view;
    let switcher_open = state.account_switcher_open;
    let inbox_empty = state.feed_sections.is_empty();
    let compose_layout = state.compose.layout;
    let compose_has_draft = state.compose.draft_id.is_some();

    drop(state);

    let thread_open = open_thread.read().is_some();
    // M36 Phase 11: render the 35/65 inline split when all four
    // conditions hold. ThreadDetailView occupies the top 35% so the
    // user can keep reading the conversation while drafting their
    // reply in the bottom 65%.
    let inline_split =
        thread_open && compose_has_draft && matches!(compose_layout, ComposeLayout::Inline);

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
                    if inline_split {
                        rsx! {
                            div {
                                class: "compose-inline-split",
                                div {
                                    class: "compose-inline-top",
                                    ThreadDetailView {}
                                }
                                div {
                                    class: "compose-inline-bottom",
                                    ComposeView {}
                                }
                            }
                        }
                    } else if thread_open {
                        rsx! { ThreadDetailView {} }
                    } else if inbox_empty {
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
                ActiveView::Compose => rsx! { ComposeView {} },
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
