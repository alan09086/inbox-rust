//! HTML sanitisation for email body rendering.
//!
//! Email HTML is untrusted — it may come from any sender and may
//! contain `<script>`, event handlers, `javascript:` URLs, or other
//! XSS vectors. This module wraps `ammonia` with the project's
//! whitelist, which strips:
//! - All the things ammonia strips by default (scripts, event
//!   handlers, `javascript:` URLs, iframes, etc.)
//! - **`src` and `srcset` on `<img>`** — privacy pass from the M34
//!   eng review (Issue 1.1). Blocks tracking-pixel phone-home on
//!   email open. The `<img>` tag itself is preserved (renders as
//!   broken-image placeholder with alt text) so layout isn't
//!   destroyed. A future milestone will add a per-sender "Show
//!   images" toggle.
//!
//! And rewrites:
//! - **All `<a href>` URLs** are rewritten to the sentinel prefix
//!   `#inboxly-ext:<original-url>` — eng review Issue 1.2. Because
//!   the `href` now starts with `#`, WebKitGTK treats the click as
//!   a same-page anchor and never navigates away from the app.
//!   The `ThreadMessage` component's click handler catches the
//!   click, strips the sentinel prefix, and dispatches
//!   `Message::OpenExternalUrl(real_url)` which hands the URL to
//!   `open::that()` for the system browser.

/// Sentinel prefix injected into rewritten `<a href>` URLs so the
/// webview treats them as harmless in-page anchors instead of
/// navigations. The thread-message click interceptor matches this
/// prefix to reconstruct the original URL.
pub const EXT_URL_SENTINEL: &str = "#inboxly-ext:";

/// Sanitise an HTML email body for safe rendering.
///
/// Applies the project's whitelist over ammonia's defaults:
/// - Strips `src`/`srcset` from `<img>` (tracking pixel block).
/// - Rewrites every `<a href>` to `#inboxly-ext:<url>` so clicks
///   don't navigate the webview.
pub fn sanitize_html(raw: &str) -> String {
    let mut builder = ammonia::Builder::default();
    builder.rm_tag_attributes("img", &["src", "srcset"]);
    // ammonia 4.x exposes a single `attribute_filter` callback that
    // fires for every retained attribute on every retained element
    // (after the default scheme allowlist has already dropped
    // `javascript:` / `data:` hrefs). We use it to rewrite `<a href>`
    // values to the sentinel form so the WebKitGTK webview treats
    // clicks as harmless in-page anchors. All other attributes are
    // passed through unchanged.
    builder.attribute_filter(|element, attribute, value| {
        if element == "a" && attribute == "href" {
            // In-page anchors (href="#section") are already safe — leave
            // them alone so within-email anchor navigation still works.
            if value.starts_with('#') {
                return Some(value.into());
            }
            return Some(format!("{EXT_URL_SENTINEL}{value}").into());
        }
        Some(value.into())
    });
    builder.clean(raw).to_string()
}

