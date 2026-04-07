// Task 2: Standard Maildir folders and initialization
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use maildir::Maildir;
use mailparse::{DispositionType, MailAddr, MailHeaderMap, addrparse, dateparse, parse_mail};

use inboxly_core::{
    AccountId, Attachment, AttachmentMeta, Contact, EmailContent, EmailFlags, EmailId, EmailMeta,
    SlimEmailContent, ThreadId,
};

use crate::error::StoreError;

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
            Self::Inbox => ".",
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
        StandardFolder::from_imap_name(imap_folder).map(|f| self.maildir_for(&f))
    }
}

// Task 3: IMAP flag encoding/decoding

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

/// Decode a Maildir flag suffix string into EmailFlags.
/// Input is the characters after `:2,` in the filename.
///
/// Example: "FS" → starred=true, read=true
pub fn suffix_to_flags(suffix: &str) -> EmailFlags {
    EmailFlags {
        read: suffix.contains('S'),
        starred: suffix.contains('F'),
        answered: suffix.contains('R'),
        draft: suffix.contains('D'),
    }
}

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

// Task 4: Write email bytes to Maildir (atomic tmp -> new)

/// Result of storing an email to Maildir.
/// Contains the unique Maildir ID that can be used for subsequent operations.
pub struct StoredEmail {
    /// Unique Maildir message ID (the filename stem before `:2,`).
    pub id: String,
    /// Full path to the stored file on disk.
    pub path: PathBuf,
}

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
        let id = md
            .store_new(data)
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
        let id = md
            .store_cur_with_flags(data, &suffix)
            .map_err(|e| StoreError::Maildir(format!("store_cur failed: {e}")))?;

        // The file is now in <folder>/cur/<id>:2,<flags>
        let filename = format!("{id}:2,{suffix}");
        let path = match folder {
            StandardFolder::Inbox => self.root.join("cur").join(&filename),
            other => self.root.join(other.dirname()).join("cur").join(&filename),
        };

        Ok(StoredEmail { id, path })
    }
}

// Task 5: Deliver message (move new -> cur with flags)

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
            .map_err(|e| {
                StoreError::Maildir(format!(
                    "deliver (move new->cur) failed for {maildir_id}: {e}"
                ))
            })
    }

    /// Deliver a message with no flags set (unread, unflagged).
    pub fn deliver_unread(
        &self,
        folder: &StandardFolder,
        maildir_id: &str,
    ) -> Result<(), StoreError> {
        let md = self.maildir_for(folder);
        md.move_new_to_cur(maildir_id).map_err(|e| {
            StoreError::Maildir(format!(
                "deliver (move new->cur) failed for {maildir_id}: {e}"
            ))
        })
    }
}

// Task 6: Update flags (rename file with new suffix)

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
            .map_err(|e| StoreError::Maildir(format!("set_flags failed for {maildir_id}: {e}")))
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
            .map_err(|e| StoreError::Maildir(format!("add_flags failed for {maildir_id}: {e}")))
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
            .map_err(|e| StoreError::Maildir(format!("remove_flags failed for {maildir_id}: {e}")))
    }
}

// Task 7: Parse .eml file to EmailMeta (lightweight metadata extraction)

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
    let parsed =
        parse_mail(data).map_err(|e| StoreError::Parse(format!("Failed to parse email: {e}")))?;

    let headers = &parsed.headers;

    // Message-ID → EmailId
    let message_id = headers
        .get_first_value("Message-ID")
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
    let from = headers
        .get_first_value("From")
        .and_then(|v| parse_contact_first(&v))
        .unwrap_or_else(|| Contact {
            name: String::new(),
            address: String::new(),
        });

    // To
    let to = headers
        .get_first_value("To")
        .map(|v| parse_contacts(&v))
        .unwrap_or_default();

    // Cc
    let cc = headers
        .get_first_value("Cc")
        .map(|v| parse_contacts(&v))
        .unwrap_or_default();

    // Subject
    let subject = headers.get_first_value("Subject").unwrap_or_default();

    // Date
    let date = headers
        .get_first_value("Date")
        .and_then(|v| dateparse(&v).ok())
        .and_then(|ts| Utc.timestamp_opt(ts, 0).single())
        .unwrap_or_else(Utc::now);

    // Snippet — first ~200 chars of plaintext body
    let snippet = extract_snippet(&parsed, 200);

    // Attachment metadata — walk MIME tree, collect non-inline parts
    let attachments = extract_attachment_meta(&parsed);

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
    })
}

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
    let Ok(addrs) = addrparse(value) else {
        return vec![];
    };
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

