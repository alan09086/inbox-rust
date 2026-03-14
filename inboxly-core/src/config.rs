use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Authentication method for an email account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    /// Plain username + password (IMAP LOGIN / STARTTLS).
    #[default]
    Password,
    /// OAuth2 with XOAUTH2 SASL (Gmail, Microsoft, etc.).
    #[serde(rename = "oauth2")]
    OAuth2,
    /// App-specific password (Fastmail, etc.).
    AppPassword,
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

fn default_morning_hour() -> u8 {
    8
}
fn default_afternoon_hour() -> u8 {
    13
}
fn default_evening_hour() -> u8 {
    18
}
fn default_weekend_day() -> u8 {
    5
}

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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
        Some(Self {
            config_dir,
            data_dir,
            cache_dir,
        })
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
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }
    /// Path to the SQLite database.
    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join("inboxly.db")
    }
    /// Path to the Maildir root.
    pub fn maildir_root(&self) -> PathBuf {
        self.data_dir.join("maildir")
    }
    /// Path to the tantivy index directory.
    pub fn search_index_dir(&self) -> PathBuf {
        self.data_dir.join("index")
    }

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

    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), ConfigError> {
        for (i, account) in self.accounts.iter().enumerate() {
            let ctx = format!("accounts[{}]", i);
            if account.email.is_empty() {
                return Err(ConfigError::Validation(format!(
                    "{}: email address is required",
                    ctx
                )));
            }
            if !account.email.contains('@') {
                return Err(ConfigError::Validation(format!(
                    "{}: '{}' is not a valid email address (missing '@')",
                    ctx, account.email
                )));
            }
            if account.imap_host.is_empty() {
                return Err(ConfigError::Validation(format!(
                    "{}: IMAP host is required",
                    ctx
                )));
            }
            if account.smtp_host.is_empty() {
                return Err(ConfigError::Validation(format!(
                    "{}: SMTP host is required",
                    ctx
                )));
            }
            if account.imap_port == 0 {
                return Err(ConfigError::Validation(format!(
                    "{}: IMAP port must be between 1 and 65535",
                    ctx
                )));
            }
            if account.smtp_port == 0 {
                return Err(ConfigError::Validation(format!(
                    "{}: SMTP port must be between 1 and 65535",
                    ctx
                )));
            }
        }
        if self.snooze.morning_hour > 23 {
            return Err(ConfigError::Validation(format!(
                "snooze.morning_hour {} is out of range (0-23)",
                self.snooze.morning_hour
            )));
        }
        if self.snooze.afternoon_hour > 23 {
            return Err(ConfigError::Validation(format!(
                "snooze.afternoon_hour {} is out of range (0-23)",
                self.snooze.afternoon_hour
            )));
        }
        if self.snooze.evening_hour > 23 {
            return Err(ConfigError::Validation(format!(
                "snooze.evening_hour {} is out of range (0-23)",
                self.snooze.evening_hour
            )));
        }
        if self.snooze.weekend_day > 6 {
            return Err(ConfigError::Validation(format!(
                "snooze.weekend_day {} is out of range (0=Monday .. 6=Sunday)",
                self.snooze.weekend_day
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Task 12: AuthMethod serialization (3 tests) ===

    // Helper wrapper so we can serialize AuthMethod as a TOML value (TOML requires a table root).
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct AuthWrapper {
        method: AuthMethod,
    }

    #[test]
    fn auth_method_default_is_password() {
        assert_eq!(AuthMethod::default(), AuthMethod::Password);
    }

    #[test]
    fn auth_method_serializes_to_snake_case() {
        let toml_str = toml::to_string(&AuthWrapper {
            method: AuthMethod::OAuth2,
        })
        .unwrap();
        assert!(toml_str.contains("oauth2"), "got: {toml_str}");

        let toml_str = toml::to_string(&AuthWrapper {
            method: AuthMethod::AppPassword,
        })
        .unwrap();
        assert!(toml_str.contains("app_password"), "got: {toml_str}");
    }

    #[test]
    fn auth_method_round_trip() {
        for method in [
            AuthMethod::Password,
            AuthMethod::OAuth2,
            AuthMethod::AppPassword,
        ] {
            let serialized = toml::to_string(&AuthWrapper {
                method: method.clone(),
            })
            .unwrap();
            let deserialized: AuthWrapper = toml::from_str(&serialized).unwrap();
            assert_eq!(method, deserialized.method);
        }
    }

    // === Task 13: SnoozePresets (2 tests) ===

    #[test]
    fn snooze_presets_defaults() {
        let presets = SnoozePresets::default();
        assert_eq!(presets.morning_hour, 8);
        assert_eq!(presets.afternoon_hour, 13);
        assert_eq!(presets.evening_hour, 18);
        assert_eq!(presets.weekend_day, 5);
    }

    #[test]
    fn snooze_presets_partial_toml_uses_defaults() {
        let toml_str = r#"morning_hour = 7"#;
        let presets: SnoozePresets = toml::from_str(toml_str).unwrap();
        assert_eq!(presets.morning_hour, 7);
        assert_eq!(presets.afternoon_hour, 13);
        assert_eq!(presets.evening_hour, 18);
        assert_eq!(presets.weekend_day, 5);
    }

    // === Task 14: AccountConfig (3 tests) ===

    #[test]
    fn account_config_default_ports() {
        let toml_str = r#"
            email = "test@example.com"
            imap_host = "imap.example.com"
            smtp_host = "smtp.example.com"
        "#;
        let account: AccountConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(account.imap_port, 993);
        assert_eq!(account.smtp_port, 587);
        assert_eq!(account.provider, "generic");
        assert_eq!(account.auth_method, AuthMethod::Password);
        assert_eq!(account.display_name, "");
    }

    #[test]
    fn account_config_explicit_ports() {
        let toml_str = r#"
            email = "user@gmail.com"
            display_name = "Test User"
            provider = "gmail"
            auth_method = "oauth2"
            imap_host = "imap.gmail.com"
            imap_port = 993
            smtp_host = "smtp.gmail.com"
            smtp_port = 465
        "#;
        let account: AccountConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(account.email, "user@gmail.com");
        assert_eq!(account.display_name, "Test User");
        assert_eq!(account.provider, "gmail");
        assert_eq!(account.auth_method, AuthMethod::OAuth2);
        assert_eq!(account.smtp_port, 465);
    }

    #[test]
    fn account_config_round_trip() {
        let account = AccountConfig {
            email: "test@fastmail.com".to_string(),
            display_name: "Alan".to_string(),
            provider: "fastmail".to_string(),
            auth_method: AuthMethod::AppPassword,
            imap_host: "imap.fastmail.com".to_string(),
            imap_port: 993,
            smtp_host: "smtp.fastmail.com".to_string(),
            smtp_port: 587,
        };
        let serialized = toml::to_string_pretty(&account).unwrap();
        let deserialized: AccountConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(account, deserialized);
    }

    // === Task 15: AppConfig (4 tests) ===

    #[test]
    fn app_config_default_is_empty() {
        let config = AppConfig::default();
        assert!(config.accounts.is_empty());
        assert_eq!(config.theme, ThemePreference::System);
        assert!(config.data_dir.is_none());
        assert!(config.cache_dir.is_none());
    }

    #[test]
    fn app_config_full_toml_round_trip() {
        let config = AppConfig {
            accounts: vec![
                AccountConfig {
                    email: "personal@gmail.com".to_string(),
                    display_name: "Personal".to_string(),
                    provider: "gmail".to_string(),
                    auth_method: AuthMethod::OAuth2,
                    imap_host: "imap.gmail.com".to_string(),
                    imap_port: 993,
                    smtp_host: "smtp.gmail.com".to_string(),
                    smtp_port: 465,
                },
                AccountConfig {
                    email: "work@fastmail.com".to_string(),
                    display_name: "Work".to_string(),
                    provider: "fastmail".to_string(),
                    auth_method: AuthMethod::AppPassword,
                    imap_host: "imap.fastmail.com".to_string(),
                    imap_port: 993,
                    smtp_host: "smtp.fastmail.com".to_string(),
                    smtp_port: 587,
                },
            ],
            theme: ThemePreference::Dark,
            data_dir: Some(PathBuf::from("/custom/data")),
            cache_dir: None,
            snooze: SnoozePresets {
                morning_hour: 7,
                afternoon_hour: 14,
                evening_hour: 19,
                weekend_day: 6,
            },
        };
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: AppConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn app_config_minimal_toml_parses() {
        let config: AppConfig = toml::from_str("").unwrap();
        assert!(config.accounts.is_empty());
        assert_eq!(config.theme, ThemePreference::System);
    }

    #[test]
    fn app_config_partial_toml_fills_defaults() {
        let toml_str = r#"
            theme = "dark"

            [[accounts]]
            email = "user@example.com"
            imap_host = "imap.example.com"
            smtp_host = "smtp.example.com"
        "#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.theme, ThemePreference::Dark);
        assert_eq!(config.accounts.len(), 1);
        assert_eq!(config.accounts[0].imap_port, 993);
        assert_eq!(config.snooze, SnoozePresets::default());
    }

    // === Task 16: Config load/save with temp files (4 tests) ===

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let config = AppConfig::load_from(&path).unwrap();
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = AppConfig {
            accounts: vec![AccountConfig {
                email: "test@example.com".to_string(),
                display_name: "Test".to_string(),
                provider: "generic".to_string(),
                auth_method: AuthMethod::Password,
                imap_host: "imap.example.com".to_string(),
                imap_port: 993,
                smtp_host: "smtp.example.com".to_string(),
                smtp_port: 587,
            }],
            theme: ThemePreference::Light,
            data_dir: None,
            cache_dir: None,
            snooze: SnoozePresets::default(),
        };
        config.save_to(&path).unwrap();
        let loaded = AppConfig::load_from(&path).unwrap();
        assert_eq!(config, loaded);
    }

    #[test]
    fn save_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("deep").join("config.toml");
        let config = AppConfig::default();
        config.save_to(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn load_malformed_toml_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "this is not valid toml = [[[").unwrap();
        let result = AppConfig::load_from(&path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::Parse(_)));
    }

    // === Task 17: Config validation (12 tests) ===

    fn valid_account() -> AccountConfig {
        AccountConfig {
            email: "test@example.com".to_string(),
            display_name: "Test".to_string(),
            provider: "generic".to_string(),
            auth_method: AuthMethod::Password,
            imap_host: "imap.example.com".to_string(),
            imap_port: 993,
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
        }
    }

    #[test]
    fn validate_valid_config() {
        let config = AppConfig {
            accounts: vec![valid_account()],
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_empty_config_is_valid() {
        let config = AppConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_missing_email() {
        let mut account = valid_account();
        account.email = String::new();
        let config = AppConfig {
            accounts: vec![account],
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("email address is required"));
    }

    #[test]
    fn validate_invalid_email() {
        let mut account = valid_account();
        account.email = "not-an-email".to_string();
        let config = AppConfig {
            accounts: vec![account],
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("missing '@'"));
    }

    #[test]
    fn validate_missing_imap_host() {
        let mut account = valid_account();
        account.imap_host = String::new();
        let config = AppConfig {
            accounts: vec![account],
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("IMAP host is required"));
    }

    #[test]
    fn validate_missing_smtp_host() {
        let mut account = valid_account();
        account.smtp_host = String::new();
        let config = AppConfig {
            accounts: vec![account],
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("SMTP host is required"));
    }

    #[test]
    fn validate_zero_imap_port() {
        let mut account = valid_account();
        account.imap_port = 0;
        let config = AppConfig {
            accounts: vec![account],
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("IMAP port"));
    }

    #[test]
    fn validate_zero_smtp_port() {
        let mut account = valid_account();
        account.smtp_port = 0;
        let config = AppConfig {
            accounts: vec![account],
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("SMTP port"));
    }

    #[test]
    fn validate_snooze_morning_hour_out_of_range() {
        let config = AppConfig {
            snooze: SnoozePresets {
                morning_hour: 25,
                ..Default::default()
            },
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("morning_hour"));
    }

    #[test]
    fn validate_snooze_afternoon_hour_out_of_range() {
        let config = AppConfig {
            snooze: SnoozePresets {
                afternoon_hour: 24,
                ..Default::default()
            },
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("afternoon_hour"));
    }

    #[test]
    fn validate_snooze_evening_hour_out_of_range() {
        let config = AppConfig {
            snooze: SnoozePresets {
                evening_hour: 30,
                ..Default::default()
            },
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("evening_hour"));
    }

    #[test]
    fn validate_snooze_weekend_day_out_of_range() {
        let config = AppConfig {
            snooze: SnoozePresets {
                weekend_day: 7,
                ..Default::default()
            },
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("weekend_day"));
    }

    #[test]
    fn validate_multiple_accounts_second_invalid() {
        let config = AppConfig {
            accounts: vec![
                valid_account(),
                AccountConfig {
                    email: "bad".to_string(),
                    ..valid_account()
                },
            ],
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("accounts[1]"));
    }

    // === Task 18: XDG Paths resolver (8 tests) ===

    #[test]
    fn paths_resolve_returns_some() {
        let paths = Paths::resolve();
        assert!(paths.is_some());
        let paths = paths.unwrap();
        assert!(paths.config_dir.ends_with("inboxly"));
        assert!(paths.data_dir.ends_with("inboxly"));
        assert!(paths.cache_dir.ends_with("inboxly"));
    }

    #[test]
    fn paths_config_file_path() {
        let paths = Paths::resolve().unwrap();
        assert!(paths.config_file().ends_with("config.toml"));
    }

    #[test]
    fn paths_database_file_path() {
        let paths = Paths::resolve().unwrap();
        assert!(paths.database_file().ends_with("inboxly.db"));
    }

    #[test]
    fn paths_maildir_root_path() {
        let paths = Paths::resolve().unwrap();
        assert!(paths.maildir_root().ends_with("maildir"));
    }

    #[test]
    fn paths_search_index_dir_path() {
        let paths = Paths::resolve().unwrap();
        assert!(paths.search_index_dir().ends_with("index"));
    }

    #[test]
    fn paths_with_config_overrides() {
        let config = AppConfig {
            data_dir: Some(PathBuf::from("/custom/data")),
            cache_dir: Some(PathBuf::from("/custom/cache")),
            ..Default::default()
        };
        let paths = Paths::resolve_with_config(&config).unwrap();
        assert_eq!(paths.data_dir, PathBuf::from("/custom/data"));
        assert_eq!(paths.cache_dir, PathBuf::from("/custom/cache"));
        assert!(paths.config_dir.ends_with("inboxly"));
    }

    #[test]
    fn paths_without_config_overrides() {
        let config = AppConfig::default();
        let paths_default = Paths::resolve().unwrap();
        let paths_with = Paths::resolve_with_config(&config).unwrap();
        assert_eq!(paths_default.data_dir, paths_with.data_dir);
        assert_eq!(paths_default.cache_dir, paths_with.cache_dir);
    }

    #[test]
    fn paths_ensure_dirs_creates_directories() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            config_dir: dir.path().join("config").join("inboxly"),
            data_dir: dir.path().join("data").join("inboxly"),
            cache_dir: dir.path().join("cache").join("inboxly"),
        };
        assert!(!paths.config_dir.exists());
        assert!(!paths.data_dir.exists());
        assert!(!paths.cache_dir.exists());
        paths.ensure_dirs().unwrap();
        assert!(paths.config_dir.exists());
        assert!(paths.data_dir.exists());
        assert!(paths.cache_dir.exists());
    }

    // === Task 19: Realistic TOML config (1 test) ===

    #[test]
    fn realistic_config_toml_parses() {
        let toml_str = r#"
# Inboxly configuration

theme = "dark"

[snooze]
morning_hour = 7
evening_hour = 20
weekend_day = 6  # Sunday

[[accounts]]
email = "alan@gmail.com"
display_name = "Alan Gaudet"
provider = "gmail"
auth_method = "oauth2"
imap_host = "imap.gmail.com"
imap_port = 993
smtp_host = "smtp.gmail.com"
smtp_port = 465

[[accounts]]
email = "alan@fastmail.com"
display_name = "Alan (Work)"
provider = "fastmail"
auth_method = "app_password"
imap_host = "imap.fastmail.com"
smtp_host = "smtp.fastmail.com"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.theme, ThemePreference::Dark);
        assert_eq!(config.accounts.len(), 2);
        assert_eq!(config.accounts[0].provider, "gmail");
        assert_eq!(config.accounts[0].auth_method, AuthMethod::OAuth2);
        assert_eq!(config.accounts[0].smtp_port, 465);
        assert_eq!(config.accounts[1].provider, "fastmail");
        assert_eq!(config.accounts[1].auth_method, AuthMethod::AppPassword);
        assert_eq!(config.accounts[1].imap_port, 993); // default
        assert_eq!(config.accounts[1].smtp_port, 587); // default
        assert_eq!(config.snooze.morning_hour, 7);
        assert_eq!(config.snooze.afternoon_hour, 13); // default
        assert_eq!(config.snooze.evening_hour, 20);
        assert_eq!(config.snooze.weekend_day, 6);
        assert!(config.validate().is_ok());
    }
}
