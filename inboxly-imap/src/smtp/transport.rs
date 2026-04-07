//! Async SMTP transport built on lettre.
//!
//! Owns the [`AsyncSmtpTransport`] construction and the send call. The
//! retry loop lives in Phase 12's UI-side send bridge — this module is
//! side-effect-focused and synchronous-return-shaped for unit-testability.

use std::str::FromStr;

use lettre::message::Mailbox;
use lettre::transport::smtp::AsyncSmtpTransport;
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{Address, Tokio1Executor};
use tracing::{info, warn};

use inboxly_core::{AccountConfig, AuthMethod, DraftEmail};

use crate::auth::shared_oauth2::SharedOAuth2;
use crate::smtp::draft_sender::DraftSender;
use crate::smtp::error::SmtpError;
use crate::smtp::message_builder::build_rfc5322_for_smtp;
use crate::smtp::redact::redact_for_log;

/// SMTP sender for a single account configuration.
///
/// Construction is cheap — reuse the same `SmtpSender` for sequential sends,
/// but DO NOT share across threads without external synchronization; lettre's
/// transport is not `Sync`-bounded for our use case.
pub struct SmtpSender {
    config: AccountConfig,
    password: Option<String>,
    oauth2: Option<SharedOAuth2>,
}

impl SmtpSender {
    /// Construct a sender using password credentials (auth method = Password or `AppPassword`).
    #[must_use]
    pub fn with_password(config: AccountConfig, password: String) -> Self {
        Self {
            config,
            password: Some(password),
            oauth2: None,
        }
    }

    /// Construct a sender using a shared OAuth2 token cache.
    #[must_use]
    pub fn with_oauth2(config: AccountConfig, oauth2: SharedOAuth2) -> Self {
        Self {
            config,
            password: None,
            oauth2: Some(oauth2),
        }
    }

    /// Send a draft via SMTP.
    ///
    /// This is ONE send attempt — no retry. The retry loop lives in the
    /// UI-side send bridge (Phase 12) which calls
    /// [`crate::smtp::retry::should_retry`] between attempts to decide
    /// whether to re-invoke this method.
    ///
    /// # Errors
    ///
    /// Returns [`SmtpError`] if message construction, transport setup, or
    /// the send call fails. Permanent errors (5xx, malformed message) and
    /// transient errors (network, auth) are distinguished via
    /// [`SmtpError::is_permanent`].
    pub async fn send(&self, draft: &DraftEmail) -> Result<(), SmtpError> {
        // Build the from Mailbox.
        let from = self.build_from_mailbox().inspect_err(|e| {
            warn!("SMTP build from failed: {}", redact_for_log(e, draft));
        })?;

        // Build the RFC 5322 message.
        let message = build_rfc5322_for_smtp(draft, &from).inspect_err(|e| {
            warn!("SMTP message build failed: {}", redact_for_log(e, draft));
        })?;

        // Build the transport.
        let transport = self.build_transport().await.inspect_err(|e| {
            warn!("SMTP transport build failed: {}", redact_for_log(e, draft));
        })?;

        info!(
            "SMTP sending draft from {} ({} recipients)",
            self.config.email,
            draft.to.len() + draft.cc.len() + draft.bcc.len()
        );

        // Gated behind cfg(not(test)) so unit tests never actually connect
        // to an SMTP server. Integration tests are not in Phase 3's scope —
        // M34 incident precedent: side-effecting handlers in tests led to
        // ten Chrome + ten kmail launches.
        #[cfg(not(test))]
        {
            use lettre::AsyncTransport;
            transport.send(message).await.map_err(|e| {
                let smtp_err = classify_lettre_error(&e);
                warn!("SMTP send failed: {}", redact_for_log(&smtp_err, draft));
                smtp_err
            })?;
        }
        #[cfg(test)]
        {
            // In tests, just verify the message + transport were built.
            let _ = (message, transport);
        }

        Ok(())
    }

