//! SMTP transport + message building for the compose pipeline.
//!
//! Separate from the IMAP modules because SMTP has its own protocol concerns
//! (STARTTLS, auth mechanism selection, Bcc envelope handling). Shares the
//! OAuth2 token cache via [`crate::auth::shared_oauth2::SharedOAuth2`].

pub mod error;
pub mod message_builder;
pub mod redact;
pub mod retry;
pub mod transport;

pub use error::SmtpError;
pub use message_builder::{build_rfc5322_for_sent_folder, build_rfc5322_for_smtp};
pub use redact::redact_for_log;
pub use retry::{RetryDecision, should_retry};
pub use transport::SmtpSender;
