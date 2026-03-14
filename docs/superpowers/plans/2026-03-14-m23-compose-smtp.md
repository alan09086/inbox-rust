# M23: Compose + SMTP + Drafts — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Deliver full email composition — compose new, reply, reply-all, forward — with Markdown body editing, HTML generation on send, SMTP delivery, draft auto-save, attachment support, and offline queuing.

**Architecture:** Compose touches two crates. `inboxly-imap` gains SMTP send + IMAP APPEND (Sent folder save). `inboxly-ui` gains the `ComposeView` widget, Markdown preview, attachment picker, and draft auto-save timer. The compose state machine runs in the UI, dispatching send/save-draft actions to the backend via the existing `tokio::sync::mpsc` channel.

**Tech Stack:** Rust, iced, lettre (SMTP + message building), pulldown-cmark (Markdown to HTML), rfd (file dialog), tokio

**Prerequisites:**
- **M15** complete — Iced shell + nav drawer (application scaffold exists, `InboxlyApp` struct, `Message` enum, view routing, nav drawer)
- **M6** complete — IMAP connection + auth (`ImapConnection` struct, TLS, credential handling, `AccountConfig` with smtp_host/smtp_port)
- **M4** complete — Maildir operations (write to `.Drafts/`, `.Sent/` subdirectories, Maildir filename conventions with `:2,D` draft flag)
- **M11** complete — Contacts + avatar system (contacts table populated from From headers — used for address autocomplete)
- **M17** complete — Inbox feed + email rows (conversation view exists for reply/forward context)

**Spec references:**
- ComposeView widget table (Custom Widgets section)
- Compose dimensions: max width 920dp, from row 56dp, contacts row 56dp
- Typography: compose subject 18sp bold, compose body 16sp normal
- Offline Behaviour: "Compose offline → saved as draft in Maildir, sent on reconnect"
- `offline_queue` table: id, action, payload_json, created_at
- IMAP Sync Engine: "Outbound mail sent via SMTP (lettre), saved to Maildir + Sent folder via IMAP APPEND"

---

## Task Overview

| # | Task | Crate | Est. |
|---|------|-------|------|
| 1 | Add lettre, pulldown-cmark, rfd workspace dependencies | workspace | 3 min |
| 2 | Define `ComposeData` model in `inboxly-core` | `core` | 10 min |
| 3 | Define compose-related messages and actions | `core` | 8 min |
| 4 | Implement SMTP transport in `inboxly-imap` | `imap` | 20 min |
| 5 | Implement IMAP APPEND for Sent folder | `imap` | 15 min |
| 6 | Implement draft Maildir save/load/delete | `store` | 15 min |
| 7 | Implement offline send queue | `store` | 12 min |
| 8 | Implement Markdown-to-HTML conversion module | `core` | 10 min |
| 9 | Build `ComposeView` widget — layout + fields | `ui` | 30 min |
| 10 | Implement address field autocomplete from contacts | `ui` | 20 min |
| 11 | Implement Markdown preview toggle panel | `ui` | 15 min |
| 12 | Implement attachment picker and attachment list | `ui` | 15 min |
| 13 | Implement reply / reply-all / forward pre-fill | `ui` | 20 min |
| 14 | Wire draft auto-save timer (30-second interval) | `ui` | 12 min |
| 15 | Wire send flow: compose → SMTP → IMAP APPEND → close | `ui`, `imap` | 15 min |
| 16 | Wire offline compose: save draft + queue send | `ui`, `store` | 10 min |
| 17 | Hook compose into FAB speed dial and conversation view | `ui` | 10 min |
| 18 | Integration tests | all | 20 min |

---

## Task 1: Add lettre, pulldown-cmark, rfd workspace dependencies

**Files:**
- Modify: `/mnt/TempNVME/projects/inbox-rust/Cargo.toml` (workspace root)
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-imap/Cargo.toml`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-core/Cargo.toml`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/Cargo.toml`

- [ ] **Step 1: Add workspace-level dependency declarations**

In the workspace root `Cargo.toml`, add to `[workspace.dependencies]`:

```toml
lettre = { version = "0.11", default-features = false, features = ["tokio1-rustls-tls", "smtp-transport", "builder"] }
pulldown-cmark = { version = "0.13", default-features = false }
rfd = "0.15"
```

- [ ] **Step 2: Add `lettre` to `inboxly-imap/Cargo.toml`**

```toml
[dependencies]
lettre.workspace = true
```

- [ ] **Step 3: Add `pulldown-cmark` to `inboxly-core/Cargo.toml`**

```toml
[dependencies]
pulldown-cmark.workspace = true
```

The Markdown-to-HTML conversion lives in `core` so it can be tested independently and reused by any frontend crate.

- [ ] **Step 4: Add `rfd` to `inboxly-ui/Cargo.toml`**

```toml
[dependencies]
rfd.workspace = true
```

- [ ] **Step 5: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check --workspace
```

**Commit:** `feat: add lettre, pulldown-cmark, rfd workspace dependencies for M23`

---

## Task 2: Define ComposeData model in inboxly-core

**Files:**
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/compose.rs`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/lib.rs`

The `ComposeData` struct represents the full state of a compose window. It is pure data — no UI types, no Iced dependencies. This is the canonical representation shared between UI, SMTP send, and draft save.

- [ ] **Step 1: Create `compose.rs` with core compose types**

```rust
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{AccountId, Contact, EmailId, ThreadId};

/// The mode that initiated this compose window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComposeMode {
    /// New email, no prior context.
    New,
    /// Reply to a single sender.
    Reply {
        original_email_id: EmailId,
        thread_id: ThreadId,
    },
    /// Reply to all participants.
    ReplyAll {
        original_email_id: EmailId,
        thread_id: ThreadId,
    },
    /// Forward an existing email.
    Forward {
        original_email_id: EmailId,
        thread_id: ThreadId,
    },
}

/// An attachment queued for sending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeAttachment {
    /// Original filename (e.g., "report.pdf").
    pub filename: String,
    /// MIME type (e.g., "application/pdf").
    pub mime_type: String,
    /// Absolute path on disk to the source file.
    pub path: PathBuf,
    /// File size in bytes (for UI display).
    pub size_bytes: u64,
}

/// Full state of a compose window. Pure data — no UI types.
///
/// This struct is the canonical representation shared between:
/// - The UI (ComposeView widget reads/writes this)
/// - SMTP send (builds lettre Message from this)
/// - Draft save (serialized to Maildir .eml)
/// - Offline queue (serialized to JSON in offline_queue table)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeData {
    /// Which account is sending this email.
    pub account_id: AccountId,

    /// How this compose was initiated.
    pub mode: ComposeMode,

    /// Recipients in the To field.
    pub to: Vec<Contact>,

    /// Recipients in the Cc field.
    pub cc: Vec<Contact>,

    /// Recipients in the Bcc field.
    pub bcc: Vec<Contact>,

    /// Email subject line.
    pub subject: String,

    /// Body text in Markdown format (source of truth).
    pub body_markdown: String,

    /// Attachments queued for sending.
    pub attachments: Vec<ComposeAttachment>,

    /// If this is an existing draft, its Maildir path for overwrite.
    pub draft_maildir_path: Option<PathBuf>,

    /// The Message-ID that will be used for this email.
    /// Generated once on compose creation, reused for draft saves.
    pub message_id: String,

    /// In-Reply-To header value (set for Reply/ReplyAll).
    pub in_reply_to: Option<String>,

    /// References header value (set for Reply/ReplyAll/Forward).
    pub references: Option<String>,

    /// Timestamp when this draft was last saved.
    pub last_saved: Option<DateTime<Utc>>,
}

impl ComposeData {
    /// Create a new empty compose for a fresh email.
    pub fn new(account_id: AccountId) -> Self {
        Self {
            account_id,
            mode: ComposeMode::New,
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: String::new(),
            body_markdown: String::new(),
            attachments: Vec::new(),
            draft_maildir_path: None,
            message_id: generate_message_id(),
            in_reply_to: None,
            references: None,
            last_saved: None,
        }
    }

    /// Returns true if the compose has any content worth saving as a draft.
    pub fn has_content(&self) -> bool {
        !self.to.is_empty()
            || !self.cc.is_empty()
            || !self.subject.is_empty()
            || !self.body_markdown.is_empty()
            || !self.attachments.is_empty()
    }

    /// Returns true if the compose has the minimum required fields to send.
    pub fn is_sendable(&self) -> bool {
        !self.to.is_empty() && (!self.subject.is_empty() || !self.body_markdown.is_empty())
    }
}

/// Generate a unique Message-ID for a new email.
fn generate_message_id() -> String {
    let id = uuid::Uuid::new_v4();
    // Format: <uuid@inboxly>
    format!("<{id}@inboxly>")
}
```

- [ ] **Step 2: Add `pub mod compose;` to `inboxly-core/src/lib.rs` and re-export key types**

```rust
pub mod compose;
pub use compose::{ComposeAttachment, ComposeData, ComposeMode};
```

- [ ] **Step 3: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-core
```

**Commit:** `feat(core): add ComposeData model with compose mode, attachments, and draft state`

---

## Task 3: Define compose-related messages and actions

**Files:**
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/compose.rs`

These enums define the communication contract between UI and backend for compose operations. They flow through the existing `tokio::sync::mpsc` channels established in M15.

- [ ] **Step 1: Add `ComposeAction` enum — UI → backend commands**

Append to `compose.rs`:

```rust
/// Actions the UI sends to the backend for compose operations.
/// These flow through the existing UI→backend mpsc channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComposeAction {
    /// Send this email via SMTP, then save to Sent folder via IMAP APPEND.
    Send(ComposeData),

    /// Save current state as a draft to Maildir .Drafts folder.
    SaveDraft(ComposeData),

    /// Delete a previously saved draft.
    DeleteDraft {
        account_id: AccountId,
        maildir_path: PathBuf,
    },

    /// Queue a send for when connectivity is restored (offline mode).
    QueueOfflineSend(ComposeData),
}
```

- [ ] **Step 2: Add `ComposeEvent` enum — backend → UI notifications**

Append to `compose.rs`:

```rust
/// Events the backend sends to the UI for compose operations.
/// These flow through the existing backend→UI mpsc channel.
#[derive(Debug, Clone)]
pub enum ComposeEvent {
    /// Email was sent successfully.
    SendSuccess { message_id: String },

    /// Email send failed.
    SendFailed { message_id: String, error: String },

    /// Draft was saved successfully. Contains the Maildir path for future overwrites.
    DraftSaved { message_id: String, maildir_path: PathBuf },

    /// Draft save failed.
    DraftSaveFailed { message_id: String, error: String },

    /// An offline-queued email was sent on reconnect.
    OfflineQueueFlushed { message_id: String },
}
```

- [ ] **Step 3: Re-export new types from `lib.rs`**

```rust
pub use compose::{ComposeAction, ComposeEvent};
```

- [ ] **Step 4: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-core
```

**Commit:** `feat(core): add ComposeAction and ComposeEvent enums for compose messaging`

---

## Task 4: Implement SMTP transport in inboxly-imap

**Files:**
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-imap/src/smtp.rs`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-imap/src/lib.rs`

The SMTP module wraps `lettre` to send emails. It reads account config for host/port/credentials, builds a `lettre::Message` from `ComposeData`, converts Markdown body to HTML via `inboxly-core`'s conversion, and sends via TLS.

- [ ] **Step 1: Create `smtp.rs` with `SmtpSender` struct**

```rust
use std::path::Path;

