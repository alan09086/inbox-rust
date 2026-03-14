# M2: Config System — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Implement TOML-based configuration with XDG path resolution.

**Architecture:** Config types live in `inboxly-core` (shared dependency). Config file read/write uses `toml` and `dirs` crates. All other crates depend on `inboxly-core` and can import these types without circular dependencies.

**Tech Stack:** Rust, toml, dirs, serde

**Prerequisite:** M1 complete — workspace scaffolded, `inboxly-core` crate exists with `Cargo.toml`, `src/lib.rs`, core types (`AccountId`, `Email`, `Thread`, `Bundle`, etc.), and `thiserror`/`serde`/`chrono`/`uuid` dependencies.

---

## Task 1: Add `toml` and `dirs` dependencies to inboxly-core

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/Cargo.toml`

Add `toml` and `dirs` to the `[dependencies]` section:

```toml
toml = "0.8"
dirs = "6"
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-core
```

**Commit:** `feat(core): add toml and dirs dependencies for config system`

---

## Task 2: Define AuthMethod enum

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (new file)

Create the `config` module with the `AuthMethod` enum. This enum represents the supported authentication methods per the spec (Password, OAuth2, App Password).

```rust
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
```

**Also:** Add `pub mod config;` to `inboxly-core/src/lib.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-core
```

**Commit:** `feat(core): add AuthMethod enum for account authentication`

---

## Task 3: Define AccountConfig struct

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append)

Add the `AccountConfig` struct. This maps directly to the `accounts` SQLite table schema from the spec and to what `inboxly-imap` needs to establish connections.

```rust
/// Configuration for a single email account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountConfig {
    /// Email address (e.g., "user@example.com"). Required.
    pub email: String,

    /// Display name shown in the From header (e.g., "Alan Gaudet").
    #[serde(default)]
    pub display_name: String,

    /// Email provider hint (e.g., "gmail", "fastmail", "generic").
    /// Used for provider-specific defaults (folder mapping, auth flow).
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
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-core
```

**Commit:** `feat(core): add AccountConfig struct for email account settings`

---

## Task 4: Define SnoozePresets struct

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append)

Add the `SnoozePresets` struct. This controls the hours used by the snooze time presets ("Tomorrow" = morning_hour, "This Weekend" = weekend morning_hour, etc.) per the spec's snooze table.

```rust
/// Configurable hours for snooze time presets.
///
/// These control what "Tomorrow" (morning), "This Weekend" (morning on weekend_day),
/// and custom time-of-day options resolve to.
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
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-core
```

**Commit:** `feat(core): add SnoozePresets struct for configurable snooze times`

---

## Task 5: Define ThemePreference enum

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append)

Add `ThemePreference` to control light/dark/system theme selection per the spec's theme system section.

```rust
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
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-core
```

**Commit:** `feat(core): add ThemePreference enum for theme selection`

---

## Task 6: Define AppConfig struct

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append)

Add the top-level `AppConfig` struct that holds the entire configuration. This is the root type serialized to/from `config.toml`.

```rust
use std::path::PathBuf;

/// Top-level application configuration.
///
/// Serialized to/from `~/.config/inboxly/config.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    /// List of configured email accounts. At least one required for operation.
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,

    /// Theme preference (light, dark, or system). Default: system.
    #[serde(default)]
    pub theme: ThemePreference,

    /// Override for the data directory. Default: XDG data dir (~/.local/share/inboxly).
    /// This is where Maildir, SQLite, and tantivy index are stored.
    #[serde(default)]
    pub data_dir: Option<PathBuf>,

    /// Override for the cache directory. Default: XDG cache dir (~/.cache/inboxly).
    #[serde(default)]
    pub cache_dir: Option<PathBuf>,

    /// Snooze time presets (morning/afternoon/evening hours, weekend day).
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
```

**Note:** The `use std::path::PathBuf;` import should be at the top of the file alongside the serde import.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-core
```

**Commit:** `feat(core): add AppConfig as top-level config struct`

---

## Task 7: XDG path resolver

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append)

Add the `Paths` struct that resolves XDG directories for config, data, and cache. Respects `AppConfig` overrides for data and cache dirs.

