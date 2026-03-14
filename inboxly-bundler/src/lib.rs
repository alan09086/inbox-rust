//! # inboxly-bundler
//!
//! Email categorisation engine for Inboxly.  Implements a four-layer system:
//!
//! 1. **User-defined rules** (highest priority) -- explicit pattern matching
//! 2. **Sender learning** (medium priority) -- learns from user moves
//! 3. **Header heuristics** (lowest priority) -- zero-config pattern matching
//! 4. **Uncategorised** -- email stays in primary inbox
//!
//! M12 introduced Layer 3 (header heuristics) and the [`Bundler`] struct.
//! M13 adds Layers 1-2 via [`engine::BundlerEngine`] and the full pipeline.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use inboxly_bundler::{Bundler, system_bundles};
//! use inboxly_store::Store;
//! # use std::path::Path;
//!
//! # fn main() -> inboxly_bundler::Result<()> {
//! let store = Store::open(Path::new("data.db"))?;
//!
//! // Create system bundles on first run
//! system_bundles::ensure_system_bundles(&store)?;
//!
//! // Create bundler with default rules
//! let bundler = Bundler::new(None)?;
//!
//! // Categorise all uncategorised threads
//! let count = bundler.categorise_all(&store)?;
//! println!("Categorised {count} threads");
//! # Ok(())
//! # }
//! ```

pub mod affinity;
pub mod custom_bundle;
pub mod engine;
pub mod evaluator;
pub mod events;
pub mod heuristics;
pub mod recategorise;
pub mod rule_store;
pub mod rules_toml;
pub mod scheduler;
pub mod system_bundles;
pub mod user_rules;

#[cfg(test)]
mod test_utils;

// M13 re-exports: user rules, sender learning, evaluation pipeline
pub use affinity::{
    AffinityStore, AffinityStoreError, CONFIDENCE_HALF_LIFE_DAYS, CONFIDENCE_INCREMENT,
    CONFIDENCE_MAX, CONFIDENCE_OVERRIDE_PENALTY, CONFIDENCE_THRESHOLD, SenderAffinity,
};
pub use custom_bundle::{
    BundleInfo, BundleStore, BundleStoreError, CreateBundleParams, UpdateBundleParams,
};
pub use engine::{BundlerEngine, CategoriseResult, CategoriseSource, HeuristicMatch};
pub use events::BundlerEvent;
pub use evaluator::{AffinityResult, RuleResult, evaluate_affinity, evaluate_rules};
pub use recategorise::{MoveAction, MoveResult, process_move};
pub use scheduler::{ThrottleEvent, ThrottleSchedulerConfig, spawn_throttle_scheduler};
pub use rule_store::{
    CreateRuleParams, RuleStore, RuleStoreError, UpdateRuleParams, validate_rule,
};
pub use user_rules::{
    BundleRule, RuleId, RuleMatchable, UserCompiledRule, UserRuleField, UserRuleOp,
};

use std::collections::HashMap;
use std::path::Path;

use inboxly_core::{BundleCategory, BundleId, EmailMeta};
use inboxly_store::Store;
use thiserror::Error;

use crate::heuristics::CompiledRule;

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

/// The main bundler engine.
///
/// Holds pre-compiled heuristic rules and a lookup table from
/// [`BundleCategory`] to [`BundleId`] for system bundles.
pub struct Bundler {
    /// Pre-compiled heuristic rules, sorted by priority descending.
    rules: Vec<CompiledRule>,
    /// Map from system category to its deterministic [`BundleId`].
    category_to_bundle: HashMap<BundleCategory, BundleId>,
}

impl Bundler {
    /// Create a new `Bundler` with default rules plus optional user overrides.
    ///
    /// `user_config_path` -- path to user's `heuristics.toml`, typically
    /// `~/.config/inboxly/heuristics.toml`. Pass `None` to use only defaults.
    ///
    /// # Errors
    ///
    /// Returns an error if the default or user TOML is malformed, or if any
    /// rule contains an invalid regex pattern.
    pub fn new(user_config_path: Option<&Path>) -> Result<Self> {
        let raw_rules = rules_toml::load_rules(user_config_path)?;
        let rules = heuristics::compile_rules(raw_rules)?;

        // Build category -> bundle_id map for system bundles
        let category_to_bundle: HashMap<BundleCategory, BundleId> = system_bundles::SYSTEM_BUNDLES
            .iter()
            .map(|def| {
                (
                    def.category.clone(),
                    system_bundles::system_bundle_id(&def.category),
                )
            })
            .collect();

        Ok(Self {
            rules,
            category_to_bundle,
        })
    }

