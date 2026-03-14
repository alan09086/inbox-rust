# M4: Maildir++ Operations — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Maildir++ read/write operations with IMAP flag mapping in the `inboxly-store` crate.

**Architecture:** Maildir operations live in `inboxly-store` alongside SQLite. The `maildir` crate handles directory structure and atomic file operations. The `mailparse` crate handles MIME parsing of `.eml` files. All public types come from `inboxly-core`. The Maildir layer is the canonical email store — SQLite is a rebuildable index over it.

**Tech Stack:** Rust, `maildir` (directory ops, atomic store, flag management), `mailparse` (MIME parsing, header extraction, address parsing), `inboxly-core` (EmailMeta, EmailContent, EmailFlags, Contact, etc.)

**Prerequisites:** M1 (core types in `inboxly-core`), M2 (config with `data_dir`), M3 (SQLite store with `emails`/`threads` tables)

---

## Task 1: Add dependencies to `inboxly-store`

**Files:**
- Modify: `inboxly-store/Cargo.toml`

- [ ] **Step 1: Add maildir and mailparse dependencies**

Add to `[dependencies]` in `inboxly-store/Cargo.toml`:

```toml
maildir = "0.6"
mailparse = "0.15"
```

These versions are current stable. `maildir` provides `Maildir` struct with atomic `store_new()`, `store_cur_with_flags()`, `move_new_to_cur_with_flags()`, `set_flags()`, `delete()`, and iterators over `new/` and `cur/`. `mailparse` provides `parse_mail()` returning `ParsedMail` with MIME tree traversal.

- [ ] **Step 2: Add dev-dependencies for testing**

Add to `[dev-dependencies]`:

```toml
tempfile = "3"
```

All integration tests will use `tempfile::TempDir` for isolated Maildir instances.

- [ ] **Step 3: Verify compilation**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

---

## Task 2: Define standard Maildir folders and initialization

**Files:**
- Create: `inboxly-store/src/maildir_store.rs`
- Modify: `inboxly-store/src/lib.rs` (add `pub mod maildir_store;`)

- [ ] **Step 1: Define the `StandardFolder` enum**

In `inboxly-store/src/maildir_store.rs`:

```rust
use std::path::{Path, PathBuf};

/// Standard Maildir++ folders. INBOX is the root; others are dot-prefixed subdirectories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandardFolder {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Spam,
}

impl StandardFolder {
    /// Returns the Maildir++ directory name relative to the account root.
    /// INBOX is the root directory itself; others are dot-prefixed per Maildir++ spec.
    pub fn dirname(&self) -> &'static str {
        match self {
            Self::Inbox => ".",       // root maildir
            Self::Sent => ".Sent",
            Self::Drafts => ".Drafts",
            Self::Trash => ".Trash",
            Self::Spam => ".Spam",
        }
    }

    /// Returns all standard folders in creation order.
    pub fn all() -> &'static [StandardFolder] {
        &[
            Self::Inbox,
            Self::Sent,
            Self::Drafts,
            Self::Trash,
            Self::Spam,
        ]
    }

    /// Map an IMAP folder name to a standard folder.
    pub fn from_imap_name(name: &str) -> Option<Self> {
        match name {
            "INBOX" => Some(Self::Inbox),
            "Sent" | "[Gmail]/Sent Mail" => Some(Self::Sent),
            "Drafts" | "[Gmail]/Drafts" => Some(Self::Drafts),
            "Trash" | "[Gmail]/Trash" => Some(Self::Trash),
            "Spam" | "Junk" | "[Gmail]/Spam" => Some(Self::Spam),
            _ => None,
        }
    }
}
```

- [ ] **Step 2: Define the `MaildirStore` struct**

```rust
use maildir::Maildir;
use std::collections::HashMap;

/// Manages Maildir++ directory structure for one email account.
/// Layout: `<data_dir>/accounts/<account_id>/mail/`
///   - `new/`, `cur/`, `tmp/`    (INBOX)
///   - `.Sent/new/`, `.Sent/cur/`, `.Sent/tmp/`
///   - `.Drafts/new/`, `.Drafts/cur/`, `.Drafts/tmp/`
///   - `.Trash/new/`, `.Trash/cur/`, `.Trash/tmp/`
///   - `.Spam/new/`, `.Spam/cur/`, `.Spam/tmp/`
pub struct MaildirStore {
    /// Root path: `<data_dir>/accounts/<account_id>/mail/`
    root: PathBuf,
}
```

- [ ] **Step 3: Implement `MaildirStore::new()` and `init()`**

```rust
impl MaildirStore {
    /// Create a new MaildirStore for the given account mail directory.
    /// Does NOT create directories on disk — call `init()` for that.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Returns the root path of this Maildir store.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Initialize Maildir++ directory structure.
    /// Creates `new/`, `cur/`, `tmp/` for INBOX and all standard subfolders.
    /// Idempotent — safe to call multiple times.
    pub fn init(&self) -> std::io::Result<()> {
        for folder in StandardFolder::all() {
            let md = self.maildir_for(folder);
            md.create_dirs()?;
        }
        Ok(())
    }

    /// Get a `maildir::Maildir` handle for the given standard folder.
    fn maildir_for(&self, folder: &StandardFolder) -> Maildir {
        let path = match folder {
            StandardFolder::Inbox => self.root.clone(),
            other => self.root.join(other.dirname()),
        };
        Maildir::from(path)
    }

    /// Get a `maildir::Maildir` handle for a folder by IMAP name.
    /// Returns None if the IMAP name doesn't map to a standard folder.
    pub fn maildir_for_imap(&self, imap_folder: &str) -> Option<Maildir> {
        StandardFolder::from_imap_name(imap_folder)
            .map(|f| self.maildir_for(&f))
    }
}
```

- [ ] **Step 4: Register the module**

In `inboxly-store/src/lib.rs`, add:

```rust
pub mod maildir_store;
```

- [ ] **Step 5: Verify compilation**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

---

## Task 3: IMAP flag encoding/decoding

**Files:**
- Modify: `inboxly-store/src/maildir_store.rs`

The Maildir filename suffix encodes flags as `:2,<FLAGS>` where flags are single uppercase letters in ASCII-sorted order. IMAP flags map to: `S` = \Seen, `F` = \Flagged, `R` = \Answered (replied), `D` = \Draft, `T` = \Deleted (trashed). The `inboxly-core` `EmailFlags` bitmask stores: read, starred, answered, draft (bit positions defined in M1).

- [ ] **Step 1: Implement `flags_to_suffix()` — EmailFlags to Maildir suffix string**

