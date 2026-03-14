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
