//! Loaded thread data structure for the thread detail view.
//!
//! `LoadedThread` is a UI-owned bundle of per-thread metadata plus
//! all messages with their full content (body, headers, attachments).
//!
//! Three constructor paths:
//! - `build_loaded_thread(thread_id, emails)` — the real path,
//!   takes raw `Vec<LoadedEmail>` from `ThreadReader::load_thread()`
//!   and converts to UI shape. Added in Phase 4.
//! - `empty_thread(thread_id)` — release-build placeholder when no
//!   real data is available. Returns a thread with zero messages
//!   and a "(no content available)" subject. Always compiled in.
//! - `demo_thread(thread_id)` — debug-build fixture with two fake
//!   messages. Used during M34 development for visual verification
//!   before real sync is wired. Gated behind `#[cfg(debug_assertions)]`
//!   per eng review Issue 1.3 — production binaries don't ship the
//!   fixture data.
//!
//! Callers should use `fallback_thread(thread_id)`, which picks
//! `demo_thread` in debug builds and `empty_thread` in release.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use inboxly_core::AttachmentMeta;
use inboxly_store::thread_reader::LoadedEmail;

/// All data needed to render the thread detail view.
///
/// Messages are wrapped in `Arc<LoadedMessage>` (eng review Issue 2.8)
/// so per-render clones in the `for` loop in `ThreadDetailView` are
/// refcount bumps, not deep clones of the message body bytes. The
/// outer `LoadedThread` is also Arc-wrapped at the signal boundary
/// (Issue 1.4) — together that's two layers of Arc, each addressing
/// a different cost: the outer Arc avoids cloning the whole thread
/// on every Inboxly write, the inner Arc avoids cloning each message
/// on every ThreadDetailView re-render.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedThread {
    pub thread_id: String,
    pub subject: String,
    pub messages: Vec<Arc<LoadedMessage>>,
    /// Set when the load failed. ThreadDetailView renders this in
    /// a banner above the message list so the user sees what went
    /// wrong instead of being silently shown demo/empty content.
    /// Eng review Issue 2.1.
    pub error_message: Option<String>,
}

/// One message inside a loaded thread.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedMessage {
    pub email_id: String,
    pub from_name: String,
    pub from_address: String,
    /// `None` when the timestamp couldn't be parsed (corrupt or
    /// out-of-range value in the store). Eng review Issue 2.7:
    /// the previous design fell back to `Utc::now()` for bad
    /// timestamps, which made corrupt data look like a fresh
    /// email. Option<...> forces the renderer to handle "unknown
    /// time" as a deliberate display state.
    pub date: Option<DateTime<Utc>>,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub attachments: Vec<AttachmentMeta>,
}

/// Minimal placeholder thread shown when no real data is available
/// (e.g., release builds before sync is wired). Always compiled in.
/// Carries no error message — the user clicked a thread but the
/// system genuinely had nothing to show, which is different from
/// a load failure.
pub fn empty_thread(thread_id: &str) -> LoadedThread {
    LoadedThread {
        thread_id: thread_id.to_string(),
        subject: "(no content available)".to_string(),
        messages: Vec::new(),
        error_message: None,
    }
}

/// Construct a thread that represents a load failure. The
/// ThreadDetailView renders the `error_message` in a red banner
/// above the (empty) message list. Eng review Issue 2.1 — the
/// alternative was silent fallback to demo/empty content, which
/// hides bugs from users and developers.
pub fn error_thread(thread_id: &str, message: impl Into<String>) -> LoadedThread {
    LoadedThread {
        thread_id: thread_id.to_string(),
        subject: "(failed to load)".to_string(),
        messages: Vec::new(),
        error_message: Some(message.into()),
    }
}

/// Sentinel thread shown while a load is in progress. Eng review
/// Issue 4.1 — the App-level use_effect bridge sets this immediately
/// after the user clicks a thread row, then runs the actual load
/// in a Dioxus `spawn`'d task. The sentinel makes the UI show a
/// "loading" state instead of an awkward gap or stale content.
///
/// `subject == "(loading…)"` is the visible signal; the renderer
/// can also detect a loading state by checking three conditions
/// together: `messages.is_empty()`, `error_message.is_none()`, and
/// that specific subject. Future work could promote this to a typed
/// `LoadState` enum at the signal layer if more states are needed.
pub fn loading_thread(thread_id: &str) -> LoadedThread {
    LoadedThread {
        thread_id: thread_id.to_string(),
        subject: "(loading\u{2026})".to_string(),
        messages: Vec::new(),
        error_message: None,
    }
}

