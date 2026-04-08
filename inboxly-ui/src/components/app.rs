//! Root application component -- shell layout with theme context.

use std::sync::Arc;

use dioxus::prelude::*;
use inboxly_core::{AuthMethod, ComposeMode};

use crate::startup::OAuth2Contexts;

use crate::app::{Inboxly, Message};
use crate::components::content_area::ContentArea;
use crate::components::nav_drawer::NavDrawer;
use crate::components::snooze_picker::SnoozePicker;
use crate::components::speed_dial_fab::SpeedDialFab;
use crate::components::toolbar::Toolbar;
use crate::components::undo_snackbar::UndoSnackbar;
use crate::loaded_thread::{
    LoadedThread, build_loaded_thread, error_thread, fallback_thread, loading_thread,
};
use crate::state::{ComposeSendState, ComposeState};
use crate::theme::{ActiveView, ThemeConfig};

/// UUID v5 namespace for deriving deterministic [`inboxly_core::AccountId`]
/// values from an account's email address.
///
/// `AccountConfig` carries an `email` field but no UUID id (see
/// `inboxly-core/src/config.rs`). The drafts table column, on the other
/// hand, expects an `inboxly_core::AccountId` (a `Uuid`). Until the
/// account model is refactored to carry a real id (out of M35b scope),
/// the auto-save bridge derives one deterministically from the email so
/// that successive saves of the same draft target the same row, and so
/// the Phase 12 send pipeline can resolve the same id from the same
/// account without coordination.
///
/// The namespace UUID below is a fixed v4 token chosen specifically for
/// inboxly compose drafts — it has no special meaning beyond being a
/// stable seed. Do NOT change it without a migration plan: every
/// existing draft row references AccountIds derived from this namespace.
const COMPOSE_ACCOUNT_NAMESPACE: uuid::Uuid = uuid::uuid!("a4b3c4d5-e6f7-4a8b-9c0d-1e2f3a4b5c6d");

/// Derive a deterministic [`inboxly_core::AccountId`] from an email
/// address. See [`COMPOSE_ACCOUNT_NAMESPACE`] for the rationale.
fn account_id_from_email(email: &str) -> inboxly_core::AccountId {
    inboxly_core::AccountId(uuid::Uuid::new_v5(
        &COMPOSE_ACCOUNT_NAMESPACE,
        email.as_bytes(),
    ))
}

/// Build a [`inboxly_core::DraftEmail`] snapshot from the current
/// [`ComposeState`].
///
/// The compose state holds `Arc`-wrapped contacts and attachments for
/// cheap per-render clones; this helper materialises them into the owned
/// shape the storage layer expects. Returns `None` if `compose.draft_id`
/// is unset (which shouldn't happen after `OpenCompose` — Gemini G4
/// ensures eager id assignment — but defensively skip rather than panic).
///
/// Used by:
/// - The Phase 10 auto-save bridge (30 s timer)
/// - (Future) the Phase 12 send bridge to build the SMTP sender input
pub(crate) fn compose_state_to_draft_email(
    compose: &ComposeState,
    account_id: inboxly_core::AccountId,
) -> Option<inboxly_core::DraftEmail> {
    use chrono::Utc;
    use inboxly_core::DraftEmail;

    let draft_id = compose.draft_id.clone()?;
    let message_id = format!("<{draft_id}@inboxly.local>");
    let now = Utc::now();

    Some(DraftEmail {
        id: draft_id,
        account_id,
        message_id,
        subject: compose.subject.clone(),
        body_markdown: compose.body_markdown.clone(),
        to: compose.to.iter().map(|arc| (**arc).clone()).collect(),
        cc: compose.cc.iter().map(|arc| (**arc).clone()).collect(),
        bcc: compose.bcc.iter().map(|arc| (**arc).clone()).collect(),
        attachments: compose
            .attachments
            .iter()
            .map(|arc| (**arc).clone())
            .collect(),
        mode: compose.mode.clone(),
        in_reply_to: compose.in_reply_to.clone(),
        references: compose.references.clone(),
        maildir_path: None,
        // Auto-save doesn't track creation separately from updates; both
        // timestamps reflect the same instant. The storage layer's
        // `update_draft` deliberately ignores `created_at` (see
        // `inboxly-store/src/drafts.rs`), so the only consequence here is
        // that the very first `insert_draft` call records "created at the
        // first save" rather than "created at compose-open".
        created_at: now,
        updated_at: now,
    })
}

// ---------------------------------------------------------------------------
// M36 Phase 7: compose_state_from_original helper + ComposeMode dispatch
// ---------------------------------------------------------------------------

/// Build a [`Contact`] from the `from_name` / `from_address` columns of an
/// [`inboxly_store::EmailRow`].
///
/// `EmailRow` carries the sender as two separate columns rather than a
/// pre-built `Contact` (the storage layer is intentionally schema-flat).
/// Reply/Forward both need a real `Contact` for quoting and addressing,
/// so this helper centralises the conversion. An absent display name
/// becomes the empty string — the `Contact::Display` impl already handles
/// that gracefully ("addr@host" rather than " <addr@host>").
pub(crate) fn sender_contact_from_row(row: &inboxly_store::EmailRow) -> inboxly_core::Contact {
    inboxly_core::Contact {
        name: row.from_name.clone().unwrap_or_default(),
        address: row.from_address.clone(),
    }
}

/// Format the `date` column (Unix epoch seconds) of an
/// [`inboxly_store::EmailRow`] as a Gmail-style attribution date string,
/// e.g. `"Thu, 7 Apr 2026 at 14:32"`.
///
/// Returns the empty string if the timestamp can't be converted (e.g.
/// astronomically out-of-range values from a corrupt row). The pure
/// quote-formatting helpers in `inboxly_core::reply` accept any string
/// for `date_formatted`, so an empty value just produces an attribution
/// line of the form `"On , Alice <…> wrote:"` rather than panicking.
fn format_email_row_date(row: &inboxly_store::EmailRow) -> String {
    chrono::DateTime::from_timestamp(row.date, 0)
        .map(|dt| dt.format("%a, %-d %b %Y at %H:%M").to_string())
        .unwrap_or_default()
}

/// Extract the plaintext body of a [`LoadedEmail`] for use in a
/// reply/forward quote block.
///
/// Returns an empty string when:
/// - the body has not been downloaded yet (`content` is `None`), or
/// - the body was downloaded as HTML-only (`body_text` is `None`).
///
/// The Phase 8 dispatch handler will gate on `body_downloaded` and
/// trigger a body fetch if needed; Phase 7 just renders what's there.
fn extract_body_text(original: &inboxly_store::thread_reader::LoadedEmail) -> String {
    original
        .content
        .as_ref()
        .and_then(|c| c.body_text.clone())
        .unwrap_or_default()
}

/// Parse a JSON-encoded contact list (the `to_json` / `cc_json` columns of
/// an `EmailRow`) into a `Vec<Contact>`.
///
/// Returns an empty vector on parse failure rather than propagating the
/// error: a malformed contact list is logged at the storage layer and
/// should not block the user from composing a reply.
fn parse_contact_json(json: &str) -> Vec<inboxly_core::Contact> {
    serde_json::from_str(json).unwrap_or_default()
}

/// Parse the JSON-encoded `references_json` column of an `EmailRow` into a
/// space-separated chain suitable for
/// [`inboxly_core::reply::build_references_chain`].
///
/// `references_json` is stored as a JSON array of Message-ID strings (see
/// `inboxly-store/src/threading/headers.rs::threading_headers_from_fields`).
/// The reply builder wants a flat space-joined string. Returns `None`
/// when the column is `None` or parses to an empty array.
fn parent_references_chain(references_json: Option<&str>) -> Option<String> {
    let json = references_json?;
    let parsed: Vec<String> = serde_json::from_str(json).ok()?;
    if parsed.is_empty() {
        None
    } else {
        Some(parsed.join(" "))
    }
}

/// Apply the Reply / ReplyAll header bundle to a [`ComposeState`].
///
/// Sets `subject` (via [`inboxly_core::reply::subject_for_reply`]),
/// prepends a `"\n\n"`-padded reply quote to `body_markdown` (via
/// [`inboxly_core::reply::format_reply_quote`]), copies the parent's
/// `Message-ID` into `in_reply_to`, and builds the JWZ-pruned
/// `references` chain via
/// [`inboxly_core::reply::build_references_chain`].
///
/// Shared between `Reply` and `ReplyAll` so the four header-setting lines
/// live in exactly one place (eng review B1: DRY violation between the
/// two reply variants).
///
/// Recipient population (`to` / `cc`) is **not** done here — the
/// dispatcher branches on the mode after calling this helper because
/// Reply uses `[original.from]` while ReplyAll consults
/// [`inboxly_core::reply::reply_all_recipients`].
pub(crate) fn apply_reply_headers(
    state: &mut ComposeState,
    original: &inboxly_store::thread_reader::LoadedEmail,
) {
    use inboxly_core::reply::{build_references_chain, format_reply_quote, subject_for_reply};

    state.subject = subject_for_reply(&original.row.subject);

    let from = sender_contact_from_row(&original.row);
    let date = format_email_row_date(&original.row);
    let body = extract_body_text(original);
    let quote = format_reply_quote(&from, &date, &body);
    state.body_markdown = format!("\n\n{quote}");

    state.in_reply_to = original.row.message_id_header.clone();

    if let Some(parent_msg_id) = original.row.message_id_header.as_deref()
        && !parent_msg_id.is_empty()
    {
        let parent_refs = parent_references_chain(original.row.references_json.as_deref());
        state.references = Some(build_references_chain(
            parent_refs.as_deref(),
            parent_msg_id,
        ));
    }
}

/// Apply the Forward header bundle to a [`ComposeState`].
///
/// Sets `subject` (via [`inboxly_core::reply::subject_for_forward`]) and
/// prepends a `"\n\n"`-padded forward quote (via
/// [`inboxly_core::reply::format_forward_quote`]) to `body_markdown`.
///
/// Forwards start a brand-new thread per Gmail convention, so
/// `in_reply_to` and `references` are explicitly cleared. The
/// dispatcher leaves `to` / `cc` empty for the user to fill, and
/// Phase 9's streaming attachment extractor will populate the
/// `attachments` field on top of this state.
pub(crate) fn apply_forward_headers(
    state: &mut ComposeState,
    original: &inboxly_store::thread_reader::LoadedEmail,
) {
    use inboxly_core::reply::{format_forward_quote, subject_for_forward};

    state.subject = subject_for_forward(&original.row.subject);

    let from = sender_contact_from_row(&original.row);
    let date = format_email_row_date(&original.row);
    let body = extract_body_text(original);
    let to_list = parse_contact_json(&original.row.to_json);
    let quote = format_forward_quote(&from, &date, &original.row.subject, &to_list, &body);
    state.body_markdown = format!("\n\n{quote}");

    state.in_reply_to = None;
    state.references = None;
}

