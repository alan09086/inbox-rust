//! Main inbox feed component that renders date-grouped sections.

use dioxus::prelude::*;

use crate::app::Inboxly;
use crate::components::bundle_row::BundleRow;
use crate::components::email_row::EmailRow;
use crate::components::section_header::SectionHeader;
use crate::feed::FeedEntry;

/// The main inbox feed list.
///
/// Iterates pre-built `feed_sections` from app state and renders
/// `SectionHeader`, `EmailRow`, and `BundleRow` components.
#[component]
pub fn InboxFeed() -> Element {
    let app_state = use_context::<Signal<Inboxly>>();
    let state = app_state.read();
    let sections = state.feed_sections.clone();
    drop(state);

    rsx! {
        div {
            class: "inbox-feed",
            for section in sections {
                SectionHeader { group: section.group }
                for entry in section.items {
                    match entry {
                        FeedEntry::Thread(item) => rsx! { EmailRow { item: item } },
                        FeedEntry::Bundle(summary) => rsx! { BundleRow { summary: summary } },
                    }
                }
            }
        }
    }
}