use lettre::message::{
    header::ContentType, Attachment, Mailbox, MessageBuilder, MultiPart, SinglePart,
};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use tokio_rustls::rustls;

use inboxly_core::compose::{ComposeAttachment, ComposeData};
use inboxly_core::config::AccountConfig;
use inboxly_core::markdown::markdown_to_html;
use inboxly_core::Contact;

/// SMTP sender — one instance per account.
pub struct SmtpSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from_mailbox: Mailbox,
}

impl SmtpSender {
    /// Create a new SMTP sender from account configuration.
    ///
    /// Establishes TLS connection to the SMTP server. The transport
    /// is reusable for multiple sends within a session.
    pub async fn connect(
        config: &AccountConfig,
        password: &str,
    ) -> Result<Self, SmtpError> {
        let creds = Credentials::new(config.email.clone(), password.to_string());

        let transport = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host)?
            .port(config.smtp_port)
            .credentials(creds)
            .build();

        let from_mailbox = Mailbox::new(
            Some(config.display_name.clone()),
            config.email.parse().map_err(|e| SmtpError::InvalidAddress {
                address: config.email.clone(),
                source: e,
            })?,
        );

        Ok(Self {
            transport,
            from_mailbox,
        })
    }

    /// Send an email from ComposeData.
    ///
    /// Builds a MIME message with:
    /// - multipart/alternative (text/plain from Markdown source + text/html from rendered Markdown)
    /// - multipart/mixed wrapper if attachments are present
    /// - In-Reply-To and References headers for replies
    ///
    /// Returns the raw RFC 2822 bytes of the sent message (for IMAP APPEND to Sent).
    pub async fn send(&self, compose: &ComposeData) -> Result<Vec<u8>, SmtpError> {
        let message = self.build_message(compose)?;
        let raw_message = message.formatted();

        self.transport
            .send(message)
            .await
            .map_err(SmtpError::SendFailed)?;

        Ok(raw_message)
    }

    /// Build a lettre::Message from ComposeData without sending.
    /// Useful for draft serialization and testing.
    fn build_message(&self, compose: &ComposeData) -> Result<Message, SmtpError> {
        let mut builder = Message::builder()
            .from(self.from_mailbox.clone())
            .message_id(Some(compose.message_id.clone()))
            .subject(&compose.subject);

        // Add To recipients
        for contact in &compose.to {
            let mailbox = contact_to_mailbox(contact)?;
            builder = builder.to(mailbox);
        }

        // Add Cc recipients
        for contact in &compose.cc {
            let mailbox = contact_to_mailbox(contact)?;
            builder = builder.cc(mailbox);
        }

        // Add Bcc recipients
        for contact in &compose.bcc {
            let mailbox = contact_to_mailbox(contact)?;
            builder = builder.bcc(mailbox);
        }

        // Add In-Reply-To header for replies
        if let Some(ref in_reply_to) = compose.in_reply_to {
            builder = builder.in_reply_to(in_reply_to.clone());
        }

        // Add References header for threading
        if let Some(ref references) = compose.references {
            builder = builder.references(references.clone());
        }

        // Build body: multipart/alternative with text + HTML
        let plain_body = SinglePart::builder()
            .header(ContentType::TEXT_PLAIN)
            .body(compose.body_markdown.clone());

        let html_content = markdown_to_html(&compose.body_markdown);
        let html_body = SinglePart::builder()
            .header(ContentType::TEXT_HTML)
            .body(html_content);

        let alternative = MultiPart::alternative()
            .singlepart(plain_body)
            .singlepart(html_body);

        // If attachments exist, wrap in multipart/mixed
        let message = if compose.attachments.is_empty() {
            builder.multipart(alternative)?
        } else {
            let mut mixed = MultiPart::mixed().multipart(alternative);

            for att in &compose.attachments {
                let file_body = std::fs::read(&att.path).map_err(|e| SmtpError::AttachmentRead {
                    path: att.path.clone(),
                    source: e,
                })?;

                let content_type = ContentType::parse(&att.mime_type)
                    .unwrap_or(ContentType::parse("application/octet-stream").unwrap());

                let attachment = Attachment::new(att.filename.clone())
                    .body(file_body, content_type);

                mixed = mixed.singlepart(attachment);
            }

            builder.multipart(mixed)?
        };

        Ok(message)
    }
}

/// Convert an inboxly Contact to a lettre Mailbox.
fn contact_to_mailbox(contact: &Contact) -> Result<Mailbox, SmtpError> {
    let name = if contact.name.is_empty() {
        None
    } else {
        Some(contact.name.clone())
    };

    let address = contact
        .address
        .parse()
        .map_err(|e| SmtpError::InvalidAddress {
            address: contact.address.clone(),
            source: e,
        })?;

    Ok(Mailbox::new(name, address))
}

