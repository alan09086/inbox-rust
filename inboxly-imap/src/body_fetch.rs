//! RFC822 body FETCH commands for Phase 2 body download.
//!
//! Provides batch and single-email fetch functions that retrieve raw
//! RFC822 message bodies from IMAP.

use std::fmt::Debug;

use async_imap::Session;
use futures::TryStreamExt;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::ImapError;

/// Maximum batch size for RFC822 FETCH commands.
pub const BODY_FETCH_BATCH_SIZE: usize = 500;

/// Fetch RFC822 bodies for a batch of UIDs.
///
/// Returns `(uid, raw_bytes)` pairs for each successfully fetched message.
/// UIDs that don't appear in the IMAP response are silently skipped
/// (the server may have expunged them between the UID list query and this fetch).
///
/// # Errors
///
/// Returns `ImapError` if the IMAP FETCH command itself fails
/// (connection error, protocol error, etc.).
pub async fn fetch_bodies_batch<S>(
    session: &mut Session<S>,
    uids: &[i64],
) -> Result<Vec<(u32, Vec<u8>)>, ImapError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Debug,
{
    if uids.is_empty() {
        return Ok(Vec::new());
    }

    // Build UID set string: "45000,44999,44998,...,44501"
    let uid_set = uids
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let fetches: Vec<_> = session
        .uid_fetch(&uid_set, "RFC822")
        .await?
        .try_collect()
        .await?;

    let mut results = Vec::with_capacity(uids.len());
    for fetch in &fetches {
        if let (Some(uid), Some(body)) = (fetch.uid, fetch.body()) {
            results.push((uid, body.to_vec()));
        }
    }

    Ok(results)
}

/// Fetch a single email's RFC822 body by UID (for on-demand fetch).
///
/// Returns `None` if the UID does not exist or has no body.
///
/// # Errors
///
/// Returns `ImapError` if the IMAP FETCH command fails.
pub async fn fetch_body_single<S>(
    session: &mut Session<S>,
    uid: u32,
) -> Result<Option<Vec<u8>>, ImapError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Debug,
{
    let uid_str = uid.to_string();
    let fetches: Vec<_> = session
        .uid_fetch(&uid_str, "RFC822")
        .await?
        .try_collect()
        .await?;

    for fetch in &fetches {
        if fetch.uid == Some(uid) && fetch.body().is_some() {
            return Ok(fetch.body().map(|b| b.to_vec()));
        }
    }

    Ok(None)
}