    fn build_from_mailbox(&self) -> Result<Mailbox, SmtpError> {
        let address =
            Address::from_str(&self.config.email).map_err(|e| SmtpError::MessageBuildError {
                reason: format!("invalid from address {}: {}", self.config.email, e),
            })?;
        let name = if self.config.display_name.is_empty() {
            None
        } else {
            Some(self.config.display_name.clone())
        };
        Ok(Mailbox::new(name, address))
    }

    async fn build_transport(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>, SmtpError> {
        let relay = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.config.smtp_host)
            .map_err(|e| SmtpError::ConnectionFailed {
                reason: format!("build relay for {}: {}", self.config.smtp_host, e),
            })?
            .port(self.config.smtp_port);

        let (creds, mechanisms) = match self.config.auth_method {
            AuthMethod::Password | AuthMethod::AppPassword => {
                let password = self.password.clone().ok_or_else(|| SmtpError::AuthFailed {
                    reason: "password required but none provided".to_string(),
                })?;
                (
                    Credentials::new(self.config.email.clone(), password),
                    vec![Mechanism::Plain, Mechanism::Login],
                )
            }
            AuthMethod::OAuth2 => {
                let oauth2 = self.oauth2.as_ref().ok_or_else(|| SmtpError::AuthFailed {
                    reason: "SharedOAuth2 required for OAuth2 auth but none provided".to_string(),
                })?;
                let access_token =
                    oauth2
                        .get_valid_access_token()
                        .await
                        .map_err(|e| SmtpError::AuthFailed {
                            reason: format!("token refresh: {e}"),
                        })?;
                (
                    Credentials::new(self.config.email.clone(), access_token),
                    // CRITICAL: Xoauth2 is NOT in DEFAULT_MECHANISMS — must
                    // set explicitly or lettre falls back to PLAIN/LOGIN and
                    // Gmail rejects with `534-5.7.9 Application-specific
                    // password required`. See Phase 0 verification notes.
                    vec![Mechanism::Xoauth2],
                )
            }
        };

        Ok(relay.credentials(creds).authentication(mechanisms).build())
    }
}

/// Translate a lettre transport error into the appropriate [`SmtpError`] variant.
///
/// Uses lettre 0.11's structured error API ([`is_permanent`](lettre::transport::smtp::Error::is_permanent),
/// [`is_transient`](lettre::transport::smtp::Error::is_transient), [`status`](lettre::transport::smtp::Error::status),
/// [`is_timeout`](lettre::transport::smtp::Error::is_timeout)) rather than string
/// matching, so classification stays correct even when lettre's Display impl
/// changes.
#[cfg(not(test))]
fn classify_lettre_error(err: &lettre::transport::smtp::Error) -> SmtpError {
    let message = err.to_string();

    if err.is_permanent() {
        let code = err.status().map_or(0_u16, u16::from);
        return SmtpError::Rejected { code, message };
    }

    if err.is_response() {
        // The server gave an unintelligible response — treat as a network/protocol
        // hiccup that may succeed on retry.
        return SmtpError::NetworkError { reason: message };
    }

    if err.is_transient() {
        // Could be 4xx auth-style failure (e.g. 454 temporary auth failure).
        // Classify as AuthFailed if the textual hint says so, otherwise as
        // a generic network error so the retry loop can decide.
        if message.to_lowercase().contains("auth") {
            return SmtpError::AuthFailed { reason: message };
        }
        return SmtpError::NetworkError { reason: message };
    }

    // Connection / Network / Tls / TransportShutdown / Client / Timeout — all
    // map to transient NetworkError. The retry loop handles them.
    SmtpError::NetworkError { reason: message }
}

#[async_trait::async_trait]
impl DraftSender for SmtpSender {
    async fn send_draft(&self, draft: &DraftEmail) -> Result<(), SmtpError> {
        self.send(draft).await
    }
}
