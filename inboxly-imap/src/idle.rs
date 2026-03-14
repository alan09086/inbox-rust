//! IMAP IDLE command handling for push-based sync.
//!
//! Provides:
//! - IDLE response classification (EXISTS, EXPUNGE, FETCH)
//! - `IdleWakeup` signal type for sync loop integration
//! - `IdleLoopConfig` for configuring reconnect behavior
//!
//! The actual IDLE session is driven from `sync_loop.rs` because
//! `async-imap`'s `Handle` takes ownership of the `Session`,
//! making it impossible to abstract cleanly behind a simple function.

use std::time::Duration;

/// Maximum IDLE duration before we proactively restart.
/// RFC 2177 recommends clients restart IDLE every 29 minutes.
/// Servers commonly drop connections at 30 min.
pub const IDLE_TIMEOUT_SECS: u64 = 29 * 60;

/// Outcome of an IDLE session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdleEvent {
    /// Server reported new messages exist (EXISTS count increased).
    NewMessages { exists: u32 },

    /// Server reported messages were expunged.
    Expunge { seq: u32 },

    /// Server sent a flags update (FETCH response during IDLE).
    FlagsChanged,

    /// Our 29-minute timeout fired — need to restart IDLE.
    Timeout,

    /// IDLE was cancelled (manual interrupt or StopSource dropped).
    Cancelled,
}

/// Signal from the IDLE loop to the sync loop.
#[derive(Debug, Clone)]
pub enum IdleWakeup {
    /// New mail detected on the server.
    NewMail { account_id: String, folder: String },
    /// Message(s) expunged on the server.
    Expunge { account_id: String, folder: String },
    /// Flags changed on existing messages.
    FlagsChanged { account_id: String, folder: String },
    /// Periodic timeout — do a catch-up sync before re-entering IDLE.
    TimeoutCatchup { account_id: String, folder: String },
}

/// Configuration for the IDLE reconnect loop.
#[derive(Debug, Clone)]
pub struct IdleLoopConfig {
    /// Initial backoff delay on connection failure.
    pub initial_backoff: Duration,
    /// Maximum backoff delay.
    pub max_backoff: Duration,
    /// Backoff multiplier (2.0 = double each failure).
    pub backoff_multiplier: f64,
    /// Maximum consecutive failures before giving up and falling back to polling.
    pub max_consecutive_failures: u32,
    /// IDLE timeout in seconds (override for testing).
    pub idle_timeout_secs: u64,
}

impl Default for IdleLoopConfig {
    fn default() -> Self {
        Self {
            initial_backoff: Duration::from_secs(5),
            max_backoff: Duration::from_secs(300), // 5 minutes
            backoff_multiplier: 2.0,
            max_consecutive_failures: 10,
            idle_timeout_secs: IDLE_TIMEOUT_SECS,
        }
    }
}

/// Parse "* 42 EXISTS" -> Some(42)
pub fn parse_exists_count(s: &str) -> Option<u32> {
    let s = s.trim();
    let rest = s.strip_prefix("* ")?;
    let num_str = rest.split_whitespace().next()?;
    if rest.to_uppercase().contains("EXISTS") {
        num_str.parse().ok()
    } else {
        None
    }
}

/// Parse "* 7 EXPUNGE" -> Some(7)
pub fn parse_expunge_seq(s: &str) -> Option<u32> {
    let s = s.trim();
    let rest = s.strip_prefix("* ")?;
    let num_str = rest.split_whitespace().next()?;
    if rest.to_uppercase().contains("EXPUNGE") {
        num_str.parse().ok()
    } else {
        None
    }
}

/// Classify an IDLE response line into an `IdleEvent`.
pub fn classify_idle_response(response: &str) -> IdleEvent {
    let upper = response.to_uppercase();
    if upper.contains("EXISTS") {
        let exists = parse_exists_count(response).unwrap_or(0);
        IdleEvent::NewMessages { exists }
    } else if upper.contains("EXPUNGE") {
        let seq = parse_expunge_seq(response).unwrap_or(0);
        IdleEvent::Expunge { seq }
    } else if upper.contains("FETCH") {
        IdleEvent::FlagsChanged
    } else {
        // Unknown untagged response — treat as new messages to be safe
        IdleEvent::NewMessages { exists: 0 }
    }
}

/// Convert an `async_imap::extensions::idle::IdleResponse` to our `IdleEvent`.
pub fn convert_idle_response(response: &async_imap::extensions::idle::IdleResponse) -> IdleEvent {
    match response {
        async_imap::extensions::idle::IdleResponse::NewData(data) => {
            // ResponseData contains parsed IMAP response — try to extract
            // meaningful event from its string representation
            let repr = format!("{data:?}");
            classify_idle_response(&repr)
        }
        async_imap::extensions::idle::IdleResponse::Timeout => IdleEvent::Timeout,
        async_imap::extensions::idle::IdleResponse::ManualInterrupt => IdleEvent::Cancelled,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_exists_count() {
        assert_eq!(parse_exists_count("* 42 EXISTS"), Some(42));
    }

    #[test]
    fn test_parse_exists_count_single_digit() {
        assert_eq!(parse_exists_count("* 1 EXISTS"), Some(1));
    }

    #[test]
    fn test_parse_exists_count_malformed() {
        assert_eq!(parse_exists_count("* EXISTS"), None);
    }

    #[test]
    fn test_parse_exists_count_no_prefix() {
        assert_eq!(parse_exists_count("42 EXISTS"), None);
    }

    #[test]
    fn test_parse_expunge_seq() {
        assert_eq!(parse_expunge_seq("* 7 EXPUNGE"), Some(7));
    }

    #[test]
    fn test_parse_expunge_seq_malformed() {
        assert_eq!(parse_expunge_seq("* EXPUNGE"), None);
    }

    #[test]
    fn test_classify_idle_response_exists() {
        assert_eq!(
            classify_idle_response("* 15 EXISTS"),
            IdleEvent::NewMessages { exists: 15 }
        );
    }

    #[test]
    fn test_classify_idle_response_expunge() {
        assert_eq!(
            classify_idle_response("* 3 EXPUNGE"),
            IdleEvent::Expunge { seq: 3 }
        );
    }

    #[test]
    fn test_classify_idle_response_fetch() {
        assert_eq!(
            classify_idle_response("* 5 FETCH (FLAGS (\\Seen))"),
            IdleEvent::FlagsChanged
        );
    }

    #[test]
    fn test_classify_idle_response_unknown() {
        // Unknown responses default to NewMessages for safety
        assert_eq!(
            classify_idle_response("* OK something"),
            IdleEvent::NewMessages { exists: 0 }
        );
    }

    #[test]
    fn test_idle_loop_config_default() {
        let config = IdleLoopConfig::default();
        assert_eq!(config.initial_backoff, Duration::from_secs(5));
        assert_eq!(config.max_backoff, Duration::from_secs(300));
        assert_eq!(config.max_consecutive_failures, 10);
        assert_eq!(config.idle_timeout_secs, 29 * 60);
    }
}