/// Build a fully-populated [`ComposeState`] from an original
/// [`LoadedEmail`] and a Reply/ReplyAll/Forward [`ComposeMode`].
///
/// This is the bridge between the Phase 6 pure helpers in
/// `inboxly_core::reply` and the live `ComposeState` that the compose
/// view consumes. Wired up by the Phase 8
/// [`crate::app::Message::OpenComposeReply`] handler once the parent
/// email's body has been downloaded.
///
/// **Per-mode behaviour:**
/// - `Reply`: calls [`apply_reply_headers`] then sets
///   `to = [Arc::new(original_sender)]`.
/// - `ReplyAll`: calls [`apply_reply_headers`] then computes `(to, cc)`
///   via [`inboxly_core::reply::reply_all_recipients`], handling the G5
///   reply-to-self edge case (the user excludes themselves and inherits
///   the original recipient list when continuing a thread they started).
///   Auto-expands the Cc/Bcc row when the resulting Cc list is non-empty.
/// - `Forward`: calls [`apply_forward_headers`] and leaves `to` / `cc`
///   empty for the user to fill. Phase 9 plumbs in stream-extracted
///   attachments on top of the returned state.
/// - `New`: unreachable. The `OpenCompose` message dispatches a fresh
///   compose; this dispatcher is for replies and forwards only. In
///   debug builds the helper panics with a clear message; release
///   builds simply return the partially-initialised state without
///   touching headers.
///
/// `user_account_index` is the index into `Inboxly::accounts` that the
/// reply should be sent FROM (typically the same account that received
/// the original message — the Phase 8 handler computes this).
/// `user_email` is the address used by
/// [`inboxly_core::reply::reply_all_recipients`] to exclude the user
/// from the recipient list.
///
/// A fresh UUID `draft_id` is generated eagerly so the per-draft
/// attachment directory can be created before the first auto-save tick
/// (consistent with `OpenCompose`, Gemini G4).
///
/// # Panics
///
/// In debug builds, panics if `mode` is [`ComposeMode::New`]. Use
/// [`crate::app::Message::OpenCompose`] for fresh compose flows; this
/// helper is exclusively for Reply/ReplyAll/Forward.
pub(crate) fn compose_state_from_original(
    original: &inboxly_store::thread_reader::LoadedEmail,
    mode: inboxly_core::ComposeMode,
    user_email: &str,
    user_account_index: usize,
) -> ComposeState {
    use inboxly_core::ComposeMode;
    use inboxly_core::reply::reply_all_recipients;

    let mut state = ComposeState::new();
    state.draft_id = Some(uuid::Uuid::new_v4().to_string());
    state.from_account_index = user_account_index;
    state.mode = mode.clone();

    match mode {
        ComposeMode::Reply { .. } => {
            apply_reply_headers(&mut state, original);
            state.to = vec![Arc::new(sender_contact_from_row(&original.row))];
        }
        ComposeMode::ReplyAll { .. } => {
            apply_reply_headers(&mut state, original);
            let sender = sender_contact_from_row(&original.row);
            let orig_to = parse_contact_json(&original.row.to_json);
            let orig_cc = parse_contact_json(&original.row.cc_json);
            let (to, cc) = reply_all_recipients(&sender, &orig_to, &orig_cc, user_email);
            state.to = to.into_iter().map(Arc::new).collect();
            state.cc = cc.into_iter().map(Arc::new).collect();
            state.show_cc_bcc = !state.cc.is_empty();
        }
        ComposeMode::Forward { .. } => {
            apply_forward_headers(&mut state, original);
            // to / cc / bcc stay empty — the user picks recipients.
            // Phase 9 will populate `attachments` from the parent.
        }
        ComposeMode::New => {
            debug_assert!(
                false,
                "compose_state_from_original requires a Reply / ReplyAll / Forward mode \
                 — use Message::OpenCompose for a fresh compose"
            );
            // Release-build fallthrough: return the partially-initialised
            // state with the wrong mode. The caller is responsible for
            // not invoking this helper with `New`; this branch only
            // exists so we don't crash a shipped binary.
        }
    }

    state
}

/// Infer a MIME type from a filename's extension.
///
/// Used by the Phase 11 attachment picker bridge to populate
/// `AttachmentDraft::mime_type` from the user-picked filename. Covers
/// the common attachment types (PDFs, images, archives, Office docs,
/// plain text variants, JSON/XML); everything else falls through to
/// `application/octet-stream`.
///
/// The match is case-insensitive on the extension. Returns owned
/// `String` because the call site stores it in [`inboxly_core::AttachmentDraft`].
///
/// This deliberately does NOT use the `mime_guess` crate — the workspace
/// already keeps the dependency surface tight, and the ~20 entries below
/// cover the realistic compose-attachment universe. If a use case
/// emerges that needs broader coverage, swap to `mime_guess` then.
fn mime_from_extension(filename: &str) -> String {
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "txt" | "md" => "text/plain",
        "html" | "htm" => "text/html",
        "csv" => "text/csv",
        "json" => "application/json",
        "xml" => "application/xml",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// Window title -- updates reactively based on active view.
#[allow(dead_code)]
fn window_title(view: ActiveView) -> String {
    format!("Inboxly \u{2014} {}", view.title())
}

/// Write a local Maildir `.Sent/cur/` copy of a successfully-sent draft.
///
/// This is the M36 Phase 4 bridge between the SMTP success path and the
/// user's on-disk Sent folder. Called from [`run_send_pipeline`] after
/// the SMTP send returns `Ok(())` and BEFORE the SQLite draft delete,
/// so that the offline replay handler has a concrete on-disk source for
/// the eventual server-side IMAP APPEND.
///
/// Construction of the [`inboxly_store::MaildirStore`] is on-demand
/// because the binary does not currently maintain a long-lived handle
/// in `Inboxly::store` / `Inboxly::thread_reader` (pre-existing gap
/// from M35). A new store is cheap — it's a `PathBuf` wrapper — and
/// [`inboxly_store::MaildirStore::init`] is idempotent, so calling it
/// on every send is safe.
///
/// # Fail-soft semantics
///
/// Every error path in this function is logged with `tracing::warn!`
/// and then the function returns normally. The SMTP send already
/// succeeded; the user MUST see the "Sent" overlay regardless of
/// whether the local copy write succeeded. Worst-case failure modes:
///
/// - `Paths::resolve` returns `None` on a machine without a recognised
///   home directory — skip the write.
/// - `MaildirStore::init` fails on disk I/O (permission denied, disk
///   full) — skip the write.
/// - `build_rfc5322_for_sent_folder` fails because of a malformed
///   address — should not happen at this point because the SMTP send
///   just succeeded with the same draft, but we check anyway.
/// - `store_cur` fails on disk I/O — skip the write.
///
/// In every case, the [`inboxly_core::OfflineAction::AppendSent`] queue
/// entry is still enqueued by the caller so a future replay pass can
/// reconcile — the only consequence here is that this particular local
/// copy is missing, which the next full sync will fix.
#[cfg(not(test))]
fn write_local_maildir_sent(
    account_config: &inboxly_core::AccountConfig,
    draft: &inboxly_core::DraftEmail,
    account_id: inboxly_core::AccountId,
) {
    use inboxly_core::EmailFlags;
    use inboxly_core::config::Paths;
    use inboxly_imap::smtp::build_sent_folder_bytes;
    use inboxly_store::{MaildirStore, StandardFolder};

    let Some(paths) = Paths::resolve() else {
        tracing::warn!(
            email = %account_config.email,
            "Paths::resolve returned None; skipping Maildir Sent write"
        );
        return;
    };

    let mail_root = paths
        .maildir_root()
        .join(account_id.0.to_string())
        .join("mail");

    let maildir_store = MaildirStore::new(mail_root);

    if let Err(e) = maildir_store.init() {
        tracing::warn!(
            error = %e,
            "failed to init Maildir for Sent write; skipping local copy"
        );
        return;
    }

    // keep_bcc=true branch via `build_sent_folder_bytes`: the user's
    // local Sent folder retains the Bcc list for audit (Gemini G1).
    // Mirrors the `imap_append_sent` helper at
    // `inboxly-imap/src/append.rs`. This helper is the single-crate
    // wrapper that keeps `lettre` out of `inboxly-ui`'s dep graph.
    let bytes = match build_sent_folder_bytes(account_config, draft) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                error = %e,
                message_id = %draft.message_id,
                "build_sent_folder_bytes failed; skipping Maildir Sent write"
            );
            return;
        }
    };

    // Sent mail is marked \Seen so it never shows as unread in the
    // user's own Sent view. Starred/Answered/Draft are all false.
    let flags_sent = EmailFlags {
        read: true,
        starred: false,
        answered: false,
        draft: false,
    };

    match maildir_store.store_cur(&StandardFolder::Sent, &bytes, &flags_sent) {
        Ok(stored) => {
            tracing::info!(
                message_id = %draft.message_id,
                path = ?stored.path,
                "Maildir Sent copy written"
            );
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                message_id = %draft.message_id,
                "Maildir Sent store_cur failed; skipping local copy"
            );
        }
    }
}

/// **M36 Phase 5**: write a `DraftEmail` snapshot into the account's
/// local Maildir `.Drafts/cur/` folder.
///
/// Mirrors [`write_local_maildir_sent`] one-for-one — same on-demand
/// `MaildirStore` construction (the binary still does not own a
/// long-lived `MaildirStore`), same fail-soft semantics, same
/// `build_sent_folder_bytes` body builder. The only differences are
/// the target folder ([`StandardFolder::Drafts`]) and the
/// [`EmailFlags`] (the `\Draft` flag is set, the message is also
/// marked `\Seen` so it never shows as unread in the user's own
/// Drafts view).
///
/// We deliberately reuse `build_sent_folder_bytes` rather than
/// introducing a parallel `build_draft_folder_bytes` helper: a draft
/// is just a sent-shaped RFC 5322 message that has not been sent yet,
/// the body builder produces identical bytes, and reusing the helper
/// guarantees that a future fix to the body builder (e.g. a header
/// bug) cannot drift between the Sent and Drafts code paths.
///
/// Failure modes are identical to `write_local_maildir_sent`:
///
/// - `Paths::resolve` returns `None` — skip the write.
/// - `MaildirStore::init` fails on disk I/O — skip the write.
/// - `build_sent_folder_bytes` fails on a malformed address —
///   should not happen at this point because the user just typed
///   recipients through the chip parser, but we check anyway.
/// - `store_cur` fails on disk I/O — skip the write.
///
/// In every case, the [`inboxly_core::OfflineAction::AppendDraft`]
/// queue entry is still enqueued by the caller so a future replay
/// pass (post-M36) can reconcile — the only consequence here is that
/// this particular local copy is missing, which the next full sync
/// will fix once the IMAP-side handler is wired.
#[cfg(not(test))]
fn write_local_maildir_drafts(
    account_config: &inboxly_core::AccountConfig,
    draft: &inboxly_core::DraftEmail,
    account_id: inboxly_core::AccountId,
) {
    use inboxly_core::EmailFlags;
    use inboxly_core::config::Paths;
    use inboxly_imap::smtp::build_sent_folder_bytes;
    use inboxly_store::{MaildirStore, StandardFolder};

    let Some(paths) = Paths::resolve() else {
        tracing::warn!(
            email = %account_config.email,
            "Paths::resolve returned None; skipping Maildir Drafts write"
        );
        return;
    };

    let mail_root = paths
        .maildir_root()
        .join(account_id.0.to_string())
        .join("mail");

    let maildir_store = MaildirStore::new(mail_root);

    if let Err(e) = maildir_store.init() {
        tracing::warn!(
            error = %e,
            "failed to init Maildir for Drafts write; skipping local copy"
        );
        return;
    }

    let bytes = match build_sent_folder_bytes(account_config, draft) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                error = %e,
                message_id = %draft.message_id,
                "build_sent_folder_bytes failed for Drafts; skipping Maildir Drafts write"
            );
            return;
        }
    };

    // Drafts get the `\Draft` flag (so any future IMAP APPEND replay
    // sets it server-side) and `\Seen` so the user's own Drafts view
    // never shows the row as unread. The other flags are false.
    let flags_drafts = EmailFlags {
        read: true,
        starred: false,
        answered: false,
        draft: true,
    };

    match maildir_store.store_cur(&StandardFolder::Drafts, &bytes, &flags_drafts) {
        Ok(stored) => {
            tracing::info!(
                message_id = %draft.message_id,
                path = ?stored.path,
                "Maildir Drafts copy written"
            );
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                message_id = %draft.message_id,
                "Maildir Drafts store_cur failed; skipping local copy"
            );
        }
    }
}