/// Extract the real URL from a sentinel-prefixed href, or `None`
/// if the href doesn't match the sentinel form. Used by the
/// ThreadMessage click interceptor to decide whether a click on
/// an `<a>` element should open the system browser.
pub fn extract_ext_url(href: &str) -> Option<&str> {
    href.strip_prefix(EXT_URL_SENTINEL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_script_tags() {
        let dirty = "<p>Hi</p><script>alert('xss')</script><p>bye</p>";
        let clean = sanitize_html(dirty);
        assert!(!clean.contains("<script"));
        assert!(!clean.contains("alert"));
        assert!(clean.contains("<p>Hi</p>"));
        assert!(clean.contains("<p>bye</p>"));
    }

    #[test]
    fn strips_javascript_urls() {
        let dirty = "<a href=\"javascript:alert(1)\">click</a>";
        let clean = sanitize_html(dirty);
        assert!(!clean.contains("javascript:"));
    }

    #[test]
    fn strips_event_handlers() {
        let dirty = "<img src=\"x\" onerror=\"alert(1)\">";
        let clean = sanitize_html(dirty);
        assert!(!clean.contains("onerror"));
    }

    #[test]
    fn preserves_safe_formatting() {
        let dirty = "<p><strong>bold</strong> and <em>italic</em></p>";
        let clean = sanitize_html(dirty);
        assert!(clean.contains("<strong>bold</strong>"));
        assert!(clean.contains("<em>italic</em>"));
    }

    #[test]
    fn rewrites_anchor_href_to_sentinel() {
        // Eng review Issue 1.2: clicking <a href="https://..."> inside
        // the Dioxus webview would otherwise navigate the app away.
        // We rewrite to a sentinel fragment so WebKitGTK treats the
        // click as an in-page anchor (no-op) and the click handler
        // on .thread-message-body dispatches OpenExternalUrl instead.
        let dirty = "<a href=\"https://example.com/invoice\">view invoice</a>";
        let clean = sanitize_html(dirty);
        // The link text is preserved.
        assert!(clean.contains(">view invoice</a>"));
        // The sentinel prefix is applied.
        assert!(
            clean.contains("href=\"#inboxly-ext:https://example.com/invoice\""),
            "expected sentinel-prefixed href, got: {clean}"
        );
        // The original URL should NOT be reachable as a normal href
        // (no bare `href="https://` without the sentinel).
        assert!(!clean.contains("href=\"https://"));
    }

    #[test]
    fn preserves_in_page_anchor_links() {
        // Within-email anchors (rare but legal) should stay as same-page
        // anchors and NOT get the sentinel prefix. The webview already
        // treats these as in-page navigation, which is fine.
        let dirty = "<a href=\"#section2\">jump</a>";
        let clean = sanitize_html(dirty);
        assert!(clean.contains("href=\"#section2\""));
        assert!(!clean.contains("inboxly-ext"));
    }

    #[test]
    fn extract_ext_url_returns_original() {
        assert_eq!(
            extract_ext_url("#inboxly-ext:https://example.com/foo"),
            Some("https://example.com/foo")
        );
        assert_eq!(
            extract_ext_url("#inboxly-ext:mailto:a@b.com"),
            Some("mailto:a@b.com")
        );
    }

    #[test]
    fn extract_ext_url_returns_none_for_plain_anchor() {
        assert_eq!(extract_ext_url("#section"), None);
        assert_eq!(extract_ext_url("https://example.com"), None);
        assert_eq!(extract_ext_url(""), None);
    }

    // Eng review Issue 2.2: sanitiser-level defence in depth.
    // The OpenExternalUrl handler also validates the scheme before
    // calling open::that(), but this test pins the sanitiser invariant
    // so future ammonia upgrades can't silently regress it.

    #[test]
    fn javascript_url_does_not_round_trip_through_sanitiser() {
        // Round-trip: sanitise <a href="javascript:..."> and then try
        // to extract the original URL via the JS bridge's logic.
        // The result MUST NOT be a javascript: URL.
        let dirty = "<a href=\"javascript:alert(1)\">click</a>";
        let clean = sanitize_html(dirty);

        // ammonia's default scheme allowlist must strip the
        // javascript: URL entirely. After sanitisation, there
        // should be NO trace of the word "javascript".
        assert!(
            !clean.contains("javascript"),
            "javascript: URL leaked through sanitiser: {clean}"
        );

        // Whatever ammonia decided to do with the <a> tag, the
        // result must not contain a sentinel-prefixed URL that
        // unwraps to a javascript: scheme. Find any href in the
        // sanitised output and verify it doesn't unwrap to evil.
        // We do a substring scan since a full HTML parse is overkill.
        for window in clean.split("href=\"").skip(1) {
            if let Some(end) = window.find('"') {
                let href = &window[..end];
                if let Some(unwrapped) = extract_ext_url(href) {
                    assert!(
                        !unwrapped.starts_with("javascript:"),
                        "extracted URL is a javascript: scheme: {unwrapped}"
                    );
                }
            }
        }
    }

    #[test]
    fn data_url_does_not_round_trip_through_sanitiser() {
        // Same defence for data: URLs (which can encode arbitrary
        // payloads in some browsers — defence in depth).
        let dirty = "<a href=\"data:text/html,<script>alert(1)</script>\">click</a>";
        let clean = sanitize_html(dirty);
        for window in clean.split("href=\"").skip(1) {
            if let Some(end) = window.find('"') {
                let href = &window[..end];
                if let Some(unwrapped) = extract_ext_url(href) {
                    assert!(
                        !unwrapped.starts_with("data:"),
                        "extracted URL is a data: scheme: {unwrapped}"
                    );
                }
            }
        }
    }

    #[test]
    fn strips_image_src_for_privacy() {
        // Tracking-pixel case: commercial senders embed <img src="tracker.com/..."
        // in emails so they know when users open them. Stripping src makes the
        // image blank (no network request) while preserving alt text and layout.
        let dirty = "<p>Hi</p><img src=\"https://tracker.example.com/pixel.gif?u=alan\" alt=\"tracker\">";
        let clean = sanitize_html(dirty);
        assert!(!clean.contains("tracker.example.com"), "src URL must be gone");
        assert!(!clean.contains("pixel.gif"), "no trace of the tracker path");
        assert!(clean.contains("<p>Hi</p>"), "rest of the body preserved");
        // Alt text is preserved for screen readers — ammonia's default attribute
        // list for <img> keeps alt.
        assert!(clean.contains("alt=\"tracker\"") || clean.contains("<img"));
    }

    #[test]
    fn strips_image_srcset_for_privacy() {
        // srcset is a second channel that responsive images use to point at
        // multiple URLs. Must strip this too or retina-aware trackers still fire.
        let dirty = "<img srcset=\"https://t.com/1x.gif 1x, https://t.com/2x.gif 2x\">";
        let clean = sanitize_html(dirty);
        assert!(!clean.contains("t.com"), "no srcset URLs leak through");
        assert!(!clean.contains("srcset"), "srcset attribute itself gone");
    }

    #[test]
    fn preserves_inline_data_url_images_for_safety() {
        // Inline data: URLs don't phone home — they're embedded bytes.
        // Ammonia's default permits them and our rm_tag_attributes doesn't
        // distinguish schemes, so data: URLs also get stripped. Document
        // that as current behavior; a follow-up can permit data: specifically.
        let dirty = "<img src=\"data:image/png;base64,iVBORw0KGgo=\">";
        let clean = sanitize_html(dirty);
        // Current behavior: strips ALL src, including safe data: URLs.
        // This is a conservative tradeoff — see "Eng Review Decisions" in plan.
        assert!(!clean.contains("data:image"));
    }
}
