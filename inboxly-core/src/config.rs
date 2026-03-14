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

/// Top-level application configuration.
///
/// Serialized to/from `~/.config/inboxly/config.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    /// List of configured email accounts.
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,
    /// Theme preference (light, dark, or system). Default: system.
    #[serde(default)]
    pub theme: ThemePreference,
    /// Override for the data directory.
    #[serde(default)]
    pub data_dir: Option<PathBuf>,
    /// Override for the cache directory.
    #[serde(default)]
    pub cache_dir: Option<PathBuf>,
    /// Snooze time presets.
    #[serde(default)]
    pub snooze: SnoozePresets,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            accounts: Vec::new(),
            theme: ThemePreference::default(),
            data_dir: None,
            cache_dir: None,
            snooze: SnoozePresets::default(),
        }
    }
}

/// Resolved filesystem paths for the application.
#[derive(Debug, Clone)]
pub struct Paths {
    /// Config directory: `~/.config/inboxly/`
    pub config_dir: PathBuf,
    /// Data directory: `~/.local/share/inboxly/`
    pub data_dir: PathBuf,
    /// Cache directory: `~/.cache/inboxly/`
    pub cache_dir: PathBuf,
}

const APP_NAME: &str = "inboxly";

impl Paths {
    /// Resolve paths using XDG defaults.
    pub fn resolve() -> Option<Self> {
        let config_dir = dirs::config_dir()?.join(APP_NAME);
        let data_dir = dirs::data_dir()?.join(APP_NAME);
        let cache_dir = dirs::cache_dir()?.join(APP_NAME);
        Some(Self { config_dir, data_dir, cache_dir })
    }

    /// Resolve paths, applying overrides from an `AppConfig`.
    pub fn resolve_with_config(config: &AppConfig) -> Option<Self> {
        let mut paths = Self::resolve()?;
        if let Some(ref data_dir) = config.data_dir {
            paths.data_dir = data_dir.clone();
        }
        if let Some(ref cache_dir) = config.cache_dir {
            paths.cache_dir = cache_dir.clone();
        }
        Some(paths)
    }

    /// Path to the TOML config file.
    pub fn config_file(&self) -> PathBuf { self.config_dir.join("config.toml") }
    /// Path to the SQLite database.
    pub fn database_file(&self) -> PathBuf { self.data_dir.join("inboxly.db") }
    /// Path to the Maildir root.
    pub fn maildir_root(&self) -> PathBuf { self.data_dir.join("maildir") }
    /// Path to the tantivy index directory.
    pub fn search_index_dir(&self) -> PathBuf { self.data_dir.join("index") }

    /// Ensure all directories exist, creating them if necessary.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.cache_dir)?;
        Ok(())
    }
}

/// Errors that can occur during config operations.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("config parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("config serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("config validation error: {0}")]
    Validation(String),
    #[error("could not determine home directory")]
    NoHomeDir,
}

impl AppConfig {
    /// Load from default XDG path. Returns default if file missing.
    pub fn load() -> Result<Self, ConfigError> {
        let paths = Paths::resolve().ok_or(ConfigError::NoHomeDir)?;
        Self::load_from(&paths.config_file())
    }

    /// Load from specific path. Returns default if file missing.
    pub fn load_from(path: &std::path::Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                let config: AppConfig = toml::from_str(&contents)?;
                Ok(config)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(AppConfig::default()),
            Err(e) => Err(ConfigError::Io(e)),
        }
    }

    /// Save to default XDG path.
    pub fn save(&self) -> Result<(), ConfigError> {
        let paths = Paths::resolve().ok_or(ConfigError::NoHomeDir)?;
        self.save_to(&paths.config_file())
    }

    /// Save to specific path. Creates parent directories.
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}