/// Errors from the SMTP subsystem.
#[derive(Debug, thiserror::Error)]
pub enum SmtpError {
    #[error("failed to connect to SMTP server: {0}")]
    ConnectionFailed(#[from] lettre::transport::smtp::Error),

    #[error("invalid email address '{address}': {source}")]
    InvalidAddress {
        address: String,
        source: lettre::address::AddressError,
    },

    #[error("failed to send email: {0}")]
    SendFailed(lettre::transport::smtp::Error),

    #[error("failed to build email message: {0}")]
    MessageBuild(#[from] lettre::error::Error),

    #[error("failed to read attachment at {path}: {source}")]
    AttachmentRead {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
}
```

- [ ] **Step 2: Add `pub mod smtp;` to `inboxly-imap/src/lib.rs`**

```rust
pub mod smtp;
pub use smtp::SmtpSender;
```

- [ ] **Step 3: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-imap
```

**Commit:** `feat(imap): implement SmtpSender with lettre for email delivery`

---

## Task 5: Implement IMAP APPEND for Sent folder

**Files:**
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-imap/src/append.rs`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-imap/src/lib.rs`

After SMTP send succeeds, the raw message bytes are saved to the Sent folder via IMAP APPEND. This ensures the sent message appears in the user's Sent folder on the server (and will sync back down). Gmail auto-saves to Sent, but generic IMAP servers require explicit APPEND.

- [ ] **Step 1: Create `append.rs` with `imap_append_sent` function**

```rust
use async_imap::types::Flag;

use crate::connection::ImapConnection;

/// Append a raw RFC 2822 message to the Sent folder via IMAP.
///
/// The message is stored with the `\Seen` flag set (user already knows about it).
///
/// # Arguments
/// * `conn` — Active IMAP connection (from M6)
/// * `sent_folder` — The IMAP folder name for Sent (e.g., "Sent", "[Gmail]/Sent Mail")
/// * `raw_message` — The raw RFC 2822 bytes of the message
pub async fn imap_append_sent(
    conn: &mut ImapConnection,
    sent_folder: &str,
    raw_message: &[u8],
) -> Result<(), AppendError> {
    let session = conn.session_mut();

    session
        .append(sent_folder, raw_message)
        .flag(Flag::Seen)
        .finish()
        .await
        .map_err(|e| AppendError::AppendFailed {
            folder: sent_folder.to_string(),
            source: e,
        })?;

    Ok(())
}

/// Append a raw RFC 2822 message to the Drafts folder via IMAP.
///
/// Stored with `\Seen` and `\Draft` flags.
pub async fn imap_append_draft(
    conn: &mut ImapConnection,
    drafts_folder: &str,
    raw_message: &[u8],
) -> Result<(), AppendError> {
    let session = conn.session_mut();

    session
        .append(drafts_folder, raw_message)
        .flag(Flag::Seen)
        .flag(Flag::Draft)
        .finish()
        .await
        .map_err(|e| AppendError::AppendFailed {
            folder: drafts_folder.to_string(),
            source: e,
        })?;

    Ok(())
}

/// Resolve the server-side Sent folder name.
///
/// Uses IMAP LIST with SPECIAL-USE (RFC 6154) to find the folder
/// with `\Sent` attribute. Falls back to common names.
pub async fn resolve_sent_folder(conn: &mut ImapConnection) -> Result<String, AppendError> {
    resolve_special_folder(conn, "\\Sent", &["Sent", "Sent Items", "Sent Mail", "[Gmail]/Sent Mail"])
        .await
}

/// Resolve the server-side Drafts folder name.
pub async fn resolve_drafts_folder(conn: &mut ImapConnection) -> Result<String, AppendError> {
    resolve_special_folder(conn, "\\Drafts", &["Drafts", "[Gmail]/Drafts"])
        .await
}

/// Generic resolver: check SPECIAL-USE attribute first, then fall back to common names.
async fn resolve_special_folder(
    conn: &mut ImapConnection,
    special_use_attr: &str,
    fallback_names: &[&str],
) -> Result<String, AppendError> {
    let session = conn.session_mut();

    // Try LIST with SPECIAL-USE attributes
    let folders = session
        .list(Some(""), Some("*"))
        .await
        .map_err(AppendError::ListFailed)?;

    // Look for the SPECIAL-USE attribute
    for folder in &folders {
        let attrs: Vec<String> = folder
            .attributes()
            .iter()
            .map(|a| format!("{a:?}"))
            .collect();
        if attrs.iter().any(|a| a.contains(special_use_attr)) {
            return Ok(folder.name().to_string());
        }
    }

    // Fall back to common names — check which ones exist
    let folder_names: Vec<&str> = folders.iter().map(|f| f.name()).collect();
    for name in fallback_names {
        if folder_names.iter().any(|f| f.eq_ignore_ascii_case(name)) {
            return Ok(name.to_string());
        }
    }

    // Last resort: use first fallback name and hope for the best
    Ok(fallback_names[0].to_string())
}

/// Errors from IMAP APPEND operations.
#[derive(Debug, thiserror::Error)]
pub enum AppendError {
    #[error("failed to append message to folder '{folder}': {source}")]
    AppendFailed {
        folder: String,
        source: async_imap::error::Error,
    },

    #[error("failed to list IMAP folders: {0}")]
    ListFailed(async_imap::error::Error),
}
```

- [ ] **Step 2: Add `pub mod append;` to `inboxly-imap/src/lib.rs`**

```rust
pub mod append;
```

- [ ] **Step 3: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-imap
```

**Commit:** `feat(imap): implement IMAP APPEND for Sent and Drafts folders with SPECIAL-USE resolution`

---

## Task 6: Implement draft Maildir save/load/delete

**Files:**
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/drafts.rs`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/lib.rs`

Drafts are saved as standard `.eml` files in the Maildir `.Drafts/` subdirectory with the `:2,DS` flag suffix (Draft + Seen). The `ComposeData` is serialized into a valid RFC 2822 message so it's compatible with any Maildir-aware tool.

- [ ] **Step 1: Create `drafts.rs` with save/load/delete operations**

```rust
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use inboxly_core::compose::ComposeData;
use inboxly_core::AccountId;

/// Save a ComposeData as a draft .eml file in the Maildir .Drafts/ folder.
///
/// If `compose.draft_maildir_path` is Some, overwrites the existing draft.
/// Otherwise, creates a new file in .Drafts/cur/ with the `:2,DS` suffix.
///
/// Returns the path to the saved draft file.
pub fn save_draft(
    maildir_root: &Path,
    compose: &ComposeData,
) -> Result<PathBuf, DraftError> {
    let drafts_dir = maildir_root.join(".Drafts").join("cur");
    fs::create_dir_all(&drafts_dir).map_err(|e| DraftError::IoError {
        path: drafts_dir.clone(),
        source: e,
    })?;

    // Build the raw RFC 2822 message from ComposeData
    let raw_message = compose_to_rfc2822(compose);

    // Determine the target path
    let target_path = if let Some(ref existing_path) = compose.draft_maildir_path {
        // Overwrite existing draft
        existing_path.clone()
    } else {
        // Generate a new Maildir filename
        // Format: <timestamp>.<unique>.<hostname>:2,DS
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let unique = uuid::Uuid::new_v4().simple();
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "localhost".to_string());

        let filename = format!("{timestamp}.{unique}.{hostname}:2,DS");
        drafts_dir.join(filename)
    };

    // Write atomically via tmp + rename
    let tmp_dir = maildir_root.join(".Drafts").join("tmp");
    fs::create_dir_all(&tmp_dir).map_err(|e| DraftError::IoError {
        path: tmp_dir.clone(),
        source: e,
    })?;

    let tmp_file = tmp_dir.join(format!("draft.{}", uuid::Uuid::new_v4().simple()));
    let mut f = fs::File::create(&tmp_file).map_err(|e| DraftError::IoError {
        path: tmp_file.clone(),
        source: e,
    })?;
    f.write_all(raw_message.as_bytes())
        .map_err(|e| DraftError::IoError {
            path: tmp_file.clone(),
            source: e,
        })?;
    f.sync_all().map_err(|e| DraftError::IoError {
        path: tmp_file.clone(),
        source: e,
    })?;

    fs::rename(&tmp_file, &target_path).map_err(|e| DraftError::IoError {
        path: target_path.clone(),
        source: e,
    })?;

    Ok(target_path)
}

/// Delete a draft file from Maildir.
pub fn delete_draft(draft_path: &Path) -> Result<(), DraftError> {
    if draft_path.exists() {
        fs::remove_file(draft_path).map_err(|e| DraftError::IoError {
            path: draft_path.to_path_buf(),
            source: e,
        })?;
    }
    Ok(())
}

/// List all draft files in the .Drafts/ Maildir folder.
///
/// Returns paths to all .eml files in .Drafts/cur/ and .Drafts/new/.
pub fn list_drafts(maildir_root: &Path) -> Result<Vec<PathBuf>, DraftError> {
    let mut drafts = Vec::new();

    for subdir in &["cur", "new"] {
        let dir = maildir_root.join(".Drafts").join(subdir);
        if !dir.exists() {
            continue;
        }
        let entries = fs::read_dir(&dir).map_err(|e| DraftError::IoError {
            path: dir.clone(),
            source: e,
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| DraftError::IoError {
                path: dir.clone(),
                source: e,
            })?;
            drafts.push(entry.path());
        }
    }

    Ok(drafts)
}

/// Build a minimal RFC 2822 message from ComposeData for draft storage.
///
/// This produces a valid .eml file that can be reopened for editing
/// or sent later. The body is stored as Markdown in the text/plain part
/// with an `X-Inboxly-Markdown: true` custom header so we know to
/// re-render HTML on send.
fn compose_to_rfc2822(compose: &ComposeData) -> String {
    let mut msg = String::with_capacity(4096);

    // Headers
    msg.push_str(&format!("Message-ID: {}\r\n", compose.message_id));
    msg.push_str(&format!("Subject: {}\r\n", compose.subject));

    // To
    if !compose.to.is_empty() {
        let to_str: Vec<String> = compose.to.iter().map(format_contact).collect();
        msg.push_str(&format!("To: {}\r\n", to_str.join(", ")));
    }

    // Cc
    if !compose.cc.is_empty() {
        let cc_str: Vec<String> = compose.cc.iter().map(format_contact).collect();
        msg.push_str(&format!("Cc: {}\r\n", cc_str.join(", ")));
    }

    // Bcc (included in draft storage, stripped on send by SMTP)
    if !compose.bcc.is_empty() {
        let bcc_str: Vec<String> = compose.bcc.iter().map(format_contact).collect();
        msg.push_str(&format!("Bcc: {}\r\n", bcc_str.join(", ")));
    }

    // In-Reply-To
    if let Some(ref irt) = compose.in_reply_to {
        msg.push_str(&format!("In-Reply-To: {irt}\r\n"));
    }

    // References
    if let Some(ref refs) = compose.references {
        msg.push_str(&format!("References: {refs}\r\n"));
    }

    // Custom header: marks this as a Markdown draft
    msg.push_str("X-Inboxly-Markdown: true\r\n");

    // Date
    let now = chrono::Utc::now();
    msg.push_str(&format!("Date: {}\r\n", now.to_rfc2822()));

    // Content type
    msg.push_str("Content-Type: text/plain; charset=utf-8\r\n");
    msg.push_str("MIME-Version: 1.0\r\n");

    // Attachment list as custom header (for draft reopening)
    if !compose.attachments.is_empty() {
        let att_json =
            serde_json::to_string(&compose.attachments).unwrap_or_else(|_| "[]".to_string());
        msg.push_str(&format!("X-Inboxly-Attachments: {att_json}\r\n"));
    }

    // Blank line separates headers from body
    msg.push_str("\r\n");

    // Body (Markdown source)
    msg.push_str(&compose.body_markdown);

    msg
}

/// Format a Contact for an RFC 2822 header.
fn format_contact(contact: &inboxly_core::Contact) -> String {
    if contact.name.is_empty() {
        contact.address.clone()
    } else {
        format!("{} <{}>", contact.name, contact.address)
    }
}

/// Errors from draft operations.
#[derive(Debug, thiserror::Error)]
pub enum DraftError {
    #[error("I/O error at {path}: {source}")]
    IoError {
        path: PathBuf,
        source: std::io::Error,
    },
}
```

- [ ] **Step 2: Add `pub mod drafts;` to `inboxly-store/src/lib.rs`**

- [ ] **Step 3: Write unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn save_and_list_draft() {
        let tmp = TempDir::new().unwrap();
        let compose = ComposeData::new(AccountId::new());
        // ... fill in subject, to, body_markdown
        let path = save_draft(tmp.path(), &compose).unwrap();
        assert!(path.exists());

        let drafts = list_drafts(tmp.path()).unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0], path);
    }

    #[test]
    fn overwrite_existing_draft() {
        let tmp = TempDir::new().unwrap();
        let mut compose = ComposeData::new(AccountId::new());
        compose.subject = "v1".to_string();
        let path = save_draft(tmp.path(), &compose).unwrap();

        compose.draft_maildir_path = Some(path.clone());
        compose.subject = "v2".to_string();
        let path2 = save_draft(tmp.path(), &compose).unwrap();
        assert_eq!(path, path2);

        let content = std::fs::read_to_string(&path2).unwrap();
        assert!(content.contains("Subject: v2"));
    }

    #[test]
    fn delete_draft_removes_file() {
        let tmp = TempDir::new().unwrap();
        let compose = ComposeData::new(AccountId::new());
        let path = save_draft(tmp.path(), &compose).unwrap();
        assert!(path.exists());

        delete_draft(&path).unwrap();
        assert!(!path.exists());
    }
}
```

- [ ] **Step 4: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store -- drafts
```

**Commit:** `feat(store): implement draft save/load/delete in Maildir .Drafts folder`

---

## Task 7: Implement offline send queue

**Files:**
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/offline_queue.rs`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/lib.rs`

The `offline_queue` table (defined in M3's SQLite schema) stores actions taken while offline. This task adds compose-specific queue operations: enqueue a send, dequeue pending sends on reconnect, and mark as flushed.

- [ ] **Step 1: Create `offline_queue.rs` with queue operations**

```rust
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use inboxly_core::compose::ComposeData;

/// An entry in the offline send queue.
#[derive(Debug, Clone)]
pub struct QueuedSend {
    pub id: i64,
    pub compose_data: ComposeData,
    pub created_at: DateTime<Utc>,
}

/// Enqueue a compose for sending when connectivity is restored.
///
/// The ComposeData is serialized to JSON and stored in the offline_queue table.
pub fn enqueue_send(conn: &Connection, compose: &ComposeData) -> Result<i64, QueueError> {
    let payload = serde_json::to_string(compose).map_err(QueueError::SerializeFailed)?;
    let now = Utc::now().timestamp();

    conn.execute(
        "INSERT INTO offline_queue (action, payload_json, created_at) VALUES (?1, ?2, ?3)",
        params!["send_email", payload, now],
    )
    .map_err(QueueError::DbError)?;

    Ok(conn.last_insert_rowid())
}

/// Retrieve all pending send operations, ordered by creation time (FIFO).
pub fn pending_sends(conn: &Connection) -> Result<Vec<QueuedSend>, QueueError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, payload_json, created_at FROM offline_queue \
             WHERE action = 'send_email' ORDER BY created_at ASC",
        )
        .map_err(QueueError::DbError)?;

    let rows = stmt
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            let payload: String = row.get(1)?;
            let created_at_ts: i64 = row.get(2)?;
            Ok((id, payload, created_at_ts))
        })
        .map_err(QueueError::DbError)?;

    let mut sends = Vec::new();
    for row in rows {
        let (id, payload, created_at_ts) = row.map_err(QueueError::DbError)?;
        let compose_data: ComposeData =
            serde_json::from_str(&payload).map_err(QueueError::DeserializeFailed)?;
        let created_at = DateTime::from_timestamp(created_at_ts, 0)
            .unwrap_or_else(|| Utc::now());
        sends.push(QueuedSend {
            id,
            compose_data,
            created_at,
        });
    }

    Ok(sends)
}

/// Remove a successfully sent item from the queue.
pub fn dequeue_send(conn: &Connection, queue_id: i64) -> Result<(), QueueError> {
    conn.execute(
        "DELETE FROM offline_queue WHERE id = ?1",
        params![queue_id],
    )
    .map_err(QueueError::DbError)?;

    Ok(())
}

/// Count pending send operations in the queue.
pub fn pending_send_count(conn: &Connection) -> Result<usize, QueueError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM offline_queue WHERE action = 'send_email'",
            [],
            |row| row.get(0),
        )
        .map_err(QueueError::DbError)?;

    Ok(count as usize)
}

/// Errors from offline queue operations.
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("database error: {0}")]
    DbError(rusqlite::Error),

    #[error("failed to serialize compose data: {0}")]
    SerializeFailed(serde_json::Error),

    #[error("failed to deserialize queued compose data: {0}")]
    DeserializeFailed(serde_json::Error),
}
```

- [ ] **Step 2: Add `pub mod offline_queue;` to `inboxly-store/src/lib.rs`**

- [ ] **Step 3: Write unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use inboxly_core::compose::ComposeData;
    use inboxly_core::AccountId;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE offline_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                action TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );",
        ).unwrap();
        conn
    }

    #[test]
    fn enqueue_and_dequeue_roundtrip() {
        let conn = setup_db();
        let compose = ComposeData::new(AccountId::new());

        let id = enqueue_send(&conn, &compose).unwrap();
        assert_eq!(pending_send_count(&conn).unwrap(), 1);

        let pending = pending_sends(&conn).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, id);

        dequeue_send(&conn, id).unwrap();
        assert_eq!(pending_send_count(&conn).unwrap(), 0);
    }

    #[test]
    fn fifo_ordering() {
        let conn = setup_db();
        let c1 = ComposeData::new(AccountId::new());
        let c2 = ComposeData::new(AccountId::new());

        let id1 = enqueue_send(&conn, &c1).unwrap();
        let id2 = enqueue_send(&conn, &c2).unwrap();

        let pending = pending_sends(&conn).unwrap();
        assert_eq!(pending[0].id, id1);
        assert_eq!(pending[1].id, id2);
    }
}
```

