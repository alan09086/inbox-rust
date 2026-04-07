//! Per-draft attachment directory helpers.
//!
//! Compose attachments live on disk under `~/.local/share/inboxly/drafts/<draft_id>/`.
//! The picker bridge (M35 Phase 11) copies the source file into this directory using
//! a UUID-suffixed filename to avoid collisions when the user attaches two files
//! with the same name from different folders. The display name (the chip + the
//! MIME `Content-Disposition` header) preserves the original filename.

use std::path::PathBuf;

use uuid::Uuid;

use crate::error::{Result, StoreError};

/// Return the per-draft attachment directory for the given draft id.
///
/// Resolves to `<data_dir>/inboxly/drafts/<draft_id>/`. Creates the
/// directory (and all parents) if it does not already exist.
///
/// # Errors
///
/// Returns [`StoreError::Io`] if the platform data directory cannot be
/// resolved (e.g. `$HOME` unset on Linux) or if `create_dir_all` fails.
pub fn ensure_draft_dir(draft_id: &str) -> Result<PathBuf> {
    let base = dirs::data_dir()
        .ok_or_else(|| {
            StoreError::Io(std::io::Error::other(
                "no platform data directory available",
            ))
        })?
        .join("inboxly")
        .join("drafts")
        .join(draft_id);
    std::fs::create_dir_all(&base)?;
    Ok(base)
}

/// Build a collision-resistant on-disk filename for a draft attachment.
///
/// Format: `<basename>-<uuid8>.<ext>` (e.g. `invoice-7f3b2a9c.pdf`). The
/// `uuid8` portion uses the first 8 hex characters of the supplied UUID
/// in its hyphen-free `simple` form. The chip plus the MIME
/// `Content-Disposition` header still use the original filename — only
/// the on-disk path uses the UUID suffix (Gemini G2).
///
/// Edge cases:
/// - No extension (`README`) -> `README-<uuid8>` (no extension appended)
/// - Double extension (`archive.tar.gz`) -> `archive.tar-<uuid8>.gz`
///   (split at the LAST dot, mirroring `Path::extension` semantics)
/// - Dotfile (`.gitignore`) -> `<uuid8>.gitignore` (no basename to suffix)
#[must_use]
pub fn make_draft_filename(original_name: &str, uuid: Uuid) -> String {
    let uuid8: String = uuid.simple().to_string().chars().take(8).collect();
    if let Some(last_dot) = original_name.rfind('.') {
        if last_dot == 0 {
            // Dotfile case: ".gitignore" -> "<uuid8>.gitignore"
            return format!("{uuid8}{original_name}");
        }
        let (basename, ext_with_dot) = original_name.split_at(last_dot);
        format!("{basename}-{uuid8}{ext_with_dot}")
    } else {
        // No extension at all.
        format!("{original_name}-{uuid8}")
    }
}

/// Recursively delete the per-draft directory and all its contents.
///
/// Called by `ComposeDiscardDraft` and after a successful
/// `ComposeSendComplete` (when the user dismisses the Sent overlay).
/// Safe to call when the directory does not exist (no-op).
///
/// # Errors
///
/// Returns [`StoreError::Io`] if the directory exists but cannot be
/// removed. A missing directory is treated as success. A missing
/// platform data directory is also treated as success — there is
/// nothing to clean up.
pub fn cleanup_draft_dir(draft_id: &str) -> Result<()> {
    let Some(data_dir) = dirs::data_dir() else {
        return Ok(());
    };
    let base = data_dir.join("inboxly").join("drafts").join(draft_id);
    if !base.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&base)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Uuid, make_draft_filename};

    /// Reference UUID whose hyphen-free `simple` form starts with
    /// `7f3b2a9c`. Reused across the four naming tests so the assertions
    /// can hard-code the expected suffix.
    fn fixture_uuid() -> Uuid {
        Uuid::parse_str("7f3b2a9c-0000-0000-0000-000000000000").expect("valid uuid literal")
    }

    /// Common case: a single extension. The filename is split at the
    /// last dot and the UUID suffix is inserted between basename and
    /// extension.
    #[test]
    fn make_draft_filename_simple_extension() {
        let result = make_draft_filename("invoice.pdf", fixture_uuid());
        assert_eq!(result, "invoice-7f3b2a9c.pdf");
    }

    /// No extension at all (`README`): the suffix is appended after a
    /// hyphen and no extension is fabricated.
    #[test]
    fn make_draft_filename_no_extension() {
        let result = make_draft_filename("README", fixture_uuid());
        assert_eq!(result, "README-7f3b2a9c");
    }

    /// Double extension (`archive.tar.gz`): the split is at the LAST
    /// dot, matching `Path::extension`. The leading `archive.tar` is
    /// preserved as the basename.
    #[test]
    fn make_draft_filename_double_extension_splits_at_last_dot() {
        let result = make_draft_filename("archive.tar.gz", fixture_uuid());
        assert_eq!(result, "archive.tar-7f3b2a9c.gz");
    }

    /// Dotfile (`.gitignore`): no basename to suffix, so the UUID is
    /// prepended directly to the filename.
    #[test]
    fn make_draft_filename_dotfile() {
        let result = make_draft_filename(".gitignore", fixture_uuid());
        assert_eq!(result, "7f3b2a9c.gitignore");
    }
}
