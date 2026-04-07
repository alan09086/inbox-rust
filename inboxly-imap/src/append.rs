//! IMAP `APPEND` helpers for compose drafts and Sent-folder copies.
//!
//! These helpers are the entry points for writing drafts to the IMAP
//! server's Drafts folder ([`imap_append_draft`]) and Sent folder
//! ([`imap_append_sent`]). Both render the message via
//! [`crate::smtp::build_rfc5322_for_sent_folder`] (`keep_bcc` = true) so
//! the user's server-side copies retain the Bcc list for audit — see
//! Gemini G1 in the M35 plan.
//!
//! Both helpers are fail-soft callers: they return [`Result`] so the
//! send pipeline can log and continue even if the server rejects the
//! `APPEND`. A failed [`imap_append_sent`] in Phase 12 is recovered via
//! an offline replay queue (Gemini G6).
//!
//! The caller owns the [`async_imap::Session`] lifetime. These helpers
//! do not authenticate or reconnect — pass an already-authenticated
//! session.
//!
//! # M35b folder name TODO
//!
//! For M35b the helpers hardcode `"Drafts"` and `"Sent"` as the IMAP
//! mailbox names. These are correct for IMAP servers that follow RFC
//! 6154 SPECIAL-USE conventions and for most providers, but Gmail uses
//! `"[Gmail]/Drafts"` and `"[Gmail]/Sent Mail"`. The richer fix is to
//! plumb a resolved [`crate::folders::WellKnownFolders`] (or its
//! `drafts` / `sent` strings) through the call site so each provider's
//! actual folder name is used. That refactor is deferred to M36 once
//! Phase 13 manual verification confirms whether Gmail breaks.

use std::str::FromStr;

use async_imap::Session;
use lettre::Address;
use lettre::message::Mailbox;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tracing::{info, warn};

use inboxly_core::{AccountConfig, DraftEmail};

use crate::error::{ImapError, Result};
use crate::smtp::build_rfc5322_for_sent_folder;

/// IMAP mailbox name to use for drafts. See module-level TODO.
const DRAFTS_MAILBOX: &str = "Drafts";

/// IMAP mailbox name to use for sent messages. See module-level TODO.
const SENT_MAILBOX: &str = "Sent";

/// `APPEND` a draft to the server's Drafts folder.
///
/// Sets the `\Draft` flag so IMAP clients recognise it as a draft. The
/// rendered message retains the `Bcc:` header so the user's Drafts
/// folder copy shows who they intended to Bcc.
///
/// # Errors
///
/// Returns [`ImapError::Io`] if the configured `from` address is
/// malformed or the message cannot be assembled, and
/// [`ImapError::Imap`] if the server rejects the `APPEND` command.
///
/// The caller is expected to log-and-continue on error rather than
/// abort the send pipeline.
pub async fn imap_append_draft(
    session: &mut Session<TlsStream<TcpStream>>,
    config: &AccountConfig,
    draft: &DraftEmail,
) -> Result<()> {
    let from = build_from_mailbox(config)?;
    let message = build_rfc5322_for_sent_folder(draft, &from)
        .map_err(|e| ImapError::Io(std::io::Error::other(format!("build draft message: {e}"))))?;
    let bytes = message.formatted();

    info!(
        "IMAP APPEND draft (message_id={}, {} bytes) to {}",
        draft.message_id,
        bytes.len(),
        DRAFTS_MAILBOX
    );

    session
        .append(DRAFTS_MAILBOX, Some(r"(\Draft)"), None, bytes.as_slice())
        .await
        .map_err(|e| {
            warn!("IMAP APPEND draft failed: {}", e);
            ImapError::Imap(e)
        })?;

    Ok(())
}

/// `APPEND` a sent message to the server's Sent folder.
///
/// Sets the `\Seen` flag so the message doesn't show as unread in the
/// user's own Sent view. The rendered message retains the `Bcc:` header
/// so the user can later see who they Bcc'd from their own Sent folder.
///
/// # Errors
///
/// Returns [`ImapError::Io`] if the configured `from` address is
/// malformed or the message cannot be assembled, and
/// [`ImapError::Imap`] if the server rejects the `APPEND` command.
///
/// Phase 12's send bridge enqueues a replay action on failure rather
/// than failing the overall send (Gemini G6).
pub async fn imap_append_sent(
    session: &mut Session<TlsStream<TcpStream>>,
    config: &AccountConfig,
    draft: &DraftEmail,
) -> Result<()> {
    let from = build_from_mailbox(config)?;
    let message = build_rfc5322_for_sent_folder(draft, &from)
        .map_err(|e| ImapError::Io(std::io::Error::other(format!("build sent message: {e}"))))?;
    let bytes = message.formatted();

    info!(
        "IMAP APPEND sent (message_id={}, {} bytes) to {}",
        draft.message_id,
        bytes.len(),
        SENT_MAILBOX
    );

    session
        .append(SENT_MAILBOX, Some(r"(\Seen)"), None, bytes.as_slice())
        .await
        .map_err(|e| {
            warn!("IMAP APPEND sent failed: {}", e);
            ImapError::Imap(e)
        })?;

    Ok(())
}

/// Build a [`Mailbox`] from an [`AccountConfig`].
///
/// Mirrors the same construction used by
/// [`crate::smtp::SmtpSender`] so both transports render an identical
/// `From:` header. Centralising this in one place would mean exposing
/// it from the SMTP module, but for now the duplication is small enough
/// (a single function) that we keep `append` self-contained.
fn build_from_mailbox(config: &AccountConfig) -> Result<Mailbox> {
    let address = Address::from_str(&config.email).map_err(|e| {
        ImapError::Io(std::io::Error::other(format!(
            "invalid from address {}: {e}",
            config.email
        )))
    })?;
    let name = if config.display_name.is_empty() {
        None
    } else {
        Some(config.display_name.clone())
    };
    Ok(Mailbox::new(name, address))
}