- [ ] **Step 4: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store -- offline_queue
```

**Commit:** `feat(store): implement offline send queue for compose-while-disconnected`

---

## Task 8: Implement Markdown-to-HTML conversion module

**Files:**
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/markdown.rs`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/lib.rs`

Converts Markdown body text to HTML for two purposes: (1) the send path (multipart/alternative text/html part), and (2) the compose preview panel. Uses `pulldown-cmark` for parsing and rendering.

- [ ] **Step 1: Create `markdown.rs`**

```rust
use pulldown_cmark::{html, Options, Parser};

/// Convert Markdown source text to an HTML string.
///
/// Supports CommonMark with tables, strikethrough, and task lists.
/// The output is a standalone HTML fragment (no <html>/<body> wrapper)
/// suitable for embedding in a multipart/alternative email or
/// rendering in the compose preview panel.
pub fn markdown_to_html(markdown: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;

    let parser = Parser::new_ext(markdown, options);

    let mut html_output = String::with_capacity(markdown.len() * 2);
    html::push_html(&mut html_output, parser);

    html_output
}

/// Wrap a Markdown-rendered HTML fragment in a full HTML document
/// suitable for email sending.
///
/// Adds basic inline CSS for readability across email clients.
/// Email clients strip <style> tags and external CSS, so all
/// styling must be inline.
pub fn markdown_to_email_html(markdown: &str) -> String {
    let body = markdown_to_html(markdown);

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; font-size: 16px; line-height: 1.6; color: #212121; max-width: 600px; margin: 0 auto; padding: 16px;">
{body}
</body>
</html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_markdown_conversion() {
        let md = "Hello **world**!";
        let html = markdown_to_html(md);
        assert!(html.contains("<strong>world</strong>"));
        assert!(html.contains("<p>"));
    }

    #[test]
    fn empty_input() {
        let html = markdown_to_html("");
        assert!(html.is_empty());
    }

    #[test]
    fn table_support() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let html = markdown_to_html(md);
        assert!(html.contains("<table>"));
        assert!(html.contains("<td>1</td>"));
    }

    #[test]
    fn strikethrough() {
        let md = "~~deleted~~";
        let html = markdown_to_html(md);
        assert!(html.contains("<del>deleted</del>"));
    }

    #[test]
    fn task_list() {
        let md = "- [x] Done\n- [ ] Todo";
        let html = markdown_to_html(md);
        assert!(html.contains("checked"));
    }

    #[test]
    fn email_html_has_doctype() {
        let html = markdown_to_email_html("test");
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("font-family"));
    }

    #[test]
    fn multiline_paragraphs() {
        let md = "First paragraph.\n\nSecond paragraph.";
        let html = markdown_to_html(md);
        assert_eq!(html.matches("<p>").count(), 2);
    }

    #[test]
    fn inline_code_and_code_blocks() {
        let md = "Use `cargo build` to compile.\n\n```rust\nfn main() {}\n```";
        let html = markdown_to_html(md);
        assert!(html.contains("<code>cargo build</code>"));
        assert!(html.contains("<pre>"));
    }

    #[test]
    fn links() {
        let md = "[Inboxly](https://example.com)";
        let html = markdown_to_html(md);
        assert!(html.contains(r#"href="https://example.com""#));
        assert!(html.contains(">Inboxly</a>"));
    }
}
```

- [ ] **Step 2: Add `pub mod markdown;` to `inboxly-core/src/lib.rs`**

```rust
pub mod markdown;
pub use markdown::{markdown_to_email_html, markdown_to_html};
```

- [ ] **Step 3: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-core -- markdown
```

**Commit:** `feat(core): add Markdown-to-HTML conversion with pulldown-cmark`

---

## Task 9: Build ComposeView widget — layout + fields

**Files:**
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/compose_view.rs`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/mod.rs`

The `ComposeView` is the primary compose widget. It renders as a centered panel (max 920dp wide) with address fields, subject, Markdown body editor, attachment list, and a toolbar with Send/Discard/Attach buttons.

This task builds the layout and basic text input fields. Autocomplete (Task 10), Markdown preview (Task 11), and attachments (Task 12) are layered on top.

- [ ] **Step 1: Create `compose_view.rs` with the `ComposeView` component**

The ComposeView is an Iced component (not a raw widget) because it has internal state and generates its own messages that bubble up to the parent.

```rust
use iced::widget::{
    button, column, container, horizontal_rule, row, scrollable, text, text_input, Column, Row,
};
use iced::{Alignment, Element, Length, Padding};

use inboxly_core::compose::{ComposeAttachment, ComposeData, ComposeMode};
use inboxly_core::Contact;

/// Messages generated by the ComposeView.
#[derive(Debug, Clone)]
pub enum ComposeMessage {
    /// User typed in the To field.
    ToInputChanged(String),
    /// User pressed Enter/Tab in the To field — commit the current text as a contact.
    ToContactAdded,
    /// User removed a To contact chip by index.
    ToContactRemoved(usize),
    /// User typed in the Cc field.
    CcInputChanged(String),
    /// User pressed Enter/Tab in the Cc field.
    CcContactAdded,
    /// User removed a Cc contact chip by index.
    CcContactRemoved(usize),
    /// User typed in the Bcc field.
    BccInputChanged(String),
    /// User pressed Enter/Tab in the Bcc field.
    BccContactAdded,
    /// User removed a Bcc contact chip by index.
    BccContactRemoved(usize),
    /// User toggled the Cc/Bcc fields visibility.
    ToggleCcBcc,
    /// Subject field changed.
    SubjectChanged(String),
    /// Body Markdown text changed.
    BodyChanged(String),
    /// User toggled Markdown preview panel.
    TogglePreview,
    /// User clicked the Attach button.
    AttachFile,
    /// User removed an attachment by index.
    RemoveAttachment(usize),
    /// User clicked Send.
    Send,
    /// User clicked Save Draft (or triggered by auto-save timer).
    SaveDraft,
    /// User clicked Discard.
    Discard,
    /// Autocomplete suggestion selected (from Task 10).
    AutocompleteSelected(Contact),
}

/// Compose view state.
pub struct ComposeViewState {
    /// The underlying compose data (shared with backend).
    pub data: ComposeData,

    /// Current text in the To input (before committing as a contact).
    pub to_input: String,
    /// Current text in the Cc input.
    pub cc_input: String,
    /// Current text in the Bcc input.
    pub bcc_input: String,

    /// Whether Cc/Bcc fields are visible.
    pub show_cc_bcc: bool,
    /// Whether Markdown preview panel is visible.
    pub show_preview: bool,

    /// Autocomplete suggestions for the currently active address field.
    pub autocomplete_suggestions: Vec<Contact>,
    /// Which field is currently receiving autocomplete suggestions.
    pub autocomplete_field: AddressField,

    /// Whether the compose has unsaved changes since last draft save.
    pub dirty: bool,
}

/// Which address field is active for autocomplete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressField {
    To,
    Cc,
    Bcc,
}

impl ComposeViewState {
    /// Create a new compose state for a fresh email.
    pub fn new(data: ComposeData) -> Self {
        // Show Cc/Bcc if pre-filled (e.g., reply-all)
        let show_cc_bcc = !data.cc.is_empty() || !data.bcc.is_empty();

        Self {
            data,
            to_input: String::new(),
            cc_input: String::new(),
            bcc_input: String::new(),
            show_cc_bcc,
            show_preview: false,
            autocomplete_suggestions: Vec::new(),
            autocomplete_field: AddressField::To,
            dirty: false,
        }
    }

    /// Update state in response to a ComposeMessage.
    /// Returns true if the message was handled.
    pub fn update(&mut self, message: ComposeMessage) -> bool {
        match message {
            ComposeMessage::ToInputChanged(text) => {
                self.to_input = text;
                self.autocomplete_field = AddressField::To;
                self.dirty = true;
            }
            ComposeMessage::ToContactAdded => {
                if let Some(contact) = parse_contact_input(&self.to_input) {
                    self.data.to.push(contact);
                    self.to_input.clear();
                    self.dirty = true;
                }
            }
            ComposeMessage::ToContactRemoved(idx) => {
                if idx < self.data.to.len() {
                    self.data.to.remove(idx);
                    self.dirty = true;
                }
            }
            ComposeMessage::CcInputChanged(text) => {
                self.cc_input = text;
                self.autocomplete_field = AddressField::Cc;
                self.dirty = true;
            }
            ComposeMessage::CcContactAdded => {
                if let Some(contact) = parse_contact_input(&self.cc_input) {
                    self.data.cc.push(contact);
                    self.cc_input.clear();
                    self.dirty = true;
                }
            }
            ComposeMessage::CcContactRemoved(idx) => {
                if idx < self.data.cc.len() {
                    self.data.cc.remove(idx);
                    self.dirty = true;
                }
            }
            ComposeMessage::BccInputChanged(text) => {
                self.bcc_input = text;
                self.autocomplete_field = AddressField::Bcc;
                self.dirty = true;
            }
            ComposeMessage::BccContactAdded => {
                if let Some(contact) = parse_contact_input(&self.bcc_input) {
                    self.data.bcc.push(contact);
                    self.bcc_input.clear();
                    self.dirty = true;
                }
            }
            ComposeMessage::BccContactRemoved(idx) => {
                if idx < self.data.bcc.len() {
                    self.data.bcc.remove(idx);
                    self.dirty = true;
                }
            }
            ComposeMessage::ToggleCcBcc => {
                self.show_cc_bcc = !self.show_cc_bcc;
            }
            ComposeMessage::SubjectChanged(text) => {
                self.data.subject = text;
                self.dirty = true;
            }
            ComposeMessage::BodyChanged(text) => {
                self.data.body_markdown = text;
                self.dirty = true;
            }
            ComposeMessage::TogglePreview => {
                self.show_preview = !self.show_preview;
            }
            ComposeMessage::AutocompleteSelected(contact) => {
                match self.autocomplete_field {
                    AddressField::To => {
                        self.data.to.push(contact);
                        self.to_input.clear();
                    }
                    AddressField::Cc => {
                        self.data.cc.push(contact);
                        self.cc_input.clear();
                    }
                    AddressField::Bcc => {
                        self.data.bcc.push(contact);
                        self.bcc_input.clear();
                    }
                }
                self.autocomplete_suggestions.clear();
                self.dirty = true;
            }
            // Send, SaveDraft, Discard, AttachFile, RemoveAttachment
            // are handled by the parent (InboxlyApp) — they trigger backend actions.
            _ => return false,
        }
        true
    }

    /// Build the Iced view for the compose window.
    pub fn view(&self) -> Element<'_, ComposeMessage> {
        let content = column![
            self.view_toolbar(),
            horizontal_rule(1),
            self.view_to_field(),
            horizontal_rule(1),
        ];

        let mut content = content;

        // Cc/Bcc fields (conditionally visible)
        if self.show_cc_bcc {
            content = content
                .push(self.view_cc_field())
                .push(horizontal_rule(1))
                .push(self.view_bcc_field())
                .push(horizontal_rule(1));
        }

        content = content
            .push(self.view_subject_field())
            .push(horizontal_rule(1));

        // Body: either editor only, or side-by-side editor + preview
        if self.show_preview {
            content = content.push(self.view_body_with_preview());
        } else {
            content = content.push(self.view_body_editor());
        }

        // Attachment list (if any)
        if !self.data.attachments.is_empty() {
            content = content
                .push(horizontal_rule(1))
                .push(self.view_attachments());
        }

        // Wrap in a container with max width 920dp, centered
        container(
            container(content.spacing(0).width(Length::Fill))
                .max_width(920)
                .padding(Padding::new(0.0)),
        )
        .width(Length::Fill)
        .center_x(Length::Fill)
        .into()
    }

    /// Compose toolbar: Send button, Attach, Preview toggle, Discard.
    fn view_toolbar(&self) -> Element<'_, ComposeMessage> {
        let send_btn = button(text("Send"))
            .on_press_maybe(
                self.data.is_sendable().then_some(ComposeMessage::Send),
            );

        let attach_btn = button(text("Attach"))
            .on_press(ComposeMessage::AttachFile);

        let preview_label = if self.show_preview {
            "Hide Preview"
        } else {
            "Preview"
        };
        let preview_btn = button(text(preview_label))
            .on_press(ComposeMessage::TogglePreview);

        let discard_btn = button(text("Discard"))
            .on_press(ComposeMessage::Discard);

        // Mode indicator (Reply, Reply All, Forward, or New)
        let mode_text = match &self.data.mode {
            ComposeMode::New => "New Message",
            ComposeMode::Reply { .. } => "Reply",
            ComposeMode::ReplyAll { .. } => "Reply All",
            ComposeMode::Forward { .. } => "Forward",
        };

        row![
            text(mode_text).size(20),
            iced::widget::horizontal_space(),
            preview_btn,
            attach_btn,
            send_btn,
            discard_btn,
        ]
        .spacing(8)
        .padding(Padding::from([8, 16]))
        .align_y(Alignment::Center)
        .height(56)
        .into()
    }

    /// To field: contact chips + text input.
    fn view_to_field(&self) -> Element<'_, ComposeMessage> {
        let mut row_items: Vec<Element<'_, ComposeMessage>> = Vec::new();

        row_items.push(text("To").size(14).width(48).into());

        // Contact chips
        for (i, contact) in self.data.to.iter().enumerate() {
            let chip = button(text(&contact.display_label()).size(14))
                .on_press(ComposeMessage::ToContactRemoved(i));
            row_items.push(chip.into());
        }

        // Text input for new contacts
        let input = text_input("Add recipient...", &self.to_input)
            .on_input(ComposeMessage::ToInputChanged)
            .on_submit(ComposeMessage::ToContactAdded)
            .size(14)
            .width(Length::Fill);
        row_items.push(input.into());

        // Cc/Bcc toggle (only show if Cc/Bcc hidden)
        if !self.show_cc_bcc {
            let toggle = button(text("Cc/Bcc").size(12))
                .on_press(ComposeMessage::ToggleCcBcc);
            row_items.push(toggle.into());
        }

        Row::with_children(row_items)
            .spacing(4)
            .padding(Padding::from([4, 16]))
            .align_y(Alignment::Center)
            .height(56)
            .into()
    }

    /// Cc field: contact chips + text input.
    fn view_cc_field(&self) -> Element<'_, ComposeMessage> {
        let mut row_items: Vec<Element<'_, ComposeMessage>> = Vec::new();

        row_items.push(text("Cc").size(14).width(48).into());

        for (i, contact) in self.data.cc.iter().enumerate() {
            let chip = button(text(&contact.display_label()).size(14))
                .on_press(ComposeMessage::CcContactRemoved(i));
            row_items.push(chip.into());
        }

        let input = text_input("", &self.cc_input)
            .on_input(ComposeMessage::CcInputChanged)
            .on_submit(ComposeMessage::CcContactAdded)
            .size(14)
            .width(Length::Fill);
        row_items.push(input.into());

        Row::with_children(row_items)
            .spacing(4)
            .padding(Padding::from([4, 16]))
            .align_y(Alignment::Center)
            .height(56)
            .into()
    }

    /// Bcc field: contact chips + text input.
    fn view_bcc_field(&self) -> Element<'_, ComposeMessage> {
        let mut row_items: Vec<Element<'_, ComposeMessage>> = Vec::new();

        row_items.push(text("Bcc").size(14).width(48).into());

        for (i, contact) in self.data.bcc.iter().enumerate() {
            let chip = button(text(&contact.display_label()).size(14))
                .on_press(ComposeMessage::BccContactRemoved(i));
            row_items.push(chip.into());
        }

        let input = text_input("", &self.bcc_input)
            .on_input(ComposeMessage::BccInputChanged)
            .on_submit(ComposeMessage::BccContactAdded)
            .size(14)
            .width(Length::Fill);
        row_items.push(input.into());

        Row::with_children(row_items)
            .spacing(4)
            .padding(Padding::from([4, 16]))
            .align_y(Alignment::Center)
            .height(56)
            .into()
    }

    /// Subject field: bold 18sp text input.
    fn view_subject_field(&self) -> Element<'_, ComposeMessage> {
        let input = text_input("Subject", &self.data.subject)
            .on_input(ComposeMessage::SubjectChanged)
            .size(18);
        // Note: bold text_input requires custom styling; apply via theme

        container(input)
            .padding(Padding::from([8, 16]))
            .width(Length::Fill)
            .into()
    }

    /// Body editor (Markdown text area), full height.
    fn view_body_editor(&self) -> Element<'_, ComposeMessage> {
        let editor = text_input("Compose email...", &self.data.body_markdown)
            .on_input(ComposeMessage::BodyChanged)
            .size(16);
        // Note: Iced's text_input is single-line. For multiline, use iced::widget::text_editor.
        // The actual implementation should use text_editor which supports multiline input.
        // Shown here as text_input for structural clarity; swap to text_editor at build time.

        container(
            scrollable(
                container(editor)
                    .padding(Padding::from([8, 16]))
                    .width(Length::Fill),
            ),
        )
        .height(Length::Fill)
        .into()
    }

    /// Body with side-by-side Markdown preview.
    fn view_body_with_preview(&self) -> Element<'_, ComposeMessage> {
        let editor = self.view_body_editor();

        // Render Markdown to HTML, then display as text
        // Note: Iced doesn't have a native HTML renderer. The preview shows
        // a simplified rendered view using iced::widget::markdown (if available)
        // or falls back to rendering the HTML as styled text blocks.
        let preview_text = inboxly_core::markdown::markdown_to_html(&self.data.body_markdown);
        let preview = container(
            scrollable(
                container(text(&preview_text).size(16))
                    .padding(Padding::from([8, 16]))
                    .width(Length::Fill),
            ),
        )
        .width(Length::FillPortion(1))
        .height(Length::Fill);

        row![
            container(editor).width(Length::FillPortion(1)),
            iced::widget::vertical_rule(1),
            preview,
        ]
        .height(Length::Fill)
        .into()
    }

    /// Attachment list.
    fn view_attachments(&self) -> Element<'_, ComposeMessage> {
        let mut items: Vec<Element<'_, ComposeMessage>> = Vec::new();

        for (i, att) in self.data.attachments.iter().enumerate() {
            let size_label = format_file_size(att.size_bytes);
            let label = format!("{} ({})", att.filename, size_label);
            let remove_btn = button(text("x").size(12))
                .on_press(ComposeMessage::RemoveAttachment(i));

            items.push(
                row![text(label).size(14), remove_btn]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into(),
            );
        }

        Column::with_children(items)
            .spacing(4)
            .padding(Padding::from([8, 16]))
            .into()
    }
}

/// Parse a raw text input into a Contact.
/// Accepts formats: "email@example.com" or "Name <email@example.com>".
fn parse_contact_input(input: &str) -> Option<Contact> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    if let Some(start) = input.find('<') {
        if let Some(end) = input.find('>') {
            let name = input[..start].trim().to_string();
            let address = input[start + 1..end].trim().to_string();
            return Some(Contact { name, address });
        }
    }

    // Bare email address
    if input.contains('@') {
        return Some(Contact {
            name: String::new(),
            address: input.to_string(),
        });
    }

    None
}

/// Format file size as human-readable string.
fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
```

**Key design notes in code comments:**
- Iced `text_input` is single-line. The actual implementation must use `iced::widget::text_editor` (multiline) for the body. The plan uses `text_input` for structural clarity.
- The Markdown preview in Iced cannot render HTML natively. Two approaches at implementation time: (a) use `iced::widget::markdown` if available in the Iced version used, or (b) render pulldown-cmark events to styled Iced `text`/`column` elements directly.
- Contact chips are implemented as small buttons with `on_press` for removal. A more polished implementation would use a custom chip widget with a close icon.

- [ ] **Step 2: Add to `widgets/mod.rs`**

```rust
pub mod compose_view;
pub use compose_view::{ComposeMessage, ComposeViewState};
```

- [ ] **Step 3: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): build ComposeView widget with address fields, subject, body, and attachment list`

---

## Task 10: Implement address field autocomplete from contacts

**Files:**
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/compose_view.rs`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/contacts.rs` (from M11)

Address autocomplete queries the contacts table (populated by M11 from email From headers) as the user types. Results are shown as a dropdown overlay below the active address field.

- [ ] **Step 1: Add contact search query to `inboxly-store`**

In `inboxly-store/src/contacts.rs`, add a function that queries by prefix:

```rust
/// Search contacts by prefix match on address or display_name.
///
/// Returns up to `limit` results, ordered by last_seen descending
/// (most recently seen contacts first).
pub fn search_contacts(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<Contact>, rusqlite::Error> {
    let pattern = format!("{query}%");
    let mut stmt = conn.prepare(
        "SELECT address, display_name FROM contacts \
         WHERE address LIKE ?1 OR display_name LIKE ?1 \
         ORDER BY last_seen DESC LIMIT ?2",
    )?;

    let contacts = stmt
        .query_map(params![pattern, limit as i64], |row| {
            Ok(Contact {
                address: row.get(0)?,
                name: row.get(1)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(contacts)
}
```

- [ ] **Step 2: Add autocomplete dropdown to ComposeView**

In `compose_view.rs`, add a method to render autocomplete suggestions below the active address field:

```rust
/// Render autocomplete dropdown overlay.
/// Called after the address field that currently has focus.
fn view_autocomplete(&self) -> Option<Element<'_, ComposeMessage>> {
    if self.autocomplete_suggestions.is_empty() {
        return None;
    }

    let items: Vec<Element<'_, ComposeMessage>> = self
        .autocomplete_suggestions
        .iter()
        .map(|contact| {
            let label = if contact.name.is_empty() {
                contact.address.clone()
            } else {
                format!("{} <{}>", contact.name, contact.address)
            };
            button(text(label).size(14))
                .on_press(ComposeMessage::AutocompleteSelected(contact.clone()))
                .width(Length::Fill)
                .into()
        })
        .collect();

    Some(
        container(Column::with_children(items).spacing(2))
            .padding(4)
            .width(Length::Fill)
            .into(),
    )
}
```

- [ ] **Step 3: Wire autocomplete into the update cycle**

When `ToInputChanged`/`CcInputChanged`/`BccInputChanged` fires:
1. If the input text is >= 2 characters, send a query to the store via the backend channel
2. The backend responds with matching contacts via a `ComposeEvent::AutocompleteSuggestions` event
3. ComposeViewState stores the suggestions and the view re-renders with the dropdown

The query is debounced: only fire after 150ms of no typing (use `iced::time::every` or a simple flag + subscription).

- [ ] **Step 4: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add address field autocomplete from contacts database`

---

## Task 11: Implement Markdown preview toggle panel

**Files:**
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/compose_view.rs`

The Markdown preview is a side-by-side panel that shows the rendered HTML alongside the Markdown editor. Toggled via the "Preview" button in the compose toolbar.

- [ ] **Step 1: Implement Markdown-to-Iced-elements renderer**

Since Iced cannot render raw HTML, we need a function that converts pulldown-cmark events into Iced widget elements (text, bold text, code blocks, lists, etc.):

```rust
use pulldown_cmark::{Event, Parser, Tag, TagEnd, Options};

/// Render Markdown to a column of styled Iced elements.
///
/// This provides a native preview without an HTML renderer.
/// Supports: paragraphs, headings, bold, italic, code, code blocks,
/// links (as underlined text), lists, horizontal rules.
fn render_markdown_preview(markdown: &str) -> Element<'_, ComposeMessage> {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(markdown, options);

    let mut elements: Vec<Element<'_, ComposeMessage>> = Vec::new();
    let mut current_text = String::new();
    let mut in_heading = false;
    let mut heading_level = 0u8;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                flush_text(&mut current_text, &mut elements, 16, false);
                in_heading = true;
                heading_level = level as u8;
            }
            Event::End(TagEnd::Heading(_)) => {
                let size = match heading_level {
                    1 => 28,
                    2 => 24,
                    3 => 20,
                    _ => 18,
                };
                flush_text(&mut current_text, &mut elements, size, true);
                in_heading = false;
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                flush_text(&mut current_text, &mut elements, 16, false);
            }
            Event::Text(t) => {
                current_text.push_str(&t);
            }
            Event::Code(code) => {
                // Inline code — append with backtick markers for now
                current_text.push('`');
                current_text.push_str(&code);
                current_text.push('`');
            }
            Event::HardBreak | Event::SoftBreak => {
                current_text.push('\n');
            }
            Event::Rule => {
                flush_text(&mut current_text, &mut elements, 16, false);
                elements.push(horizontal_rule(1).into());
            }
            _ => {}
        }
    }

    flush_text(&mut current_text, &mut elements, 16, false);

    scrollable(
        Column::with_children(elements)
            .spacing(8)
            .padding(Padding::from([8, 16]))
            .width(Length::Fill),
    )
    .into()
}

