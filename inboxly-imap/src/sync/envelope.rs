use super::error::{SyncError, SyncResult};
use chrono::{DateTime, Utc};

/// Parsed envelope data ready for SQLite insertion.
///
/// This is an intermediate struct — not the full `EmailMeta` from `inboxly-core`.
/// It contains exactly the columns needed for the `emails` table INSERT.
#[derive(Debug, Clone)]
pub struct EnvelopeData {
    pub message_id: String,
    pub account_id: String,
    pub imap_uid: u32,
    pub imap_folder: String,
    pub from_name: String,
    pub from_address: String,
    pub to_json: String,
    pub cc_json: String,
    pub subject: String,
    pub date_unix: i64,
    pub size_bytes: u64,
    /// Bitmask: bit 0 = seen, bit 1 = flagged, bit 2 = answered, bit 3 = draft, bit 4 = deleted
    pub flags: u32,
    pub in_reply_to: Option<String>,
    pub references_json: Option<String>,
}

/// Parse an IMAP ENVELOPE date string (RFC 2822 format) into a UTC DateTime.
pub fn parse_envelope_date(raw: &[u8]) -> SyncResult<DateTime<Utc>> {
    let s = std::str::from_utf8(raw)
        .map_err(|_| SyncError::DateParse {
            uid: 0,
            raw: String::from_utf8_lossy(raw).to_string(),
        })?
        .trim();

    // chrono's parse_from_rfc2822 does not handle the optional weekday prefix
    // (e.g. "Mon, "). Strip it before trying the standard parser.
    let without_dayname = if let Some(idx) = s.find(", ") {
        let prefix = &s[..idx];
        if prefix.len() == 3 && prefix.chars().all(|c| c.is_ascii_alphabetic()) {
            &s[idx + 2..]
        } else {
            s
        }
    } else {
        s
    };

    // Try RFC 2822 (the standard ENVELOPE date format)
    if let Ok(dt) = DateTime::parse_from_rfc2822(without_dayname) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Some servers omit the day name — try common alternative formats
    for fmt in &[
        "%d %b %Y %H:%M:%S %z",
        "%d %b %Y %H:%M:%S",
        "%a, %d %b %Y %H:%M:%S",
    ] {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Ok(dt.and_utc());
        }
        if let Ok(dt) = DateTime::parse_from_str(s, fmt) {
            return Ok(dt.with_timezone(&Utc));
        }
    }

    Err(SyncError::DateParse {
        uid: 0,
        raw: s.to_string(),
    })
}

/// Decode bytes from an ENVELOPE field to a String.
/// IMAP ENVELOPE fields are `Option<Cow<'a, [u8]>>` in imap-proto.
pub fn decode_envelope_bytes(raw: &[u8]) -> String {
    // Try UTF-8 first, fall back to lossy
    String::from_utf8(raw.to_vec()).unwrap_or_else(|_| String::from_utf8_lossy(raw).to_string())
}

/// Extract display name and email address from IMAP Address components.
///
/// imap-proto Address has: name, adl (routing), mailbox (local-part), host (domain).
pub fn extract_address_string(
    name: Option<&str>,
    mailbox: Option<&str>,
    host: Option<&str>,
) -> (String, String) {
    let display_name = name.unwrap_or("").to_string();
    let email = match (mailbox, host) {
        (Some(m), Some(h)) => format!("{m}@{h}"),
        (Some(m), None) => m.to_string(),
        _ => String::new(),
    };
    (display_name, email)
}