/// Build a fake thread with two messages for visual verification
/// during M34 development. Debug builds only — release binaries
/// do not ship this fixture data (eng review Issue 1.3).
///
/// The first message includes an HTML body with an external link
/// so the Phase 9 link-click interceptor can be exercised by hand.
#[cfg(debug_assertions)]
pub fn demo_thread(thread_id: &str) -> LoadedThread {
    LoadedThread {
        thread_id: thread_id.to_string(),
        subject: "Welcome to Inboxly".to_string(),
        error_message: None,
        messages: vec![
            Arc::new(LoadedMessage {
                email_id: format!("{thread_id}-1"),
                from_name: "Alan Gaudet".to_string(),
                from_address: "alan@example.com".to_string(),
                date: Some(chrono::Utc::now() - chrono::Duration::hours(2)),
                body_html: Some(
                    "<p>Hi there,</p>\
                     <p>This is a <strong>demo</strong> message rendered \
                     from the M34 thread detail view. The body is sanitised HTML.</p>\
                     <p>More info: <a href=\"https://example.com\">example.com</a> \
                     (clicking should open in your default browser, not navigate the app)</p>\
                     <p>Cheers,<br>Alan</p>"
                        .to_string(),
                ),
                body_text: None,
                attachments: vec![AttachmentMeta {
                    filename: "report.pdf".to_string(),
                    mime_type: "application/pdf".to_string(),
                    size_bytes: 124_532,
                }],
            }),
            Arc::new(LoadedMessage {
                email_id: format!("{thread_id}-2"),
                from_name: "Test Sender".to_string(),
                from_address: "test@example.com".to_string(),
                date: Some(chrono::Utc::now() - chrono::Duration::minutes(15)),
                body_html: None,
                body_text: Some(
                    "Reply with a plain-text body.\n\nNo HTML, no formatting.\n\nLine three."
                        .to_string(),
                ),
                attachments: vec![],
            }),
        ],
    }
}

/// Pick the right placeholder thread for the current build mode.
/// Debug builds get the demo fixture; release builds get the empty
/// placeholder. Single call site for the cfg switch so callers don't
/// have to repeat the gate. **This is for the no-data case, NOT
/// for load failures** — use `error_thread()` for those.
pub fn fallback_thread(thread_id: &str) -> LoadedThread {
    #[cfg(debug_assertions)]
    {
        demo_thread(thread_id)
    }
    #[cfg(not(debug_assertions))]
    {
        empty_thread(thread_id)
    }
}