```rust
use inboxly_core::EmailFlags;

/// Encode EmailFlags into a Maildir flag suffix string.
/// Flags are single uppercase ASCII letters in sorted order.
/// Mapping: read→S, starred→F, answered→R, draft→D
///
/// Example: read + starred → "FS" (alphabetical order)
pub fn flags_to_suffix(flags: &EmailFlags) -> String {
    let mut s = String::with_capacity(4);
    if flags.draft {
        s.push('D');
    }
    if flags.starred {
        s.push('F');
    }
    if flags.answered {
        s.push('R');
    }
    if flags.read {
        s.push('S');
    }
    s
}
```

- [ ] **Step 2: Implement `suffix_to_flags()` — Maildir suffix string to EmailFlags**

```rust
/// Decode a Maildir flag suffix string into EmailFlags.
/// Input is the characters after `:2,` in the filename.
///
/// Example: "FS" → read=false, starred=true, answered=false, draft=false
///          wait — F=starred, S=read → starred=true, read=true
pub fn suffix_to_flags(suffix: &str) -> EmailFlags {
    EmailFlags {
        read: suffix.contains('S'),
        starred: suffix.contains('F'),
        answered: suffix.contains('R'),
        draft: suffix.contains('D'),
    }
}
```

- [ ] **Step 3: Implement `flags_from_filename()` — extract flags from a full Maildir filename**

```rust
/// Extract EmailFlags from a Maildir filename.
/// Filename format: `<unique>:2,<flags>` (info separator is `:2,`).
/// If no `:2,` is present, returns default (all false) flags.
pub fn flags_from_filename(filename: &str) -> EmailFlags {
    if let Some(idx) = filename.find(":2,") {
        suffix_to_flags(&filename[idx + 3..])
    } else {
        EmailFlags::default()
    }
}
```

- [ ] **Step 4: Unit tests for flag encoding/decoding**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flags_roundtrip_empty() {
        let flags = EmailFlags::default();
        let suffix = flags_to_suffix(&flags);
        assert_eq!(suffix, "");
        assert_eq!(suffix_to_flags(&suffix), flags);
    }

    #[test]
    fn test_flags_roundtrip_all() {
        let flags = EmailFlags { read: true, starred: true, answered: true, draft: true };
        let suffix = flags_to_suffix(&flags);
        assert_eq!(suffix, "DFRS");
        assert_eq!(suffix_to_flags(&suffix), flags);
    }

    #[test]
    fn test_flags_roundtrip_read_only() {
        let flags = EmailFlags { read: true, starred: false, answered: false, draft: false };
        let suffix = flags_to_suffix(&flags);
        assert_eq!(suffix, "S");
        assert_eq!(suffix_to_flags(&suffix), flags);
    }

    #[test]
    fn test_flags_from_filename_with_info() {
        let flags = flags_from_filename("1710000000.12345.hostname:2,FS");
        assert_eq!(flags, EmailFlags { read: true, starred: true, answered: false, draft: false });
    }

    #[test]
    fn test_flags_from_filename_no_info() {
        let flags = flags_from_filename("1710000000.12345.hostname");
        assert_eq!(flags, EmailFlags::default());
    }
}
```

- [ ] **Step 5: Run tests**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store -- flags
```

---

## Task 4: Write email bytes to Maildir (atomic tmp -> new)

**Files:**
- Modify: `inboxly-store/src/maildir_store.rs`

- [ ] **Step 1: Define the `StoreResult` with Maildir ID**

```rust
/// Result of storing an email to Maildir.
/// Contains the unique Maildir ID that can be used for subsequent operations.
pub struct StoredEmail {
    /// Unique Maildir message ID (the filename stem before `:2,`).
    pub id: String,
    /// Full path to the stored file on disk.
    pub path: PathBuf,
}
```

- [ ] **Step 2: Implement `store_new()` — write raw .eml bytes to a folder's `new/` directory**

```rust
use crate::error::StoreError;

impl MaildirStore {
    /// Store raw email bytes into the `new/` directory of the given folder.
    /// Uses atomic write: creates in `tmp/`, then renames to `new/`.
    /// Returns the Maildir message ID for subsequent operations.
    ///
    /// This is the primary ingest path — IMAP sync writes here, then
    /// `deliver()` moves from `new/` to `cur/` with flags.
    pub fn store_new(
        &self,
        folder: &StandardFolder,
        data: &[u8],
    ) -> Result<StoredEmail, StoreError> {
        let md = self.maildir_for(folder);
        let id = md.store_new(data)
            .map_err(|e| StoreError::Maildir(format!("store_new failed: {e}")))?;

        // The file is now in <folder>/new/<id>
        let path = match folder {
            StandardFolder::Inbox => self.root.join("new").join(&id),
            other => self.root.join(other.dirname()).join("new").join(&id),
        };

        Ok(StoredEmail { id, path })
    }

    /// Store raw email bytes directly into `cur/` with initial flags.
    /// Used when the email has already been processed (e.g., restoring from backup,
    /// storing sent mail that should be immediately marked as read).
    pub fn store_cur(
        &self,
        folder: &StandardFolder,
        data: &[u8],
        flags: &EmailFlags,
    ) -> Result<StoredEmail, StoreError> {
        let md = self.maildir_for(folder);
        let suffix = flags_to_suffix(flags);
        let id = md.store_cur_with_flags(data, &suffix)
            .map_err(|e| StoreError::Maildir(format!("store_cur failed: {e}")))?;

        // The file is now in <folder>/cur/<id>:2,<flags>
        let filename = if suffix.is_empty() {
            format!("{id}:2,")
        } else {
            format!("{id}:2,{suffix}")
        };
        let path = match folder {
            StandardFolder::Inbox => self.root.join("cur").join(&filename),
            other => self.root.join(other.dirname()).join("cur").join(&filename),
        };

        Ok(StoredEmail { id, path })
    }
}
```

- [ ] **Step 3: Add `StoreError::Maildir` variant**

In the store error type (from M3), add a variant for Maildir errors:

```rust
// In inboxly-store/src/error.rs (or wherever StoreError is defined)
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    // ... existing variants from M3 ...

    #[error("Maildir operation failed: {0}")]
    Maildir(String),

    #[error("Email parse error: {0}")]
    Parse(String),
}
```

- [ ] **Step 4: Verify compilation**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

---

## Task 5: Deliver message (move new -> cur with flags)

**Files:**
- Modify: `inboxly-store/src/maildir_store.rs`

- [ ] **Step 1: Implement `deliver()` — move from new/ to cur/ setting initial flags**