/// Serialize a list of (name, address) pairs to JSON for the to_json/cc_json columns.
pub fn extract_contacts_json(contacts: &[(String, String)]) -> String {
    let entries: Vec<String> = contacts
        .iter()
        .map(|(name, addr)| {
            format!(
                r#"{{"name":"{}","address":"{}"}}"#,
                name.replace('\\', "\\\\").replace('"', "\\\""),
                addr.replace('\\', "\\\\").replace('"', "\\\""),
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

/// Convert IMAP flag names to our bitmask representation.
///
/// Bit layout: 0=Seen, 1=Flagged(starred), 2=Answered, 3=Draft, 4=Deleted
pub fn flags_to_bitmask(flags: &[async_imap::types::Flag<'_>]) -> u32 {
    use async_imap::types::Flag;
    let mut mask = 0u32;
    for flag in flags {
        match flag {
            Flag::Seen => mask |= 1 << 0,
            Flag::Flagged => mask |= 1 << 1,
            Flag::Answered => mask |= 1 << 2,
            Flag::Draft => mask |= 1 << 3,
            Flag::Deleted => mask |= 1 << 4,
            _ => {} // ignore custom/unknown flags
        }
    }
    mask
}

/// Parse one IMAP Fetch response into an EnvelopeData.
///
/// Requires that the FETCH included `(ENVELOPE FLAGS RFC822.SIZE)`.
pub fn parse_fetch_to_envelope(
    fetch: &async_imap::types::Fetch,
    account_id: &str,
    folder: &str,
) -> SyncResult<EnvelopeData> {
    let uid = fetch.uid.ok_or_else(|| SyncError::MalformedEnvelope {
        uid: 0,
        field: "UID".to_string(),
    })?;

    let envelope = fetch
        .envelope()
        .ok_or_else(|| SyncError::MalformedEnvelope {
            uid,
            field: "ENVELOPE".to_string(),
        })?;

    // Message-ID
    let message_id = envelope
        .message_id
        .as_ref()
        .map(|b| decode_envelope_bytes(b))
        .unwrap_or_else(|| format!("<generated-{uid}@inboxly>"));

    // Subject
    let subject = envelope
        .subject
        .as_ref()
        .map(|b| decode_envelope_bytes(b))
        .unwrap_or_default();

    // Date
    let date_unix = match &envelope.date {
        Some(raw) => parse_envelope_date(raw)
            .map(|dt| dt.timestamp())
            .unwrap_or(0),
        None => 0,
    };

    // From (first address)
    let (from_name, from_address) = envelope
        .from
        .as_ref()
        .and_then(|addrs| addrs.first())
        .map(|addr| {
            extract_address_string(
                addr.name
                    .as_ref()
                    .map(|b| std::str::from_utf8(b).unwrap_or("")),
                addr.mailbox
                    .as_ref()
                    .map(|b| std::str::from_utf8(b).unwrap_or("")),
                addr.host
                    .as_ref()
                    .map(|b| std::str::from_utf8(b).unwrap_or("")),
            )
        })
        .unwrap_or_default();

    // To
    let to_contacts: Vec<(String, String)> = envelope
        .to
        .as_ref()
        .map(|addrs| {
            addrs
                .iter()
                .map(|addr| {
                    extract_address_string(
                        addr.name
                            .as_ref()
                            .map(|b| std::str::from_utf8(b).unwrap_or("")),
                        addr.mailbox
                            .as_ref()
                            .map(|b| std::str::from_utf8(b).unwrap_or("")),
                        addr.host
                            .as_ref()
                            .map(|b| std::str::from_utf8(b).unwrap_or("")),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    // CC
    let cc_contacts: Vec<(String, String)> = envelope
        .cc
        .as_ref()
        .map(|addrs| {
            addrs
                .iter()
                .map(|addr| {
                    extract_address_string(
                        addr.name
                            .as_ref()
                            .map(|b| std::str::from_utf8(b).unwrap_or("")),
                        addr.mailbox
                            .as_ref()
                            .map(|b| std::str::from_utf8(b).unwrap_or("")),
                        addr.host
                            .as_ref()
                            .map(|b| std::str::from_utf8(b).unwrap_or("")),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    // In-Reply-To
    let in_reply_to = envelope
        .in_reply_to
        .as_ref()
        .map(|b| decode_envelope_bytes(b));

    // References — not in ENVELOPE; will be populated from headers in Phase 2.
    // For Phase 1, we use In-Reply-To for basic threading.
    let references_json = None;

    // Flags
    let flags = flags_to_bitmask(&fetch.flags().collect::<Vec<_>>());

    // Size
    let size_bytes = fetch.size.unwrap_or(0) as u64;

    Ok(EnvelopeData {
        message_id,
        account_id: account_id.to_string(),
        imap_uid: uid,
        imap_folder: folder.to_string(),
        from_name,
        from_address,
        to_json: extract_contacts_json(&to_contacts),
        cc_json: extract_contacts_json(&cc_contacts),
        subject,
        date_unix,
        size_bytes,
        flags,
        in_reply_to,
        references_json,
    })
}