/// Convert raw `LoadedEmail` rows from the storage facade into the
/// UI's `LoadedThread` shape. Picks display names (sender name OR
/// sender address as fallback), converts UNIX timestamps to
/// `DateTime<Utc>`, and passes through attachment metadata.
///
/// `LoadedEmail.content` is already a `SlimEmailContent` (eng review
/// Issue 2.6) carrying only body text/HTML and attachment metadata —
/// headers and attachment byte content are NEVER loaded for the
/// thread detail view. This converter just maps the slim fields 1:1.
///
/// Returns `Err` if `emails` is empty (the caller should fall back
/// to `fallback_thread()`).
pub fn build_loaded_thread(
    thread_id: &str,
    emails: Vec<LoadedEmail>,
) -> Result<LoadedThread, String> {
    if emails.is_empty() {
        return Err(format!("no emails in thread {thread_id}"));
    }
    let subject = emails[0].row.subject.clone();
    let messages = emails
        .into_iter()
        .map(|le| {
            let LoadedEmail { row, content } = le;
            let (body_text, body_html, attachments) = match content {
                Some(c) => (c.body_text, c.body_html, c.attachments),
                None => (
                    Some("(body not yet downloaded)".to_string()),
                    None,
                    Vec::new(),
                ),
            };
            // Issue 2.8: wrap each LoadedMessage in Arc so per-render
            // clones in ThreadDetailView's `for` loop are refcount
            // bumps, not deep clones of the body bytes.
            Arc::new(LoadedMessage {
                email_id: row.id,
                from_name: row.from_name.unwrap_or_else(|| row.from_address.clone()),
                from_address: row.from_address,
                // Issue 2.7: from_timestamp returns None for invalid
                // input. Pass it through as-is — the renderer will
                // show "(unknown time)" for None instead of the
                // misleading "right now" fallback.
                date: chrono::DateTime::<chrono::Utc>::from_timestamp(row.date, 0),
                body_text,
                body_html,
                attachments,
            })
        })
        .collect();
    Ok(LoadedThread {
        thread_id: thread_id.to_string(),
        subject,
        messages,
        error_message: None, // success path
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // empty_thread tests (always run)

    #[test]
    fn empty_thread_has_no_messages_and_no_error() {
        let thread = empty_thread("t-empty");
        assert_eq!(thread.thread_id, "t-empty");
        assert!(thread.messages.is_empty());
        assert_eq!(thread.subject, "(no content available)");
        assert!(thread.error_message.is_none(), "empty != error");
    }

    // error_thread tests (eng review Issue 2.1)

    #[test]
    fn error_thread_carries_error_message() {
        let thread = error_thread("t-bad", "DB locked");
        assert_eq!(thread.thread_id, "t-bad");
        assert!(thread.messages.is_empty());
        assert_eq!(thread.subject, "(failed to load)");
        assert_eq!(thread.error_message.as_deref(), Some("DB locked"));
    }

    #[test]
    fn error_thread_distinguishes_from_empty_thread() {
        // Both have zero messages, but error_thread has Some(msg) and
        // empty_thread has None. ThreadDetailView's banner conditional
        // depends on this distinction.
        let empty = empty_thread("t1");
        let err = error_thread("t1", "boom");
        assert!(empty.error_message.is_none());
        assert!(err.error_message.is_some());
    }

    // loading_thread tests (eng review Issue 4.1)

    #[test]
    fn loading_thread_has_no_messages_no_error() {
        let thread = loading_thread("t-loading");
        assert_eq!(thread.thread_id, "t-loading");
        assert!(thread.messages.is_empty());
        assert!(thread.error_message.is_none());
        // Subject contains the loading sentinel.
        assert!(thread.subject.contains("loading"));
    }

    // demo_thread tests (debug builds only)

    #[cfg(debug_assertions)]
    #[test]
    fn demo_thread_has_two_messages() {
        let thread = demo_thread("demo-thread-1");
        assert_eq!(thread.thread_id, "demo-thread-1");
        assert_eq!(thread.messages.len(), 2);
        assert_eq!(thread.subject, "Welcome to Inboxly");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn demo_thread_first_message_has_html_body() {
        let thread = demo_thread("demo");
        let first = &thread.messages[0];
        assert!(first.body_html.is_some());
        assert!(first.body_html.as_ref().unwrap().contains("<p>"));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn demo_thread_first_message_has_a_link() {
        // Phase 9 manual verification depends on this — the demo body
        // must contain at least one <a href> so the link-click path
        // can be exercised by clicking through the demo.
        let thread = demo_thread("demo");
        let first_html = thread.messages[0].body_html.as_ref().unwrap();
        assert!(first_html.contains("<a href"));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn demo_thread_second_message_is_plain_text() {
        let thread = demo_thread("demo");
        let second = &thread.messages[1];
        assert!(second.body_html.is_none());
        assert!(second.body_text.is_some());
    }

    // fallback_thread always returns SOMETHING

    #[test]
    fn fallback_thread_returns_a_loaded_thread() {
        let thread = fallback_thread("any-id");
        assert_eq!(thread.thread_id, "any-id");
        // In debug builds this is the demo (2 messages); in release
        // it's the empty placeholder (0 messages). Either is valid —
        // we just want to confirm the function compiles in both modes
        // and returns the right shape.
        #[cfg(debug_assertions)]
        assert_eq!(thread.messages.len(), 2);
        #[cfg(not(debug_assertions))]
        assert!(thread.messages.is_empty());
    }

    // build_loaded_thread tests (M34 phase 4 / eng review Issue 3.2)

    #[test]
    fn build_loaded_thread_empty_returns_err() {
        let result = build_loaded_thread("t1", vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn build_loaded_thread_uses_address_when_name_missing() {
        use inboxly_store::EmailRow;
        use inboxly_store::thread_reader::LoadedEmail;

        let row = EmailRow {
            id: "e1".into(),
            account_id: "a1".into(),
            thread_id: "t1".into(),
            from_name: None,
            from_address: "alice@example.com".into(),
            to_json: "[]".into(),
            cc_json: "[]".into(),
            subject: "Hello".into(),
            snippet: "Hi there".into(),
            date: 1_700_000_000,
            maildir_path: String::new(),
            flags: 0,
            size_bytes: 100,
            imap_uid: 1,
            imap_folder: "INBOX".into(),
            has_attachments: false,
            body_downloaded: false,
            message_id_header: None,
            in_reply_to: None,
            references_json: None,
        };
        let loaded = LoadedEmail { row, content: None };
        let result = build_loaded_thread("t1", vec![loaded]).expect("non-empty");
        assert_eq!(result.subject, "Hello");
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0].from_name, "alice@example.com");
        assert_eq!(result.messages[0].from_address, "alice@example.com");
        assert!(result.messages[0].body_text.is_some());
        assert!(result.messages[0].body_html.is_none());
        assert!(result.messages[0].date.is_some());
    }

    #[test]
    fn build_loaded_thread_with_content_passes_through_body_and_attachments() {
        use inboxly_core::{AttachmentMeta, EmailId, SlimEmailContent};
        use inboxly_store::EmailRow;
        use inboxly_store::thread_reader::LoadedEmail;

        let row = EmailRow {
            id: "e1".into(),
            account_id: "a1".into(),
            thread_id: "t1".into(),
            from_name: Some("Alice".into()),
            from_address: "alice@example.com".into(),
            to_json: "[]".into(),
            cc_json: "[]".into(),
            subject: "Re: Hello".into(),
            snippet: "snip".into(),
            date: 1_700_000_000,
            maildir_path: "/tmp/fake.eml".into(),
            flags: 0,
            size_bytes: 200,
            imap_uid: 1,
            imap_folder: "INBOX".into(),
            has_attachments: true,
            body_downloaded: true,
            message_id_header: None,
            in_reply_to: None,
            references_json: None,
        };
        let content = SlimEmailContent {
            id: EmailId("<e1@example.com>".into()),
            body_text: Some("plain text body".into()),
            body_html: Some("<p>html <strong>body</strong></p>".into()),
            attachments: vec![AttachmentMeta {
                filename: "invoice.pdf".into(),
                mime_type: "application/pdf".into(),
                size_bytes: 4096,
            }],
        };
        let loaded = LoadedEmail {
            row,
            content: Some(content),
        };
        let result = build_loaded_thread("t1", vec![loaded]).expect("non-empty");
        assert_eq!(result.thread_id, "t1");
        assert_eq!(result.subject, "Re: Hello");
        assert!(result.error_message.is_none(), "success path has no error");
        assert_eq!(result.messages.len(), 1);
        let msg = &result.messages[0];
        assert_eq!(msg.from_name, "Alice");
        assert_eq!(msg.from_address, "alice@example.com");
        assert_eq!(msg.body_text.as_deref(), Some("plain text body"));
        assert_eq!(
            msg.body_html.as_deref(),
            Some("<p>html <strong>body</strong></p>")
        );
        assert_eq!(msg.attachments.len(), 1);
        assert_eq!(msg.attachments[0].filename, "invoice.pdf");
        assert_eq!(msg.attachments[0].mime_type, "application/pdf");
        assert_eq!(msg.attachments[0].size_bytes, 4096);
        assert!(msg.date.is_some(), "valid Unix timestamp must yield Some");
    }

    #[test]
    fn build_loaded_thread_handles_invalid_timestamp() {
        use inboxly_store::EmailRow;
        use inboxly_store::thread_reader::LoadedEmail;

        let row = EmailRow {
            id: "e-bad-date".into(),
            account_id: "a1".into(),
            thread_id: "t1".into(),
            from_name: Some("Alice".into()),
            from_address: "alice@example.com".into(),
            to_json: "[]".into(),
            cc_json: "[]".into(),
            subject: "Bad date".into(),
            snippet: "".into(),
            date: i64::MIN,
            maildir_path: String::new(),
            flags: 0,
            size_bytes: 0,
            imap_uid: 1,
            imap_folder: "INBOX".into(),
            has_attachments: false,
            body_downloaded: false,
            message_id_header: None,
            in_reply_to: None,
            references_json: None,
        };
        let loaded = LoadedEmail { row, content: None };
        let result = build_loaded_thread("t1", vec![loaded]).expect("non-empty");
        assert!(
            result.messages[0].date.is_none(),
            "i64::MIN must yield None, not a fallback timestamp"
        );
    }
}