```rust
impl MaildirStore {
    /// Deliver a message: move from `new/` to `cur/` with the given flags.
    /// This is the standard Maildir delivery handoff. Messages in `new/` are
    /// "not yet seen by the MUA"; moving to `cur/` marks them as delivered.
    ///
    /// Typically called after IMAP sync writes to `new/` and the message
    /// has been indexed in SQLite.
    pub fn deliver(
        &self,
        folder: &StandardFolder,
        maildir_id: &str,
        flags: &EmailFlags,
    ) -> Result<(), StoreError> {
        let md = self.maildir_for(folder);
        let suffix = flags_to_suffix(flags);
        md.move_new_to_cur_with_flags(maildir_id, &suffix)
            .map_err(|e| StoreError::Maildir(format!(
                "deliver (move new->cur) failed for {maildir_id}: {e}"
            )))
    }

    /// Deliver a message with no flags set (unread, unflagged).
    pub fn deliver_unread(
        &self,
        folder: &StandardFolder,
        maildir_id: &str,
    ) -> Result<(), StoreError> {
        let md = self.maildir_for(folder);
        md.move_new_to_cur(maildir_id)
            .map_err(|e| StoreError::Maildir(format!(
                "deliver (move new->cur) failed for {maildir_id}: {e}"
            )))
    }
}
```

- [ ] **Step 2: Verify compilation**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

---

## Task 6: Update flags (rename file with new suffix)

**Files:**
- Modify: `inboxly-store/src/maildir_store.rs`

- [ ] **Step 1: Implement `set_flags()` — replace all flags on a message**

```rust
impl MaildirStore {
    /// Set flags on a message in `cur/`. Replaces all existing flags.
    /// The maildir crate handles the file rename (changing the `:2,<flags>` suffix).
    pub fn set_flags(
        &self,
        folder: &StandardFolder,
        maildir_id: &str,
        flags: &EmailFlags,
    ) -> Result<(), StoreError> {
        let md = self.maildir_for(folder);
        let suffix = flags_to_suffix(flags);
        md.set_flags(maildir_id, &suffix)
            .map_err(|e| StoreError::Maildir(format!(
                "set_flags failed for {maildir_id}: {e}"
            )))
    }

    /// Add flags to a message without clearing existing ones.
    pub fn add_flags(
        &self,
        folder: &StandardFolder,
        maildir_id: &str,
        flags: &str,
    ) -> Result<(), StoreError> {
        let md = self.maildir_for(folder);
        md.add_flags(maildir_id, flags)
            .map_err(|e| StoreError::Maildir(format!(
                "add_flags failed for {maildir_id}: {e}"
            )))
    }

    /// Remove specific flags from a message.
    pub fn remove_flags(
        &self,
        folder: &StandardFolder,
        maildir_id: &str,
        flags: &str,
    ) -> Result<(), StoreError> {
        let md = self.maildir_for(folder);
        md.remove_flags(maildir_id, flags)
            .map_err(|e| StoreError::Maildir(format!(
                "remove_flags failed for {maildir_id}: {e}"
            )))
    }
}
```

- [ ] **Step 2: Verify compilation**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

---

## Task 7: Parse .eml file to EmailMeta (lightweight metadata extraction)

**Files:**
- Modify: `inboxly-store/src/maildir_store.rs`

This is the critical parsing task. `EmailMeta` is what lives in SQLite and memory — lightweight, no body content. We extract envelope data (From, To, Cc, Subject, Date, Message-ID, References, In-Reply-To) and attachment metadata from the MIME tree without loading attachment bodies.

- [ ] **Step 1: Implement `parse_email_meta()` — extract EmailMeta from raw .eml bytes**

```rust
use inboxly_core::{EmailMeta, EmailId, AccountId, ThreadId, Contact, AttachmentMeta, EmailFlags};
use mailparse::{parse_mail, MailHeaderMap, addrparse_header, dateparse};
use chrono::{DateTime, Utc, TimeZone};

/// Parse raw .eml bytes into an EmailMeta (lightweight metadata).
/// Does NOT load attachment content — only extracts names, MIME types, and sizes.
///
/// Parameters:
/// - `data`: raw RFC 5322 email bytes
/// - `account_id`: the account this email belongs to
/// - `maildir_path`: on-disk path where the .eml is stored
/// - `flags`: current Maildir flags
/// - `imap_uid`: IMAP UID (0 if not from IMAP sync)
/// - `imap_folder`: IMAP folder name (e.g., "INBOX")
///
/// Thread assignment is NOT done here — that's M10 (threading algorithm).
/// The returned `thread_id` is a placeholder (nil UUID).
pub fn parse_email_meta(
    data: &[u8],
    account_id: AccountId,
    maildir_path: PathBuf,
    flags: EmailFlags,
    imap_uid: u32,
    imap_folder: String,
) -> Result<EmailMeta, StoreError> {
    let parsed = parse_mail(data)
        .map_err(|e| StoreError::Parse(format!("Failed to parse email: {e}")))?;

    let headers = &parsed.headers;

    // Message-ID → EmailId
    let message_id = headers.get_first_value("Message-ID")
        .unwrap_or_default()
        .trim_matches(|c| c == '<' || c == '>')
        .to_string();
    let id = EmailId(if message_id.is_empty() {
        // Fallback: generate a synthetic ID from hash of raw data
        format!("synthetic-{:x}", {
            let mut h: u64 = 0;
            for &b in data.iter().take(1024) {
                h = h.wrapping_mul(31).wrapping_add(b as u64);
            }
            h
        })
    } else {
        message_id
    });

    // From
    let from = headers.get_first_value("From")
        .and_then(|v| parse_contact_first(&v))
        .unwrap_or_else(|| Contact { name: String::new(), address: String::new() });

    // To
    let to = headers.get_first_value("To")
        .map(|v| parse_contacts(&v))
        .unwrap_or_default();

    // Cc
    let cc = headers.get_first_value("Cc")
        .map(|v| parse_contacts(&v))
        .unwrap_or_default();

    // Subject
    let subject = headers.get_first_value("Subject")
        .unwrap_or_default();

    // Date
    let date = headers.get_first_value("Date")
        .and_then(|v| dateparse(&v).ok())
        .map(|ts| Utc.timestamp_opt(ts, 0).single())
        .flatten()
        .unwrap_or_else(Utc::now);

    // Snippet — first ~200 chars of plaintext body
    let snippet = extract_snippet(&parsed, 200);

    // Attachment metadata — walk MIME tree, collect non-inline parts
    let attachments = extract_attachment_meta(&parsed);

    // In-Reply-To and References — stored for threading (M10)
    let in_reply_to = headers.get_first_value("In-Reply-To")
        .map(|v| v.trim_matches(|c| c == '<' || c == '>').to_string());
    let references_json = headers.get_first_value("References")
        .map(|v| {
            v.split_whitespace()
                .map(|r| r.trim_matches(|c| c == '<' || c == '>').to_string())
                .collect::<Vec<_>>()
        });

    let has_attachments = !attachments.is_empty();

    Ok(EmailMeta {
        id,
        account_id,
        thread_id: ThreadId(uuid::Uuid::nil()), // placeholder — M10 assigns real thread
        from,
        to,
        cc,
        subject,
        snippet,
        date,
        maildir_path,
        attachments,
        flags,
        size_bytes: data.len() as u64,
        imap_uid,
        imap_folder,
        has_attachments,
        message_id_header: headers.get_first_value("Message-ID").unwrap_or_default(),
        in_reply_to,
        references_json,
    })
}
```

