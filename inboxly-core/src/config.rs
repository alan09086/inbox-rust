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

/// Configurable hours for snooze time presets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnoozePresets {
    /// Hour (0-23) for "Morning" snooze. Default: 8.
    #[serde(default = "default_morning_hour")]
    pub morning_hour: u8,
    /// Hour (0-23) for "Afternoon" snooze. Default: 13.
    #[serde(default = "default_afternoon_hour")]
    pub afternoon_hour: u8,
    /// Hour (0-23) for "Evening" snooze. Default: 18.
    #[serde(default = "default_evening_hour")]
    pub evening_hour: u8,
    /// Day of week for "This Weekend" snooze (0=Monday .. 6=Sunday). Default: 5 (Saturday).
    #[serde(default = "default_weekend_day")]
    pub weekend_day: u8,
}

fn default_morning_hour() -> u8 { 8 }
fn default_afternoon_hour() -> u8 { 13 }
fn default_evening_hour() -> u8 { 18 }
fn default_weekend_day() -> u8 { 5 }

impl Default for SnoozePresets {
    fn default() -> Self {
        Self {
            morning_hour: default_morning_hour(),
            afternoon_hour: default_afternoon_hour(),
            evening_hour: default_evening_hour(),
            weekend_day: default_weekend_day(),
        }
    }
}

/// User's preferred theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    /// Follow system theme via freedesktop portal.
    #[default]
    System,
    /// Always use light theme.
    Light,
    /// Always use dark theme.
    Dark,
}
