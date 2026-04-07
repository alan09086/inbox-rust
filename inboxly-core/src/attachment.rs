use serde::{Deserialize, Serialize};

/// Lightweight attachment metadata — stored in SQLite, no content bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentMeta {
    /// Original filename (e.g., "invoice.pdf").
    pub filename: String,
    /// MIME type (e.g., "application/pdf").
    pub mime_type: String,
    /// Size in bytes.
    pub size_bytes: u64,
}

/// Full attachment including content bytes — loaded on demand from Maildir.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Metadata (name, MIME, size).
    pub meta: AttachmentMeta,
    /// Raw content bytes.
    pub content: Vec<u8>,
}

/// An attachment file the user picked while composing a draft.
///
/// Distinct from [`AttachmentMeta`] (which describes an attachment on an
/// already-received email): `AttachmentDraft` describes a file the user is
/// currently attaching to a draft. The bytes live ON DISK in the per-draft
/// directory (`~/.local/share/inboxly/drafts/<draft_id>/`), NOT in memory.
/// On send, lettre reads from disk; on discard, the directory is `rm -rf`'d.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentDraft {
    /// Original filename as the user picked it (e.g., "invoice.pdf").
    /// Used for the MIME `Content-Disposition` header so recipients see the
    /// natural name. Does NOT include the per-draft UUID suffix.
    pub filename: String,
    /// MIME type (e.g., "application/pdf"), inferred from the file extension.
    pub mime_type: String,
    /// File size in bytes (read from filesystem metadata at pick time).
    pub size_bytes: u64,
    /// Where the bytes live.
    pub source: AttachmentSource,
}

/// Where an [`AttachmentDraft`]'s bytes live.
///
/// Currently disk-only. The plan deliberately drops the speculative `Memory`
/// variant — keeping the SQLite drafts table small and avoiding any chance
/// of stuffing 20 MB attachments into JSON column values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentSource {
    /// Bytes stored on disk at the given path. The path is inside the
    /// per-draft directory `~/.local/share/inboxly/drafts/<draft_id>/`.
    Disk(std::path::PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_meta_creation() {
        let meta = AttachmentMeta {
            filename: "report.pdf".into(),
            mime_type: "application/pdf".into(),
            size_bytes: 1024,
        };
        assert_eq!(meta.filename, "report.pdf");
        assert_eq!(meta.size_bytes, 1024);
    }

    #[test]
    fn attachment_with_content() {
        let att = Attachment {
            meta: AttachmentMeta {
                filename: "test.txt".into(),
                mime_type: "text/plain".into(),
                size_bytes: 5,
            },
            content: b"hello".to_vec(),
        };
        assert_eq!(att.content.len(), 5);
    }
}