/// Helper: flush accumulated text into an element.
fn flush_text(
    buf: &mut String,
    elements: &mut Vec<Element<'_, ComposeMessage>>,
    size: u16,
    bold: bool,
) {
    if !buf.is_empty() {
        let t = text(buf.clone()).size(size);
        // Note: apply bold via font weight in the theme; Iced's text() supports .font()
        elements.push(t.into());
        buf.clear();
    }
}
```

- [ ] **Step 2: Replace the HTML-text-based preview with the native renderer**

Update `view_body_with_preview()` to call `render_markdown_preview()` instead of displaying raw HTML text.

- [ ] **Step 3: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement native Markdown preview panel with pulldown-cmark rendering`

---

## Task 12: Implement attachment picker and attachment list

**Files:**
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/compose_view.rs`

Uses `rfd` (Rust File Dialog) to open a native file picker. Selected files are added to `ComposeData.attachments`. The attachment list shows filename, size, and a remove button.

- [ ] **Step 1: Handle `ComposeMessage::AttachFile` in the parent app**

The file dialog must run asynchronously. In the `InboxlyApp::update()` method (not in ComposeViewState, because rfd needs async + the Iced Command system):

```rust
ComposeMessage::AttachFile => {
    return iced::Task::perform(
        async {
            let handle = rfd::AsyncFileDialog::new()
                .set_title("Attach File")
                .pick_files()
                .await;
            handle
        },
        |files| {
            if let Some(files) = files {
                let attachments: Vec<ComposeAttachment> = files
                    .iter()
                    .map(|f| {
                        let path = f.path().to_path_buf();
                        let filename = f.file_name();
                        let size_bytes = std::fs::metadata(&path)
                            .map(|m| m.len())
                            .unwrap_or(0);
                        let mime_type = mime_guess::from_path(&path)
                            .first_or_octet_stream()
                            .to_string();

                        ComposeAttachment {
                            filename,
                            mime_type,
                            path,
                            size_bytes,
                        }
                    })
                    .collect();
                Message::AttachmentsAdded(attachments)
            } else {
                Message::Noop
            }
        },
    );
}
```

- [ ] **Step 2: Add `mime_guess` dependency**

Add `mime_guess = "2"` to workspace dependencies and `inboxly-ui/Cargo.toml`.

- [ ] **Step 3: Handle `Message::AttachmentsAdded` in app update**

Push each `ComposeAttachment` into `compose_state.data.attachments` and mark dirty.

- [ ] **Step 4: Handle `ComposeMessage::RemoveAttachment(idx)` in ComposeViewState::update**

Already present in the match — just remove the attachment at the given index.

- [ ] **Step 5: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add file attachment picker via rfd and attachment list display`