/// Run the SMTP send pipeline for the currently-Sending compose draft.
///
/// Hosted as a free function (gated behind `cfg(not(test))`) so the
/// Phase 12 send bridge in `App` can call into it without dragging a
/// pile of `#[cfg(test)]` arms through the closure body. The bridge is
/// a complete no-op in test builds — see the test arm of the
/// `use_effect` for `send_state_signal` below for the rationale.
///
/// This function:
///
/// 1. Snapshots the FROM `AccountConfig`, builds a `DraftEmail` via
///    [`compose_state_to_draft_email`], and captures the current
///    `save_generation` (Gemini G9 stale-result guard for failures
///    only — see step 4).
/// 2. Constructs an [`inboxly_imap::SmtpSender`]:
///      - `Password` / `AppPassword` -> reads the credential via
///        [`inboxly_core::secrets::get_password`], which checks the
///        keyring first and falls back to the `INBOXLY_SMTP_PASSWORD`
///        environment variable when no entry is present (M36 phase 1
///        wired the keyring backend; M36 phase 2 wires it here).
///      - `OAuth2` -> looks up the per-account [`inboxly_imap::SharedOAuth2`]
///        from the [`OAuth2Contexts`] map (passed in via the second
///        argument). The map is built once at app startup from
///        [`crate::startup::STARTUP_ACCOUNTS`] and the keyring's stored
///        refresh tokens; if no entry exists for the FROM account the
///        send fails with a clear "run `inboxly oauth2-authorize`"
///        error. (M36 phase 2.)
/// 3. Enters a retry loop using [`inboxly_imap::smtp::should_retry`]:
///    up to three attempts, with 1 s and 2 s delays for transient
///    errors, immediate stop for permanent rejections.
/// 4. **Stale-result guard for FAILURES ONLY** (Gemini G9 fix): on
///    send SUCCESS we ALWAYS commit the result -- the email already
///    left the wire and the user MUST be told, regardless of whether
///    they typed in the compose view during the send window. On send
///    FAILURE we drop the result if `save_generation` advanced, so a
///    stale failure can never clobber a fresh edit.
/// 5. On success the function:
///      - Enqueues an [`inboxly_core::OfflineAction::AppendSent`] so
///        the next replay pass copies the message to the IMAP Sent
///        folder (Gemini G6 fallback). The actual `APPEND` is deferred
///        because this bridge does not own a session.
///      - Deletes the SQLite draft row via `Store::delete_draft`.
///      - Calls [`inboxly_store::cleanup_draft_dir`] to remove the
///        per-draft attachment directory.
///      - Dispatches `Message::ComposeSendComplete { success: true }`
///        which transitions `send_state` to
///        `Sent { dismiss_pending: true }`. The `ComposeView` shows a
///        "Sent -- Dismiss" overlay until the user clicks dismiss.
/// 6. On failure the function dispatches
///    `Message::ComposeSendComplete { success: false, error: Some(redacted) }`
///    using [`inboxly_imap::smtp::redact_for_log`] so credentials and
///    PII never reach the UI's error banner.
#[cfg(not(test))]
async fn run_send_pipeline(mut app_state: Signal<Inboxly>, oauth2_contexts: OAuth2Contexts) {
    // -- Snapshot pass: resolve the account, draft, and captured
    //    generation from a single `peek()`. peek does NOT subscribe
    //    so this task cannot re-fire its parent effect from inside.
    //    The borrow is dropped at the end of the inner block before
    //    any `app_state.write()` calls so the dispatch path is free
    //    of guard-overlap borrow issues (mirrors the Phase 10 bridge
    //    pattern).
    let (account_config_opt, draft_opt, captured_generation) = {
        let snapshot = app_state.peek();
        let account_config = snapshot
            .accounts
            .get(snapshot.compose.from_account_index)
            .cloned();
        let draft = match account_config.as_ref() {
            Some(cfg) => {
                let account_id = account_id_from_email(&cfg.email);
                compose_state_to_draft_email(&snapshot.compose, account_id)
            }
            None => None,
        };
        let captured_generation = snapshot.compose.save_generation;
        (account_config, draft, captured_generation)
    };

    let Some(account_config) = account_config_opt else {
        app_state.write().update(Message::ComposeSendComplete {
            success: false,
            error: Some("no account configured for this draft".to_string()),
        });
        return;
    };
    let Some(draft) = draft_opt else {
        app_state.write().update(Message::ComposeSendComplete {
            success: false,
            error: Some("compose has no draft_id (OpenCompose did not run)".to_string()),
        });
        return;
    };

    // -- Build the SmtpSender for the resolved auth method.
    let sender = match account_config.auth_method {
        AuthMethod::Password | AuthMethod::AppPassword => {
            // M36 phase 2: route password lookups through the keyring
            // backend. `secrets::get_password` checks the per-account
            // keyring entry first and only falls back to
            // `INBOXLY_SMTP_PASSWORD` if no entry is present (and the
            // env var is non-empty). A `SecretsError::Keyring` from a
            // locked-wallet or DBus failure surfaces as a real error
            // rather than degrading silently.
            let password = match inboxly_core::secrets::get_password(&account_config.email) {
                Ok(Some(value)) => value,
                Ok(None) => {
                    app_state.write().update(Message::ComposeSendComplete {
                        success: false,
                        error: Some(format!(
                            "no password stored for {}; run `inboxly set-password {}` first",
                            account_config.email, account_config.email,
                        )),
                    });
                    return;
                }
                Err(err) => {
                    app_state.write().update(Message::ComposeSendComplete {
                        success: false,
                        error: Some(format!(
                            "keyring lookup failed for {}: {err}",
                            account_config.email,
                        )),
                    });
                    return;
                }
            };
            inboxly_imap::SmtpSender::with_password(account_config.clone(), password)
        }
        AuthMethod::OAuth2 => {
            // M36 phase 2: look up the SharedOAuth2 instance built at
            // startup from this account's keyring-stored refresh
            // token. Keys are lowercased to match the
            // `secrets::user_field` normalization.
            let key = account_config.email.to_ascii_lowercase();
            match oauth2_contexts.get(&key).cloned() {
                Some(oauth2) => {
                    inboxly_imap::SmtpSender::with_oauth2(account_config.clone(), oauth2)
                }
                None => {
                    app_state.write().update(Message::ComposeSendComplete {
                        success: false,
                        error: Some(format!(
                            "no OAuth2 refresh token registered for {}; run `inboxly oauth2-authorize {}` first",
                            account_config.email, account_config.email,
                        )),
                    });
                    return;
                }
            }
        }
    };

    // -- Retry loop. Up to three attempts: 1 s delay, 2 s delay,
    //    then stop. Permanent errors (5xx, malformed message) stop
    //    immediately regardless of attempt number.
    let send_result: Result<(), inboxly_imap::smtp::SmtpError> = {
        use inboxly_imap::smtp::{RetryDecision, should_retry};

        let mut attempt: u32 = 0;
        loop {
            attempt = attempt.saturating_add(1);
            match sender.send(&draft).await {
                Ok(()) => break Ok(()),
                Err(err) => match should_retry(&err, attempt) {
                    RetryDecision::Retry { delay_ms } => {
                        tracing::warn!(
                            attempt,
                            delay_ms,
                            error = ?err,
                            "SMTP send failed, will retry"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                    RetryDecision::Stop => break Err(err),
                },
            }
        }
    };

    match send_result {
        Ok(()) => {
            tracing::info!(message_id = %draft.message_id, "SMTP send succeeded");

            // -- M36 Phase 4: write a local copy to the account's
            //    Maildir `.Sent/cur/` folder BEFORE deleting the SQLite
            //    draft. This closes the "outgoing mail has no local
            //    record" gap from M35 and gives the offline replay
            //    handler a concrete on-disk source for the eventual
            //    server-side IMAP APPEND.
            //
            //    Fail-soft: any error here is logged but does NOT
            //    block the success dispatch. The email already left
            //    the wire, the user MUST be told.
            //
            //    NOTE: the binary does not currently instantiate a
            //    long-lived `MaildirStore` at startup (pre-existing
            //    state from M35), so this block constructs one
            //    on-demand via `Paths::resolve` + per-account
            //    subdirectory. `MaildirStore::new` is a cheap
            //    `PathBuf` wrapper and `init()` is idempotent.
            //
            //    Path mismatch note: the `MaildirStore` doc comment
            //    at `inboxly-store/src/maildir_store.rs:63` says
            //    `<data_dir>/accounts/<account_id>/mail/`, but the
            //    running app resolves it via
            //    `Paths::maildir_root()` = `<data_dir>/maildir/`.
            //    We use the `Paths` helper (source of truth for the
            //    running app) and join `<account_id>/mail/` beneath
            //    it, even though the `accounts/` intermediate from
            //    the doc comment is stale.
            let sent_account_id = account_id_from_email(&account_config.email);
            write_local_maildir_sent(&account_config, &draft, sent_account_id);

            // -- Step 5a: enqueue the AppendSent fallback so the next
            //    replay pass writes the IMAP Sent folder copy. The
            //    bridge does not own an IMAP session so it cannot
            //    APPEND directly; the queued action is the recovery
            //    path.
            let store_handle = app_state.peek().store.as_ref().cloned();
            if let Some(store) = store_handle.as_ref() {
                let queue_action = inboxly_core::OfflineAction::AppendSent {
                    account_id: account_config.email.clone(),
                    draft_message_id: draft.message_id.clone(),
                };
                match serde_json::to_string(&queue_action) {
                    Ok(payload) => {
                        if let Err(e) =
                            store.enqueue_offline_action(queue_action.variant_name(), &payload)
                        {
                            tracing::warn!(
                                error = %e,
                                "failed to enqueue AppendSent offline action"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "failed to serialize AppendSent offline action"
                        );
                    }
                }
            }

            // -- Step 5b: delete the SQLite draft row + clean up the
            //    per-draft attachment directory. The
            //    DismissSentNotice handler will call
            //    cleanup_draft_dir again as a safety net -- that
            //    second call is a no-op when the dir is already
            //    gone.
            if let Some(store) = store_handle
                && let Err(e) = store.delete_draft(&draft.id)
            {
                tracing::warn!(
                    draft_id = %draft.id,
                    error = %e,
                    "failed to delete draft after send"
                );
            }
            if let Err(e) = inboxly_store::cleanup_draft_dir(&draft.id) {
                tracing::warn!(
                    draft_id = %draft.id,
                    error = %e,
                    "failed to cleanup draft attachment dir after send"
                );
            }

            // captured_generation is intentionally unused on the
            // success path -- Gemini G9 mandates we ALWAYS commit
            // success regardless of generation drift.
            let _ = captured_generation;

            app_state.write().update(Message::ComposeSendComplete {
                success: true,
                error: None,
            });
        }
        Err(err) => {
            // Stale-result guard FOR FAILURES ONLY (Gemini G9 fix):
            // if the user typed during the send window, drop the
            // failure rather than clobber the fresh edit.
            let current_generation = app_state.peek().compose.save_generation;
            if current_generation != captured_generation {
                tracing::warn!(
                    captured = captured_generation,
                    current = current_generation,
                    "SMTP send failed but compose generation drifted -- dropping stale failure"
                );
                return;
            }
            let redacted = inboxly_imap::smtp::redact_for_log(&err, &draft);
            tracing::warn!(error = %redacted, "SMTP send failed");
            app_state.write().update(Message::ComposeSendComplete {
                success: false,
                error: Some(redacted),
            });
        }
    }
}

/// Main stylesheet source embedded at compile time.
///
/// Dioxus 0.7's `document::Stylesheet { href: CSS }` pattern with `asset!()`
/// is supposed to serve the asset path through the desktop webview's custom
/// protocol, but for this build the CSS doesn't get fetched at runtime —
/// everything that relies on CSS classes falls back to browser defaults.
/// Embedding via `include_str!` and injecting through a `<style>` tag with
/// `dangerous_inner_html` in the App component guarantees the styles reach
/// the DOM. See `components/app.rs::App` for the inline injection.
static CSS_INLINE: &str = include_str!("../../assets/main.css");

/// Root application component.
///
/// Creates the `Inboxly` state and provides it as context to all children.
/// Sets the `data-theme` attribute for CSS theming, and renders the shell
/// layout: toolbar + body row (nav drawer + content area).
#[component]
pub fn App() -> Element {
    // Initialise app state once on first render.
    //
    // M36 phase 2: pre-existing-bug fix — pull the configured accounts
    // from `crate::startup::STARTUP_ACCOUNTS`, which the binary's
    // `main()` populates from `~/.config/inboxly/config.toml` BEFORE
    // launching Dioxus. Prior to this, the binary set its own
    // `STARTUP_ACCOUNTS` static and the UI never read it, so every
    // running Inboxly instance booted with `accounts: Vec::new()` and
    // the send pipeline could not see the configured FROM addresses.
    // Tests skip this since they construct `Inboxly` directly via
    // `with_accounts` rather than going through `App()`.
    let app_state = use_context_provider(|| {
        let startup_accounts = crate::startup::STARTUP_ACCOUNTS
            .get()
            .cloned()
            .unwrap_or_default();
        Signal::new(Inboxly {
            theme: ThemeConfig::from_system(),
            accounts: startup_accounts,
            ..Inboxly::default()
        })
    });

    // M36 phase 2: provide the per-account OAuth2 context map.
    //
    // The map is read-only after startup (the binary's `main()` builds
    // it once from the keyring-stored refresh tokens) so this is a
    // plain `Arc<HashMap>` rather than a `Signal`. Components that need
    // OAuth2 lookup pull the context via `consume_context::<OAuth2Contexts>()`
    // (or read the cloned local in the spawn-task closure below).
    let oauth2_contexts: OAuth2Contexts = use_context_provider(crate::startup::oauth2_contexts);

    // Body data context — separate from Inboxly so per-write clones
    // don't drag the thread body bytes around. ThreadDetailView reads
    // from this signal directly. (Eng review Issue 1.4.)
    let mut open_thread = use_context_provider(|| Signal::new(None::<Arc<LoadedThread>>));

    // Bridge: watch Inboxly::open_thread_id (the intent), and when it
    // changes, run the loader through the ThreadReader facade and
    // write the result into open_thread (the body). use_memo on the
    // id ensures the effect only re-runs when intent changes, not on
    // every Inboxly write.
    let open_id = use_memo(move || app_state.read().open_thread_id.clone());
    use_effect(move || {
        let id_opt = open_id.read().clone();
        match id_opt {
            Some(id) => {
                // Eng review Issue 4.1: decouple the click handler from
                // the I/O. Set the loading sentinel synchronously, then
                // spawn a Dioxus task to do the actual load. The click
                // handler returns immediately and the UI shows
                // "(loading…)" until the task completes.
                //
                // CAVEAT: this is "cooperative async", not preemptive
                // async. The spawned task runs on the local executor
                // (Dioxus desktop is single-threaded). When the task
                // calls reader.load_thread(), the SQLite query and
                // each per-row file read are still synchronous syscalls
                // — they block the local runtime briefly. For 50
                // messages × 5 ms per read = ~250 ms during which the
                // runtime is blocked. The benefit is that the click
                // event itself returns immediately, the loading state
                // is visible, and the load runs OUTSIDE the click handler.
                //
                // True non-blocking would require either making `Store`
                // Send (large refactor) or switching `read_email_slim`
                // to async file I/O via `tokio::fs::read` (medium
                // refactor). Both are out of M34 scope. See "Out of
                // Scope" section for the deferral note.
                open_thread.set(Some(Arc::new(loading_thread(&id))));
                spawn(async move {
                    // peek() does NOT subscribe — we don't want this
                    // effect to re-fire on every thread_reader field
                    // change. The only reactive dependency is open_id.
                    let snapshot = app_state.peek();
                    let loaded = match snapshot.thread_reader.as_ref() {
                        Some(reader) => {
                            // Issue 1.5 facade: ThreadReader is the
                            // single handle hiding both Store and
                            // MaildirStore.
                            match reader.load_thread(&id) {
                                Ok(emails) => match build_loaded_thread(&id, emails) {
                                    Ok(thread) => thread,
                                    Err(e) => {
                                        // Issue 2.1: surface load failures
                                        // to the user via the error banner,
                                        // AND log for developer visibility.
                                        tracing::warn!("build_loaded_thread({id}) failed: {e}");
                                        error_thread(
                                            &id,
                                            format!("Failed to build thread view: {e}"),
                                        )
                                    }
                                },
                                Err(e) => {
                                    tracing::warn!("ThreadReader::load_thread({id}) failed: {e}");
                                    error_thread(&id, format!("Failed to load thread: {e}"))
                                }
                            }
                        }
                        None => fallback_thread(&id), // no reader wired — not an error
                    };
                    drop(snapshot);

                    // F-1: stale-result guard. If the user has navigated away
                    // (or to a different thread) while we were loading, drop
                    // the result instead of clobbering the current state.
                    // Without this, a slow load for thread A that finishes
                    // after the user clicks thread B would overwrite B's
                    // content with A's. M34's in-memory fallback_thread() is
                    // always fast so the race is latent, but once M36/M37
                    // wires the real ThreadReader on cold file reads this
                    // becomes a user-visible bug.
                    let still_current = app_state.peek().open_thread_id.as_deref() == Some(&id);
                    if !still_current {
                        tracing::debug!(
                            "ThreadReader: dropping stale load result for thread {id} (user navigated away)"
                        );
                        return;
                    }

                    open_thread.set(Some(Arc::new(loaded)));
                });
            }
            None => {
                open_thread.set(None);
            }
        }
    });

    // ===== M36 Phase 8: Reply / Forward prefill bridge =====
    //
    // Mirrors the M34 thread loader pattern at lines 998-1084 above.
    // Watches `compose.pending_reply`. When it transitions from `None`
    // to `Some((thread_id, mode))`, spawns a Dioxus-local task that:
    //
    //   1. Snapshots `app_state.peek()` to grab the live
    //      `thread_reader` handle, the active account index, and the
    //      active account email. Drops the borrow before any work to
    //      keep `Inboxly` writable for the dispatched messages below.
    //   2. If `thread_reader` is `None` (the binary still doesn't
    //      wire one — pre-existing M35 gap), dispatches
    //      `ComposeReplyFailed` and returns. The user sees the
    //      tracing warning; future work will land a real handle.
    //   3. Extracts the `original_email_id` from the `ComposeMode`
    //      enum (every reply variant carries the field).
    //   4. Calls `reader.load_email(...)`. On `BodyNotDownloaded`,
    //      dispatches `ComposeReplyFailed` with a "wait for sync"
    //      reason (G3 fallback path; a future post-M36 phase will
    //      kick a real body fetch off here). On other store errors,
    //      dispatches `ComposeReplyFailed` with the error string.
    //   5. On success, calls
    //      `compose_state_from_original(&original, mode, user_email,
    //      account_index)` to build the prefilled `ComposeState` and
    //      dispatches `Message::ComposeReplyReady { state: Box::new }`.
    //
    // **Threading note:** `ThreadReader` is `!Send + !Sync` (it owns a
    // rusqlite Connection). The Dioxus desktop runtime is
    // single-threaded so the `spawn` future runs on the local
    // executor — passing the `Arc<ThreadReader>` across the await
    // boundary inside that task is fine. Do NOT switch this to
    // `tokio::spawn`; it will fail to compile.
    //
    // **Stale-result guard (mirrors thread loader Issue F-1):** the
    // task captures the `pending_reply` tuple at spawn time. Before
    // dispatching `ComposeReplyReady`, it re-peeks `compose.pending_reply`
    // and verifies it still matches; if the user has dispatched a
    // second `OpenComposeReply` (or any other compose action) in the
    // meantime, the older result is dropped instead of clobbering the
    // newer state.
    //
    // **Initial-render guard:** unlike the auto-save and explicit-save
    // bridges (which use a counter and skip the initial render), this
    // bridge keys on `Option` transitions. The default state has
    // `pending_reply = None` so the initial render is naturally a
    // no-op until the user clicks Reply.
    let pending_reply_signal = use_memo(move || app_state.read().compose.pending_reply.clone());
    use_effect(move || {
        let pending = pending_reply_signal.read().clone();
        let Some((thread_id, mode)) = pending else {
            // None → nothing to do (initial render or post-completion).
            return;
        };

        // Snapshot the live state before spawning. peek() does NOT
        // subscribe — we don't want this effect to re-fire on every
        // unrelated Inboxly write.
        let snapshot = app_state.peek();
        let reader_opt = snapshot.thread_reader.clone();
        let account_index = snapshot.active_account_index;
        let user_email = snapshot
            .accounts
            .get(account_index)
            .map(|a| a.email.clone())
            .unwrap_or_default();
        drop(snapshot);

        spawn(async move {
            // Shadow as `mut` so the dispatch path can call
            // `.write()` (mirrors the M35 send pipeline + auto-save
            // bridges; `Signal` is `Copy` so this is just a binding
            // mode change, not a clone).
            let mut app_state = app_state;
            let Some(reader) = reader_opt else {
                tracing::warn!(
                    thread_id = %thread_id,
                    "ComposeReply prefill: no thread_reader wired (M35 pre-existing gap)"
                );
                app_state.write().update(Message::ComposeReplyFailed {
                    reason: "thread reader not wired — sync path is post-M36 scope".to_string(),
                });
                return;
            };

            // Every reply variant of ComposeMode carries
            // `original_email_id`. The `New` variant is unreachable
            // here because `OpenComposeReply` only dispatches reply
            // modes — but we handle it defensively.
            let original_email_id: String = match &mode {
                ComposeMode::Reply {
                    original_email_id, ..
                }
                | ComposeMode::ReplyAll {
                    original_email_id, ..
                }
                | ComposeMode::Forward {
                    original_email_id, ..
                } => original_email_id.0.clone(),
                ComposeMode::New => {
                    tracing::error!(
                        "ComposeReply prefill bridge dispatched with ComposeMode::New — \
                         this is a logic bug in OpenComposeReply"
                    );
                    app_state.write().update(Message::ComposeReplyFailed {
                        reason: "internal: New mode dispatched to reply bridge".to_string(),
                    });
                    return;
                }
            };

            let original = match reader.load_email(&original_email_id) {
                Ok(loaded) => loaded,
                Err(inboxly_store::thread_reader::ThreadReaderError::BodyNotDownloaded {
                    email_id,
                }) => {
                    tracing::warn!(
                        email_id = %email_id,
                        thread_id = %thread_id,
                        "ComposeReply prefill: body not downloaded — \
                         deferring to (post-M36) on-demand body fetch"
                    );
                    app_state.write().update(Message::ComposeReplyFailed {
                        reason: "body not downloaded — wait for sync to fetch the original"
                            .to_string(),
                    });
                    return;
                }
                Err(inboxly_store::thread_reader::ThreadReaderError::Store(e)) => {
                    tracing::warn!(
                        email_id = %original_email_id,
                        error = %e,
                        "ComposeReply prefill: store error loading original"
                    );
                    app_state.write().update(Message::ComposeReplyFailed {
                        reason: format!("failed to load original: {e}"),
                    });
                    return;
                }
            };

            // Stale-result guard: if pending_reply changed under us
            // (user clicked another Reply, or dispatched
            // CloseCompose), drop this result instead of clobbering
            // the newer state.
            let still_current = matches!(
                app_state.peek().compose.pending_reply.as_ref(),
                Some((tid, _)) if tid == &thread_id
            );
            if !still_current {
                tracing::debug!(
                    thread_id = %thread_id,
                    "ComposeReply prefill: dropping stale result (pending_reply changed)"
                );
                return;
            }

            let new_state =
                compose_state_from_original(&original, mode, &user_email, account_index);
            app_state.write().update(Message::ComposeReplyReady {
                state: Box::new(new_state),
            });
        });
    });

    // ===== M35 Phase 10: Compose auto-save bridge =====
    //
    // Watches the compose dirty flag + save_generation. When the user has
    // unsaved changes, spawn a 30-second timer task. On tick, verify the
    // state machine is still in a savable state (NOT Sending/Sent —
    // Gemini G5 handoff to the send pipeline) and the user hasn't already
    // saved (dirty still true). Capture the save_generation BEFORE the
    // disk write (Issue 1.8) so the post-save dispatch can decide whether
    // the user typed mid-save and the dirty flag should stay set.
    //
    // The Store::insert_draft / Store::update_draft calls are gated
    // behind cfg(not(test)) per the M34 side-effects-in-tests precedent
    // (overnight test runs must not mutate the user's SQLite store).
    //
    // The reactive memo deliberately combines (dirty, save_generation,
    // has_id) so a single rapid edit (which bumps save_generation but
    // leaves dirty=true) re-fires the effect — that's the desired
    // behaviour: each user edit resets the 30 s clock by spawning a
    // fresh timer; the captured generation guard prevents the older
    // timer from clearing dirty after the newer save has already run.
    let compose_dirty_signal = use_memo(move || {
        let s = app_state.read();
        (
            s.compose.dirty,
            s.compose.save_generation,
            s.compose.draft_id.is_some(),
        )
    });
    use_effect(move || {
        let (dirty, _generation, has_id) = *compose_dirty_signal.read();
        if !dirty || !has_id {
            return;
        }
        let mut app_state = app_state;
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;

            // Resolve everything we need from a single peek() — peek
            // does NOT subscribe, so re-firing this effect is impossible
            // from within the spawned task. We extract owned values
            // before the .await-free section ends so the borrow guard
            // is dropped before any further work.
            let (send_state_is_send_pipeline_owned, still_dirty, captured_generation, draft, store) = {
                let snapshot = app_state.peek();

                // Gemini G5: abort if we entered Sending/Sent state
                // during the sleep — the send pipeline owns canonical
                // state during a send.
                let send_state_is_send_pipeline_owned = matches!(
                    snapshot.compose.send_state,
                    ComposeSendState::Sending | ComposeSendState::Sent { .. }
                );

                // Re-check dirty: someone may have already saved.
                let still_dirty = snapshot.compose.dirty;

                // Issue 1.8: capture the generation BEFORE writing,
                // not after. The dispatch back to the handler uses
                // this value as the "if generation still matches,
                // clear dirty" guard.
                let captured_generation = snapshot.compose.save_generation;

                // Resolve the FROM account email so we can derive the
                // deterministic AccountId. Phase 12 will resolve the
                // same id from the same account, so the auto-save row
                // and the send pipeline target the same draft.
                let account_email = snapshot
                    .accounts
                    .get(snapshot.compose.from_account_index)
                    .map(|a| a.email.clone());

                let draft = match account_email {
                    Some(email) => {
                        let account_id = account_id_from_email(&email);
                        compose_state_to_draft_email(&snapshot.compose, account_id)
                    }
                    None => None,
                };

                let store = snapshot.store.clone();

                (
                    send_state_is_send_pipeline_owned,
                    still_dirty,
                    captured_generation,
                    draft,
                    store,
                )
            };

            if send_state_is_send_pipeline_owned {
                tracing::debug!(
                    "compose auto-save aborted: send_state is Sending/Sent (Gemini G5)"
                );
                return;
            }
            if !still_dirty {
                // Someone already saved (manual save in a future phase,
                // or this is a stale timer from a now-cleared dirty
                // flag). No-op.
                return;
            }
            let Some(draft) = draft else {
                tracing::warn!("compose auto-save: no FROM account or no draft_id, skipping save");
                // Still dispatch the tick so the handler can clear
                // dirty if generation matches — there's nothing to
                // persist but we don't want the bridge to keep firing.
                app_state.write().update(Message::ComposeAutoSaveTick {
                    generation: captured_generation,
                });
                return;
            };

            // Gated behind cfg(not(test)) per the M34 side-effects-in-
            // tests precedent. Overnight test runs must never write to
            // the user's real SQLite store.
            #[cfg(not(test))]
            if let Some(store) = store.as_ref() {
                // Try update first (the row should exist on every
                // save after the first); fall back to insert on
                // NotFound for the very first save of a brand-new
                // draft. Other errors are logged and the tick is
                // still dispatched so the bridge doesn't retry on
                // every keystroke.
                match store.update_draft(&draft) {
                    Ok(()) => {
                        tracing::debug!(
                            draft_id = %draft.id,
                            "compose auto-save: update_draft ok"
                        );
                    }
                    Err(inboxly_store::StoreError::NotFound(_)) => {
                        match store.insert_draft(&draft) {
                            Ok(()) => {
                                tracing::debug!(
                                    draft_id = %draft.id,
                                    "compose auto-save: insert_draft ok"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(
                                    draft_id = %draft.id,
                                    error = %e,
                                    "compose auto-save: insert_draft failed"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            draft_id = %draft.id,
                            error = %e,
                            "compose auto-save: update_draft failed"
                        );
                    }
                }
            }
            #[cfg(test)]
            {
                let _ = store; // silence unused warning under cfg(test)
                tracing::debug!(
                    draft_id = %draft.id,
                    "compose auto-save: store write skipped in test mode"
                );
            }

            // Dispatch the tick — the handler clears dirty IFF the
            // captured generation still matches (Issue 1.8 stale-
            // result guard). If the user typed during the save, the
            // generation will have advanced and dirty stays true so
            // the next tick re-saves.
            app_state.write().update(Message::ComposeAutoSaveTick {
                generation: captured_generation,
            });
        });
    });

    // ===== M36 Phase 5: Explicit Save Draft bridge =====
    //
    // Watches `compose.explicit_save_counter`, which is bumped by:
    //
    //   1. The user clicking "Save Draft" — `Message::ComposeSaveDraft`
    //      handler in `app.rs`.
    //   2. The Phase 5 Navigate-with-compose guard — when the user
    //      navigates AWAY from compose with `dirty == true`, the
    //      `Message::Navigate` handler bumps the counter so the
    //      bridge persists the in-flight draft before the view
    //      switch commits.
    //
    // Unlike the Phase 10 auto-save bridge, this one fires
    // synchronously (no `tokio::time::sleep`) — explicit saves are
    // user-initiated, the user expects an immediate write. The
    // structural shape is otherwise identical: peek-and-clone the
    // owned values out of the snapshot, drop the borrow, then do
    // the side-effects (SQLite + Maildir + offline-queue enqueue)
    // on the owned copies.
    //
    // **Scope reduction (M36 Phase 5)**: only SQLite + Maildir
    // `.Drafts/` tiers are wired here. The IMAP `APPEND` tier is
    // enqueued as `OfflineAction::AppendDraft` with a warn-and-skip
    // replay stub in `inboxly-imap::offline_replay`. A future phase
    // will fill in the real replay handler (the Phase 4 `AppendSent`
    // arm is the template).
    //
    // **Initial-render guard**: the `use_memo` fires on first
    // render with `counter == 0`. We deliberately skip in that
    // case so the bridge does not try to save a nonexistent draft
    // on app startup.
    //
    // **Store-is-None guard**: the binary still does not
    // instantiate a long-lived `Store` (pre-existing M35 gap), so
    // every `store.update_draft` call would be a no-op. We
    // structure the code as if the store WAS wired, so a future
    // wiring pass needs no changes here. The on-demand
    // `MaildirStore` construction (mirroring
    // `write_local_maildir_sent`) does NOT require a long-lived
    // `Store`, so the Maildir tier works today even without one.
    let compose_explicit_save_signal = use_memo(move || {
        let s = app_state.read();
        (
            s.compose.explicit_save_counter,
            s.compose.draft_id.is_some(),
        )
    });
    use_effect(move || {
        let (counter, has_id) = *compose_explicit_save_signal.read();
        if counter == 0 || !has_id {
            // Initial render (counter == 0) or no active draft. The
            // toolbar chip + Save Draft button are both gated on
            // `draft_id.is_some()`, so reaching this branch with
            // counter > 0 + has_id == false is defensive.
            return;
        }
        let mut app_state = app_state;
        spawn(async move {
            // Single peek() to extract everything we need. peek does
            // NOT subscribe, so the spawned task can never re-fire
            // the parent effect from inside.
            let (
                send_state_is_send_pipeline_owned,
                captured_generation,
                draft,
                store,
                account_config,
            ) = {
                let snapshot = app_state.peek();

                // Gemini G5: abort if the send pipeline owns canonical
                // state. The user clicked Save Draft after clicking
                // Send (or the Navigate guard fired during a send),
                // and re-saving would race the send pipeline.
                let send_state_is_send_pipeline_owned = matches!(
                    snapshot.compose.send_state,
                    ComposeSendState::Sending | ComposeSendState::Sent { .. }
                );

                // Issue 1.8: capture the generation BEFORE writing.
                // The dispatch back to the handler uses this value
                // as the "if generation still matches, clear dirty"
                // guard.
                let captured_generation = snapshot.compose.save_generation;

                // Resolve the FROM account config so we can derive
                // the deterministic AccountId AND build the message
                // body via `build_sent_folder_bytes`.
                let account_config = snapshot
                    .accounts
                    .get(snapshot.compose.from_account_index)
                    .cloned();

                let draft = match account_config.as_ref() {
                    Some(cfg) => {
                        let account_id = account_id_from_email(&cfg.email);
                        compose_state_to_draft_email(&snapshot.compose, account_id)
                    }
                    None => None,
                };

                let store = snapshot.store.clone();

                (
                    send_state_is_send_pipeline_owned,
                    captured_generation,
                    draft,
                    store,
                    account_config,
                )
            };

            if send_state_is_send_pipeline_owned {
                tracing::debug!("explicit save aborted: send_state is Sending/Sent (Gemini G5)");
                return;
            }
            let Some(draft) = draft else {
                tracing::warn!("explicit save: no FROM account or no draft_id, skipping save");
                // Dispatch the committed message anyway so the dirty
                // flag can be cleared if generation matches — there
                // is nothing to persist but we do not want the bridge
                // to keep firing on subsequent counter bumps.
                app_state
                    .write()
                    .update(Message::ComposeSaveDraftCommitted {
                        generation: captured_generation,
                    });
                return;
            };

            // -- Tier 1: SQLite write. Gated behind cfg(not(test))
            //    per the M34 side-effects-in-tests precedent.
            //    Overnight test runs must never write to the user's
            //    real SQLite store.
            #[cfg(not(test))]
            if let Some(store) = store.as_ref() {
                match store.update_draft(&draft) {
                    Ok(()) => {
                        tracing::debug!(
                            draft_id = %draft.id,
                            "explicit save: update_draft ok"
                        );
                    }
                    Err(inboxly_store::StoreError::NotFound(_)) => {
                        // First save of a brand-new draft (the
                        // auto-save bridge has not run yet). Insert
                        // rather than update.
                        match store.insert_draft(&draft) {
                            Ok(()) => {
                                tracing::debug!(
                                    draft_id = %draft.id,
                                    "explicit save: insert_draft ok"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(
                                    draft_id = %draft.id,
                                    error = %e,
                                    "explicit save: insert_draft failed"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            draft_id = %draft.id,
                            error = %e,
                            "explicit save: update_draft failed"
                        );
                    }
                }
            }
            #[cfg(test)]
            {
                let _ = store.as_ref(); // silence unused under cfg(test)
                tracing::debug!(
                    draft_id = %draft.id,
                    "explicit save: SQLite write skipped in test mode"
                );
            }

            // -- Tier 2: Maildir `.Drafts/` write. Gated behind
            //    cfg(not(test)) for the same reason.
            #[cfg(not(test))]
            if let Some(cfg) = account_config.as_ref() {
                let drafts_account_id = account_id_from_email(&cfg.email);
                write_local_maildir_drafts(cfg, &draft, drafts_account_id);
            }
            #[cfg(test)]
            {
                let _ = account_config.as_ref(); // silence unused under cfg(test)
            }

            // -- Tier 3 (deferred): enqueue an
            //    OfflineAction::AppendDraft so the next sync's
            //    replay loop can `APPEND` to the server's Drafts
            //    folder. The replay handler is a warn-and-skip stub
            //    in M36 Phase 5; a future phase will wire the real
            //    `APPEND` (the Phase 4 AppendSent arm is the
            //    template).
            #[cfg(not(test))]
            if let Some(store) = store.as_ref()
                && let Some(cfg) = account_config.as_ref()
            {
                let queue_action = inboxly_core::OfflineAction::AppendDraft {
                    account_id: cfg.email.clone(),
                    draft_message_id: draft.message_id.clone(),
                };
                match serde_json::to_string(&queue_action) {
                    Ok(payload) => {
                        if let Err(e) =
                            store.enqueue_offline_action(queue_action.variant_name(), &payload)
                        {
                            tracing::warn!(
                                error = %e,
                                "failed to enqueue AppendDraft offline action"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "failed to serialize AppendDraft offline action"
                        );
                    }
                }
            }

            // Dispatch the committed message — the handler clears
            // dirty IFF the captured generation still matches
            // (Issue 1.8 stale-result guard). If the user typed
            // during the save, the generation will have advanced
            // and dirty stays true so the next save (auto or
            // explicit) re-runs.
            app_state
                .write()
                .update(Message::ComposeSaveDraftCommitted {
                    generation: captured_generation,
                });
        });
    });

    // ===== M35 Phase 11: Compose attachment picker bridge =====
    //
    // Watches `compose.attach_picker_counter`, which the
    // `Message::ComposeAttachFile` handler bumps. When the counter
    // changes (and is non-zero — initial mount renders with counter=0
    // and we deliberately do nothing), the bridge spawns a task that:
    //
    //   1. Snapshots `draft_id` and the current total attachment size
    //      from a single `peek()` (peek does NOT subscribe, so the
    //      spawned task can never re-fire the effect from inside).
    //   2. Opens `rfd::AsyncFileDialog::pick_file()` — gated behind
    //      cfg(not(test)) so test runs never open a real dialog
    //      (M34 side-effects-in-tests precedent).
    //   3. Reads file *metadata* via `tokio::fs::metadata` — NOT the
    //      bytes (Gemini G3 metadata-first). The 20 MB total cap is
    //      enforced from the metadata size before any copy work.
    //   4. Generates a UUID-suffixed disk filename via
    //      `inboxly_store::draft_attachments::make_draft_filename`
    //      (Gemini G2 collision-resistant naming) and copies the
    //      source into the per-draft directory. The copy is also
    //      gated behind cfg(not(test)).
    //   5. Builds an `AttachmentDraft` whose `filename` is the ORIGINAL
    //      name (for the chip and the eventual MIME `Content-
    //      Disposition` header) and whose `source` points at the
    //      UUID-suffixed disk path.
    //   6. Dispatches `Message::ComposeAttachmentAdded` so the pure
    //      state machine can register the new attachment.
    //
    // Errors from any step are logged via `tracing::warn!` and the
    // bridge silently aborts — Phase 12 / Phase 13 surface a snackbar.
    let attach_counter_signal = use_memo(move || {
        let s = app_state.read();
        s.compose.attach_picker_counter
    });
    use_effect(move || {
        let counter = *attach_counter_signal.read();
        if counter == 0 {
            // Initial render — Phase 6's `ComposeState::new` starts
            // the counter at 0 so the bridge does not pop a dialog
            // on app start.
            return;
        }
        let mut app_state = app_state;
        spawn(async move {
            // Snapshot the draft_id and current total attachment size
            // from a single peek() — peek does NOT subscribe, so this
            // spawned task can never re-fire the parent effect from
            // within. Mirrors the auto-save bridge pattern.
            let (draft_id, current_total_size) = {
                let snapshot = app_state.peek();
                let Some(draft_id) = snapshot.compose.draft_id.clone() else {
                    tracing::warn!(
                        "attach picker: no draft_id on ComposeState, aborting (OpenCompose should set this eagerly per Gemini G4)"
                    );
                    return;
                };
                let total: u64 = snapshot
                    .compose
                    .attachments
                    .iter()
                    .map(|a| a.size_bytes)
                    .sum();
                (draft_id, total)
            };

            // Step 2: open the file picker. Gated behind cfg(not(test))
            // — test runs must NEVER pop a dialog (M34 precedent).
            #[cfg(not(test))]
            let picked_path: Option<std::path::PathBuf> = {
                use rfd::AsyncFileDialog;
                AsyncFileDialog::new()
                    .add_filter("All files", &["*"])
                    .pick_file()
                    .await
                    .map(|h| h.path().to_path_buf())
            };
            #[cfg(test)]
            let picked_path: Option<std::path::PathBuf> = None;

            let Some(src_path) = picked_path else {
                // User cancelled the dialog (or test mode no-op).
                return;
            };

            let original_name = src_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "attachment".to_string());

            // Step 3: metadata-first size check (Gemini G3) — never
            // load the file bytes into memory.
            let size = match tokio::fs::metadata(&src_path).await {
                Ok(m) => m.len(),
                Err(e) => {
                    tracing::warn!(
                        path = %src_path.display(),
                        error = %e,
                        "attach picker: failed to stat source file"
                    );
                    return;
                }
            };

            /// Hard 20 MB cap on the *total* draft attachment size,
            /// applied across the existing attachments plus the
            /// candidate. Matches the cap used by the
            /// `ComposeAttachmentTooLarge` snackbar.
            const MAX_TOTAL_BYTES: u64 = 20 * 1024 * 1024;
            // saturating_add: defensive against pathological inputs
            // (a 18 EB file would otherwise wrap, and clippy
            // arithmetic_side_effects would catch a bare `+`).
            if current_total_size.saturating_add(size) > MAX_TOTAL_BYTES {
                tracing::warn!(
                    filename = %original_name,
                    size_bytes = size,
                    current_total_bytes = current_total_size,
                    "attach picker: file would push compose draft over the 20 MB cap"
                );
                app_state.write().update(Message::ComposeAttachmentTooLarge);
                return;
            }

            // Step 4: per-draft directory + UUID-suffixed filename.
            let dir = match inboxly_store::draft_attachments::ensure_draft_dir(&draft_id) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(
                        draft_id = %draft_id,
                        error = %e,
                        "attach picker: ensure_draft_dir failed"
                    );
                    return;
                }
            };
            let uuid = uuid::Uuid::new_v4();
            let disk_name =
                inboxly_store::draft_attachments::make_draft_filename(&original_name, uuid);
            let dest = dir.join(&disk_name);

            // Copy the source into the per-draft directory. Gated
            // behind cfg(not(test)) so test runs never touch disk.
            #[cfg(not(test))]
            if let Err(e) = tokio::fs::copy(&src_path, &dest).await {
                tracing::warn!(
                    src = %src_path.display(),
                    dest = %dest.display(),
                    error = %e,
                    "attach picker: copy to per-draft directory failed"
                );
                return;
            }
            #[cfg(test)]
            {
                // Suppress unused-variable warnings under cfg(test).
                let _ = &src_path;
                let _ = &dest;
            }

            let mime_type = mime_from_extension(&original_name);

            let att = inboxly_core::AttachmentDraft {
                filename: original_name,
                mime_type,
                size_bytes: size,
                source: inboxly_core::AttachmentSource::Disk(dest),
            };

            app_state
                .write()
                .update(Message::ComposeAttachmentAdded(Arc::new(att)));
        });
    });

    // ===== M35 Phase 12: Compose send bridge =====
    //
    // Watches `compose.send_state`. When the
    // `Message::ComposeSendDraft` handler transitions
    // `Idle -> Sending`, this effect spawns
    // [`run_send_pipeline`] (defined above) which owns the entire
    // SMTP -> retry -> AppendSent -> dismiss flow. The pipeline is a
    // hard no-op in test builds (M34 side-effects-in-tests
    // precedent). See `run_send_pipeline`'s doc comment for the full
    // success/failure/Gemini G6/G9 semantics.
    let send_state_signal = use_memo(move || {
        let s = app_state.read();
        matches!(s.compose.send_state, ComposeSendState::Sending)
    });
    use_effect(move || {
        let is_sending = *send_state_signal.read();
        if !is_sending {
            return;
        }
        let app_state = app_state;
        // Clone the OAuth2Contexts handle into the spawned task. The
        // contexts map is wrapped in `Arc` (the type alias is
        // `Arc<HashMap<String, SharedOAuth2>>`) so the clone is one
        // atomic increment.
        let oauth2_contexts = oauth2_contexts.clone();
        spawn(async move {
            // The entire send pipeline is a no-op in test builds. The
            // bridge would otherwise need to dial real SMTP servers,
            // touch the user's SQLite store, and read environment
            // variables -- side effects that are forbidden in tests
            // per the M34 incident precedent. Tests that exercise
            // the state machine after a "send" should dispatch
            // `Message::ComposeSendComplete` directly rather than
            // relying on this bridge.
            #[cfg(not(test))]
            run_send_pipeline(app_state, oauth2_contexts).await;
            #[cfg(test)]
            {
                let _ = app_state;
                let _ = oauth2_contexts;
            }
        });
    });

    // Eng review Issue 2.3: install a document-level Escape handler.
    // The JS side checks `event.target` to avoid intercepting Escape
    // inside text inputs (search box, compose fields, settings), where
    // the user almost certainly meant "clear my input". Only when the
    // active element is NOT an input does the listener forward the
    // Escape to Rust, which closes the open thread.
    //
    // Eng review Issue 4.2: the listener is bound to a globally-accessible
    // window property so the use_drop cleanup hook below can remove it
    // by reference. Without that, App component re-mounts (hot reload,
    // future tests) would stack listeners.
    use_effect(move || {
        let mut app_state = app_state;
        spawn(async move {
            let mut ch = document::eval(
                r#"
                window.__inboxly_escape_handler = (e) => {
                    if (e.key !== 'Escape') return;
                    // Don't intercept Escape inside text inputs.
                    const t = e.target;
                    const tag = t && t.tagName;
                    if (tag === 'INPUT' || tag === 'TEXTAREA') return;
                    if (t && t.isContentEditable) return;
                    e.preventDefault();
                    dioxus.send('escape');
                };
                document.addEventListener('keydown', window.__inboxly_escape_handler, true);
                "#,
            );
            // The JS side sends a literal "escape" string on every
            // qualifying keypress. We don't care about the value —
            // any message on the channel is an Escape intent.
            while let Ok(_token) = ch.recv::<String>().await {
                let state = app_state.peek();
                if state.open_thread_id.is_some() {
                    drop(state);
                    app_state.write().update(Message::CloseThread);
                }
            }
        });
    });

    // Eng review Issue 4.2: cleanup on App component unmount.
    // Removes the global Escape listener so re-mounts don't stack.
    // Fire-and-forget — we don't need to await the JS execution.
    use_drop(|| {
        let _ = document::eval(
            r#"
            if (window.__inboxly_escape_handler) {
                document.removeEventListener('keydown', window.__inboxly_escape_handler, true);
                delete window.__inboxly_escape_handler;
            }
            "#,
        );
    });

    // Install a one-shot global click interceptor for email-body links.
    // Sanitised email HTML has all `<a href>` rewritten to a sentinel
    // prefix (see `crate::sanitize::EXT_URL_SENTINEL`) so clicks don't
    // navigate the webview away from the app. This listener catches
    // those clicks, strips the sentinel prefix, and dispatches
    // OpenExternalUrl which calls open::that() to hand the URL to the
    // system browser.
    //
    // Eng review Issue 2.4: the sentinel string is interpolated from
    // the Rust constant via format! so there's a single source of
    // truth. Do NOT hardcode the prefix in the JS source.
    //
    // The listener is attached once at mount (no cleanup) and runs
    // for the life of the app. It's a no-op for any href that doesn't
    // start with the sentinel, so it doesn't interfere with Dioxus's
    // own button onclicks.
    use_effect(move || {
        let mut app_state = app_state;
        spawn(async move {
            // Single source of truth for the sentinel prefix.
            let sentinel = crate::sanitize::EXT_URL_SENTINEL;
            // Build the JS source at runtime so the prefix is
            // interpolated from the Rust constant. Note the doubled
            // braces `{{` / `}}` are format! literal escapes — the
            // resulting JS has single braces as expected.
            //
            // Eng review Issue 4.2: the handler is bound to a globally-
            // accessible window property so the use_drop cleanup hook
            // below can remove it on App unmount.
            let js_source = format!(
                r#"
                window.__inboxly_link_click_handler = (e) => {{
                    const a = e.target.closest && e.target.closest('a');
                    if (!a) return;
                    const href = a.getAttribute('href');
                    const SENTINEL = '{sentinel}';
                    if (href && href.startsWith(SENTINEL)) {{
                        e.preventDefault();
                        e.stopPropagation();
                        const url = href.substring(SENTINEL.length);
                        dioxus.send(url);
                    }}
                }};
                document.addEventListener('click', window.__inboxly_link_click_handler, true);
                "#
            );
            let mut ch = document::eval(&js_source);
            // Drain the channel: every URL the JS side forwards becomes
            // one OpenExternalUrl dispatch. The `recv` loop never exits
            // until the component unmounts (app close).
            while let Ok(url) = ch.recv::<String>().await {
                app_state.write().update(Message::OpenExternalUrl(url));
            }
        });
    });

    // Eng review Issue 4.2: cleanup on App component unmount.
    // Removes the global click listener so re-mounts don't stack.
    use_drop(|| {
        let _ = document::eval(
            r#"
            if (window.__inboxly_link_click_handler) {
                document.removeEventListener('click', window.__inboxly_link_click_handler, true);
                delete window.__inboxly_link_click_handler;
            }
            "#,
        );
    });

    let is_dark = app_state.read().theme.colors.is_dark;
    let drawer_open = app_state.read().drawer_open;
    let active_view = app_state.read().active_view;

    rsx! {
        // Dioxus 0.7's `document::Stylesheet { href: CSS }` pattern does not
        // reach the desktop webview in our current setup (the asset protocol
        // returns nothing and every .css-class rule falls back to browser
        // defaults). The inline `<style>` tag below embeds the same CSS via
        // `include_str!` and guarantees the rules land in the DOM.
        style { dangerous_inner_html: CSS_INLINE }

        div {
            class: "app-shell",
            "data-theme": if is_dark { "dark" } else { "light" },

            Toolbar {}

            div {
                class: "body-row",

                if drawer_open && active_view != ActiveView::Settings {
                    NavDrawer {}
                }

                ContentArea {}
            }
        }

        UndoSnackbar {}

        // SpeedDialFab dispatches OpenCompose on click. Hide it in Compose
        // view so (a) it doesn't visually stack on top of the Send button
        // in the compose footer and (b) clicking it mid-compose doesn't
        // reset the current draft via the "idempotent second OpenCompose"
        // branch. Also hide in Settings per the existing rule.
        if !matches!(active_view, ActiveView::Settings | ActiveView::Compose) {
            SpeedDialFab {}
            SnoozePicker {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use inboxly_core::{ComposeMode, Contact, EmailId, SlimEmailContent, ThreadId};
    use inboxly_store::EmailRow;
    use inboxly_store::thread_reader::LoadedEmail;

    use super::{
        ComposeState, account_id_from_email, apply_forward_headers, apply_reply_headers,
        compose_state_from_original, compose_state_to_draft_email,
    };

    /// Build a `LoadedEmail` fixture with sensible defaults for the
    /// Phase 7 helper tests. Each test mutates only the fields it cares
    /// about (subject, from, to_json, references, body) and accepts the
    /// rest as defaults. The Message-ID column is non-empty so reply
    /// helpers exercise the references-chain branch.
    #[allow(clippy::too_many_arguments)] // test fixture: each arg overrides one EmailRow column
    fn fake_loaded_email(
        subject: &str,
        from_name: Option<&str>,
        from_address: &str,
        to_json: &str,
        cc_json: &str,
        body_text: Option<&str>,
        message_id: Option<&str>,
        references_json: Option<&str>,
    ) -> LoadedEmail {
        let row = EmailRow {
            id: "e1".into(),
            account_id: "a1".into(),
            thread_id: "t1".into(),
            from_name: from_name.map(str::to_string),
            from_address: from_address.into(),
            to_json: to_json.into(),
            cc_json: cc_json.into(),
            subject: subject.into(),
            snippet: String::new(),
            // 2026-04-07 14:32 UTC ish — exact value isn't relevant; the
            // tests assert on the helper's behaviour, not the exact
            // attribution string (date format is verified separately).
            date: 1_775_572_320,
            maildir_path: String::new(),
            flags: 0,
            size_bytes: 0,
            imap_uid: 1,
            imap_folder: "INBOX".into(),
            has_attachments: false,
            body_downloaded: body_text.is_some(),
            message_id_header: message_id.map(str::to_string),
            in_reply_to: None,
            references_json: references_json.map(str::to_string),
        };
        let content = body_text.map(|body| SlimEmailContent {
            id: EmailId("<e1@example.com>".into()),
            body_text: Some(body.to_string()),
            body_html: None,
            attachments: Vec::new(),
        });
        LoadedEmail { row, content }
    }

    /// Convenience: a Reply mode with throwaway thread/email ids.
    fn reply_mode() -> ComposeMode {
        ComposeMode::Reply {
            thread_id: ThreadId::new(),
            original_email_id: EmailId("<e1@example.com>".into()),
        }
    }

    /// Convenience: a ReplyAll mode with throwaway thread/email ids.
    fn reply_all_mode() -> ComposeMode {
        ComposeMode::ReplyAll {
            thread_id: ThreadId::new(),
            original_email_id: EmailId("<e1@example.com>".into()),
        }
    }

    /// Convenience: a Forward mode with throwaway thread/email ids.
    fn forward_mode() -> ComposeMode {
        ComposeMode::Forward {
            thread_id: ThreadId::new(),
            original_email_id: EmailId("<e1@example.com>".into()),
        }
    }

    /// `compose_state_to_draft_email` returns `None` when the compose
    /// state has no `draft_id`. The bridge depends on this so it can
    /// safely skip the disk write rather than panicking on a half-
    /// initialised state.
    #[test]
    fn helper_returns_none_without_draft_id() {
        let compose = ComposeState::new();
        assert!(compose.draft_id.is_none());
        let id = account_id_from_email("alice@example.com");
        assert!(compose_state_to_draft_email(&compose, id).is_none());
    }

    /// Happy path: with a `draft_id` set, the helper materialises the
    /// `Arc`-wrapped recipients into owned `Contact` values, copies the
    /// subject/body, and stamps a Message-ID derived from the draft id.
    #[test]
    fn helper_materialises_arc_recipients_into_owned() {
        let mut compose = ComposeState::new();
        compose.draft_id = Some("d8a17e3b-1234-4abc-9def-000000000001".to_string());
        compose.subject = "Hello".to_string();
        compose.body_markdown = "Body".to_string();
        compose
            .to
            .push(Arc::new(Contact::new("Bob", "bob@example.com")));

        let id = account_id_from_email("alice@example.com");
        let draft = compose_state_to_draft_email(&compose, id)
            .expect("draft_id is set, helper must return Some");
        assert_eq!(draft.id, "d8a17e3b-1234-4abc-9def-000000000001");
        assert_eq!(draft.subject, "Hello");
        assert_eq!(draft.body_markdown, "Body");
        assert_eq!(
            draft.message_id,
            "<d8a17e3b-1234-4abc-9def-000000000001@inboxly.local>"
        );
        assert_eq!(draft.to.len(), 1);
        assert_eq!(draft.to[0].address, "bob@example.com");
        assert_eq!(draft.account_id, id);
    }

    /// `account_id_from_email` is deterministic — same email always
    /// produces the same `AccountId`. The auto-save bridge relies on
    /// this so successive saves of the same draft target the same row,
    /// and Phase 12's send pipeline can resolve the same id.
    #[test]
    fn account_id_from_email_is_deterministic() {
        let a = account_id_from_email("alice@example.com");
        let b = account_id_from_email("alice@example.com");
        let c = account_id_from_email("bob@example.com");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // -----------------------------------------------------------------
    // M36 Phase 7: apply_reply_headers / apply_forward_headers /
    // compose_state_from_original tests
    // -----------------------------------------------------------------

    /// `apply_reply_headers` populates subject (Re:), body quote, and
    /// the In-Reply-To / References threading headers from the parent.
    #[test]
    fn apply_reply_headers_sets_subject_body_in_reply_to_references() {
        let original = fake_loaded_email(
            "hello",
            Some("Alice"),
            "alice@example.com",
            "[]",
            "[]",
            Some("first line\nsecond line"),
            Some("<parent-msg@host>"),
            Some(r#"["<root@host>","<a@host>"]"#),
        );
        let mut state = ComposeState::new();
        apply_reply_headers(&mut state, &original);

        assert_eq!(state.subject, "Re: hello");
        assert!(
            state.body_markdown.starts_with("\n\n"),
            "body is prepended with two newlines so the cursor lands above the quote"
        );
        assert!(
            state
                .body_markdown
                .contains("Alice <alice@example.com> wrote:")
        );
        assert!(state.body_markdown.contains("> first line"));
        assert!(state.body_markdown.contains("> second line"));
        assert_eq!(state.in_reply_to.as_deref(), Some("<parent-msg@host>"));
        // References = parent.references + parent.message-id, joined by space.
        assert_eq!(
            state.references.as_deref(),
            Some("<root@host> <a@host> <parent-msg@host>")
        );
    }

    /// `apply_reply_headers` collapses an existing `Re:` prefix on the
    /// parent subject (Phase 6's `subject_for_reply` does the dedup).
    #[test]
    fn apply_reply_headers_preserves_re_prefix_dedup() {
        let original = fake_loaded_email(
            "Re: foo",
            Some("Alice"),
            "alice@example.com",
            "[]",
            "[]",
            None,
            Some("<m1@host>"),
            None,
        );
        let mut state = ComposeState::new();
        apply_reply_headers(&mut state, &original);

        assert_eq!(state.subject, "Re: foo");
        // Without parent references_json, the chain is just the parent msgid.
        assert_eq!(state.references.as_deref(), Some("<m1@host>"));
    }

    /// `apply_forward_headers` adds an `Fwd:` prefix and a
    /// "Forwarded message" quote block built from the parent's headers
    /// and body.
    #[test]
    fn apply_forward_headers_sets_fwd_prefix_and_quote() {
        let to_json = r#"[{"name":"Bob","address":"bob@example.com"}]"#;
        let original = fake_loaded_email(
            "hello",
            Some("Alice"),
            "alice@example.com",
            to_json,
            "[]",
            Some("body line"),
            Some("<parent@host>"),
            None,
        );
        let mut state = ComposeState::new();
        apply_forward_headers(&mut state, &original);

        assert_eq!(state.subject, "Fwd: hello");
        assert!(state.body_markdown.starts_with("\n\n"));
        assert!(
            state
                .body_markdown
                .contains("---------- Forwarded message ----------\n")
        );
        assert!(
            state
                .body_markdown
                .contains("From: Alice <alice@example.com>\n")
        );
        assert!(state.body_markdown.contains("Subject: hello\n"));
        assert!(state.body_markdown.contains("To: Bob <bob@example.com>\n"));
        assert!(state.body_markdown.ends_with("body line"));
    }

    /// Forward starts a brand-new thread per Gmail convention, so the
    /// In-Reply-To / References headers are explicitly None even when
    /// the parent has a Message-ID.
    #[test]
    fn apply_forward_headers_leaves_in_reply_to_none() {
        let original = fake_loaded_email(
            "hello",
            Some("Alice"),
            "alice@example.com",
            "[]",
            "[]",
            None,
            Some("<parent@host>"),
            Some(r#"["<root@host>"]"#),
        );
        // Pre-populate the state with bogus values to prove the helper
        // actively clears them rather than just leaving the default.
        let mut state = ComposeState::new();
        state.in_reply_to = Some("<should-be-cleared@host>".into());
        state.references = Some("<should-be-cleared@host>".into());

        apply_forward_headers(&mut state, &original);

        assert!(state.in_reply_to.is_none());
        assert!(state.references.is_none());
    }

    /// Reply mode dispatch: To = [original.from], Cc empty, mode set.
    #[test]
    fn compose_state_from_original_reply_sets_single_to_recipient() {
        let original = fake_loaded_email(
            "hello",
            Some("Alice"),
            "alice@example.com",
            r#"[{"name":"Carol","address":"carol@example.com"}]"#,
            "[]",
            Some("body"),
            Some("<m1@host>"),
            None,
        );
        let state = compose_state_from_original(&original, reply_mode(), "me@example.com", 0);

        assert_eq!(state.to.len(), 1);
        assert_eq!(state.to[0].address, "alice@example.com");
        assert!(
            state.cc.is_empty(),
            "Reply (not ReplyAll) does not populate Cc"
        );
        assert!(matches!(state.mode, ComposeMode::Reply { .. }));
        assert!(state.subject.starts_with("Re: "));
    }

    /// ReplyAll dispatch: user is excluded from the merged To+Cc list,
    /// the original sender becomes the sole To, and Cc auto-expands.
    #[test]
    fn compose_state_from_original_reply_all_excludes_user_email() {
        let original = fake_loaded_email(
            "hello",
            Some("Alice"),
            "alice@example.com",
            // To: me@... and carol@... — me must be dropped.
            r#"[{"name":"Me","address":"me@example.com"},{"name":"Carol","address":"carol@example.com"}]"#,
            r#"[{"name":"Dave","address":"dave@example.com"}]"#,
            Some("body"),
            Some("<m1@host>"),
            None,
        );
        let state = compose_state_from_original(&original, reply_all_mode(), "me@example.com", 0);

        assert_eq!(state.to.len(), 1);
        assert_eq!(state.to[0].address, "alice@example.com");
        // Cc = (orig.to ∪ orig.cc) \ user → carol + dave (me dropped).
        assert_eq!(state.cc.len(), 2);
        assert_eq!(state.cc[0].address, "carol@example.com");
        assert_eq!(state.cc[1].address, "dave@example.com");
        assert!(
            state.show_cc_bcc,
            "Cc/Bcc row auto-expands when ReplyAll produces a non-empty Cc list"
        );
    }

    /// Forward dispatch leaves To/Cc/Bcc empty (the user picks the
    /// recipient list manually).
    #[test]
    fn compose_state_from_original_forward_empty_to_list() {
        let original = fake_loaded_email(
            "hello",
            Some("Alice"),
            "alice@example.com",
            r#"[{"name":"Bob","address":"bob@example.com"}]"#,
            "[]",
            Some("body"),
            Some("<m1@host>"),
            None,
        );
        let state = compose_state_from_original(&original, forward_mode(), "me@example.com", 0);

        assert!(state.to.is_empty());
        assert!(state.cc.is_empty());
        assert!(state.bcc.is_empty());
        assert!(state.subject.starts_with("Fwd: "));
        assert!(matches!(state.mode, ComposeMode::Forward { .. }));
        assert!(state.in_reply_to.is_none());
        assert!(state.references.is_none());
    }

    /// The dispatcher generates an eager UUID `draft_id` (Gemini G4 —
    /// matches `OpenCompose` behaviour so the per-draft attachment dir
    /// can be created before the first auto-save tick) and propagates
    /// `user_account_index` into `from_account_index`.
    #[test]
    fn compose_state_from_original_sets_draft_id_and_from_account_index() {
        let original = fake_loaded_email(
            "hello",
            Some("Alice"),
            "alice@example.com",
            "[]",
            "[]",
            None,
            Some("<m1@host>"),
            None,
        );
        let state = compose_state_from_original(&original, reply_mode(), "me@example.com", 3);

        let draft_id = state.draft_id.as_deref().expect("eager draft_id assigned");
        assert!(
            uuid::Uuid::parse_str(draft_id).is_ok(),
            "draft_id is a valid UUID, got {draft_id}"
        );
        assert_eq!(state.from_account_index, 3);
    }
}