- [ ] **Step 2: Implement `parse_contact_first()` and `parse_contacts()` helpers**

```rust
use mailparse::{addrparse, MailAddr};

/// Parse an address header value and return the first address as a Contact.
fn parse_contact_first(value: &str) -> Option<Contact> {
    let addrs = addrparse(value).ok()?;
    addrs.iter().next().and_then(|addr| match addr {
        MailAddr::Single(info) => Some(Contact {
            name: info.display_name.clone().unwrap_or_default(),
            address: info.addr.clone(),
        }),
        MailAddr::Group(info) => info.addrs.first().map(|a| Contact {
            name: a.display_name.clone().unwrap_or_default(),
            address: a.addr.clone(),
        }),
    })
}

/// Parse an address header value into a Vec<Contact>.
fn parse_contacts(value: &str) -> Vec<Contact> {
    let Ok(addrs) = addrparse(value) else { return vec![] };
    let mut contacts = Vec::new();
    for addr in addrs.iter() {
        match addr {
            MailAddr::Single(info) => {
                contacts.push(Contact {
                    name: info.display_name.clone().unwrap_or_default(),
                    address: info.addr.clone(),
                });
            }
            MailAddr::Group(info) => {
                for a in &info.addrs {
                    contacts.push(Contact {
                        name: a.display_name.clone().unwrap_or_default(),
                        address: a.addr.clone(),
                    });
                }
            }
        }
    }
    contacts
}
```

- [ ] **Step 3: Implement `extract_snippet()` — get first N chars of plaintext body**

```rust
use mailparse::ParsedMail;

/// Extract the first `max_len` characters of plaintext body for use as snippet.
/// Walks the MIME tree to find `text/plain` parts, falling back to stripping
/// HTML from `text/html` parts if no plaintext is available.
fn extract_snippet(parsed: &ParsedMail, max_len: usize) -> String {
    // Try text/plain first
    if let Some(text) = find_body_text(parsed) {
        let trimmed = text.trim();
        if trimmed.len() <= max_len {
            return trimmed.to_string();
        }
        // Truncate at a word boundary
        let truncated = &trimmed[..max_len];
        if let Some(last_space) = truncated.rfind(' ') {
            return format!("{}...", &truncated[..last_space]);
        }
        return format!("{truncated}...");
    }

    // Fallback: strip HTML tags (very basic — just remove <...> and decode entities)
    if let Some(html) = find_body_html(parsed) {
        let stripped = strip_html_basic(&html);
        let trimmed = stripped.trim();
        if trimmed.len() <= max_len {
            return trimmed.to_string();
        }
        let truncated = &trimmed[..max_len];
        if let Some(last_space) = truncated.rfind(' ') {
            return format!("{}...", &truncated[..last_space]);
        }
        return format!("{truncated}...");
    }

    String::new()
}

/// Walk MIME tree depth-first looking for text/plain content.
fn find_body_text(parsed: &ParsedMail) -> Option<String> {
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
fn find_body_html(parsed: &ParsedMail) -> Option<String> {
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

/// Very basic HTML tag stripping. Not a full parser — sufficient for snippets.
fn strip_html_basic(html: &str) -> String {
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
    // Basic entity decoding
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}
```

- [ ] **Step 4: Implement `extract_attachment_meta()` — collect attachment metadata from MIME tree**

```rust
use mailparse::parse_content_disposition;

/// Walk the MIME tree and collect attachment metadata (name, MIME type, size).
/// Does NOT load attachment content — only metadata for the `EmailMeta.attachments` field.
fn extract_attachment_meta(parsed: &ParsedMail) -> Vec<AttachmentMeta> {
    let mut attachments = Vec::new();
    collect_attachments(parsed, &mut attachments);
    attachments
}

fn collect_attachments(parsed: &ParsedMail, out: &mut Vec<AttachmentMeta>) {
    let disposition = parsed.get_content_disposition();

    // An attachment is any part with Content-Disposition: attachment,
    // or a non-text/non-multipart part that isn't inline text.
    let is_attachment = disposition.disposition == mailparse::DispositionType::Attachment
        || (disposition.disposition != mailparse::DispositionType::Inline
            && !parsed.ctype.mimetype.starts_with("text/")
            && !parsed.ctype.mimetype.starts_with("multipart/"));

    if is_attachment {
        let name = disposition.params.get("filename")
            .or_else(|| parsed.ctype.params.get("name"))
            .cloned()
            .unwrap_or_else(|| "unnamed".to_string());

        let size = parsed.get_body_raw()
            .map(|b| b.len() as u64)
            .unwrap_or(0);

        out.push(AttachmentMeta {
            name,
            mime_type: parsed.ctype.mimetype.clone(),
            size_bytes: size,
        });
    }

    for sub in &parsed.subparts {
        collect_attachments(sub, out);
    }
}
```

- [ ] **Step 5: Verify compilation**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

---

## Task 8: Read full EmailContent from .eml (lazy load)

**Files:**
- Modify: `inboxly-store/src/maildir_store.rs`

- [ ] **Step 1: Implement `read_email_content()` — parse full body + headers + attachments from disk**