/// Extract the first `max_len` characters of plaintext body for use as snippet.
/// Walks the MIME tree to find `text/plain` parts, falling back to stripping
/// HTML from `text/html` parts if no plaintext is available.
fn extract_snippet(parsed: &mailparse::ParsedMail, max_len: usize) -> String {
    // Try text/plain first
    if let Some(text) = find_body_text(parsed) {
        let trimmed = text.trim().to_string();
        if trimmed.len() <= max_len {
            return trimmed;
        }
        let truncated = &trimmed[..max_len];
        if let Some(last_space) = truncated.rfind(' ') {
            return format!("{}...", &truncated[..last_space]);
        }
        return format!("{truncated}...");
    }

    // Fallback: strip HTML tags
    if let Some(html) = find_body_html(parsed) {
        let stripped = strip_html_basic(&html);
        let trimmed = stripped.trim().to_string();
        if trimmed.len() <= max_len {
            return trimmed;
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
fn find_body_text(parsed: &mailparse::ParsedMail) -> Option<String> {
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
fn find_body_html(parsed: &mailparse::ParsedMail) -> Option<String> {
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
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Walk the MIME tree and collect attachment metadata (name, MIME type, size).
/// Does NOT load attachment content — only metadata for the `EmailMeta.attachments` field.
fn extract_attachment_meta(parsed: &mailparse::ParsedMail) -> Vec<AttachmentMeta> {
    let mut attachments = Vec::new();
    collect_attachment_meta(parsed, &mut attachments);
    attachments
}

fn collect_attachment_meta(parsed: &mailparse::ParsedMail, out: &mut Vec<AttachmentMeta>) {
    let disposition = parsed.get_content_disposition();

    let is_attachment = disposition.disposition == DispositionType::Attachment
        || (disposition.disposition != DispositionType::Inline
            && !parsed.ctype.mimetype.starts_with("text/")
            && !parsed.ctype.mimetype.starts_with("multipart/"));

    if is_attachment {
        let filename = disposition
            .params
            .get("filename")
            .or_else(|| parsed.ctype.params.get("name"))
            .cloned()
            .unwrap_or_else(|| "unnamed".to_string());

        let size_bytes = parsed.get_body_raw().map(|b| b.len() as u64).unwrap_or(0);

        out.push(AttachmentMeta {
            filename,
            mime_type: parsed.ctype.mimetype.clone(),
            size_bytes,
        });
    }

    for sub in &parsed.subparts {
        collect_attachment_meta(sub, out);
    }
}

// Task 8: Read full EmailContent from .eml (lazy load)

impl MaildirStore {
    /// Read and parse full email content from a Maildir file path.
    /// This is the lazy-load path — called when the user opens an email
    /// in the conversation view. The `maildir_path` comes from `EmailMeta`.
    ///
    /// Returns body text, body HTML, all headers, and full attachment content.
    pub fn read_email_content(&self, maildir_path: &Path) -> Result<EmailContent, StoreError> {
        let data = std::fs::read(maildir_path).map_err(|e| {
            StoreError::Maildir(format!("Failed to read {}: {e}", maildir_path.display()))
        })?;

        parse_email_content(&data)
    }

    /// Read and parse SLIM email content from a Maildir file path.
    /// Returns body text, body HTML, and attachment metadata only —
    /// skips headers and attachment byte content. Used by the
    /// thread detail view (eng review Issue 2.6). Callers that need
    /// the full EmailContent should use `read_email_content()` instead.
    pub fn read_email_slim(&self, maildir_path: &Path) -> Result<SlimEmailContent, StoreError> {
        let data = std::fs::read(maildir_path).map_err(|e| {
            StoreError::Maildir(format!("Failed to read {}: {e}", maildir_path.display()))
        })?;
        parse_email_slim(&data)
    }
}

/// Parse raw .eml bytes into full EmailContent.
/// Extracts body (text + HTML), all headers, and full attachment content.
pub fn parse_email_content(data: &[u8]) -> Result<EmailContent, StoreError> {
    let parsed =
        parse_mail(data).map_err(|e| StoreError::Parse(format!("Failed to parse email: {e}")))?;

    // Extract Message-ID for the EmailId
    let message_id = parsed
        .headers
        .get_first_value("Message-ID")
        .unwrap_or_default()
        .trim_matches(|c| c == '<' || c == '>')
        .to_string();
    let id = EmailId(message_id);

    // Body text and HTML
    let body_text = find_body_text(&parsed);
    let body_html = find_body_html(&parsed);

    // All headers as HashMap
    let mut headers = std::collections::HashMap::new();
    for header in &parsed.headers {
        let key = header.get_key();
        let value = header.get_value();
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

/// Parse raw .eml bytes into a slim email content struct.
/// Extracts body (text + HTML) and attachment metadata only.
pub fn parse_email_slim(data: &[u8]) -> Result<SlimEmailContent, StoreError> {
    let parsed =
        parse_mail(data).map_err(|e| StoreError::Parse(format!("Failed to parse email: {e}")))?;

    let message_id = parsed
        .headers
        .get_first_value("Message-ID")
        .unwrap_or_default()
        .trim_matches(|c| c == '<' || c == '>')
        .to_string();
    let id = EmailId(message_id);

    let body_text = find_body_text(&parsed);
    let body_html = find_body_html(&parsed);

    // Use the existing `collect_attachment_meta` helper (already
    // defined in this file for the EmailMeta build path), which
    // walks the MIME tree and returns only metadata, no byte content.
    let mut attachments = Vec::new();
    collect_attachment_meta(&parsed, &mut attachments);

    Ok(SlimEmailContent {
        id,
        body_text,
        body_html,
        attachments,
    })
}

/// Walk the MIME tree and collect full attachments (metadata + content bytes).
fn extract_full_attachments(parsed: &mailparse::ParsedMail) -> Vec<Attachment> {
    let mut attachments = Vec::new();
    collect_full_attachments(parsed, &mut attachments);
    attachments
}

fn collect_full_attachments(parsed: &mailparse::ParsedMail, out: &mut Vec<Attachment>) {
    let disposition = parsed.get_content_disposition();

    let is_attachment = disposition.disposition == DispositionType::Attachment
        || (disposition.disposition != DispositionType::Inline
            && !parsed.ctype.mimetype.starts_with("text/")
            && !parsed.ctype.mimetype.starts_with("multipart/"));

    if is_attachment {
        let filename = disposition
            .params
            .get("filename")
            .or_else(|| parsed.ctype.params.get("name"))
            .cloned()
            .unwrap_or_else(|| "unnamed".to_string());

        if let Ok(content) = parsed.get_body_raw() {
            let size_bytes = content.len() as u64;
            out.push(Attachment {
                meta: AttachmentMeta {
                    filename,
                    mime_type: parsed.ctype.mimetype.clone(),
                    size_bytes,
                },
                content,
            });
        }
    }

    for sub in &parsed.subparts {
        collect_full_attachments(sub, out);
    }
}

// Task 9: List all messages in a folder

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

impl MaildirStore {
    /// List all messages in a folder (both `new/` and `cur/`).
    /// Returns lightweight entries with ID, path, and flags — no parsing.
    /// Messages in `new/` have `is_new = true`.
    pub fn list_messages(&self, folder: &StandardFolder) -> Result<Vec<MaildirEntry>, StoreError> {
        let md = self.maildir_for(folder);
        let mut entries = Vec::new();

        // List new/ messages
        for entry in md.list_new() {
            let entry = entry.map_err(|e| {
                StoreError::Maildir(format!("Failed to list new/ in {:?}: {e}", folder))
            })?;
            let id = entry.id().to_string();
            let path = entry.path().clone();
            let flags = suffix_to_flags(entry.flags());
            entries.push(MaildirEntry {
                id,
                path,
                flags,
                is_new: true,
            });
        }

        // List cur/ messages
        for entry in md.list_cur() {
            let entry = entry.map_err(|e| {
                StoreError::Maildir(format!("Failed to list cur/ in {:?}: {e}", folder))
            })?;
            let id = entry.id().to_string();
            let path = entry.path().clone();
            let flags = suffix_to_flags(entry.flags());
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

// Task 10: Delete message from Maildir

impl MaildirStore {
    /// Delete a message from a folder by its Maildir ID.
    /// Removes the file from disk (searches both `new/` and `cur/`).
    pub fn delete_message(
        &self,
        folder: &StandardFolder,
        maildir_id: &str,
    ) -> Result<(), StoreError> {
        let md = self.maildir_for(folder);
        md.delete(maildir_id).map_err(|e| {
            StoreError::Maildir(format!(
                "delete failed for {maildir_id} in {:?}: {e}",
                folder
            ))
        })
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
        from_md.move_to(maildir_id, &to_md).map_err(|e| {
            StoreError::Maildir(format!(
                "move failed for {maildir_id} from {:?} to {:?}: {e}",
                from_folder, to_folder
            ))
        })
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
        from_md.copy_to(maildir_id, &to_md).map_err(|e| {
            StoreError::Maildir(format!(
                "copy failed for {maildir_id} from {:?} to {:?}: {e}",
                from_folder, to_folder
            ))
        })
    }
}

// Task 11: Rebuild SQLite from Maildir (disaster recovery)

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
                account_id,
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
    pub fn scan_all(&self, account_id: AccountId) -> (Vec<EmailMeta>, Vec<ScanError>) {
        let mut all_metas = Vec::new();
        let mut all_errors = Vec::new();

        for folder in StandardFolder::all() {
            let (metas, errors) = self.scan_folder(folder, account_id);
            all_metas.extend(metas);
            all_errors.extend(errors);
        }

        (all_metas, all_errors)
    }
}

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
    sqlite: &crate::Store,
    account_id: AccountId,
) -> Result<(usize, Vec<ScanError>), StoreError> {
    let (metas, errors) = maildir.scan_all(account_id);

    // Clear existing emails for this account (rebuild = drop all and re-insert)
    // We rebuild via the Store::rebuild() method which recreates all tables,
    // then re-insert only the emails we scanned.
    // For per-account rebuild, we delete by account_id.
    if let Some(first) = metas.first() {
        let account_id_str = first.account_id.to_string();
        sqlite
            .conn()
            .execute(
                "DELETE FROM emails WHERE account_id = ?1",
                rusqlite::params![account_id_str],
            )
            .map_err(StoreError::Sqlite)?;
    }

    // Insert all recovered emails
    let count = metas.len();
    for meta in &metas {
        // Convert EmailMeta to EmailRow for insertion
        let row = crate::EmailRow {
            id: meta.id.0.clone(),
            account_id: meta.account_id.to_string(),
            thread_id: meta.thread_id.to_string(),
            from_name: if meta.from.name.is_empty() {
                None
            } else {
                Some(meta.from.name.clone())
            },
            from_address: meta.from.address.clone(),
            to_json: serde_json::to_string(&meta.to).unwrap_or_default(),
            cc_json: serde_json::to_string(&meta.cc).unwrap_or_default(),
            subject: meta.subject.clone(),
            snippet: meta.snippet.clone(),
            date: meta.date.timestamp(),
            maildir_path: meta.maildir_path.to_string_lossy().into_owned(),
            flags: meta.flags.to_bitmask() as i64,
            size_bytes: meta.size_bytes as i64,
            imap_uid: meta.imap_uid as i64,
            imap_folder: meta.imap_folder.clone(),
            has_attachments: !meta.attachments.is_empty(),
            body_downloaded: true, // Rebuilding from Maildir means body is on disk
            message_id_header: None,
            in_reply_to: None,
            references_json: None,
        };
        sqlite.insert_email(&row)?;
    }

    Ok((count, errors))
}

// M35b Phase 4: Message-ID dedup lookup for the compose draft sync path.

impl MaildirStore {
    /// Return the on-disk path of any message in `folder` whose `Message-ID:`
    /// header matches `message_id`, or `Ok(None)` if no match is found.
    ///
    /// Used by the M35b draft sync dedup path (Phase 4b) to avoid downloading
    /// an IMAP Drafts/Sent folder entry that was already written locally via
    /// the compose-time Maildir save. The IMAP body processor calls this
    /// helper to discover the existing file's path so it can point the
    /// `emails` row at it (via `mark_body_downloaded`) without writing a
    /// duplicate `.eml`.
    ///
    /// Walks the folder's `cur/` and `new/` entries and parses each one's
    /// headers via `mailparse` until a match is found or the folder is
    /// exhausted.
    ///
    /// `message_id` may be passed with or without surrounding angle brackets
    /// (e.g. both `"<uuid@inboxly.local>"` and `"uuid@inboxly.local"` work) —
    /// the helper normalises both sides by trimming `<` and `>` before
    /// comparison.
    ///
    /// # Behaviour on missing folders
    ///
    /// Returns `Ok(None)` (NOT `Err`) if the folder's underlying directory
    /// does not yet exist on disk. This is the expected state for a fresh
    /// account that has never had a draft synced. The `maildir` crate's
    /// `list_cur` / `list_new` iterators yield zero entries (rather than
    /// erroring) when the directory is missing, so the loop simply
    /// completes with no match.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Maildir`] if a Maildir entry can be enumerated
    /// but its underlying file metadata operation fails (e.g. permission
    /// denied during the directory walk). Per-file read or parse failures
    /// for individual `.eml` files are SKIPPED — a single corrupt file must
    /// not prevent the dedup check from completing.
    ///
    /// # Performance
    ///
    /// This is an O(n) linear scan with one `read` + `parse_mail` per entry.
    /// Drafts folders are typically tiny (<100 entries) so this is acceptable
    /// for M35b. A future optimisation can maintain an in-memory Message-ID
    /// index alongside the Maildir.
    pub fn find_message_id(
        &self,
        folder: StandardFolder,
        message_id: &str,
    ) -> Result<Option<std::path::PathBuf>, StoreError> {
        let normalized_target = message_id.trim_matches(|c| c == '<' || c == '>');
        if normalized_target.is_empty() {
            // An empty needle would match every parsed message that lacks a
            // Message-ID header (because we'd compare "" == ""). Bail early.
            return Ok(None);
        }

        let md = self.maildir_for(&folder);

        // Chain new/ and cur/ — drafts may live in either depending on
        // whether they've been "delivered" (moved from new -> cur) yet.
        for entry in md.list_new().chain(md.list_cur()) {
            let entry = entry.map_err(|e| {
                StoreError::Maildir(format!(
                    "find_message_id: failed to enumerate {:?}: {e}",
                    folder
                ))
            })?;

            let path = entry.path().to_path_buf();
            let data = match std::fs::read(&path) {
                Ok(d) => d,
                Err(_) => continue, // skip unreadable files
            };

            let parsed = match parse_mail(&data) {
                Ok(p) => p,
                Err(_) => continue, // skip unparseable files
            };

            let existing = parsed
                .headers
                .get_first_value("Message-ID")
                .unwrap_or_default();
            let existing_normalized = existing.trim().trim_matches(|c| c == '<' || c == '>');

            if existing_normalized == normalized_target {
                return Ok(Some(path));
            }
        }

        Ok(None)
    }

    /// Return `true` if any message in `folder` has a `Message-ID:` header
    /// matching `message_id`.
    ///
    /// Thin wrapper around [`MaildirStore::find_message_id`] for callers that
    /// only need a yes/no answer. Prefer `find_message_id` when you need the
    /// matched path (e.g. to point an `emails` row at it via
    /// `mark_body_downloaded`).
    ///
    /// Normalisation, missing-folder behaviour, errors, and performance are
    /// identical to `find_message_id` — see its docs for details.
    ///
    /// # Errors
    ///
    /// Same as [`MaildirStore::find_message_id`]: propagates
    /// [`StoreError::Maildir`] from a failed directory enumeration.
    pub fn has_message_id(
        &self,
        folder: StandardFolder,
        message_id: &str,
    ) -> Result<bool, StoreError> {
        self.find_message_id(folder, message_id)
            .map(|opt| opt.is_some())
    }
}

// Flag encoding/decoding tests (Task 3)
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
        let flags = EmailFlags {
            read: true,
            starred: true,
            answered: true,
            draft: true,
        };
        let suffix = flags_to_suffix(&flags);
        assert_eq!(suffix, "DFRS");
        assert_eq!(suffix_to_flags(&suffix), flags);
    }

    #[test]
    fn test_flags_roundtrip_read_only() {
        let flags = EmailFlags {
            read: true,
            starred: false,
            answered: false,
            draft: false,
        };
        let suffix = flags_to_suffix(&flags);
        assert_eq!(suffix, "S");
        assert_eq!(suffix_to_flags(&suffix), flags);
    }

    #[test]
    fn test_flags_from_filename_with_info() {
        let flags = flags_from_filename("1710000000.12345.hostname:2,FS");
        assert_eq!(
            flags,
            EmailFlags {
                read: true,
                starred: true,
                answered: false,
                draft: false
            }
        );
    }

    #[test]
    fn test_flags_from_filename_no_info() {
        let flags = flags_from_filename("1710000000.12345.hostname");
        assert_eq!(flags, EmailFlags::default());
    }

    #[test]
    fn parse_email_slim_strips_headers_and_attachment_bytes() {
        let eml = b"From: alice@example.com\r\n\
                    To: bob@example.com\r\n\
                    Subject: Test\r\n\
                    Message-ID: <test@ex.com>\r\n\
                    X-Spam-Score: 0.1\r\n\
                    \r\n\
                    Hello world";
        let slim = parse_email_slim(eml).expect("parse");
        assert_eq!(slim.body_text.as_deref(), Some("Hello world"));
        // SlimEmailContent has NO headers field — if this test compiles,
        // that's half the assertion. The other half: no way to access
        // header data means we never carry it.
        assert!(slim.attachments.is_empty());
    }
}