---

## Task 13: Implement reply / reply-all / forward pre-fill

**Files:**
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/compose_prefill.rs`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/lib.rs`

When the user clicks Reply, Reply All, or Forward, the compose window opens with pre-filled fields derived from the original email. This logic lives in `inboxly-core` (not UI) because it's pure data transformation.

- [ ] **Step 1: Create `compose_prefill.rs` with builder functions**

```rust
use crate::compose::{ComposeData, ComposeMode};
use crate::{AccountId, Contact, EmailContent, EmailMeta};

/// Build a ComposeData pre-filled for a Reply.
///
/// - To: original sender (From header)
/// - Subject: "Re: " prefix (if not already present)
/// - Body: quoted original message
/// - In-Reply-To: original Message-ID
/// - References: original References + original Message-ID
pub fn prefill_reply(
    account_id: AccountId,
    original_meta: &EmailMeta,
    original_content: &EmailContent,
) -> ComposeData {
    let mut data = ComposeData::new(account_id);
    data.mode = ComposeMode::Reply {
        original_email_id: original_meta.id.clone(),
        thread_id: original_meta.thread_id.clone(),
    };

    // To: original sender
    data.to.push(original_meta.from.clone());

    // Subject: add "Re: " prefix if not present
    data.subject = prefixed_subject("Re", &original_meta.subject);

    // Body: quote original
    data.body_markdown = quote_message(original_meta, original_content);

    // Threading headers
    data.in_reply_to = Some(original_meta.id.0.clone());
    data.references = build_references(original_content, &original_meta.id.0);

    data
}

/// Build a ComposeData pre-filled for Reply All.
///
/// - To: original sender
/// - Cc: all original To + Cc recipients, minus the current account's address
/// - Subject: "Re: " prefix
/// - Body: quoted original message
pub fn prefill_reply_all(
    account_id: AccountId,
    account_email: &str,
    original_meta: &EmailMeta,
    original_content: &EmailContent,
) -> ComposeData {
    let mut data = prefill_reply(account_id, original_meta, original_content);
    data.mode = ComposeMode::ReplyAll {
        original_email_id: original_meta.id.clone(),
        thread_id: original_meta.thread_id.clone(),
    };

    // Cc: all original To + Cc, minus ourselves
    let mut cc: Vec<Contact> = Vec::new();
    for contact in &original_meta.to {
        if !contact.address.eq_ignore_ascii_case(account_email)
            && !data.to.iter().any(|c| c.address.eq_ignore_ascii_case(&contact.address))
        {
            cc.push(contact.clone());
        }
    }
    for contact in &original_meta.cc {
        if !contact.address.eq_ignore_ascii_case(account_email)
            && !data.to.iter().any(|c| c.address.eq_ignore_ascii_case(&contact.address))
            && !cc.iter().any(|c| c.address.eq_ignore_ascii_case(&contact.address))
        {
            cc.push(contact.clone());
        }
    }
    data.cc = cc;

    data
}

/// Build a ComposeData pre-filled for Forward.
///
/// - To: empty (user fills in)
/// - Subject: "Fwd: " prefix
/// - Body: forwarded message header block + original body
/// - Attachments: original attachments carried over (by path reference)
pub fn prefill_forward(
    account_id: AccountId,
    original_meta: &EmailMeta,
    original_content: &EmailContent,
) -> ComposeData {
    let mut data = ComposeData::new(account_id);
    data.mode = ComposeMode::Forward {
        original_email_id: original_meta.id.clone(),
        thread_id: original_meta.thread_id.clone(),
    };

    // Subject: add "Fwd: " prefix
    data.subject = prefixed_subject("Fwd", &original_meta.subject);

    // Body: forwarded message block
    data.body_markdown = forward_message(original_meta, original_content);

    // References header (for threading awareness, though forwards often break threads)
    data.references = build_references(original_content, &original_meta.id.0);

    // Carry over attachments (reference by Maildir path)
    for att in &original_meta.attachments {
        data.attachments.push(crate::compose::ComposeAttachment {
            filename: att.name.clone(),
            mime_type: att.mime_type.clone(),
            path: original_meta.maildir_path.clone(), // attachment extracted at send time
            size_bytes: att.size_bytes,
        });
    }

    data
}

/// Add a prefix ("Re" or "Fwd") to a subject, avoiding duplication.
fn prefixed_subject(prefix: &str, subject: &str) -> String {
    let check = format!("{prefix}: ");
    if subject.starts_with(&check) {
        subject.to_string()
    } else {
        format!("{check}{subject}")
    }
}

/// Build a quoted body for reply.
///
/// Format:
/// ```
/// \n\n---\n
/// On <date>, <sender> wrote:\n
/// > original line 1\n
/// > original line 2\n
/// ```
fn quote_message(meta: &EmailMeta, content: &EmailContent) -> String {
    let date = meta.date.format("%a, %b %d, %Y at %I:%M %p");
    let sender = if meta.from.name.is_empty() {
        meta.from.address.clone()
    } else {
        format!("{} <{}>", meta.from.name, meta.from.address)
    };

    let body = content
        .body_text
        .as_deref()
        .unwrap_or("");

    let quoted: String = body
        .lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!("\n\n---\nOn {date}, {sender} wrote:\n{quoted}")
}

/// Build a forwarded message body with a header block.
fn forward_message(meta: &EmailMeta, content: &EmailContent) -> String {
    let date = meta.date.format("%a, %b %d, %Y at %I:%M %p");
    let from = if meta.from.name.is_empty() {
        meta.from.address.clone()
    } else {
        format!("{} <{}>", meta.from.name, meta.from.address)
    };
    let to: Vec<String> = meta.to.iter().map(|c| {
        if c.name.is_empty() { c.address.clone() } else { format!("{} <{}>", c.name, c.address) }
    }).collect();

    let body = content.body_text.as_deref().unwrap_or("");

    format!(
        "\n\n---------- Forwarded message ----------\n\
         From: {from}\n\
         Date: {date}\n\
         Subject: {subject}\n\
         To: {to}\n\n\
         {body}",
        subject = meta.subject,
        to = to.join(", "),
    )
}

