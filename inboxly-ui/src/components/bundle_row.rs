//! Bundle row component with category dot, expand/collapse, and child threads.

use dioxus::prelude::*;

use crate::app::{Inboxly, Message};
use crate::components::email_row::EmailRow;
use crate::feed::FeedItem;
use crate::theme::bundle_colors;
use inboxly_store::BundleSummary;

/// A collapsed (or expanded) bundle row in the inbox feed.
///
/// Shows a category-coloured dot, bundle name, sender preview names,
/// unread count badge, and a chevron to expand/collapse child threads.
#[component]
pub fn BundleRow(summary: BundleSummary) -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();

    let bundle_id = summary.bundle_id.clone();
    let colors = bundle_colors::for_category_str(&summary.category);
    let title_color = colors.title.to_css();
    let badge_color = colors.badge.to_css();

    let is_expanded = app_state.read().expanded_bundles.contains(&summary.bundle_id);

    // Get bundle threads if expanded.
    let expanded_threads: Vec<FeedItem> = if is_expanded {
        let state = app_state.read();
        if let Some(ref store) = state.store {
            store
                .query_bundle_threads(&summary.bundle_id)
                .unwrap_or_default()
                .into_iter()
                .map(|t| FeedItem {
                    thread_id: t.id,
                    sender_name: if t.sender_name.is_empty() {
                        t.sender_address.clone()
                    } else {
                        t.sender_name
                    },
                    sender_address: t.sender_address,
                    avatar_letter: t.avatar_letter,
                    avatar_color_index: t.avatar_color_index,
                    subject: t.subject,
                    snippet: t.snippet,
                    timestamp: t.newest_date,
                    timestamp_display: crate::feed::format_timestamp(t.newest_date),
                    is_unread: t.unread_count > 0,
                    has_attachments: t.has_attachments,
                    is_pinned: t.pinned,
                    email_count: t.email_count,
                })
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let sender_preview_text = summary
        .sender_previews
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let chevron_class = if is_expanded {
        "bundle-chevron expanded"
    } else {
        "bundle-chevron"
    };
    let bid = bundle_id.clone();

    rsx! {
        div {
            // Header row (clickable to expand/collapse)
            div {
                class: "bundle-row",
                onclick: move |_| {
                    app_state.write().update(Message::ToggleBundleExpand(bid.clone()));
                },
                // Category-coloured dot
                div {
                    class: "bundle-dot",
                    style: "background: {title_color};",
                }
                // Content: name + unread badge, sender previews
                div {
                    class: "bundle-content",
                    div {
                        class: "bundle-top-row",
                        span {
                            class: "bundle-name",
                            style: "color: {title_color};",
                            "{summary.name}"
                        }
                        if summary.unread_count > 0 {
                            span {
                                class: "bundle-unread-badge",
                                style: "background: {title_color}; color: {badge_color};",
                                "{summary.unread_count}"
                            }
                        }
                    }
                    span {
                        class: "bundle-sender-previews",
                        "{sender_preview_text}"
                    }
                }
                // Expand/collapse chevron
                span {
                    class: "{chevron_class}",
                    "\u{25BC}"
                }
            }
            // Expanded child threads
            if is_expanded {
                div {
                    class: "bundle-children",
                    for thread in expanded_threads {
                        EmailRow { item: thread }
                    }
                }
            }
        }
    }
}
