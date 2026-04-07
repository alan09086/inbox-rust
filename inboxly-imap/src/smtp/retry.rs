//! Retry decision function.
//!
//! Pure, side-effect-free decision logic extracted for unit testing. The
//! SMTP send pipeline calls [`should_retry`] after each send attempt to
//! decide whether to re-attempt (after a delay) or abort.

use crate::smtp::error::SmtpError;

/// Outcome of a retry decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Retry after the given delay (in milliseconds).
    Retry {
        /// Delay in milliseconds to wait before retrying.
        delay_ms: u64,
    },
    /// Abort — do not retry.
    Stop,
}

/// Decide whether to retry after a failed send attempt.
///
/// - Attempt 1 of 3 and transient error → retry with 1s delay
/// - Attempt 2 of 3 and transient error → retry with 2s delay
/// - Attempt 3 of 3 → stop (max attempts exhausted)
/// - Permanent error → stop immediately regardless of attempt number
///
/// The send pipeline should call this AFTER a failed attempt with
/// `attempt` set to the 1-based attempt number that just failed
/// (first attempt = 1, not 0). Max attempts is 3.
#[must_use]
pub fn should_retry(error: &SmtpError, attempt: u32) -> RetryDecision {
    if error.is_permanent() {
        return RetryDecision::Stop;
    }
    match attempt {
        1 => RetryDecision::Retry { delay_ms: 1000 },
        2 => RetryDecision::Retry { delay_ms: 2000 },
        _ => RetryDecision::Stop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_failed_first_attempt_retries_1s() {
        let err = SmtpError::AuthFailed {
            reason: "token expired".into(),
        };
        assert_eq!(
            should_retry(&err, 1),
            RetryDecision::Retry { delay_ms: 1000 }
        );
    }

    #[test]
    fn auth_failed_second_attempt_retries_2s() {
        let err = SmtpError::AuthFailed {
            reason: "token expired".into(),
        };
        assert_eq!(
            should_retry(&err, 2),
            RetryDecision::Retry { delay_ms: 2000 }
        );
    }

    #[test]
    fn auth_failed_third_attempt_stops() {
        let err = SmtpError::AuthFailed {
            reason: "token expired".into(),
        };
        assert_eq!(should_retry(&err, 3), RetryDecision::Stop);
    }

    #[test]
    fn network_error_first_attempt_retries_1s() {
        let err = SmtpError::NetworkError {
            reason: "connection reset".into(),
        };
        assert_eq!(
            should_retry(&err, 1),
            RetryDecision::Retry { delay_ms: 1000 }
        );
    }

    #[test]
    fn network_error_third_attempt_stops() {
        let err = SmtpError::NetworkError {
            reason: "connection reset".into(),
        };
        assert_eq!(should_retry(&err, 3), RetryDecision::Stop);
    }

    #[test]
    fn rejected_5xx_is_permanent_stops_immediately() {
        let err = SmtpError::Rejected {
            code: 550,
            message: "mailbox unavailable".into(),
        };
        assert_eq!(should_retry(&err, 1), RetryDecision::Stop);
    }

    #[test]
    fn message_build_error_is_permanent_stops_immediately() {
        let err = SmtpError::MessageBuildError {
            reason: "invalid from".into(),
        };
        assert_eq!(should_retry(&err, 1), RetryDecision::Stop);
    }
}
