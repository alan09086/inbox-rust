//! Body processing pipeline: Maildir write + search index update + SQLite update.
//!
//! Called for each email after RFC822 body is fetched from IMAP.
//! Reuses existing infrastructure:
//! - `MaildirStore::store_cur()` for atomic Maildir write
//! - `SearchIndex::update_email()` for tantivy re-index with body text
//! - `Store::mark_body_downloaded()` for SQLite flag update
//!
//! ```text
//!   raw RFC822 bytes
//!       │
//!       ├──► MaildirStore::store_cur(folder, bytes, flags)
//!       │        └──► StoredEmail { id, path }
//!       │
//!       ├──► extract_body_text(bytes)
//!       │        └──► String (plaintext for indexing)
//!       │
//!       ├──► SearchIndex::update_email(meta, body_text, None)
//!       │        └──► delete old doc + re-add with body
//!       │
//!       └──► Store::mark_body_downloaded(email_id, maildir_path)
//!                └──► UPDATE emails SET body_downloaded = 1
//! ```

use inboxly_store::Store;
use inboxly_store::maildir_store::{MaildirStore, StandardFolder};

use crate::error::ImapError;

/// Process a single fetched RFC822 body:
/// 1. Write raw .eml to Maildir
/// 2. Extract plaintext body for search indexing
/// 3. Mark `body_downloaded = true` in SQLite and update `maildir_path`
///
/// Search index update is handled separately by the caller (Phase 2
/// orchestrator or on-demand fetch) because the `SearchIndex` requires
/// an `EmailMeta` which the caller already has access to.
///
/// Returns the Maildir path where the email was stored.
pub fn process_body(
    email_id: &str,
    imap_folder: &str,
    raw_rfc822: &[u8],
    flags: &inboxly_core::EmailFlags,
    maildir: &MaildirStore,
    store: &Store,
) -> Result<String, ImapError> {
    // Step 1: Determine the standard folder from the IMAP folder name.
    let folder = StandardFolder::from_imap_name(imap_folder).unwrap_or(StandardFolder::Inbox);

    // Step 2: Write to Maildir (atomic: tmp -> cur with flags).
    let stored = maildir
        .store_cur(&folder, raw_rfc822, flags)
        .map_err(|e| ImapError::MaildirWrite(e.to_string()))?;

    let path_str = stored.path.to_string_lossy().into_owned();

    // Step 3: Mark body_downloaded = true in SQLite and update maildir_path.
    store
        .mark_body_downloaded(email_id, &path_str)
        .map_err(|e| ImapError::DatabaseError(e.to_string()))?;

    Ok(path_str)
}

/// Extract plaintext body from raw RFC822 bytes for search indexing.
///
/// Handles multipart MIME: prefers text/plain, falls back to stripping
/// HTML from text/html. Returns empty string if no text body found.
///
/// Uses the existing `mailparse` crate (already a dependency of `inboxly-store`).
pub fn extract_body_text(raw: &[u8]) -> String {
    let parsed = match mailparse::parse_mail(raw) {
        Ok(msg) => msg,
        Err(_) => return String::new(),
    };

    // Prefer text/plain body.
    if let Some(text) = find_body_text(&parsed) {
        return text;
    }

    // Fall back to HTML body, stripped of tags.
    if let Some(html) = find_body_html(&parsed) {
        return strip_html_tags(&html);
    }

    String::new()
}

/// Walk MIME tree depth-first looking for text/plain content.
fn find_body_text(parsed: &mailparse::ParsedMail<'_>) -> Option<String> {
    if parsed.ctype.mimetype == "text/plain" {
        return parsed.get_body().ok();
    }
    for sub in &parsed.subparts {
        if let Some(text) = find_body_text(sub) {
            return Some(text);
        }
    }
    None
}

/// Walk MIME tree depth-first looking for text/html content.
fn find_body_html(parsed: &mailparse::ParsedMail<'_>) -> Option<String> {
    if parsed.ctype.mimetype == "text/html" {
        return parsed.get_body().ok();
    }
    for sub in &parsed.subparts {
        if let Some(html) = find_body_html(sub) {
            return Some(html);
        }
    }
    None
}

/// Naive HTML tag stripping for search indexing.
///
/// Strips `<tags>`, decodes common HTML entities. Sufficient for
/// full-text search — not intended for display rendering.
pub fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_body_text_plain() {
        let raw = b"From: alice@example.com\r\n\
            To: bob@example.com\r\n\
            Subject: Hello\r\n\
            Content-Type: text/plain\r\n\
            \r\n\
            This is the body text.\r\n";
        let text = extract_body_text(raw);
        assert!(text.contains("This is the body text."));
    }

    #[test]
    fn test_extract_body_text_html_fallback() {
        let raw = b"From: alice@example.com\r\n\
            To: bob@example.com\r\n\
            Subject: Hello\r\n\
            Content-Type: text/html\r\n\
            \r\n\
            <html><body><p>Hello <b>World</b></p></body></html>\r\n";
        let text = extract_body_text(raw);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
        assert!(!text.contains("<p>"));
        assert!(!text.contains("<b>"));
    }

    #[test]
    fn test_extract_body_text_multipart() {
        let raw = b"From: alice@example.com\r\n\
            To: bob@example.com\r\n\
            Subject: Hello\r\n\
            Content-Type: multipart/alternative; boundary=\"boundary42\"\r\n\
            \r\n\
            --boundary42\r\n\
            Content-Type: text/plain\r\n\
            \r\n\
            Plain text version.\r\n\
            --boundary42\r\n\
            Content-Type: text/html\r\n\
            \r\n\
            <html><body>HTML version.</body></html>\r\n\
            --boundary42--\r\n";
        let text = extract_body_text(raw);
        // Should prefer text/plain.
        assert!(text.contains("Plain text version."));
    }

    #[test]
    fn test_extract_body_text_empty() {
        let text = extract_body_text(b"");
        assert!(text.is_empty());
    }

    #[test]
    fn test_extract_body_text_headers_only() {
        let raw = b"From: alice@example.com\r\n\
            Subject: No body\r\n\
            \r\n";
        let text = extract_body_text(raw);
        assert!(text.is_empty());
    }

    #[test]
    fn test_strip_html_tags() {
        assert_eq!(strip_html_tags("<p>Hello</p>"), "Hello");
        assert_eq!(
            strip_html_tags("<b>Bold</b> and <i>italic</i>"),
            "Bold and italic"
        );
        assert_eq!(strip_html_tags("No tags here"), "No tags here");
        assert_eq!(strip_html_tags(""), "");
        assert_eq!(strip_html_tags("<div class=\"x\">Content</div>"), "Content");
    }

    #[test]
    fn test_strip_html_entities() {
        assert_eq!(strip_html_tags("<p>A &amp; B &lt; C</p>"), "A & B < C");
    }
}
