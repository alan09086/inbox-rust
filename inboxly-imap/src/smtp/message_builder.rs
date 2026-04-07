//! RFC 5322 message construction from [`DraftEmail`].
//!
//! Two public builders with an invariant-preserving difference in Bcc handling:
//!
//! - [`build_rfc5322_for_smtp`] — for the SMTP wire transmission. Defaults to
//!   lettre's `drop_bcc: true` so Bcc recipients are in the SMTP envelope
//!   (RCPT TO) but NOT in the rendered headers. To/Cc recipients can't see
//!   who else got it.
//!
//! - [`build_rfc5322_for_sent_folder`] — for the IMAP `APPEND` to the Sent
//!   folder (and the Maildir `.Drafts/` write). Chains
//!   [`MessageBuilder::keep_bcc`] so the user's own Sent folder copy retains
//!   the Bcc list for audit. The user later viewing their Sent folder sees
//!   who they Bcc'd.
//!
//! Both builders produce a `multipart/alternative` MIME structure (text/plain
//! + text/html) wrapped in a `multipart/mixed` if attachments are present.
//!
//! The text/plain part is rendered via [`inboxly_core::markdown_to_plaintext`]
//! and the text/html part via [`inboxly_core::markdown_to_html`].

use std::str::FromStr;

use lettre::Address;
use lettre::message::header::ContentType;
use lettre::message::{
    Attachment as LettreAttachment, Mailbox, Message, MessageBuilder, MultiPart, SinglePart,
};

use crate::smtp::error::SmtpError;
use inboxly_core::{
    AttachmentDraft, AttachmentSource, Contact, DraftEmail, markdown_to_html, markdown_to_plaintext,
};

/// Build an RFC 5322 message for SMTP wire transmission.
///
/// Uses lettre's default `drop_bcc: true` — Bcc recipients end up in the
/// envelope's RCPT TO list but are stripped from the rendered headers
/// before [`Message::formatted`] runs.
///
/// # Errors
///
/// Returns [`SmtpError::MessageBuildError`] if any recipient address is
/// malformed, an attachment file cannot be read, or lettre rejects the
/// resulting message structure.
pub fn build_rfc5322_for_smtp(draft: &DraftEmail, from: &Mailbox) -> Result<Message, SmtpError> {
    build_inner(draft, from, false)
}

/// Build an RFC 5322 message for the Sent folder copy (IMAP APPEND + Maildir).
///
/// Chains [`MessageBuilder::keep_bcc`] — the `Bcc:` header is preserved in
/// the rendered output so the user's own Sent folder retains the Bcc list
/// for audit.
///
/// # Errors
///
/// See [`build_rfc5322_for_smtp`].
pub fn build_rfc5322_for_sent_folder(
    draft: &DraftEmail,
    from: &Mailbox,
) -> Result<Message, SmtpError> {
    build_inner(draft, from, true)
}

fn build_inner(
    draft: &DraftEmail,
    from: &Mailbox,
    keep_bcc_in_headers: bool,
) -> Result<Message, SmtpError> {
    let mut builder: MessageBuilder = Message::builder();

    if keep_bcc_in_headers {
        builder = builder.keep_bcc();
    }

    builder = builder
        .from(from.clone())
        .subject(&draft.subject)
        // The DraftEmail.message_id is already stored with the surrounding
        // angle brackets ("<uuid@inboxly.local>"). lettre's MessageId header
        // stores the string verbatim and does NOT add brackets, so we must
        // pass through unchanged.
        .message_id(Some(draft.message_id.clone()));

    for contact in &draft.to {
        builder = builder.to(contact_to_mailbox(contact)?);
    }
    for contact in &draft.cc {
        builder = builder.cc(contact_to_mailbox(contact)?);
    }
    for contact in &draft.bcc {
        builder = builder.bcc(contact_to_mailbox(contact)?);
    }

    if let Some(in_reply_to) = &draft.in_reply_to {
        builder = builder.in_reply_to(in_reply_to.clone());
    }
    if let Some(references) = &draft.references {
        builder = builder.references(references.clone());
    }

    // Build the multipart/alternative body.
    let plain_part = SinglePart::builder()
        .header(ContentType::TEXT_PLAIN)
        .body(markdown_to_plaintext(&draft.body_markdown));
    let html_part = SinglePart::builder()
        .header(ContentType::TEXT_HTML)
        .body(markdown_to_html(&draft.body_markdown));

    let body_multipart = MultiPart::alternative()
        .singlepart(plain_part)
        .singlepart(html_part);

    // If there are attachments, wrap in multipart/mixed.
    let final_multipart = if draft.attachments.is_empty() {
        body_multipart
    } else {
        let mut mixed = MultiPart::mixed().multipart(body_multipart);
        for att in &draft.attachments {
            mixed = mixed.singlepart(build_attachment_part(att)?);
        }
        mixed
    };

    builder
        .multipart(final_multipart)
        .map_err(|e| SmtpError::MessageBuildError {
            reason: e.to_string(),
        })
}

