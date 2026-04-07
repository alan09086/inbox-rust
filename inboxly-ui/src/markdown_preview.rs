//! Markdown preview rendering for the compose view.
//!
//! Wraps `inboxly_core::markdown_to_html` and pipes the output through
//! M34's `sanitize::sanitize_html` so the preview is XSS-safe even when
//! the Markdown source contains hostile HTML literals. Defence in depth:
//! pulldown-cmark's event filter (added in Phase 1) already drops raw
//! HTML at parse time, but the sanitiser is the second line of defence
//! and protects against any future regression in the parse-time filter.

use inboxly_core::markdown_to_html;

use crate::sanitize::sanitize_html;

/// Render Markdown source to sanitised HTML for preview.
///
/// The result is safe to inject via `dangerous_inner_html` because
/// `sanitize_html` strips `<script>`, event handlers, and any
/// non-allowlisted attributes. The "dangerous" name in the call site
/// describes the Dioxus API surface, not the safety of the input.
pub fn render_markdown_preview(markdown: &str) -> String {
    let html = markdown_to_html(markdown);
    sanitize_html(&html)
}
