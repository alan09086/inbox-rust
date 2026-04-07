//! PII-stripping helpers for SMTP error logging.
//!
//! When SMTP calls fail, we log enough to debug (account email, subject
//! prefix, recipient domain) but not enough to leak private information.
//! Subjects are truncated. Recipient addresses are hashed (SHA-256, first
//! 8 hex chars) so we can correlate log lines without exposing the address.
//! The body is NEVER logged.

use sha2::{Digest, Sha256};

use crate::smtp::error::SmtpError;
use inboxly_core::DraftEmail;

/// Maximum subject length (in characters) included in log output before truncation.
const SUBJECT_TRUNCATE: usize = 20;

/// Build a redacted, single-line log representation of an SMTP error and
/// the draft that failed.
///
/// The output includes:
/// - The error variant + its own message (error variant fields are assumed
///   not to contain PII; `AuthFailed { reason }` contains auth info only)
/// - The account id (UUID, already known to the operator)
/// - The number of recipients by field (to/cc/bcc count, not addresses)
/// - A SHA-256 hash (first 8 hex chars) of the first `to` recipient for
///   correlation — same recipient across log lines will hash identically
/// - The subject truncated to 20 chars + ellipsis
/// - **Never** the body
#[must_use]
pub fn redact_for_log(error: &SmtpError, draft: &DraftEmail) -> String {
    let subject_trunc = truncate_subject(&draft.subject);
    let first_to_hash = draft
        .to
        .first()
        .map_or_else(|| "none".to_string(), |c| hash_recipient(&c.address));

    format!(
        "SMTP error [{}] for draft from={} to_count={} to_hash={} cc_count={} bcc_count={} subject=\"{}\"",
        error,
        draft.account_id,
        draft.to.len(),
        first_to_hash,
        draft.cc.len(),
        draft.bcc.len(),
        subject_trunc,
    )
}

/// Hash a recipient email address to a short hex correlation key.
///
/// Returns the first 4 bytes (8 hex chars) of the SHA-256 hash.
fn hash_recipient(address: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(address.as_bytes());
    let result = hasher.finalize();
    let mut out = String::with_capacity(8);
    for byte in &result[..4] {
        use std::fmt::Write as _;
        // SHA-256 produces fixed-size output; writing to a String never fails.
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Truncate a subject to at most `SUBJECT_TRUNCATE` characters, appending
/// a single Unicode ellipsis if truncation occurred.
fn truncate_subject(subject: &str) -> String {
    if subject.chars().count() <= SUBJECT_TRUNCATE {
        subject.to_string()
    } else {
        let truncated: String = subject.chars().take(SUBJECT_TRUNCATE).collect();
        format!("{truncated}\u{2026}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use inboxly_core::{AccountId, ComposeMode, Contact};
    use uuid::Uuid;

    fn sample_draft_with_to(to_address: &str, subject: &str) -> DraftEmail {
        let now = Utc::now();
        DraftEmail {
            id: "test-id".into(),
            account_id: AccountId(Uuid::new_v4()),
            message_id: "<test@inboxly.local>".into(),
            subject: subject.into(),
            body_markdown: "secret body that MUST NOT be logged".into(),
            to: vec![Contact::new("", to_address)],
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

    #[test]
    fn hashes_recipient_to_8_hex_chars() {
        let hash = hash_recipient("alice@example.com");
        assert_eq!(hash.len(), 8, "hash should be 8 hex chars, got: {hash}");
        // Deterministic — same input always hashes the same
        assert_eq!(hash, hash_recipient("alice@example.com"));
        // Different addresses hash differently
        assert_ne!(hash, hash_recipient("bob@example.com"));
    }

    #[test]
    fn truncates_subject_with_ellipsis() {
        let long = "This is a very long email subject that exceeds twenty chars";
        let truncated = truncate_subject(long);
        // 20 chars + ellipsis
        assert!(truncated.ends_with('\u{2026}'));
        assert_eq!(truncated.chars().count(), 21);
    }

    #[test]
    fn short_subject_untouched() {
        assert_eq!(truncate_subject("Short"), "Short");
    }

    #[test]
    fn log_never_contains_body_content() {
        let draft = sample_draft_with_to("target@example.com", "Quarterly financials");
        let err = SmtpError::AuthFailed {
            reason: "token expired".into(),
        };
        let log = redact_for_log(&err, &draft);
        assert!(!log.contains("secret body"), "body leaked: {log}");
        assert!(!log.contains("MUST NOT"), "body leaked: {log}");
    }

    #[test]
    fn log_contains_hashed_first_recipient_not_plain() {
        let draft = sample_draft_with_to("alice@example.com", "Re: meeting");
        let err = SmtpError::NetworkError {
            reason: "timeout".into(),
        };
        let log = redact_for_log(&err, &draft);
        assert!(
            !log.contains("alice@example.com"),
            "plain recipient leaked: {log}"
        );
        let expected_hash = hash_recipient("alice@example.com");
        assert!(log.contains(&expected_hash), "hash missing: {log}");
    }
}