```rust
/// Resolved filesystem paths for the application.
///
/// Uses XDG base directories by default, with optional overrides
/// from `AppConfig.data_dir` and `AppConfig.cache_dir`.
#[derive(Debug, Clone)]
pub struct Paths {
    /// Config directory: `~/.config/inboxly/`
    pub config_dir: PathBuf,
    /// Data directory: `~/.local/share/inboxly/` (Maildir, SQLite, tantivy)
    pub data_dir: PathBuf,
    /// Cache directory: `~/.cache/inboxly/`
    pub cache_dir: PathBuf,
}

const APP_NAME: &str = "inboxly";

impl Paths {
    /// Resolve paths using XDG defaults.
    ///
    /// Returns `None` if the home directory cannot be determined
    /// (e.g., running in a minimal container without `HOME` set).
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
    ///
    /// If `config.data_dir` or `config.cache_dir` is `Some`, that value
    /// is used instead of the XDG default.
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

    /// Path to the TOML config file: `<config_dir>/config.toml`.
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// Path to the SQLite database: `<data_dir>/inboxly.db`.
    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join("inboxly.db")
    }

    /// Path to the Maildir root: `<data_dir>/maildir/`.
    pub fn maildir_root(&self) -> PathBuf {
        self.data_dir.join("maildir")
    }

    /// Path to the tantivy index directory: `<data_dir>/index/`.
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
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-core
```

**Commit:** `feat(core): add XDG path resolver with config overrides`

---

## Task 8: Config load from TOML file

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append)

Add `load` and `load_from` methods to `AppConfig`. If the config file does not exist, return `AppConfig::default()` (first-run experience). If the file exists but is malformed, return an error.

```rust
use std::io;

/// Errors that can occur during config operations.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read or write the config file.
    #[error("config I/O error: {0}")]
    Io(#[from] io::Error),

    /// Failed to parse the TOML config file.
    #[error("config parse error: {0}")]
    Parse(#[from] toml::de::Error),

    /// Failed to serialize the config to TOML.
    #[error("config serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),

    /// Config validation failed.
    #[error("config validation error: {0}")]
    Validation(String),

    /// Could not determine home directory for XDG paths.
    #[error("could not determine home directory")]
    NoHomeDir,
}

impl AppConfig {
    /// Load config from the default XDG path (`~/.config/inboxly/config.toml`).
    ///
    /// Returns `AppConfig::default()` if the file does not exist (first run).
    /// Returns an error if the file exists but cannot be parsed.
    pub fn load() -> Result<Self, ConfigError> {
        let paths = Paths::resolve().ok_or(ConfigError::NoHomeDir)?;
        Self::load_from(&paths.config_file())
    }

    /// Load config from a specific file path.
    ///
    /// Returns `AppConfig::default()` if the file does not exist.
    /// Returns an error if the file exists but cannot be parsed.
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
}
```

**Note:** The `use std::io;` import should be at the top of the file.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-core
```

**Commit:** `feat(core): implement config loading from TOML with first-run defaults`

---

## Task 9: Config save to TOML file

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append to `impl AppConfig`)

Add `save` and `save_to` methods. Creates parent directories if they don't exist. Uses `toml::to_string_pretty` for human-readable output.

```rust
impl AppConfig {
    // ... (existing load methods)

    /// Save config to the default XDG path (`~/.config/inboxly/config.toml`).
    ///
    /// Creates the config directory if it does not exist.
    pub fn save(&self) -> Result<(), ConfigError> {
        let paths = Paths::resolve().ok_or(ConfigError::NoHomeDir)?;
        self.save_to(&paths.config_file())
    }

    /// Save config to a specific file path.
    ///
    /// Creates parent directories if they do not exist.
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}
```

**Note:** These methods should be added to the existing `impl AppConfig` block from Task 8, not a separate block. The implementer may use one or two `impl` blocks — either is fine as long as it compiles.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-core
```

**Commit:** `feat(core): implement config save to TOML file`

---

## Task 10: Config validation

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append to `impl AppConfig`)

Add a `validate` method that checks for common configuration errors. This catches problems at startup rather than at first IMAP connection.

```rust
impl AppConfig {
    // ... (existing methods)

    /// Validate the configuration.
    ///
    /// Checks:
    /// - Each account has a non-empty email address
    /// - Each account has a non-empty IMAP host
    /// - Each account has a non-empty SMTP host
    /// - Port numbers are in valid range (1-65535)
    /// - Snooze hours are in range (0-23)
    /// - Weekend day is in range (0-6)
    /// - Email addresses contain '@'
    ///
    /// Returns `Ok(())` if valid, or `Err(ConfigError::Validation)` with
    /// a description of the first error found.
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
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-core
```

**Commit:** `feat(core): implement config validation`

---

## Task 11: Re-export config module from lib.rs

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/lib.rs`

Ensure the config module is publicly re-exported so other crates can use it. This should already be partially done from Task 2, but verify the full public API:

```rust
pub mod config;