fn contact_to_mailbox(contact: &Contact) -> Result<Mailbox, SmtpError> {
    let address =
        Address::from_str(&contact.address).map_err(|e| SmtpError::MessageBuildError {
            reason: format!("invalid address '{}': {}", contact.address, e),
        })?;
    let name = if contact.name.is_empty() {
        None
    } else {
        Some(contact.name.clone())
    };
    Ok(Mailbox::new(name, address))
}

fn build_attachment_part(att: &AttachmentDraft) -> Result<SinglePart, SmtpError> {
    match &att.source {
        AttachmentSource::Disk(path) => {
            let bytes = std::fs::read(path).map_err(|e| SmtpError::MessageBuildError {
                reason: format!("read attachment {}: {}", att.filename, e),
            })?;
            let content_type =
                ContentType::parse(&att.mime_type).map_err(|e| SmtpError::MessageBuildError {
                    reason: format!("invalid mime_type {}: {}", att.mime_type, e),
                })?;
            Ok(LettreAttachment::new(att.filename.clone()).body(bytes, content_type))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use inboxly_core::{AccountId, ComposeMode};
    use uuid::Uuid;

    fn sample_draft() -> DraftEmail {
        let now = Utc::now();
        DraftEmail {
            id: "test-id".into(),
            account_id: AccountId(Uuid::new_v4()),
            message_id: "<test-draft@inboxly.local>".into(),
            subject: "Hello".into(),
            body_markdown: "This is **bold**".into(),
            to: vec![Contact::new("Alice", "alice@example.com")],
            cc: vec![],
            bcc: vec![],
            attachments: vec![],
            mode: ComposeMode::New,
            in_reply_to: None,
            references: None,
            maildir_path: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn from_mailbox() -> Mailbox {
        Mailbox::new(
            Some("Sender".into()),
            Address::from_str("sender@example.com").expect("test addr is valid"),
        )
    }

    #[test]
    fn builds_simple_text_only_message() {
        let draft = sample_draft();
        let msg = build_rfc5322_for_smtp(&draft, &from_mailbox()).expect("build ok");
        let formatted = String::from_utf8(msg.formatted()).expect("utf8");
        assert!(formatted.contains("Subject: Hello"));
        assert!(formatted.contains("alice@example.com"));
        assert!(formatted.contains("This is"));
        // Both plain and HTML parts should be present (multipart/alternative)
        assert!(formatted.contains("text/plain"));
        assert!(formatted.contains("text/html"));
        assert!(formatted.contains("<strong>bold</strong>"));
    }

    #[test]
    fn builds_with_single_attachment_wraps_in_multipart_mixed() {
        let mut draft = sample_draft();
        let tmpdir = tempfile::tempdir().expect("tmpdir");
        let att_path = tmpdir.path().join("hello.txt");
        std::fs::write(&att_path, b"hello world").expect("write attachment");
        draft.attachments.push(AttachmentDraft {
            filename: "hello.txt".into(),
            mime_type: "text/plain".into(),
            size_bytes: 11,
            source: AttachmentSource::Disk(att_path),
        });

        let msg = build_rfc5322_for_smtp(&draft, &from_mailbox()).expect("build ok");
        let formatted = String::from_utf8(msg.formatted()).expect("utf8");
        assert!(
            formatted.contains("multipart/mixed"),
            "expected multipart/mixed wrapper: {formatted}"
        );
        assert!(
            formatted.contains("multipart/alternative"),
            "expected inner alternative"
        );
        assert!(
            formatted.contains("hello.txt"),
            "expected attachment filename"
        );
    }

    #[test]
    fn builds_with_multiple_attachments() {
        let mut draft = sample_draft();
        let tmpdir = tempfile::tempdir().expect("tmpdir");
        let a = tmpdir.path().join("a.txt");
        let b = tmpdir.path().join("b.txt");
        std::fs::write(&a, b"alpha").expect("write a");
        std::fs::write(&b, b"beta").expect("write b");
        draft.attachments.push(AttachmentDraft {
            filename: "a.txt".into(),
            mime_type: "text/plain".into(),
            size_bytes: 5,
            source: AttachmentSource::Disk(a),
        });
        draft.attachments.push(AttachmentDraft {
            filename: "b.txt".into(),
            mime_type: "text/plain".into(),
            size_bytes: 4,
            source: AttachmentSource::Disk(b),
        });

        let msg = build_rfc5322_for_smtp(&draft, &from_mailbox()).expect("build ok");
        let formatted = String::from_utf8(msg.formatted()).expect("utf8");
        assert!(formatted.contains("a.txt"));
        assert!(formatted.contains("b.txt"));
    }

    #[test]
    fn builds_with_reply_headers() {
        let mut draft = sample_draft();
        draft.in_reply_to = Some("<parent@example.com>".into());
        draft.references = Some("<root@example.com> <parent@example.com>".into());
        let msg = build_rfc5322_for_smtp(&draft, &from_mailbox()).expect("build ok");
        let formatted = String::from_utf8(msg.formatted()).expect("utf8");
        assert!(formatted.contains("In-Reply-To:"));
        assert!(formatted.contains("parent@example.com"));
        assert!(formatted.contains("References:"));
    }

    #[test]
    fn smtp_builder_drops_bcc_from_headers() {
        // Gemini G1 / Issue 2.4: the SMTP wire form must NOT include Bcc in headers.
        let mut draft = sample_draft();
        draft.bcc.push(Contact::new("", "hidden@example.com"));
        let msg = build_rfc5322_for_smtp(&draft, &from_mailbox()).expect("build ok");
        let formatted = String::from_utf8(msg.formatted()).expect("utf8");
        assert!(
            !formatted.to_lowercase().contains("bcc:"),
            "Bcc header leaked in SMTP form: {formatted}"
        );
        assert!(
            !formatted.contains("hidden@example.com"),
            "Bcc recipient leaked: {formatted}"
        );
        // But the envelope DOES have the bcc (RCPT TO)
        let envelope = msg.envelope();
        let envelope_addrs: Vec<String> = envelope.to().iter().map(ToString::to_string).collect();
        assert!(
            envelope_addrs
                .iter()
                .any(|a| a.contains("hidden@example.com")),
            "envelope missing bcc: {envelope_addrs:?}"
        );
    }

    #[test]
    fn sent_folder_builder_keeps_bcc_in_headers() {
        // Gemini G1: the Sent folder copy MUST retain Bcc header so user sees their own Bcc list.
        let mut draft = sample_draft();
        draft.bcc.push(Contact::new("", "hidden@example.com"));
        let msg = build_rfc5322_for_sent_folder(&draft, &from_mailbox()).expect("build ok");
        let formatted = String::from_utf8(msg.formatted()).expect("utf8");
        assert!(
            formatted.to_lowercase().contains("bcc:"),
            "Bcc header missing in Sent form: {formatted}"
        );
        assert!(
            formatted.contains("hidden@example.com"),
            "Bcc recipient missing in Sent form: {formatted}"
        );
    }

    #[test]
    fn empty_body_builds_without_crash() {
        let mut draft = sample_draft();
        draft.body_markdown = String::new();
        // Should still produce a valid message (empty paragraphs are legal)
        let msg = build_rfc5322_for_smtp(&draft, &from_mailbox()).expect("build ok");
        let formatted = String::from_utf8(msg.formatted()).expect("utf8");
        assert!(formatted.contains("Subject: Hello"));
    }

    #[test]
    fn message_id_format_is_rfc5322_valid() {
        let draft = sample_draft();
        let msg = build_rfc5322_for_smtp(&draft, &from_mailbox()).expect("build ok");
        let formatted = String::from_utf8(msg.formatted()).expect("utf8");
        // The rendered Message-ID should have the angle brackets (lettre stores
        // the string verbatim — DraftEmail.message_id already includes them).
        assert!(
            formatted.contains("Message-ID: <"),
            "Message-ID header missing or unbracketed: {formatted}"
        );
        assert!(formatted.contains("test-draft@inboxly.local"));
    }

    #[test]
    fn date_header_is_rfc2822_format() {
        let draft = sample_draft();
        let msg = build_rfc5322_for_smtp(&draft, &from_mailbox()).expect("build ok");
        let formatted = String::from_utf8(msg.formatted()).expect("utf8");
        assert!(formatted.contains("Date:"));
        // Lettre handles the exact format; just confirm the header is there.
    }

    #[test]
    fn invalid_recipient_address_returns_build_error() {
        let mut draft = sample_draft();
        draft.to[0].address = "not-an-email-address".into();
        let result = build_rfc5322_for_smtp(&draft, &from_mailbox());
        assert!(result.is_err());
        match result.unwrap_err() {
            SmtpError::MessageBuildError { reason } => {
                assert!(reason.contains("not-an-email-address"));
            }
            other => panic!("wrong error variant: {other:?}"),
        }
    }

    #[test]
    fn smtp_and_sent_folder_share_subject_and_body() {
        // Parameterized shape: both builders should produce identical Subject and body content.
        let draft = sample_draft();
        let smtp = build_rfc5322_for_smtp(&draft, &from_mailbox()).expect("smtp build ok");
        let sent = build_rfc5322_for_sent_folder(&draft, &from_mailbox()).expect("sent build ok");
        let smtp_str = String::from_utf8(smtp.formatted()).expect("utf8");
        let sent_str = String::from_utf8(sent.formatted()).expect("utf8");
        assert!(smtp_str.contains("Subject: Hello"));
        assert!(sent_str.contains("Subject: Hello"));
        assert!(smtp_str.contains("<strong>bold</strong>"));
        assert!(sent_str.contains("<strong>bold</strong>"));
    }
}