```rust
use inboxly_core::{EmailContent, EmailId, Attachment};
use std::collections::HashMap;

impl MaildirStore {
    /// Read and parse full email content from a Maildir file path.
    /// This is the lazy-load path — called when the user opens an email
    /// in the conversation view. The `maildir_path` comes from `EmailMeta`.
    ///
    /// Returns body text, body HTML, all headers, and full attachment content.
    pub fn read_email_content(
        &self,
        maildir_path: &Path,
    ) -> Result<EmailContent, StoreError> {
        let data = std::fs::read(maildir_path)
            .map_err(|e| StoreError::Maildir(format!(
                "Failed to read {}: {e}", maildir_path.display()
            )))?;

        parse_email_content(&data)
    }
}

/// Parse raw .eml bytes into full EmailContent.
/// Extracts body (text + HTML), all headers, and full attachment content.
pub fn parse_email_content(data: &[u8]) -> Result<EmailContent, StoreError> {
    let parsed = parse_mail(data)
        .map_err(|e| StoreError::Parse(format!("Failed to parse email: {e}")))?;

    // Extract Message-ID for the EmailId
    let message_id = parsed.headers.get_first_value("Message-ID")
        .unwrap_or_default()
        .trim_matches(|c| c == '<' || c == '>')
        .to_string();
    let id = EmailId(message_id);

    // Body text and HTML
    let body_text = find_body_text(&parsed);
    let body_html = find_body_html(&parsed);

    // All headers as HashMap
    let mut headers = HashMap::new();
    for header in &parsed.headers {
        let key = header.get_key()
            .unwrap_or_default();
        let value = header.get_value()
            .unwrap_or_default();
        headers.insert(key, value);
    }

    // Full attachments with content
    let attachments = extract_full_attachments(&parsed);

    Ok(EmailContent {
        id,
        body_text,
        body_html,
        headers,
        attachments,
    })
}

/// Walk the MIME tree and collect full attachments (metadata + content bytes).
fn extract_full_attachments(parsed: &ParsedMail) -> Vec<Attachment> {
    let mut attachments = Vec::new();
    collect_full_attachments(parsed, &mut attachments);
    attachments
}

fn collect_full_attachments(parsed: &ParsedMail, out: &mut Vec<Attachment>) {
    let disposition = parsed.get_content_disposition();

    let is_attachment = disposition.disposition == mailparse::DispositionType::Attachment
        || (disposition.disposition != mailparse::DispositionType::Inline
            && !parsed.ctype.mimetype.starts_with("text/")
            && !parsed.ctype.mimetype.starts_with("multipart/"));

    if is_attachment {
        let name = disposition.params.get("filename")
            .or_else(|| parsed.ctype.params.get("name"))
            .cloned()
            .unwrap_or_else(|| "unnamed".to_string());

        if let Ok(content) = parsed.get_body_raw() {
            out.push(Attachment {
                name,
                mime_type: parsed.ctype.mimetype.clone(),
                size_bytes: content.len() as u64,
                content,
            });
        }
    }

    for sub in &parsed.subparts {
        collect_full_attachments(sub, out);
    }
}
```

- [ ] **Step 2: Verify compilation**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

---

## Task 9: List all messages in a folder

**Files:**
- Modify: `inboxly-store/src/maildir_store.rs`

- [ ] **Step 1: Define `MaildirEntry` — lightweight entry from directory listing**

```rust
/// A lightweight entry from listing a Maildir folder.
/// Contains only what can be determined from the filename — no parsing.
pub struct MaildirEntry {
    /// Unique Maildir message ID (filename stem).
    pub id: String,
    /// Full path to the .eml file on disk.
    pub path: PathBuf,
    /// Flags decoded from the filename suffix.
    pub flags: EmailFlags,
    /// Whether this message is in `new/` (true) or `cur/` (false).
    pub is_new: bool,
}
```

- [ ] **Step 2: Implement `list_messages()` — enumerate all messages in a folder**

```rust
impl MaildirStore {
    /// List all messages in a folder (both `new/` and `cur/`).
    /// Returns lightweight entries with ID, path, and flags — no parsing.
    /// Messages in `new/` have `is_new = true`.
    pub fn list_messages(
        &self,
        folder: &StandardFolder,
    ) -> Result<Vec<MaildirEntry>, StoreError> {
        let md = self.maildir_for(folder);
        let mut entries = Vec::new();

        // List new/ messages
        for entry in md.list_new() {
            let entry = entry.map_err(|e| StoreError::Maildir(format!(
                "Failed to list new/ in {:?}: {e}", folder
            )))?;
            let id = entry.id().to_string();
            let path = entry.path().clone();
            let flags_str = entry.flags();
            let flags = suffix_to_flags(flags_str);
            entries.push(MaildirEntry {
                id,
                path,
                flags,
                is_new: true,
            });
        }

        // List cur/ messages
        for entry in md.list_cur() {
            let entry = entry.map_err(|e| StoreError::Maildir(format!(
                "Failed to list cur/ in {:?}: {e}", folder
            )))?;
            let id = entry.id().to_string();
            let path = entry.path().clone();
            let flags_str = entry.flags();
            let flags = suffix_to_flags(flags_str);
            entries.push(MaildirEntry {
                id,
                path,
                flags,
                is_new: false,
            });
        }

        Ok(entries)
    }

    /// Count total messages in a folder (new + cur).
    pub fn count_messages(&self, folder: &StandardFolder) -> usize {
        let md = self.maildir_for(folder);
        md.count_new() + md.count_cur()
    }
}
```

- [ ] **Step 3: Verify compilation**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

---

## Task 10: Delete message from Maildir

**Files:**
- Modify: `inboxly-store/src/maildir_store.rs`

- [ ] **Step 1: Implement `delete_message()`**

```rust
impl MaildirStore {
    /// Delete a message from a folder by its Maildir ID.
    /// Removes the file from disk (searches both `new/` and `cur/`).
    pub fn delete_message(
        &self,
        folder: &StandardFolder,
        maildir_id: &str,
    ) -> Result<(), StoreError> {
        let md = self.maildir_for(folder);
        md.delete(maildir_id)
            .map_err(|e| StoreError::Maildir(format!(
                "delete failed for {maildir_id} in {:?}: {e}", folder
            )))
    }

    /// Move a message from one folder to another.
    /// The message retains its flags during the move.
    pub fn move_message(
        &self,
        from_folder: &StandardFolder,
        to_folder: &StandardFolder,
        maildir_id: &str,
    ) -> Result<(), StoreError> {
        let from_md = self.maildir_for(from_folder);
        let to_md = self.maildir_for(to_folder);
        from_md.move_to(maildir_id, &to_md)
            .map_err(|e| StoreError::Maildir(format!(
                "move failed for {maildir_id} from {:?} to {:?}: {e}",
                from_folder, to_folder
            )))
    }

    /// Copy a message from one folder to another.
    /// The original remains in the source folder.
    pub fn copy_message(
        &self,
        from_folder: &StandardFolder,
        to_folder: &StandardFolder,
        maildir_id: &str,
    ) -> Result<(), StoreError> {
        let from_md = self.maildir_for(from_folder);
        let to_md = self.maildir_for(to_folder);
        from_md.copy_to(maildir_id, &to_md)
            .map_err(|e| StoreError::Maildir(format!(
                "copy failed for {maildir_id} from {:?} to {:?}: {e}",
                from_folder, to_folder
            )))
    }
}
```

- [ ] **Step 2: Verify compilation**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

