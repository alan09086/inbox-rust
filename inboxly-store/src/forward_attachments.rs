//! Forward attachment passthrough — copy an original email's attachments
//! into a new draft's per-draft directory when the user clicks Forward.
//!
//! Phase 9 of M36. The compose state machine creates a fresh `draft_id`
//! UUID before the prefill bridge runs. For Forward mode, the bridge
//! calls [`extract_forward_attachments`] with the original `.eml` path
//! and that `draft_id`; we walk the MIME tree, write each attachment's
//! decoded bytes into the per-draft directory, and return the
//! [`AttachmentDraft`] metadata for the caller to splice into
//! `ComposeState::attachments`.
//!
//! # Scope note (M36)
//!
//! The original plan called for byte-offset streaming from the source
//! `.eml` to keep peak memory bounded for large attachments. mailparse
//! 0.15 does NOT expose per-part byte offsets, and implementing offset
//! tracking from scratch would require re-parsing the RFC 5322 boundary
//! structure by hand (~200 LOC for a speculative optimization).
//!
//! Phase 9 ships the simpler decode-to-memory approach: for each
//! attachment-classified part we call [`mailparse::ParsedMail::get_body_raw`]
//! to get the decoded bytes, write them to disk, and drop the `Vec`.
//! Peak memory per extraction is ONE attachment's decoded size — for a
//! ~20 MB PDF that's negligible on any modern machine, but pathological
//! emails (200 MB+ encoded video) could spike RSS briefly.
//!
//! `TODO(post-M36):` revisit byte-offset streaming if profiling reveals
//! memory pressure during forwards of very large attachment emails. The
//! likely shape: parse only the structural envelope, record `(start,
//! end, encoding)` per part, then stream-decode straight to disk with a
//! 64 KiB rolling buffer.
//!
//! # Fail-soft policy
//!
//! - Source file unreadable -> propagate `StoreError::Io`. The bridge
//!   logs and proceeds with no attachments (the user still gets the
//!   prefilled body and headers).
//! - `parse_mail` failure -> log a warning, return `Ok(Vec::new())`.
//!   A garbled `.eml` should not block the Forward.
//! - Per-attachment `get_body_raw` or write failure -> log a warning,
//!   skip that attachment, continue with the rest. Partial success is
//!   better than total failure.

use std::path::{Path, PathBuf};

use mailparse::{DispositionType, ParsedMail, parse_mail};
use uuid::Uuid;

use inboxly_core::{AttachmentDraft, AttachmentSource};

use crate::draft_attachments::{ensure_draft_dir, make_draft_filename};
use crate::error::{Result, StoreError};

/// Extract attachments from `eml_path` into the per-draft directory for
/// `draft_id`, returning [`AttachmentDraft`] metadata for each
/// successfully-copied attachment.
///
/// Mirrors the MIME walk in `maildir_store::collect_attachment_meta` —
/// every part whose `Content-Disposition` is `attachment` (or whose MIME
/// type is non-`text/`, non-`multipart/`, non-`inline`) is treated as
/// an attachment. Recursively descends `subparts`.
///
/// The on-disk filename is generated via [`make_draft_filename`] with a
/// fresh UUID per attachment so two parts named `image.png` from
/// different sub-trees do not collide. The `AttachmentDraft.filename`
/// preserves the original (chip-display) name.
///
/// # Errors
///
/// Returns [`StoreError::Io`] if the source file cannot be read or the
/// per-draft directory cannot be created. Per-attachment failures are
/// logged and skipped — they do NOT propagate.
pub fn extract_forward_attachments(
    eml_path: &Path,
    draft_id: &str,
) -> Result<Vec<AttachmentDraft>> {
    let draft_dir = ensure_draft_dir(draft_id)?;
    extract_forward_attachments_into(eml_path, &draft_dir)
}