// Re-export key types for convenience
pub use config::{AccountConfig, AppConfig, AuthMethod, ConfigError, Paths, SnoozePresets, ThemePreference};
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-core
```

**Commit:** `feat(core): re-export config types from crate root`

---

## Task 12: Tests — AuthMethod serialization

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append at end of file)

Add a `#[cfg(test)]` module. Start with AuthMethod round-trip tests.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_method_default_is_password() {
        assert_eq!(AuthMethod::default(), AuthMethod::Password);
    }

    #[test]
    fn auth_method_serializes_to_snake_case() {
        let toml_str = toml::to_string(&AuthMethod::OAuth2).unwrap();
        assert!(toml_str.contains("oauth2"), "got: {toml_str}");

        let toml_str = toml::to_string(&AuthMethod::AppPassword).unwrap();
        assert!(toml_str.contains("app_password"), "got: {toml_str}");
    }

    #[test]
    fn auth_method_round_trip() {
        for method in [AuthMethod::Password, AuthMethod::OAuth2, AuthMethod::AppPassword] {
            let serialized = toml::to_string(&method).unwrap();
            let deserialized: AuthMethod = toml::from_str(&serialized).unwrap();
            assert_eq!(method, deserialized);
        }
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-core -- config::tests
```

**Commit:** `test(core): add AuthMethod serialization tests`

---

## Task 13: Tests — SnoozePresets defaults and validation

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append to `mod tests`)

```rust
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
        assert_eq!(presets.afternoon_hour, 13); // default
        assert_eq!(presets.evening_hour, 18);   // default
        assert_eq!(presets.weekend_day, 5);     // default
    }
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-core -- config::tests
```

**Commit:** `test(core): add SnoozePresets default and partial-parse tests`

---

## Task 14: Tests — AccountConfig defaults and round-trip

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append to `mod tests`)

```rust
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
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-core -- config::tests
```

**Commit:** `test(core): add AccountConfig default port and round-trip tests`

---

## Task 15: Tests — AppConfig full round-trip

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append to `mod tests`)

```rust
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
        // Completely empty TOML should parse with all defaults
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
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-core -- config::tests
```

**Commit:** `test(core): add AppConfig round-trip and partial-parse tests`

---

## Task 16: Tests — Config load/save with temp files

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append to `mod tests`)

Add `tempfile` as a dev-dependency first.

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/Cargo.toml`

Add under `[dev-dependencies]`:

```toml
[dev-dependencies]
tempfile = "3"
```

Then add tests:

```rust
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
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-core -- config::tests
```

**Commit:** `test(core): add config file load/save round-trip tests`

---

## Task 17: Tests — Config validation

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append to `mod tests`)

```rust
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
        // No accounts is fine — first-run state
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
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-core -- config::tests
```

**Commit:** `test(core): add comprehensive config validation tests`

---

## Task 18: Tests — XDG Paths resolver

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append to `mod tests`)

```rust
    #[test]
    fn paths_resolve_returns_some() {
        // Should succeed on any machine with HOME set
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
        // config_dir should still be XDG default (not overridable)
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
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-core -- config::tests
```

**Commit:** `test(core): add XDG path resolver tests`

---

## Task 19: Tests — Example TOML document

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/config.rs` (append to `mod tests`)

A test that parses a realistic hand-written TOML config to ensure the format is user-friendly.

```rust
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
        assert_eq!(config.accounts[1].imap_port, 993);  // default
        assert_eq!(config.accounts[1].smtp_port, 587);  // default
        assert_eq!(config.snooze.morning_hour, 7);
        assert_eq!(config.snooze.afternoon_hour, 13);    // default
        assert_eq!(config.snooze.evening_hour, 20);
        assert_eq!(config.snooze.weekend_day, 6);
        assert!(config.validate().is_ok());
    }
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-core -- config::tests
```

**Commit:** `test(core): add realistic TOML config parse test`

---

## Task 20: Final verification and clippy

Run the full test suite and clippy to confirm everything is clean.

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-core && cargo clippy -p inboxly-core -- -D warnings
```

If any clippy warnings exist, fix them. Then do a final commit if any fixes were needed:

**Commit (if needed):** `fix(core): address clippy warnings in config module`

---

## Summary

After all 20 tasks, the `inboxly-core` crate contains:

| File | Contents |
|------|----------|
| `Cargo.toml` | `toml`, `dirs` dependencies + `tempfile` dev-dependency |
| `src/lib.rs` | `pub mod config;` + re-exports of all config types |
| `src/config.rs` | `AuthMethod`, `AccountConfig`, `SnoozePresets`, `ThemePreference`, `AppConfig`, `Paths`, `ConfigError` + load/save/validate + 28 tests |

**Total tests:** 28 (3 auth + 2 snooze + 3 account + 4 app config + 4 load/save + 12 validation + 7 paths + 1 realistic).

**Config file location:** `~/.config/inboxly/config.toml`

**Example config.toml:**

```toml
theme = "dark"

[snooze]
morning_hour = 7
evening_hour = 20

[[accounts]]
email = "user@gmail.com"
display_name = "User"
provider = "gmail"
auth_method = "oauth2"
imap_host = "imap.gmail.com"
smtp_host = "smtp.gmail.com"
smtp_port = 465
```