---

## Task 11: Rebuild SQLite from Maildir (disaster recovery)

**Files:**
- Modify: `inboxly-store/src/maildir_store.rs`

This is the key durability feature: if SQLite is lost or corrupted, we can rebuild the `emails` and `threads` tables entirely from the Maildir `.eml` files on disk. This task implements the Maildir scan portion. Thread reconstruction (M10) is not yet available, so we populate `thread_id` with nil UUID and leave thread table empty — M10 will add the `rebuild_threads()` pass.

- [ ] **Step 1: Implement `scan_folder()` — parse all .eml files in a folder into EmailMeta**

```rust
impl MaildirStore {
    /// Scan all messages in a folder and parse each into EmailMeta.
    /// Used for disaster recovery (rebuild SQLite from Maildir).
    ///
    /// Returns (successes, errors) — parsing failures are logged but don't
    /// abort the scan. A single corrupt .eml should not prevent recovery.
    pub fn scan_folder(
        &self,
        folder: &StandardFolder,
        account_id: AccountId,
    ) -> (Vec<EmailMeta>, Vec<ScanError>) {
        let mut metas = Vec::new();
        let mut errors = Vec::new();

        let imap_folder = match folder {
            StandardFolder::Inbox => "INBOX".to_string(),
            StandardFolder::Sent => "Sent".to_string(),
            StandardFolder::Drafts => "Drafts".to_string(),
            StandardFolder::Trash => "Trash".to_string(),
            StandardFolder::Spam => "Spam".to_string(),
        };

        let entries = match self.list_messages(folder) {
            Ok(e) => e,
            Err(e) => {
                errors.push(ScanError {
                    path: self.root.join(folder.dirname()),
                    error: format!("Failed to list folder: {e}"),
                });
                return (metas, errors);
            }
        };

        for entry in entries {
            let data = match std::fs::read(&entry.path) {
                Ok(d) => d,
                Err(e) => {
                    errors.push(ScanError {
                        path: entry.path,
                        error: format!("Failed to read file: {e}"),
                    });
                    continue;
                }
            };

            match parse_email_meta(
                &data,
                account_id.clone(),
                entry.path.clone(),
                entry.flags,
                0, // imap_uid unknown during rebuild
                imap_folder.clone(),
            ) {
                Ok(meta) => metas.push(meta),
                Err(e) => {
                    errors.push(ScanError {
                        path: entry.path,
                        error: format!("Failed to parse: {e}"),
                    });
                }
            }
        }

        (metas, errors)
    }

    /// Scan ALL standard folders for the given account.
    /// Returns all parsed EmailMeta entries and any scan errors.
    pub fn scan_all(
        &self,
        account_id: AccountId,
    ) -> (Vec<EmailMeta>, Vec<ScanError>) {
        let mut all_metas = Vec::new();
        let mut all_errors = Vec::new();

        for folder in StandardFolder::all() {
            let (metas, errors) = self.scan_folder(folder, account_id.clone());
            all_metas.extend(metas);
            all_errors.extend(errors);
        }

        (all_metas, all_errors)
    }
}

/// Error from scanning a single file during Maildir rebuild.
#[derive(Debug)]
pub struct ScanError {
    pub path: PathBuf,
    pub error: String,
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.error)
    }
}
```

- [ ] **Step 2: Implement `rebuild_emails_table()` — orchestrate the full rebuild**

This function connects the Maildir scan to the SQLite store (from M3). It clears the `emails` table and repopulates from disk.

```rust
use crate::SqliteStore; // from M3

/// Rebuild the SQLite `emails` table from Maildir contents.
/// This is the disaster recovery path.
///
/// Steps:
/// 1. Scan all Maildir folders for .eml files
/// 2. Parse each into EmailMeta
/// 3. Clear the existing `emails` table
/// 4. Insert all parsed EmailMeta into SQLite
///
/// Thread reconstruction is handled separately by M10's `rebuild_threads()`.
///
/// Returns the number of emails recovered and any scan errors.
pub fn rebuild_emails_from_maildir(
    maildir: &MaildirStore,
    sqlite: &SqliteStore,
    account_id: AccountId,
) -> Result<(usize, Vec<ScanError>), StoreError> {
    let (metas, errors) = maildir.scan_all(account_id);

    // Clear existing emails for this account
    sqlite.clear_emails_for_account(&metas.first().map(|m| &m.account_id))?;

    // Insert all recovered emails
    let count = metas.len();
    for meta in metas {
        sqlite.insert_email(&meta)?;
    }

    Ok((count, errors))
}
```

**Note:** `SqliteStore::clear_emails_for_account()` and `SqliteStore::insert_email()` are assumed to exist from M3. If the M3 API differs, adjust the method names accordingly.

- [ ] **Step 3: Verify compilation**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

---

## Task 12: Create test fixture .eml files

**Files:**
- Create: `inboxly-store/tests/fixtures/simple.eml`
- Create: `inboxly-store/tests/fixtures/multipart.eml`
- Create: `inboxly-store/tests/fixtures/with_attachment.eml`
- Create: `inboxly-store/tests/fixtures/reply_in_thread.eml`

These are real RFC 5322 email fixtures for integration testing.

- [ ] **Step 1: Create `simple.eml` — plain text, single recipient**

```
From: Alice Smith <alice@example.com>
To: Bob Jones <bob@example.com>
Subject: Hello from Inboxly
Date: Sat, 14 Mar 2026 10:00:00 -0400
Message-ID: <msg001@example.com>
MIME-Version: 1.0
Content-Type: text/plain; charset=utf-8
Content-Transfer-Encoding: 7bit

Hi Bob,

This is a test email for Inboxly integration tests.

Best,
Alice
```

- [ ] **Step 2: Create `multipart.eml` — text/plain + text/html**

```
From: Newsletter <news@example.com>
To: Bob Jones <bob@example.com>
Subject: Weekly Digest
Date: Sat, 14 Mar 2026 12:00:00 -0400
Message-ID: <msg002@example.com>
MIME-Version: 1.0
Content-Type: multipart/alternative; boundary="boundary123"

--boundary123
Content-Type: text/plain; charset=utf-8

Weekly digest in plain text.

--boundary123
Content-Type: text/html; charset=utf-8

<html><body><h1>Weekly Digest</h1><p>Weekly digest in HTML.</p></body></html>

--boundary123--
```

- [ ] **Step 3: Create `with_attachment.eml` — text body + attachment**

