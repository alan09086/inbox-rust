//! Toolbar component -- coloured bar at the top of the application.

use dioxus::prelude::*;

use crate::app::{Inboxly, Message};
use crate::theme::ActiveView;

/// Maximum number of characters of a draft subject the toolbar
/// "Draft:" indicator chip will show before truncating with an
/// ellipsis. Picked to fit comfortably alongside the title and
/// the search input on a 1024-wide window.
const DRAFT_CHIP_SUBJECT_MAX_CHARS: usize = 30;

/// Format the draft chip label for the toolbar indicator.
///
/// `subject` is taken verbatim from `compose.subject`. An empty or
/// whitespace-only subject becomes the placeholder
/// `"Draft: (no subject)"`. A subject longer than
/// [`DRAFT_CHIP_SUBJECT_MAX_CHARS`] is truncated on a Unicode
/// codepoint boundary (NOT a byte boundary, otherwise multibyte
/// characters would split mid-sequence) and gets a `…` suffix.
///
/// Pure helper so the unit tests in `app.rs` can assert on the
/// formatting without spinning up a Dioxus render loop.
#[must_use]
pub fn format_draft_chip_label(subject: &str) -> String {
    let trimmed = subject.trim();
    if trimmed.is_empty() {
        return "Draft: (no subject)".to_string();
    }
    let char_count = trimmed.chars().count();
    if char_count <= DRAFT_CHIP_SUBJECT_MAX_CHARS {
        return format!("Draft: {trimmed}");
    }
    // Truncate on a codepoint boundary, then append the ellipsis.
    let truncated: String = trimmed.chars().take(DRAFT_CHIP_SUBJECT_MAX_CHARS).collect();
    format!("Draft: {truncated}\u{2026}")
}

/// Toolbar component.
///
/// 56dp tall, background colour changes by active view. Contains:
/// - Left: hamburger/back button
/// - Title text
/// - Search placeholder
/// - **M36 Phase 5**: "Draft: <subject>" indicator chip when a draft
///   is open and the user is not in the Compose view
/// - Gear icon (settings)
/// - Account avatar
#[component]
pub fn Toolbar() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    let state = app_state.read();

    let toolbar_css = state.active_view.toolbar_css(&state.theme);
    let title = state.active_view.title();
    let is_settings = state.active_view == ActiveView::Settings;
    let nav_icon = if is_settings { "\u{2190}" } else { "\u{2630}" };
    let avatar_letter = state
        .active_email()
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();

    // M36 Phase 5: only show the draft chip when (a) a draft is open
    // and (b) the user is not currently looking at the compose view
    // itself (otherwise the chip would be redundant — the user is
    // already in the form). The Phase 11 inline-compose-panel case
    // (a draft open while reading a thread in Inbox) is intentionally
    // out of scope for this commit; the chip is hidden in Compose
    // only, every other view shows it.
    let draft_chip = if state.active_view != ActiveView::Compose {
        state.compose.draft_id.as_ref().map(|id| {
            (
                id.clone(),
                crate::components::toolbar::format_draft_chip_label(&state.compose.subject),
            )
        })
    } else {
        None
    };

    // Drop the read borrow before creating event handlers
    drop(state);

    rsx! {
        div {
            class: "toolbar",
            style: "background: {toolbar_css};",

            // Hamburger / back button
            button {
                class: "toolbar-btn",
                onclick: move |_| {
                    let mut state = app_state.write();
                    if state.active_view == ActiveView::Settings {
                        state.update(Message::NavigateBack);
                    } else {
                        state.update(Message::ToggleDrawer);
                    }
                },
                "{nav_icon}"
            }

            // Title
            span { class: "toolbar-title", "{title}" }

            // Search placeholder
            input {
                class: "toolbar-search",
                r#type: "text",
                placeholder: "Search mail",
                readonly: true,
            }

            // Spacer
            div { class: "toolbar-spacer" }

            // M36 Phase 5 -- "Draft: <subject>" indicator chip
            if let Some((draft_id, chip_label)) = draft_chip {
                button {
                    class: "toolbar-draft-chip",
                    aria_label: "Resume draft",
                    title: "Resume draft",
                    onclick: move |_| {
                        app_state
                            .write()
                            .update(Message::ResumeCompose { draft_id: draft_id.clone() });
                    },
                    "{chip_label}"
                }
            }

            // Gear icon (settings)
            button {
                class: "toolbar-btn",
                onclick: move |_| {
                    app_state.write().update(Message::NavigateToSettings);
                },
                "\u{2699}"
            }

            // Account avatar
            div {
                class: "toolbar-avatar",
                "{avatar_letter}"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::format_draft_chip_label;

    #[test]
    fn empty_subject_renders_no_subject_placeholder() {
        assert_eq!(format_draft_chip_label(""), "Draft: (no subject)");
    }

    #[test]
    fn whitespace_only_subject_renders_no_subject_placeholder() {
        // The helper trims before checking, so a subject of pure
        // whitespace is treated as empty.
        assert_eq!(format_draft_chip_label("   \t\n  "), "Draft: (no subject)");
    }

    #[test]
    fn short_subject_is_not_truncated() {
        assert_eq!(format_draft_chip_label("Hello there"), "Draft: Hello there");
    }

    #[test]
    fn long_subject_is_truncated_with_ellipsis() {
        // 40 chars > the 30-char cap. The truncated form keeps
        // exactly 30 source characters and appends a single `…`
        // (one codepoint, three bytes in UTF-8).
        let subject = "abcdefghijabcdefghijabcdefghijabcdefghij"; // 40
        let label = format_draft_chip_label(subject);
        assert_eq!(label, "Draft: abcdefghijabcdefghijabcdefghij\u{2026}");
    }

    #[test]
    fn truncation_respects_codepoint_boundaries() {
        // 31 multibyte characters: bare-byte truncation would split
        // the 31st codepoint mid-sequence. The helper uses
        // `chars().take(30)` so the result is always valid UTF-8.
        let subject = "ééééééééééééééééééééééééééééééé"; // 31 'é's
        let label = format_draft_chip_label(subject);
        assert!(
            label.is_char_boundary(label.len()),
            "truncated label must end on a char boundary"
        );
        assert!(label.ends_with('\u{2026}'));
        // 30 'é's + the prefix + ellipsis.
        let expected: String = std::iter::once("Draft: ".to_string())
            .chain(std::iter::repeat_n("é".to_string(), 30))
            .chain(std::iter::once("\u{2026}".to_string()))
            .collect();
        assert_eq!(label, expected);
    }
}