/// Variant of [`extract_forward_attachments`] that writes into a
/// caller-provided directory instead of resolving the per-draft
/// directory through `dirs::data_dir()`.
///
/// Exists so unit tests can isolate the on-disk side effects to a
/// `tempfile::TempDir`. Production code uses [`extract_forward_attachments`].
///
/// # Errors
///
/// Returns [`StoreError::Io`] if the source file cannot be read.
/// Per-attachment failures are logged and skipped.
pub fn extract_forward_attachments_into(
    eml_path: &Path,
    draft_dir: &Path,
) -> Result<Vec<AttachmentDraft>> {
    let raw = std::fs::read(eml_path).map_err(|e| {
        StoreError::Io(std::io::Error::other(format!(
            "forward_extract: failed to read {}: {e}",
            eml_path.display()
        )))
    })?;

    let parsed = match parse_mail(&raw) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                path = %eml_path.display(),
                error = %e,
                "forward_extract: unparseable .eml, returning empty attachment list"
            );
            return Ok(Vec::new());
        }
    };

    let mut out = Vec::new();
    extract_recursive(&parsed, draft_dir, &mut out);
    Ok(out)
}

/// Walk the parsed MIME tree depth-first and write any attachment
/// parts into `draft_dir`, pushing successful extractions onto `out`.
fn extract_recursive(parsed: &ParsedMail, draft_dir: &Path, out: &mut Vec<AttachmentDraft>) {
    let disposition = parsed.get_content_disposition();

    // Mirror the heuristic from `maildir_store::collect_attachment_meta`
    // (line 540): explicit `attachment` disposition wins; otherwise
    // anything that isn't `inline`, `text/*`, or `multipart/*` is also
    // treated as an attachment (handles parts with no Content-Disposition
    // header, which is common for inline images sent by older clients).
    let is_attachment = disposition.disposition == DispositionType::Attachment
        || (disposition.disposition != DispositionType::Inline
            && !parsed.ctype.mimetype.starts_with("text/")
            && !parsed.ctype.mimetype.starts_with("multipart/"));

    if is_attachment {
        let original_filename = disposition
            .params
            .get("filename")
            .or_else(|| parsed.ctype.params.get("name"))
            .cloned()
            .unwrap_or_else(|| "unnamed".to_string());

        match parsed.get_body_raw() {
            Ok(bytes) => {
                let on_disk_name = make_draft_filename(&original_filename, Uuid::new_v4());
                let path: PathBuf = draft_dir.join(&on_disk_name);
                match std::fs::write(&path, &bytes) {
                    Ok(()) => {
                        out.push(AttachmentDraft {
                            filename: original_filename,
                            mime_type: parsed.ctype.mimetype.clone(),
                            size_bytes: bytes.len() as u64,
                            source: AttachmentSource::Disk(path),
                        });
                    }
                    Err(e) => tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "forward_extract: failed to write attachment, skipping"
                    ),
                }
            }
            Err(e) => tracing::warn!(
                filename = %original_filename,
                error = %e,
                "forward_extract: get_body_raw failed, skipping attachment"
            ),
        }
    }

    for sub in &parsed.subparts {
        extract_recursive(sub, draft_dir, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Hand-rolled multipart fixture with a single base64-encoded PDF
    /// attachment. Mirrors `inboxly-store/tests/fixtures/with_attachment.eml`
    /// but inlined here so the test does not depend on the on-disk
    /// fixture file path.
    fn fixture_single_pdf() -> &'static [u8] {
        b"From: alice@example.com\r\n\
          To: bob@example.com\r\n\
          Subject: One PDF\r\n\
          MIME-Version: 1.0\r\n\
          Content-Type: multipart/mixed; boundary=\"bnd1\"\r\n\
          \r\n\
          --bnd1\r\n\
          Content-Type: text/plain; charset=utf-8\r\n\
          \r\n\
          See attached PDF.\r\n\
          --bnd1\r\n\
          Content-Type: application/pdf; name=\"report.pdf\"\r\n\
          Content-Disposition: attachment; filename=\"report.pdf\"\r\n\
          Content-Transfer-Encoding: base64\r\n\
          \r\n\
          JVBERi0xLjQKMSAwIG9iago8PAovVHlwZSAvQ2F0YWxvZwovUGFnZXMgMiAwIFIKPj4KZW5k\r\n\
          b2JqCg==\r\n\
          --bnd1--\r\n"
    }

    /// Two attachment fixture: PDF + PNG (the PNG payload is just a few
    /// bytes, not a real PNG — mailparse does not validate the body
    /// against the declared MIME type).
    fn fixture_pdf_and_png() -> &'static [u8] {
        b"From: alice@example.com\r\n\
          To: bob@example.com\r\n\
          Subject: PDF and PNG\r\n\
          MIME-Version: 1.0\r\n\
          Content-Type: multipart/mixed; boundary=\"bnd2\"\r\n\
          \r\n\
          --bnd2\r\n\
          Content-Type: text/plain; charset=utf-8\r\n\
          \r\n\
          Two attachments enclosed.\r\n\
          --bnd2\r\n\
          Content-Type: application/pdf; name=\"invoice.pdf\"\r\n\
          Content-Disposition: attachment; filename=\"invoice.pdf\"\r\n\
          Content-Transfer-Encoding: base64\r\n\
          \r\n\
          JVBERi0xLjQK\r\n\
          --bnd2\r\n\
          Content-Type: image/png; name=\"logo.png\"\r\n\
          Content-Disposition: attachment; filename=\"logo.png\"\r\n\
          Content-Transfer-Encoding: base64\r\n\
          \r\n\
          iVBORw0KGgo=\r\n\
          --bnd2--\r\n"
    }

    /// Plain text + HTML alternative, no attachments.
    fn fixture_no_attachments() -> &'static [u8] {
        b"From: alice@example.com\r\n\
          To: bob@example.com\r\n\
          Subject: Just text\r\n\
          MIME-Version: 1.0\r\n\
          Content-Type: multipart/alternative; boundary=\"alt1\"\r\n\
          \r\n\
          --alt1\r\n\
          Content-Type: text/plain; charset=utf-8\r\n\
          \r\n\
          Hello plain.\r\n\
          --alt1\r\n\
          Content-Type: text/html; charset=utf-8\r\n\
          \r\n\
          <p>Hello html.</p>\r\n\
          --alt1--\r\n"
    }

    /// Write `bytes` to a fresh `.eml` file under `dir` and return the
    /// path. Helper for the test fixtures.
    fn write_eml(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, bytes).expect("write fixture");
        p
    }

    /// Single attachment: extractor returns one entry whose filename,
    /// mime type, decoded size, and on-disk path all match expectations.
    #[test]
    fn extract_single_pdf_attachment() {
        let tmp = TempDir::new().expect("tempdir");
        let eml = write_eml(tmp.path(), "src.eml", fixture_single_pdf());
        let draft_dir = tmp.path().join("draft1");
        fs::create_dir_all(&draft_dir).expect("mkdir draft1");

        let out = extract_forward_attachments_into(&eml, &draft_dir).expect("extract");

        assert_eq!(out.len(), 1, "expected exactly one attachment");
        let att = &out[0];
        assert_eq!(att.filename, "report.pdf");
        assert_eq!(att.mime_type, "application/pdf");
        assert!(att.size_bytes > 0, "decoded size should be > 0");

        let AttachmentSource::Disk(ref path) = att.source;
        assert!(path.exists(), "on-disk attachment file should exist");
        assert_eq!(
            path.parent(),
            Some(draft_dir.as_path()),
            "attachment must live inside the draft dir"
        );
        let on_disk_size = fs::metadata(path).expect("stat").len();
        assert_eq!(
            on_disk_size, att.size_bytes,
            "size_bytes should match on-disk size"
        );
        // The decoded PDF starts with the standard %PDF magic.
        let bytes = fs::read(path).expect("read attachment");
        assert!(
            bytes.starts_with(b"%PDF-"),
            "decoded body should be the original PDF bytes"
        );
    }

    /// Two attachments: both are written, both metadata entries match,
    /// and the two on-disk paths are distinct.
    #[test]
    fn extract_multiple_attachments() {
        let tmp = TempDir::new().expect("tempdir");
        let eml = write_eml(tmp.path(), "src.eml", fixture_pdf_and_png());
        let draft_dir = tmp.path().join("draft2");
        fs::create_dir_all(&draft_dir).expect("mkdir draft2");

        let out = extract_forward_attachments_into(&eml, &draft_dir).expect("extract");

        assert_eq!(out.len(), 2, "expected two attachments");
        let names: Vec<&str> = out.iter().map(|a| a.filename.as_str()).collect();
        assert!(names.contains(&"invoice.pdf"));
        assert!(names.contains(&"logo.png"));

        let mimes: Vec<&str> = out.iter().map(|a| a.mime_type.as_str()).collect();
        assert!(mimes.contains(&"application/pdf"));
        assert!(mimes.contains(&"image/png"));

        // Distinct on-disk paths.
        let AttachmentSource::Disk(ref p0) = out[0].source;
        let AttachmentSource::Disk(ref p1) = out[1].source;
        assert_ne!(p0, p1, "each attachment must get its own on-disk file");
        assert!(p0.exists());
        assert!(p1.exists());
    }

    /// No attachments: extractor returns an empty Vec and writes
    /// nothing to the draft dir.
    #[test]
    fn extract_no_attachments_returns_empty() {
        let tmp = TempDir::new().expect("tempdir");
        let eml = write_eml(tmp.path(), "src.eml", fixture_no_attachments());
        let draft_dir = tmp.path().join("draft3");
        fs::create_dir_all(&draft_dir).expect("mkdir draft3");

        let out = extract_forward_attachments_into(&eml, &draft_dir).expect("extract");

        assert!(out.is_empty(), "no attachments expected");
        let entries = fs::read_dir(&draft_dir).expect("readdir").count();
        assert_eq!(entries, 0, "draft dir should remain empty");
    }

    /// Garbled .eml: parse_mail fails, extractor returns Ok(empty)
    /// instead of bubbling an error. Verifies the fail-soft policy
    /// stated in the module docs.
    #[test]
    fn extract_unparseable_eml_returns_empty() {
        let tmp = TempDir::new().expect("tempdir");
        // Pure garbage bytes — no headers, no structure.
        let eml = write_eml(tmp.path(), "garbage.eml", &[0xFFu8; 64]);
        let draft_dir = tmp.path().join("draft4");
        fs::create_dir_all(&draft_dir).expect("mkdir draft4");

        let out =
            extract_forward_attachments_into(&eml, &draft_dir).expect("extract should not error");

        // Either parse succeeds with zero parts (mailparse is permissive
        // and treats arbitrary bytes as a body) OR it fails and we hit
        // the warn path. Both routes must yield Ok with no attachments,
        // since pure garbage has no Content-Disposition: attachment.
        assert!(
            out.is_empty(),
            "unparseable / structureless .eml should yield zero attachments"
        );
    }

    /// Idempotency: calling the extractor twice with the same draft
    /// directory must succeed both times. The second call may write
    /// duplicate copies of the same attachments (each gets a fresh
    /// UUID suffix), but it must not error or panic. Mirrors the
    /// "user clicked Forward, then closed and re-opened" path.
    #[test]
    fn extract_is_idempotent_on_existing_draft_dir() {
        let tmp = TempDir::new().expect("tempdir");
        let eml = write_eml(tmp.path(), "src.eml", fixture_single_pdf());
        let draft_dir = tmp.path().join("draft5");
        fs::create_dir_all(&draft_dir).expect("mkdir draft5");

        let first = extract_forward_attachments_into(&eml, &draft_dir).expect("first call");
        let second = extract_forward_attachments_into(&eml, &draft_dir).expect("second call");

        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);

        // The two calls should produce DIFFERENT on-disk paths because
        // each attachment gets a fresh UUID suffix.
        let AttachmentSource::Disk(ref p1) = first[0].source;
        let AttachmentSource::Disk(ref p2) = second[0].source;
        assert_ne!(
            p1, p2,
            "second extraction should produce a fresh UUID-suffixed path"
        );
        assert!(p1.exists());
        assert!(p2.exists());
    }

    /// Bonus: source file does not exist -> StoreError::Io. Verifies
    /// the only error path that does propagate (not in the test count
    /// target but cheap insurance).
    #[test]
    fn extract_missing_source_file_errors() {
        let tmp = TempDir::new().expect("tempdir");
        let missing = tmp.path().join("does-not-exist.eml");
        let draft_dir = tmp.path().join("draft6");
        fs::create_dir_all(&draft_dir).expect("mkdir draft6");

        let err = extract_forward_attachments_into(&missing, &draft_dir)
            .expect_err("missing source must error");
        assert!(
            matches!(err, StoreError::Io(_)),
            "missing source should map to StoreError::Io, got {err:?}"
        );
    }
}