    /// Categorise a single email using header heuristics (Layer 1).
    ///
    /// Returns `Some((BundleCategory, BundleId))` if a rule matched, `None`
    /// otherwise (email stays in primary inbox).
    ///
    /// The caller is responsible for checking higher-priority layers (user
    /// rules, sender learning) before falling through to this method.
    ///
    /// `headers` -- the email's full headers (e.g., parsed from the `.eml`
    /// file during sync). Must include headers like `List-Id`,
    /// `List-Unsubscribe`, `X-Mailer`, `Precedence` for heuristics to fire.
    pub fn categorise(
        &self,
        email: &EmailMeta,
        headers: &HashMap<String, String>,
    ) -> Option<(BundleCategory, BundleId)> {
        let category = heuristics::evaluate(&self.rules, email, headers)?;
        let bundle_id = *self.category_to_bundle.get(&category)?;
        Some((category, bundle_id))
    }

    /// Categorise all uncategorised threads in the store.
    ///
    /// Iterates threads where `thread_state.bundle_id IS NULL`, loads the
    /// newest email's headers for each, runs the heuristic pipeline, and
    /// writes the bundle assignment back to `thread_state`.
    ///
    /// Returns the number of threads that were categorised.
    ///
    /// # Errors
    ///
    /// Returns an error if any database or I/O operation fails.
    pub fn categorise_all(&self, store: &Store) -> Result<u32> {
        let uncategorised = store.get_uncategorised_thread_ids()?;
        let mut categorised_count = 0u32;

        for thread_id in &uncategorised {
            // Get the newest email in the thread
            let Some(email_row) = store.get_newest_email_in_thread(thread_id)? else {
                continue;
            };

            // Convert EmailRow to a minimal EmailMeta for evaluation
            let email_meta = EmailMeta::from(&email_row);

            // Load headers from the .eml file on disk
            let headers = store.load_email_headers(&email_row.id)?;

            if let Some((_category, bundle_id)) = self.categorise(&email_meta, &headers) {
                let bundle_id_str = bundle_id.0.to_string();
                store.set_thread_bundle(thread_id, Some(&bundle_id_str))?;
                categorised_count = categorised_count.saturating_add(1);
            }
        }

        tracing::info!(
            total = uncategorised.len(),
            categorised = categorised_count,
            "batch categorisation complete"
        );

        Ok(categorised_count)
    }

    /// Categorise a single thread by its newest email.
    ///
    /// Convenience wrapper that loads the email and headers from the store.
    /// Writes the bundle assignment to `thread_state` if a match is found.
    ///
    /// Returns the assigned [`BundleCategory`] if categorised, `None` otherwise.
    ///
    /// # Errors
    ///
    /// Returns an error if any database or I/O operation fails.
    pub fn categorise_thread(
        &self,
        store: &Store,
        thread_id: &str,
    ) -> Result<Option<BundleCategory>> {
        let Some(email_row) = store.get_newest_email_in_thread(thread_id)? else {
            return Ok(None);
        };

        let email_meta = EmailMeta::from(&email_row);
        let headers = store.load_email_headers(&email_row.id)?;

        if let Some((category, bundle_id)) = self.categorise(&email_meta, &headers) {
            let bundle_id_str = bundle_id.0.to_string();
            store.set_thread_bundle(thread_id, Some(&bundle_id_str))?;
            Ok(Some(category))
        } else {
            Ok(None)
        }
    }

    /// Get the [`BundleId`] for a given system category.
    pub fn bundle_id_for_category(&self, category: &BundleCategory) -> Option<BundleId> {
        self.category_to_bundle.get(category).copied()
    }

    /// Return the number of compiled heuristic rules.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}