/// Build the References header value from original email headers.
fn build_references(content: &EmailContent, original_message_id: &str) -> Option<String> {
    let existing_refs = content
        .headers
        .get("References")
        .or_else(|| content.headers.get("references"))
        .cloned()
        .unwrap_or_default();

    if existing_refs.is_empty() {
        Some(original_message_id.to_string())
    } else {
        Some(format!("{existing_refs} {original_message_id}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn re_prefix_not_duplicated() {
        assert_eq!(prefixed_subject("Re", "Hello"), "Re: Hello");
        assert_eq!(prefixed_subject("Re", "Re: Hello"), "Re: Hello");
    }

    #[test]
    fn fwd_prefix_not_duplicated() {
        assert_eq!(prefixed_subject("Fwd", "Hello"), "Fwd: Hello");
        assert_eq!(prefixed_subject("Fwd", "Fwd: Hello"), "Fwd: Hello");
    }

    #[test]
    fn quote_adds_angle_brackets() {
        let meta = test_email_meta();
        let content = EmailContent {
            id: meta.id.clone(),
            body_text: Some("Line 1\nLine 2".to_string()),
            body_html: None,
            headers: Default::default(),
            attachments: Vec::new(),
        };
        let quoted = quote_message(&meta, &content);
        assert!(quoted.contains("> Line 1"));
        assert!(quoted.contains("> Line 2"));
        assert!(quoted.contains("wrote:"));
    }

    // test_email_meta() helper elided for brevity — creates a minimal EmailMeta fixture
}
```

- [ ] **Step 2: Add `pub mod compose_prefill;` to `inboxly-core/src/lib.rs`**

```rust
pub mod compose_prefill;
pub use compose_prefill::{prefill_forward, prefill_reply, prefill_reply_all};
```

- [ ] **Step 3: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-core -- compose_prefill
```

**Commit:** `feat(core): implement reply, reply-all, and forward pre-fill logic`

---

## Task 14: Wire draft auto-save timer (30-second interval)

**Files:**
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (or wherever InboxlyApp lives)

Draft auto-save fires every 30 seconds while the compose view is open. It only saves if the compose has unsaved changes (`dirty` flag is true).

- [ ] **Step 1: Add a subscription for the auto-save timer**

In `InboxlyApp::subscription()`:

```rust
fn subscription(&self) -> iced::Subscription<Message> {
    let mut subs = vec![
        // ... existing subscriptions ...
    ];

    // Draft auto-save: tick every 30 seconds while compose is open
    if self.compose_state.is_some() {
        subs.push(
            iced::time::every(std::time::Duration::from_secs(30))
                .map(|_| Message::DraftAutoSaveTick),
        );
    }

    iced::Subscription::batch(subs)
}
```

- [ ] **Step 2: Handle `Message::DraftAutoSaveTick` in update**

```rust
Message::DraftAutoSaveTick => {
    if let Some(ref compose_state) = self.compose_state {
        if compose_state.dirty && compose_state.data.has_content() {
            // Dispatch SaveDraft action to backend
            let action = ComposeAction::SaveDraft(compose_state.data.clone());
            let _ = self.backend_tx.try_send(action);
            // dirty flag is cleared when DraftSaved event arrives from backend
        }
    }
    iced::Task::none()
}
```

- [ ] **Step 3: Handle `ComposeEvent::DraftSaved` response**

When the backend confirms the draft was saved:
```rust
ComposeEvent::DraftSaved { message_id, maildir_path } => {
    if let Some(ref mut compose_state) = self.compose_state {
        if compose_state.data.message_id == message_id {
            compose_state.data.draft_maildir_path = Some(maildir_path);
            compose_state.data.last_saved = Some(Utc::now());
            compose_state.dirty = false;
        }
    }
}
```

- [ ] **Step 4: Show "Draft saved" indicator in compose toolbar**

Add a subtle "Saved at HH:MM" label in the compose toolbar that updates when `last_saved` changes.

- [ ] **Step 5: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): wire 30-second draft auto-save timer with dirty tracking`

---

## Task 15: Wire send flow — compose to SMTP to IMAP APPEND to close

**Files:**
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-imap/src/lib.rs` (backend handler)

The full send flow:
1. User clicks Send in compose
2. UI dispatches `ComposeAction::Send(data)` to backend channel
3. Backend builds message, sends via SMTP, receives raw bytes
4. Backend appends raw bytes to Sent folder via IMAP APPEND
5. Backend saves to local Maildir `.Sent/` folder
6. Backend deletes the draft from `.Drafts/` (if one was saved)
7. Backend sends `ComposeEvent::SendSuccess` to UI
8. UI closes compose view

- [ ] **Step 1: Handle `ComposeMessage::Send` in `InboxlyApp::update()`**

```rust
ComposeMessage::Send => {
    if let Some(ref compose_state) = self.compose_state {
        let data = compose_state.data.clone();
        let backend_tx = self.backend_tx.clone();

        return iced::Task::perform(
            async move {
                let _ = backend_tx.send(ComposeAction::Send(data)).await;
            },
            |_| Message::Noop,
        );
    }
    iced::Task::none()
}
```

- [ ] **Step 2: Implement send handler in the backend task**

In the backend's action handler loop (wherever `ComposeAction` messages are consumed):

```rust
ComposeAction::Send(compose_data) => {
    let account_config = self.get_account_config(&compose_data.account_id)?;
    let password = self.get_password(&compose_data.account_id)?;

    // 1. Send via SMTP
    let smtp = SmtpSender::connect(&account_config, &password).await?;
    let raw_message = smtp.send(&compose_data).await?;

    // 2. Append to Sent folder via IMAP
    if let Some(ref mut imap_conn) = self.get_imap_connection(&compose_data.account_id) {
        let sent_folder = resolve_sent_folder(imap_conn).await?;
        imap_append_sent(imap_conn, &sent_folder, &raw_message).await?;
    }

    // 3. Save to local Maildir .Sent/
    let maildir_root = self.maildir_root(&compose_data.account_id);
    save_to_sent_maildir(&maildir_root, &raw_message)?;

    // 4. Delete draft if one was saved
    if let Some(ref draft_path) = compose_data.draft_maildir_path {
        let _ = delete_draft(draft_path);
    }

    // 5. Notify UI
    let _ = ui_tx.send(ComposeEvent::SendSuccess {
        message_id: compose_data.message_id.clone(),
    }).await;
}
```

- [ ] **Step 3: Handle `ComposeEvent::SendSuccess` in UI**

```rust
ComposeEvent::SendSuccess { message_id } => {
    if let Some(ref compose_state) = self.compose_state {
        if compose_state.data.message_id == message_id {
            self.compose_state = None; // Close compose view
            // Optionally show a snackbar: "Message sent"
        }
    }
}
```

- [ ] **Step 4: Handle `ComposeEvent::SendFailed` — show error, keep compose open**

```rust
ComposeEvent::SendFailed { message_id, error } => {
    // Show error in compose toolbar or as a snackbar
    // Keep compose view open so user can retry
}
```

- [ ] **Step 5: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check --workspace
```

**Commit:** `feat: wire full send flow — SMTP send, IMAP APPEND to Sent, draft cleanup`

---

## Task 16: Wire offline compose — save draft + queue send

**Files:**
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-imap/src/lib.rs` (backend handler)

When the app is offline, the send flow changes:
1. Save as draft in Maildir `.Drafts/`
2. Queue the send in `offline_queue` table
3. Show "Queued for sending" status to user
4. On reconnect, flush the queue: SMTP send + IMAP APPEND for each queued item

- [ ] **Step 1: Detect offline state before sending**

In the backend's `ComposeAction::Send` handler, check connectivity before SMTP:

```rust
ComposeAction::Send(compose_data) => {
    if !self.is_connected(&compose_data.account_id) {
        // Offline: save draft + queue
        let maildir_root = self.maildir_root(&compose_data.account_id);
        let draft_path = save_draft(&maildir_root, &compose_data)?;

        let db = self.db_conn();
        enqueue_send(&db, &compose_data)?;

        let _ = ui_tx.send(ComposeEvent::SendSuccess {
            message_id: compose_data.message_id.clone(),
        }).await;
        // The UI sees "success" — the message is safely queued.
        // User sees "Queued" status in Drafts view.
        return;
    }

    // Online: proceed with SMTP send (as in Task 15)
    // ...
}
```

- [ ] **Step 2: Implement queue flush on reconnect**

In the sync engine's reconnect handler (called when IMAP connection is re-established):

```rust
/// Flush all pending offline sends.
async fn flush_offline_queue(&mut self) -> Result<(), Error> {
    let pending = pending_sends(&self.db)?;
    if pending.is_empty() {
        return Ok(());
    }

    log::info!("Flushing {} queued sends", pending.len());

    for queued in pending {
        match self.send_email(&queued.compose_data).await {
            Ok(_) => {
                dequeue_send(&self.db, queued.id)?;
                let _ = self.ui_tx.send(ComposeEvent::OfflineQueueFlushed {
                    message_id: queued.compose_data.message_id.clone(),
                }).await;

                // Clean up the draft file
                if let Some(ref path) = queued.compose_data.draft_maildir_path {
                    let _ = delete_draft(path);
                }
            }
            Err(e) => {
                log::error!("Failed to flush queued send {}: {e}", queued.id);
                // Leave in queue for next reconnect attempt
                break; // Stop flushing on first failure (network may be unstable)
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 3: Call `flush_offline_queue()` when IMAP reconnects**

In the sync engine's reconnect path (after successful IMAP login + IDLE setup):

```rust
// After reconnect succeeds
self.flush_offline_queue().await?;
```

- [ ] **Step 4: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check --workspace
```

**Commit:** `feat: implement offline compose with queue-and-flush on reconnect`

---

## Task 17: Hook compose into FAB speed dial and conversation view

**Files:**
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/speed_dial_fab.rs` (from M22)
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/conversation_view.rs` (from M17 or M19)
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs`

Connect the compose entry points to the ComposeView:

1. **FAB Speed Dial** (M22): "Compose" option opens a new blank compose
2. **Conversation View**: Reply / Reply All / Forward buttons open pre-filled compose

- [ ] **Step 1: Handle FAB compose action**

In the speed dial's message handling (M22 already defines `SpeedDialMessage::ComposeClicked`):

```rust
SpeedDialMessage::ComposeClicked => {
    let account_id = self.active_account_id();
    let data = ComposeData::new(account_id);
    self.compose_state = Some(ComposeViewState::new(data));
    // Speed dial auto-closes (scrim dismissed)
}
```

- [ ] **Step 2: Add Reply/Reply All/Forward buttons to conversation view**

In the conversation view's message rendering, add action buttons per message:

```rust
// Per-message action bar in conversation view
let reply_btn = button(text("Reply").size(14))
    .on_press(ConversationMessage::Reply(email_id.clone()));

let reply_all_btn = button(text("Reply All").size(14))
    .on_press(ConversationMessage::ReplyAll(email_id.clone()));

let forward_btn = button(text("Forward").size(14))
    .on_press(ConversationMessage::Forward(email_id.clone()));

row![reply_btn, reply_all_btn, forward_btn].spacing(8)
```

- [ ] **Step 3: Handle conversation view compose triggers in app update**

```rust
ConversationMessage::Reply(email_id) => {
    let (meta, content) = self.load_email(&email_id)?;
    let data = prefill_reply(self.active_account_id(), &meta, &content);
    self.compose_state = Some(ComposeViewState::new(data));
}
ConversationMessage::ReplyAll(email_id) => {
    let (meta, content) = self.load_email(&email_id)?;
    let account_email = self.active_account_email();
    let data = prefill_reply_all(self.active_account_id(), &account_email, &meta, &content);
    self.compose_state = Some(ComposeViewState::new(data));
}
ConversationMessage::Forward(email_id) => {
    let (meta, content) = self.load_email(&email_id)?;
    let data = prefill_forward(self.active_account_id(), &meta, &content);
    self.compose_state = Some(ComposeViewState::new(data));
}
```

- [ ] **Step 4: Handle Discard action**

```rust
ComposeMessage::Discard => {
    if let Some(ref compose_state) = self.compose_state {
        // Delete draft if one was saved
        if let Some(ref path) = compose_state.data.draft_maildir_path {
            let _ = delete_draft(path);
        }
    }
    self.compose_state = None; // Close compose view
}
```

- [ ] **Step 5: Render compose view in the app's view method**

The compose view overlays the main content when active:

```rust
fn view(&self) -> Element<'_, Message> {
    let main_content = self.view_main_content();

    if let Some(ref compose_state) = self.compose_state {
        // Compose overlays the main content (or replaces it, depending on design)
        let compose = compose_state.view().map(Message::Compose);
        column![main_content, compose].into()
    } else {
        main_content
    }
}
```

- [ ] **Step 6: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): hook compose into FAB speed dial and conversation view reply/forward actions`

