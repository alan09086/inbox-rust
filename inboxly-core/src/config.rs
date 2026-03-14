use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Authentication method for an email account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    /// Plain username + password (IMAP LOGIN / STARTTLS).
    Password,
    /// OAuth2 with XOAUTH2 SASL (Gmail, Microsoft, etc.).
    OAuth2,
    /// App-specific password (Fastmail, etc.).
    AppPassword,
}

impl Default for AuthMethod {
    fn default() -> Self {
        Self::Password
    }
}

/// Configuration for a single email account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountConfig {
    /// Email address (e.g., "user@example.com"). Required.
    pub email: String,
    /// Display name shown in the From header (e.g., "Alan Gaudet").
    #[serde(default)]
    pub display_name: String,
    /// Email provider hint (e.g., "gmail", "fastmail", "generic").
    #[serde(default = "default_provider")]
    pub provider: String,
    /// Authentication method.
    #[serde(default)]
    pub auth_method: AuthMethod,
    /// IMAP server hostname (e.g., "imap.gmail.com").
    pub imap_host: String,
    /// IMAP server port. Defaults to 993 (IMAPS).
    #[serde(default = "default_imap_port")]
    pub imap_port: u16,
    /// SMTP server hostname (e.g., "smtp.gmail.com").
    pub smtp_host: String,
    /// SMTP server port. Defaults to 587 (STARTTLS).
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
}

fn default_provider() -> String {
    "generic".to_string()
}

fn default_imap_port() -> u16 {
    993
}

fn default_smtp_port() -> u16 {
    587
}