```
From: Carol Davis <carol@example.com>
To: Bob Jones <bob@example.com>
Subject: Document attached
Date: Sat, 14 Mar 2026 14:00:00 -0400
Message-ID: <msg003@example.com>
MIME-Version: 1.0
Content-Type: multipart/mixed; boundary="mixbound456"

--mixbound456
Content-Type: text/plain; charset=utf-8

Please find the document attached.

--mixbound456
Content-Type: application/pdf; name="report.pdf"
Content-Disposition: attachment; filename="report.pdf"
Content-Transfer-Encoding: base64

JVBERi0xLjQKMSAwIG9iago8PAovVHlwZSAvQ2F0YWxvZwovUGFnZXMgMiAwIFIKPj4KZW5k
b2JqCg==

--mixbound456--
```

- [ ] **Step 4: Create `reply_in_thread.eml` — email with References and In-Reply-To**

```
From: Bob Jones <bob@example.com>
To: Alice Smith <alice@example.com>
Subject: Re: Hello from Inboxly
Date: Sat, 14 Mar 2026 11:00:00 -0400
Message-ID: <msg004@example.com>
In-Reply-To: <msg001@example.com>
References: <msg001@example.com>
MIME-Version: 1.0
Content-Type: text/plain; charset=utf-8
Content-Transfer-Encoding: 7bit

Hi Alice,

Thanks for the test email! Inboxly is looking great.

Bob
```

---

## Task 13: Integration tests

**Files:**
- Create: `inboxly-store/tests/maildir_integration.rs`

- [ ] **Step 1: Test Maildir initialization creates correct directory structure**

```rust
use inboxly_store::maildir_store::{MaildirStore, StandardFolder};
use tempfile::TempDir;

#[test]
fn test_init_creates_all_directories() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    // INBOX directories
    assert!(tmp.path().join("new").is_dir());
    assert!(tmp.path().join("cur").is_dir());
    assert!(tmp.path().join("tmp").is_dir());

    // Subfolder directories
    for folder in &[".Sent", ".Drafts", ".Trash", ".Spam"] {
        assert!(tmp.path().join(folder).join("new").is_dir(), "missing {folder}/new");
        assert!(tmp.path().join(folder).join("cur").is_dir(), "missing {folder}/cur");
        assert!(tmp.path().join(folder).join("tmp").is_dir(), "missing {folder}/tmp");
    }
}

#[test]
fn test_init_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();
    store.init().unwrap(); // second call should not error
}
```

- [ ] **Step 2: Test store and retrieve simple email**

```rust
use inboxly_core::{AccountId, EmailFlags};
use uuid::Uuid;

#[test]
fn test_store_new_and_list() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    let eml = include_bytes!("fixtures/simple.eml");
    let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();

    assert!(!stored.id.is_empty());

    let messages = store.list_messages(&StandardFolder::Inbox).unwrap();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].is_new);
    assert_eq!(messages[0].id, stored.id);
}
```

- [ ] **Step 3: Test deliver (new -> cur) and flag operations**

```rust
#[test]
fn test_deliver_moves_to_cur() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    let eml = include_bytes!("fixtures/simple.eml");
    let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();

    // Deliver with read flag
    let flags = EmailFlags { read: true, ..Default::default() };
    store.deliver(&StandardFolder::Inbox, &stored.id, &flags).unwrap();

    let messages = store.list_messages(&StandardFolder::Inbox).unwrap();
    assert_eq!(messages.len(), 1);
    assert!(!messages[0].is_new); // now in cur/
    assert!(messages[0].flags.read);
}

#[test]
fn test_set_flags() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    let eml = include_bytes!("fixtures/simple.eml");
    let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();
    store.deliver_unread(&StandardFolder::Inbox, &stored.id).unwrap();

    // Set starred + read
    let flags = EmailFlags { read: true, starred: true, ..Default::default() };
    store.set_flags(&StandardFolder::Inbox, &stored.id, &flags).unwrap();

    let messages = store.list_messages(&StandardFolder::Inbox).unwrap();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].flags.read);
    assert!(messages[0].flags.starred);
}
```

- [ ] **Step 4: Test parse_email_meta with fixture emails**

```rust
use inboxly_store::maildir_store::parse_email_meta;
use std::path::PathBuf;

#[test]
fn test_parse_simple_email_meta() {
    let data = include_bytes!("fixtures/simple.eml");
    let meta = parse_email_meta(
        data,
        AccountId(Uuid::nil()),
        PathBuf::from("/tmp/test.eml"),
        EmailFlags::default(),
        42,
        "INBOX".to_string(),
    ).unwrap();

    assert_eq!(meta.id.0, "msg001@example.com");
    assert_eq!(meta.from.name, "Alice Smith");
    assert_eq!(meta.from.address, "alice@example.com");
    assert_eq!(meta.to.len(), 1);
    assert_eq!(meta.to[0].address, "bob@example.com");
    assert_eq!(meta.subject, "Hello from Inboxly");
    assert!(meta.snippet.contains("test email"));
    assert_eq!(meta.imap_uid, 42);
    assert!(!meta.has_attachments);
}

#[test]
fn test_parse_multipart_email_meta() {
    let data = include_bytes!("fixtures/multipart.eml");
    let meta = parse_email_meta(
        data,
        AccountId(Uuid::nil()),
        PathBuf::from("/tmp/test.eml"),
        EmailFlags::default(),
        0,
        "INBOX".to_string(),
    ).unwrap();

    assert_eq!(meta.id.0, "msg002@example.com");
    assert!(meta.snippet.contains("Weekly digest"));
    assert!(!meta.has_attachments);
}

#[test]
fn test_parse_email_with_attachment() {
    let data = include_bytes!("fixtures/with_attachment.eml");
    let meta = parse_email_meta(
        data,
        AccountId(Uuid::nil()),
        PathBuf::from("/tmp/test.eml"),
        EmailFlags::default(),
        0,
        "INBOX".to_string(),
    ).unwrap();

    assert_eq!(meta.id.0, "msg003@example.com");
    assert!(meta.has_attachments);
    assert_eq!(meta.attachments.len(), 1);
    assert_eq!(meta.attachments[0].name, "report.pdf");
    assert_eq!(meta.attachments[0].mime_type, "application/pdf");
}

#[test]
fn test_parse_reply_has_references() {
    let data = include_bytes!("fixtures/reply_in_thread.eml");
    let meta = parse_email_meta(
        data,
        AccountId(Uuid::nil()),
        PathBuf::from("/tmp/test.eml"),
        EmailFlags::default(),
        0,
        "INBOX".to_string(),
    ).unwrap();

    assert_eq!(meta.id.0, "msg004@example.com");
    assert_eq!(meta.in_reply_to.as_deref(), Some("msg001@example.com"));
    assert!(meta.references_json.as_ref().unwrap().contains(&"msg001@example.com".to_string()));
}
```

