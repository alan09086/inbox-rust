//! Date group section header for the inbox feed.

use dioxus::prelude::*;

use crate::feed::DateGroup;

/// Section header showing a date group label (e.g., "Today", "Yesterday").
#[component]
pub fn SectionHeader(group: DateGroup) -> Element {
    rsx! {
        div {
            class: "section-header",
            "{group.label()}"
        }
    }
}
