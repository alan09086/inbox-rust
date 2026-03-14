//! # inboxly-bundler
//!
//! Email categorisation engine for Inboxly. Implements a three-layer system:
//!
//! 1. **User-defined rules** (highest priority) -- explicit sender/header rules (M13)
//! 2. **Sender learning** (medium priority) -- learned from user actions (M13)
//! 3. **Header heuristics** (lowest priority) -- zero-config pattern matching (this milestone)
//!
//! This milestone (M12) implements Layer 3: header-based heuristics.

pub mod heuristics;
pub mod rules_toml;
pub mod system_bundles;

use thiserror::Error;

/// Errors that can occur during bundler operations.
#[derive(Debug, Error)]
pub enum BundlerError {
    /// Failed to parse heuristic rules from TOML.
    #[error("failed to parse heuristic rules: {0}")]
    RuleParse(String),

    /// A rule contains an invalid regex pattern.
    #[error("invalid regex in rule '{rule_name}': {source}")]
    InvalidRegex {
        /// Name of the rule containing the bad regex.
        rule_name: String,
        /// The underlying regex compilation error.
        source: regex::Error,
    },

    /// An error propagated from the store layer.
    #[error("store error: {0}")]
    Store(#[from] inboxly_store::StoreError),

    /// An I/O error (e.g., reading a user config file).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to deserialize TOML content.
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
}

/// Convenience alias for `Result<T, BundlerError>`.
pub type Result<T> = std::result::Result<T, BundlerError>;