- [ ] **Step 5: Test read_email_content (lazy load path)**

```rust
#[test]
fn test_read_email_content() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    let eml = include_bytes!("fixtures/with_attachment.eml");
    let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();
    store.deliver_unread(&StandardFolder::Inbox, &stored.id).unwrap();

    // Get the path from listing
    let messages = store.list_messages(&StandardFolder::Inbox).unwrap();
    let content = store.read_email_content(&messages[0].path).unwrap();

    assert!(content.body_text.unwrap().contains("document attached"));
    assert_eq!(content.attachments.len(), 1);
    assert_eq!(content.attachments[0].name, "report.pdf");
    assert!(!content.attachments[0].content.is_empty());
}
```

- [ ] **Step 6: Test delete and move operations**

```rust
#[test]
fn test_delete_message() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    let eml = include_bytes!("fixtures/simple.eml");
    let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();
    store.deliver_unread(&StandardFolder::Inbox, &stored.id).unwrap();

    assert_eq!(store.count_messages(&StandardFolder::Inbox), 1);

    store.delete_message(&StandardFolder::Inbox, &stored.id).unwrap();

    assert_eq!(store.count_messages(&StandardFolder::Inbox), 0);
}

#[test]
fn test_move_between_folders() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    let eml = include_bytes!("fixtures/simple.eml");
    let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();
    store.deliver_unread(&StandardFolder::Inbox, &stored.id).unwrap();

    // Move from Inbox to Trash
    store.move_message(&StandardFolder::Inbox, &StandardFolder::Trash, &stored.id).unwrap();

    assert_eq!(store.count_messages(&StandardFolder::Inbox), 0);
    assert_eq!(store.count_messages(&StandardFolder::Trash), 1);
}
```

- [ ] **Step 7: Test Maildir scan for rebuild**

```rust
#[test]
fn test_scan_folder() {
    let tmp = TempDir::new().unwrap();
    let store = MaildirStore::new(tmp.path());
    store.init().unwrap();

    // Store several emails
    let fixtures: &[&[u8]] = &[
        include_bytes!("fixtures/simple.eml"),
        include_bytes!("fixtures/multipart.eml"),
        include_bytes!("fixtures/with_attachment.eml"),
        include_bytes!("fixtures/reply_in_thread.eml"),
    ];

    for eml in fixtures {
        let stored = store.store_new(&StandardFolder::Inbox, eml).unwrap();
        store.deliver_unread(&StandardFolder::Inbox, &stored.id).unwrap();
    }

    let (metas, errors) = store.scan_folder(&StandardFolder::Inbox, AccountId(Uuid::nil()));

    assert!(errors.is_empty(), "scan errors: {:?}", errors);
    assert_eq!(metas.len(), 4);

    // Verify all unique Message-IDs were parsed
    let ids: Vec<&str> = metas.iter().map(|m| m.id.0.as_str()).collect();
    assert!(ids.contains(&"msg001@example.com"));
    assert!(ids.contains(&"msg002@example.com"));
    assert!(ids.contains(&"msg003@example.com"));
    assert!(ids.contains(&"msg004@example.com"));
}
```

- [ ] **Step 8: Run all tests**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store
```

---

## Task 14: Clippy clean + final commit

**Files:**
- All modified files

- [ ] **Step 1: Run clippy**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo clippy -p inboxly-store -- -D warnings
```

Fix any warnings.

- [ ] **Step 2: Run full test suite**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test --workspace
```

- [ ] **Step 3: Commit**

```bash
git add inboxly-store/Cargo.toml inboxly-store/src/maildir_store.rs inboxly-store/src/lib.rs inboxly-store/src/error.rs inboxly-store/tests/
git commit -m "feat: Maildir++ operations — store, parse, flags, scan, rebuild (M4)

Implements complete Maildir++ read/write layer in inboxly-store:
- Directory initialization (INBOX, .Sent, .Drafts, .Trash, .Spam)
- Atomic email storage (tmp → new → cur delivery flow)
- IMAP flag ↔ Maildir filename suffix encoding/decoding
- .eml parsing to EmailMeta (lightweight) and EmailContent (lazy)
- MIME tree traversal for snippet extraction and attachment metadata
- Message listing, deletion, move, and copy between folders
- Full Maildir scan for SQLite disaster recovery rebuild
- Integration tests with fixture .eml files"
```

---

## File Summary

| File | Action | Purpose |
|------|--------|---------|
| `inboxly-store/Cargo.toml` | Modify | Add `maildir`, `mailparse`, `tempfile` deps |
| `inboxly-store/src/lib.rs` | Modify | Add `pub mod maildir_store;` |
| `inboxly-store/src/error.rs` | Modify | Add `Maildir` and `Parse` error variants |
| `inboxly-store/src/maildir_store.rs` | Create | All Maildir++ operations |
| `inboxly-store/tests/fixtures/simple.eml` | Create | Plain text test email |
| `inboxly-store/tests/fixtures/multipart.eml` | Create | Multipart alternative test email |
| `inboxly-store/tests/fixtures/with_attachment.eml` | Create | Email with PDF attachment |
| `inboxly-store/tests/fixtures/reply_in_thread.eml` | Create | Reply with References/In-Reply-To |
| `inboxly-store/tests/maildir_integration.rs` | Create | Integration tests |

## Key Design Decisions

1. **`maildir` crate for directory ops** — Handles atomic tmp→new writes, filename-based flag encoding, and directory listing. We avoid reimplementing Maildir filename uniqueness (timestamp.pid.hostname format) since the crate handles it correctly.

2. **`mailparse` for MIME parsing** — Pure Rust, zero-copy where possible. `parse_mail()` returns a tree of `ParsedMail` nodes that we walk for both lightweight metadata extraction and full content loading.

3. **Two-tier parsing** — `parse_email_meta()` extracts only envelope data (fast, runs on every sync). `read_email_content()` parses full body + attachments (lazy, runs when user opens an email). This matches the `EmailMeta` / `EmailContent` split from the design spec.

4. **Graceful scan errors** — `scan_folder()` returns `(Vec<EmailMeta>, Vec<ScanError>)` instead of failing on the first corrupt file. A single broken `.eml` should not prevent disaster recovery.

5. **Thread ID placeholder** — `parse_email_meta()` sets `thread_id` to nil UUID. The threading algorithm (M10) assigns real thread IDs by analyzing Message-ID/References/In-Reply-To chains. This keeps M4 independent of M10.

6. **`StandardFolder` enum** — Matches the 5 IMAP folders from the design spec. The `from_imap_name()` method handles Gmail's non-standard folder names (`[Gmail]/Sent Mail` etc.) for transparent mapping during sync.
