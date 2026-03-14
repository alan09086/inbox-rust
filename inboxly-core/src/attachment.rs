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