---

## Task 18: Integration tests

**Files:**
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-core/tests/compose_integration.rs`
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/draft_queue_integration.rs`

- [ ] **Step 1: Core compose integration tests**

```rust
//! Integration tests for compose data flow.

use inboxly_core::compose::{ComposeData, ComposeMode};
use inboxly_core::compose_prefill::{prefill_forward, prefill_reply, prefill_reply_all};
use inboxly_core::markdown::{markdown_to_email_html, markdown_to_html};
use inboxly_core::{AccountId, Contact, EmailContent, EmailId, EmailMeta, ThreadId};

#[test]
fn new_compose_generates_unique_message_id() {
    let c1 = ComposeData::new(AccountId::new());
    let c2 = ComposeData::new(AccountId::new());
    assert_ne!(c1.message_id, c2.message_id);
    assert!(c1.message_id.starts_with('<'));
    assert!(c1.message_id.ends_with("@inboxly>"));
}

#[test]
fn empty_compose_is_not_sendable() {
    let c = ComposeData::new(AccountId::new());
    assert!(!c.is_sendable());
    assert!(!c.has_content());
}

#[test]
fn compose_with_to_and_subject_is_sendable() {
    let mut c = ComposeData::new(AccountId::new());
    c.to.push(Contact {
        name: "Bob".into(),
        address: "bob@example.com".into(),
    });
    c.subject = "Hello".into();
    assert!(c.is_sendable());
    assert!(c.has_content());
}

#[test]
fn compose_with_to_and_body_only_is_sendable() {
    let mut c = ComposeData::new(AccountId::new());
    c.to.push(Contact {
        name: String::new(),
        address: "bob@example.com".into(),
    });
    c.body_markdown = "Just a message".into();
    assert!(c.is_sendable());
}

#[test]
fn reply_prefill_sets_to_from_sender() {
    let account_id = AccountId::new();
    let (meta, content) = fixture_email();
    let reply = prefill_reply(account_id, &meta, &content);

    assert_eq!(reply.to.len(), 1);
    assert_eq!(reply.to[0].address, "sender@example.com");
    assert!(reply.subject.starts_with("Re: "));
    assert!(reply.body_markdown.contains("> Hello world"));
    assert!(matches!(reply.mode, ComposeMode::Reply { .. }));
    assert!(reply.in_reply_to.is_some());
}

#[test]
fn reply_all_includes_cc_recipients() {
    let account_id = AccountId::new();
    let (meta, content) = fixture_email();
    let reply = prefill_reply_all(account_id, "me@example.com", &meta, &content);

    assert_eq!(reply.to.len(), 1);
    assert!(!reply.cc.is_empty());
    assert!(matches!(reply.mode, ComposeMode::ReplyAll { .. }));
}

#[test]
fn reply_all_excludes_own_address_from_cc() {
    let account_id = AccountId::new();
    let (mut meta, content) = fixture_email();
    meta.to.push(Contact {
        name: "Me".into(),
        address: "me@example.com".into(),
    });
    let reply = prefill_reply_all(account_id, "me@example.com", &meta, &content);

    assert!(
        !reply.cc.iter().any(|c| c.address == "me@example.com"),
        "Own address should not appear in Cc"
    );
}

#[test]
fn forward_prefill_has_forwarded_header_block() {
    let account_id = AccountId::new();
    let (meta, content) = fixture_email();
    let fwd = prefill_forward(account_id, &meta, &content);

    assert!(fwd.subject.starts_with("Fwd: "));
    assert!(fwd.body_markdown.contains("Forwarded message"));
    assert!(fwd.body_markdown.contains("From: "));
    assert!(fwd.to.is_empty());
    assert!(matches!(fwd.mode, ComposeMode::Forward { .. }));
}

#[test]
fn markdown_roundtrip_produces_valid_html() {
    let md = "# Hello\n\nThis is **bold** and *italic*.\n\n- Item 1\n- Item 2";
    let html = markdown_to_html(md);
    assert!(html.contains("<h1>Hello</h1>"));
    assert!(html.contains("<strong>bold</strong>"));
    assert!(html.contains("<em>italic</em>"));
    assert!(html.contains("<li>"));

    let email_html = markdown_to_email_html(md);
    assert!(email_html.contains("<!DOCTYPE html>"));
    assert!(email_html.contains("<h1>Hello</h1>"));
}

fn fixture_email() -> (EmailMeta, EmailContent) {
    // Minimal fixture for testing — fill in required fields
    // (actual implementation uses builders from M1)
    todo!("Create fixture using EmailMeta and EmailContent from inboxly-core")
}
```

- [ ] **Step 2: Store draft + queue integration tests**

```rust
//! Integration tests for draft save + offline queue interaction.

use tempfile::TempDir;
use rusqlite::Connection;

use inboxly_core::compose::ComposeData;
use inboxly_core::{AccountId, Contact};
use inboxly_store::drafts::{save_draft, delete_draft, list_drafts};
use inboxly_store::offline_queue::{enqueue_send, pending_sends, dequeue_send, pending_send_count};

#[test]
fn draft_save_then_queue_then_flush() {
    let tmp = TempDir::new().unwrap();
    let db = setup_db();

    let mut compose = ComposeData::new(AccountId::new());
    compose.to.push(Contact {
        name: "Bob".into(),
        address: "bob@example.com".into(),
    });
    compose.subject = "Offline email".into();
    compose.body_markdown = "Sent later".into();

    // 1. Save draft
    let draft_path = save_draft(tmp.path(), &compose).unwrap();
    assert!(draft_path.exists());

    // 2. Queue for sending
    compose.draft_maildir_path = Some(draft_path.clone());
    let queue_id = enqueue_send(&db, &compose).unwrap();
    assert_eq!(pending_send_count(&db).unwrap(), 1);

    // 3. Simulate flush: dequeue + delete draft
    let pending = pending_sends(&db).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].compose_data.subject, "Offline email");

    dequeue_send(&db, queue_id).unwrap();
    delete_draft(&draft_path).unwrap();

    assert_eq!(pending_send_count(&db).unwrap(), 0);
    assert!(!draft_path.exists());
}

#[test]
fn multiple_drafts_coexist() {
    let tmp = TempDir::new().unwrap();

    let c1 = {
        let mut c = ComposeData::new(AccountId::new());
        c.subject = "Draft 1".into();
        c
    };
    let c2 = {
        let mut c = ComposeData::new(AccountId::new());
        c.subject = "Draft 2".into();
        c
    };

    let p1 = save_draft(tmp.path(), &c1).unwrap();
    let p2 = save_draft(tmp.path(), &c2).unwrap();

    assert_ne!(p1, p2);
    let drafts = list_drafts(tmp.path()).unwrap();
    assert_eq!(drafts.len(), 2);
}

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE offline_queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            action TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );",
    ).unwrap();
    conn
}
```

- [ ] **Step 3: Run all tests**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test --workspace
```

- [ ] **Step 4: Run clippy**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo clippy --workspace -- -D warnings
```

**Commit:** `test: add integration tests for compose data flow, draft persistence, and offline queue`

---

## Verification Checklist

Before declaring M23 complete, verify all of the following:

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace -- -D warnings` is clean
- [ ] New ComposeView widget is centered at max 920dp width
- [ ] To/Cc/Bcc fields accept contact chips and autocomplete from contacts DB
- [ ] Subject field is 18sp bold, body is 16sp normal (per spec typography)
- [ ] Markdown preview toggle shows rendered Markdown alongside editor
- [ ] Markdown body converts to HTML on send (multipart/alternative)
- [ ] SMTP sends via TLS with account credentials
- [ ] Sent message is saved to Sent folder via IMAP APPEND
- [ ] Reply pre-fills To from sender, quotes original with ">" prefix
- [ ] Reply-All pre-fills To + Cc from all participants, excludes own address
- [ ] Forward pre-fills body with forwarded message block, carries attachments
- [ ] Draft auto-saves every 30 seconds to Maildir .Drafts/
- [ ] Attachment picker opens native file dialog, shows attachment list with sizes
- [ ] Offline compose saves draft + queues send, flushes on reconnect
- [ ] Compose opens from FAB speed dial (new) and conversation view (reply/forward)
- [ ] Discard deletes the saved draft file

## Files Created or Modified

### New Files
| File | Crate | Purpose |
|------|-------|---------|
| `inboxly-core/src/compose.rs` | core | ComposeData, ComposeMode, ComposeAction, ComposeEvent |
| `inboxly-core/src/compose_prefill.rs` | core | Reply/ReplyAll/Forward pre-fill logic |
| `inboxly-core/src/markdown.rs` | core | Markdown-to-HTML conversion |
| `inboxly-imap/src/smtp.rs` | imap | SmtpSender with lettre |
| `inboxly-imap/src/append.rs` | imap | IMAP APPEND for Sent/Drafts + folder resolution |
| `inboxly-store/src/drafts.rs` | store | Draft save/load/delete in Maildir |
| `inboxly-store/src/offline_queue.rs` | store | Offline send queue operations |
| `inboxly-ui/src/widgets/compose_view.rs` | ui | ComposeView widget |
| `inboxly-core/tests/compose_integration.rs` | core | Compose integration tests |
| `inboxly-store/tests/draft_queue_integration.rs` | store | Draft + queue integration tests |

### Modified Files
| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Add lettre, pulldown-cmark, rfd, mime_guess dependencies |
| `inboxly-core/Cargo.toml` | Add pulldown-cmark dependency |
| `inboxly-core/src/lib.rs` | Add compose, compose_prefill, markdown modules |
| `inboxly-imap/Cargo.toml` | Add lettre dependency |
| `inboxly-imap/src/lib.rs` | Add smtp, append modules |
| `inboxly-store/src/lib.rs` | Add drafts, offline_queue modules |
| `inboxly-store/src/contacts.rs` | Add search_contacts() for autocomplete |
| `inboxly-ui/Cargo.toml` | Add rfd, mime_guess dependencies |
| `inboxly-ui/src/widgets/mod.rs` | Add compose_view module |
| `inboxly-ui/src/app.rs` | Add compose state, auto-save subscription, send flow, FAB/reply hooks |
| `inboxly-ui/src/widgets/speed_dial_fab.rs` | Wire compose trigger |
| `inboxly-ui/src/widgets/conversation_view.rs` | Add reply/reply-all/forward buttons |
