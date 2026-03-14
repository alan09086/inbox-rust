# M12: Bundler Header Heuristics — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Implement automatic email categorisation using header-based heuristics.

**Architecture:** `inboxly-bundler` crate owns all categorisation logic. Header heuristics are the lowest-priority layer, providing zero-config categorisation out of the box.

**Tech Stack:** Rust, regex, toml, inboxly-core, inboxly-store

---

## Prerequisites

- **M1** — `inboxly-core` crate with `BundleCategory`, `BundleId`, `EmailMeta`, `EmailContent`, `ThreadId`, `Bundle`, `ThreadState`, and all supporting types
- **M3** — `inboxly-store` crate with SQLite schema including `bundles`, `bundle_rules`, `thread_state` tables and `Store` API
- **M4** — Maildir operations with email header access (reading `.eml` files, parsing headers)

## Design Decisions

1. **Rules are data, not code.** Every header heuristic is a `HeuristicRule` struct — no hardcoded if/else chains. The default rules ship as an embedded TOML string compiled into the binary, but users can override/extend via an external TOML file at `~/.config/inboxly/heuristics.toml`.

2. **Rules are ordered by priority.** The first matching rule wins. This makes evaluation deterministic and easy to reason about. Priority is explicit (integer field), not implicit from file order.

3. **Compiled regex for performance.** Regex patterns are compiled once at `Bundler` construction and reused. The `regex` crate's `RegexSet` is not used because we need to know *which* rule matched and rules have different target categories — we iterate rules in priority order and short-circuit on first match.

4. **Header heuristics are Layer 1 (lowest priority).** The evaluation order in the full bundler is: User rules > Sender learning > Header heuristics. This milestone only implements header heuristics. M13 adds the higher-priority layers. The `categorise()` method returns `Option<BundleCategory>` — `None` means uncategorised (stays in primary inbox).

5. **System bundles are idempotent.** `ensure_system_bundles()` creates the 8 default bundles if they do not already exist. Safe to call on every app startup. Uses deterministic UUIDs (v5 namespace) so bundle IDs are stable across reinstalls.

6. **Batch categorisation is a scan.** `categorise_all()` iterates all threads without a `bundle_id` in `thread_state` and runs each through the pipeline. This is designed for initial sync catch-up and is called after Phase 1 sync completes.

## File Layout After This Milestone

```
inboxly-bundler/
├── Cargo.toml
├── src/
│   ├── lib.rs              ← public API: Bundler, ensure_system_bundles, categorise, categorise_all
│   ├── heuristics.rs       ← HeuristicRule, CompiledRule, header matching logic
│   ├── rules_toml.rs       ← TOML parsing for rule definitions
│   ├── system_bundles.rs   ← default bundle definitions with BigTop colours
│   └── default_rules.toml  ← embedded default heuristic rules (included via include_str!)
└── tests/
    └── integration.rs      ← end-to-end tests with fixture emails
```

## Type Definitions

### `HeuristicRule` (TOML-loadable rule definition)

```rust
/// A single header-based heuristic rule definition.
/// Loaded from TOML, compiled into a CompiledRule for evaluation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HeuristicRule {
    /// Human-readable name for debugging/logging
    pub name: String,
    /// Which category this rule assigns
    pub category: BundleCategory,
    /// Priority — higher number = evaluated first. Default heuristics use 0-100.
    /// User overrides should use 200+ to take precedence over defaults.
    pub priority: i32,
    /// What to match against
    pub field: RuleField,
    /// How to match
    pub operator: RuleOp,
    /// The value/pattern to match
    pub value: String,
}

/// Which email field to evaluate
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum RuleField {
    /// From address (full "name <address>" or just address)
    From,
    /// A specific email header by name (e.g., "List-Id", "Precedence")
    Header(String),
    /// Subject line
    Subject,
    /// Sender domain (extracted from From address)
    SenderDomain,
}

/// How to compare the field value against the rule value
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum RuleOp {
    /// Field value contains the string (case-insensitive)
    Contains,
    /// Field value equals the string exactly (case-insensitive)
    Equals,
    /// Field value matches a regex pattern
    Matches,
    /// Header is present (value is ignored)
    Present,
    /// Compound: header is present AND another header is absent.
    /// value format: "present_header|absent_header"
    PresentWithout,
    /// Sender domain matches a glob pattern (e.g., "*.amazon.*")
    DomainGlob,
}
```

### `CompiledRule` (runtime-ready rule)

```rust
/// A HeuristicRule with its regex pre-compiled for fast evaluation.
pub(crate) struct CompiledRule {
    pub name: String,
    pub category: BundleCategory,
    pub priority: i32,
    pub field: RuleField,
    pub operator: RuleOp,
    pub value: String,
    /// Compiled regex (only populated when operator is Matches or DomainGlob)
    pub regex: Option<Regex>,
}
```

## Default Rules TOML

```toml
# Default header heuristic rules for email categorisation.
# Priority: higher number = evaluated first. First match wins.
# These are Layer 1 (lowest priority) — user rules and sender learning override these.

# --- Forums (List-Id present = mailing list) ---
[[rules]]
name = "mailing-list-id"
category = "Forums"
priority = 50
field = { Header = "List-Id" }
operator = "Present"
value = ""

# --- Promos (List-Unsubscribe without List-Id = marketing) ---
[[rules]]
name = "marketing-unsubscribe"
category = "Promos"
priority = 40
field = { Header = "List-Unsubscribe" }
operator = "PresentWithout"
value = "List-Unsubscribe|List-Id"

# --- Promos (X-Mailer campaign tools) ---
[[rules]]
name = "mailer-campaign"
category = "Promos"
priority = 45
field = { Header = "X-Mailer" }
operator = "Matches"
value = "(?i)(campaign|mailchimp|sendgrid|mailgun|constant.?contact|brevo|klaviyo|hubspot)"

# --- Low Priority (Precedence: bulk) ---
[[rules]]
name = "precedence-bulk"
category = "LowPriority"
priority = 10
field = { Header = "Precedence" }
operator = "Equals"
value = "bulk"

# --- Social (known social domains) ---
[[rules]]
name = "social-facebook"
category = "Social"
priority = 60
field = "SenderDomain"
operator = "DomainGlob"
value = "facebookmail.com"

[[rules]]
name = "social-twitter"
category = "Social"
priority = 60
field = "SenderDomain"
operator = "DomainGlob"
value = "twitter.com"

[[rules]]
name = "social-x"
category = "Social"
priority = 60
field = "SenderDomain"
operator = "DomainGlob"
value = "x.com"

[[rules]]
name = "social-linkedin"
category = "Social"
priority = 60
field = "SenderDomain"
operator = "DomainGlob"
value = "linkedin.com"

[[rules]]
name = "social-instagram"
category = "Social"
priority = 60
field = "SenderDomain"
operator = "DomainGlob"
value = "instagram.com"

[[rules]]
name = "social-github"
category = "Social"
priority = 55
field = "SenderDomain"
operator = "DomainGlob"
value = "github.com"

[[rules]]
name = "social-discord"
category = "Social"
priority = 60
field = "SenderDomain"
operator = "DomainGlob"
value = "discord.com"

[[rules]]
name = "social-reddit"
category = "Social"
priority = 60
field = "SenderDomain"
operator = "DomainGlob"
value = "redditmail.com"

# --- Purchases (e-commerce domains) ---
[[rules]]
name = "purchases-amazon"
category = "Purchases"
priority = 55
field = "SenderDomain"
operator = "DomainGlob"
value = "*.amazon.*"

[[rules]]
name = "purchases-ebay"
category = "Purchases"
priority = 55
field = "SenderDomain"
operator = "DomainGlob"
value = "ebay.*"

[[rules]]
name = "purchases-etsy"
category = "Purchases"
priority = 55
field = "SenderDomain"
operator = "DomainGlob"
value = "etsy.com"

[[rules]]
name = "purchases-shopify"
category = "Purchases"
priority = 55
field = "SenderDomain"
operator = "DomainGlob"
value = "shopify.com"

[[rules]]
name = "purchases-stripe"
category = "Purchases"
priority = 55
field = "SenderDomain"
operator = "DomainGlob"
value = "stripe.com"

# --- Finance (banking/payment domains) ---
[[rules]]
name = "finance-paypal"
category = "Finance"
priority = 55
field = "SenderDomain"
operator = "DomainGlob"
value = "paypal.*"

[[rules]]
name = "finance-bank-domain"
category = "Finance"
priority = 30
field = "SenderDomain"
operator = "Matches"
value = "(?i)(bank|banking|creditunion|visa|mastercard|amex)"

[[rules]]
name = "finance-statement-subject"
category = "Finance"
priority = 25
field = "Subject"
operator = "Matches"
value = "(?i)(statement|account.?summary|balance.?update|transaction.?alert)"

# --- Travel (booking/airline domains) ---
[[rules]]
name = "travel-booking"
category = "Travel"
priority = 55
field = "SenderDomain"
operator = "DomainGlob"
value = "booking.com"

[[rules]]
name = "travel-airbnb"
category = "Travel"
priority = 55
field = "SenderDomain"
operator = "DomainGlob"
value = "airbnb.*"

[[rules]]
name = "travel-expedia"
category = "Travel"
priority = 55
field = "SenderDomain"
operator = "DomainGlob"
value = "expedia.*"

[[rules]]
name = "travel-airline-domain"
category = "Travel"
priority = 30
field = "SenderDomain"
operator = "Matches"
value = "(?i)(airline|airways|aircanada|westjet|united\\.com|delta\\.com|southwest\\.com)"

# --- Updates (generic noreply/notification senders — catch-all, low priority) ---
[[rules]]
name = "updates-noreply"
category = "Updates"
priority = 5
field = "From"
operator = "Matches"
value = "(?i)(no.?reply|noreply|do.?not.?reply|notifications?@)"
```

## Implementation Steps

### Step 1: Create `inboxly-bundler` crate and Cargo.toml

**Action:** Create the crate directory and `Cargo.toml`.

```bash
mkdir -p inboxly-bundler/src
mkdir -p inboxly-bundler/tests
```

**File:** `inboxly-bundler/Cargo.toml`

```toml
[package]
name = "inboxly-bundler"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "Email categorisation engine for Inboxly"

[dependencies]
inboxly-core = { path = "../inboxly-core" }
inboxly-store = { path = "../inboxly-store" }

regex = "1"
toml = "0.8"
serde = { version = "1", features = ["derive"] }
thiserror = "2"
tracing = "0.1"
uuid = { version = "1", features = ["v5"] }
```

**File:** Add `"inboxly-bundler"` to workspace `members` in root `Cargo.toml`.

**Tests:** `cargo check -p inboxly-bundler` compiles.

**Commit:** `feat(bundler): scaffold inboxly-bundler crate (M12)`

---

### Step 2: Define `BundlerError` in `lib.rs`

**Action:** Create the error type for the bundler crate.

**File:** `inboxly-bundler/src/lib.rs`

```rust
//! Email categorisation engine for Inboxly.
//!
//! Three-layer categorisation with clear precedence:
//! 1. User-defined rules (highest priority) — M13
//! 2. Sender learning (if confidence > threshold) — M13
//! 3. Header heuristics (lowest priority, zero-config) — this crate, this milestone
//!
//! This milestone (M12) implements Layer 3: header-based heuristics.

pub mod heuristics;
pub mod rules_toml;
pub mod system_bundles;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum BundlerError {
    #[error("failed to parse heuristic rules: {0}")]
    RuleParse(String),

    #[error("invalid regex in rule '{rule_name}': {source}")]
    InvalidRegex {
        rule_name: String,
        source: regex::Error,
    },

    #[error("store error: {0}")]
    Store(#[from] inboxly_store::StoreError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
}

pub type Result<T> = std::result::Result<T, BundlerError>;
```

**Tests:** `cargo check -p inboxly-bundler` compiles.

**Commit:** `feat(bundler): add BundlerError type (M12)`

---

### Step 3: Implement `rules_toml.rs` — TOML rule definitions and parsing

**Action:** Define the `HeuristicRule`, `RuleField`, and `RuleOp` types. Implement TOML deserialization. Include the default rules via `include_str!`.

**File:** `inboxly-bundler/src/rules_toml.rs`

```rust
use serde::{Deserialize, Serialize};
use inboxly_core::BundleCategory;

/// A single header-based heuristic rule definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HeuristicRule {
    pub name: String,
    pub category: BundleCategory,
    pub priority: i32,
    pub field: RuleField,
    pub operator: RuleOp,
    pub value: String,
}

/// Which email field to evaluate.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum RuleField {
    From,
    Header(String),
    Subject,
    SenderDomain,
}

/// How to compare the field value against the rule value.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum RuleOp {
    Contains,
    Equals,
    Matches,
    Present,
    PresentWithout,
    DomainGlob,
}

/// Container for TOML deserialization.
#[derive(Debug, Deserialize)]
pub(crate) struct RuleSet {
    pub rules: Vec<HeuristicRule>,
}

/// The default rules compiled into the binary.
pub(crate) const DEFAULT_RULES_TOML: &str = include_str!("default_rules.toml");

/// Parse heuristic rules from a TOML string.
pub fn parse_rules(toml_str: &str) -> crate::Result<Vec<HeuristicRule>> {
    let rule_set: RuleSet = toml::from_str(toml_str)?;
    Ok(rule_set.rules)
}

/// Load default rules, optionally merging with user overrides from a file path.
/// User rules with the same `name` as a default rule replace the default.
/// User rules with new names are added.
pub fn load_rules(user_config_path: Option<&std::path::Path>) -> crate::Result<Vec<HeuristicRule>> {
    let mut rules = parse_rules(DEFAULT_RULES_TOML)?;

    if let Some(path) = user_config_path {
        if path.exists() {
            let user_toml = std::fs::read_to_string(path)?;
            let user_rules = parse_rules(&user_toml)?;

            // Replace defaults with same name, append new ones
            for user_rule in user_rules {
                if let Some(existing) = rules.iter_mut().find(|r| r.name == user_rule.name) {
                    *existing = user_rule;
                } else {
                    rules.push(user_rule);
                }
            }
        }
    }

    // Sort by priority descending — highest priority evaluated first
    rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    Ok(rules)
}
```

**File:** `inboxly-bundler/src/default_rules.toml` — the full TOML content from the "Default Rules TOML" section above.

**Tests:** Unit tests in `rules_toml.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default_rules() {
        let rules = parse_rules(DEFAULT_RULES_TOML).unwrap();
        assert!(rules.len() >= 20, "should have at least 20 default rules");
    }

    #[test]
    fn rules_sorted_by_priority_descending() {
        let rules = load_rules(None).unwrap();
        for window in rules.windows(2) {
            assert!(window[0].priority >= window[1].priority);
        }
    }

    #[test]
    fn user_override_replaces_default() {
        // Write a temp TOML that overrides "precedence-bulk" to assign Promos instead
        let user_toml = r#"
[[rules]]
name = "precedence-bulk"
category = "Promos"
priority = 100
field = { Header = "Precedence" }
operator = "Equals"
value = "bulk"
"#;
        let temp_dir = std::env::temp_dir().join("inboxly-test-rules");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let path = temp_dir.join("heuristics.toml");
        std::fs::write(&path, user_toml).unwrap();

        let rules = load_rules(Some(&path)).unwrap();
        let bulk_rule = rules.iter().find(|r| r.name == "precedence-bulk").unwrap();
        assert!(matches!(bulk_rule.category, BundleCategory::Promos));
        assert_eq!(bulk_rule.priority, 100);

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn user_adds_new_rule() {
        let user_toml = r#"
[[rules]]
name = "custom-work-domain"
category = "Updates"
priority = 200
field = "SenderDomain"
operator = "DomainGlob"
value = "mycompany.com"
"#;
        let temp_dir = std::env::temp_dir().join("inboxly-test-rules-add");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let path = temp_dir.join("heuristics.toml");
        std::fs::write(&path, user_toml).unwrap();

        let rules = load_rules(Some(&path)).unwrap();
        assert!(rules.iter().any(|r| r.name == "custom-work-domain"));
        // New rule should be first (priority 200)
        assert_eq!(rules[0].name, "custom-work-domain");

        std::fs::remove_dir_all(&temp_dir).ok();
    }
}
```

**Commit:** `feat(bundler): TOML rule definitions with default rules (M12)`

---

### Step 4: Implement `system_bundles.rs` — default bundle creation

**Action:** Define the 8 system bundles with BigTop colours. Implement `ensure_system_bundles()` which creates them idempotently in SQLite.

**File:** `inboxly-bundler/src/system_bundles.rs`

```rust
use inboxly_core::{Bundle, BundleCategory, BundleId, BundleIcon, BundleVisibility, BundleThrottle};
use uuid::Uuid;

/// UUID v5 namespace for generating deterministic system bundle IDs.
/// This ensures the same BundleId is generated across reinstalls.
const SYSTEM_BUNDLE_NAMESPACE: Uuid = Uuid::from_bytes([
    0x69, 0x6e, 0x62, 0x6f, 0x78, 0x6c, 0x79, 0x2d,
    0x62, 0x75, 0x6e, 0x64, 0x6c, 0x65, 0x73, 0x21,
]);

/// A system bundle definition with its BigTop colour scheme.
pub struct SystemBundleDef {
    pub category: BundleCategory,
    pub name: &'static str,
    /// Title colour (CSS hex, e.g., "#d23f31")
    pub color: &'static str,
    /// Pastel badge background colour
    pub badge_color: &'static str,
    pub icon: BundleIcon,
    pub default_visibility: BundleVisibility,
    pub default_throttle: BundleThrottle,
}

/// The 8 system bundles with BigTop colour definitions.
pub const SYSTEM_BUNDLES: &[SystemBundleDef] = &[
    SystemBundleDef {
        category: BundleCategory::Social,
        name: "Social",
        color: "#d23f31",
        badge_color: "#faebea",
        icon: BundleIcon::Social,
        default_visibility: BundleVisibility::Bundled,
        default_throttle: BundleThrottle::Immediate,
    },
    SystemBundleDef {
        category: BundleCategory::Promos,
        name: "Promos",
        color: "#00acc1",
        badge_color: "#e5f6f9",
        icon: BundleIcon::Promos,
        default_visibility: BundleVisibility::Bundled,
        default_throttle: BundleThrottle::Daily,
    },
    SystemBundleDef {
        category: BundleCategory::Updates,
        name: "Updates",
        color: "#f4511e",
        badge_color: "#feede8",
        icon: BundleIcon::Updates,
        default_visibility: BundleVisibility::Bundled,
        default_throttle: BundleThrottle::Immediate,
    },
    SystemBundleDef {
        category: BundleCategory::Finance,
        name: "Finance",
        color: "#558b2f",
        badge_color: "#eef3ea",
        icon: BundleIcon::Finance,
        default_visibility: BundleVisibility::Bundled,
        default_throttle: BundleThrottle::Immediate,
    },
    SystemBundleDef {
        category: BundleCategory::Purchases,
        name: "Purchases",
        color: "#6d4c41",
        badge_color: "#f0edec",
        icon: BundleIcon::Purchases,
        default_visibility: BundleVisibility::Bundled,
        default_throttle: BundleThrottle::Immediate,
    },
    SystemBundleDef {
        category: BundleCategory::Travel,
        name: "Travel",
        color: "#8e24aa",
        badge_color: "#f3e9f6",
        icon: BundleIcon::Travel,
        default_visibility: BundleVisibility::Bundled,
        default_throttle: BundleThrottle::Immediate,
    },
    SystemBundleDef {
        category: BundleCategory::Forums,
        name: "Forums",
        color: "#3949ab",
        badge_color: "#ebecf6",
        icon: BundleIcon::Forums,
        default_visibility: BundleVisibility::Bundled,
        default_throttle: BundleThrottle::Daily,
    },
    SystemBundleDef {
        category: BundleCategory::LowPriority,
        name: "Low Priority",
        color: "#212121",
        badge_color: "#e5e5e5",
        icon: BundleIcon::LowPriority,
        default_visibility: BundleVisibility::Bundled,
        default_throttle: BundleThrottle::Weekly,
    },
];

/// Generate a deterministic BundleId for a system bundle category.
pub fn system_bundle_id(category: &BundleCategory) -> BundleId {
    let name = match category {
        BundleCategory::Social => "social",
        BundleCategory::Promos => "promos",
        BundleCategory::Updates => "updates",
        BundleCategory::Finance => "finance",
        BundleCategory::Purchases => "purchases",
        BundleCategory::Travel => "travel",
        BundleCategory::Forums => "forums",
        BundleCategory::LowPriority => "low-priority",
        BundleCategory::Saved => "saved",
        BundleCategory::Custom(s) => s.as_str(),
    };
    BundleId(Uuid::new_v5(&SYSTEM_BUNDLE_NAMESPACE, name.as_bytes()))
}

/// Ensure all 8 system bundles exist in the store.
/// Idempotent — safe to call on every app startup.
/// Returns the list of BundleIds for system bundles.
pub fn ensure_system_bundles(store: &inboxly_store::Store) -> crate::Result<Vec<BundleId>> {
    let mut ids = Vec::with_capacity(SYSTEM_BUNDLES.len());

    for def in SYSTEM_BUNDLES {
        let id = system_bundle_id(&def.category);

        // Check if bundle already exists
        if store.get_bundle(&id)?.is_none() {
            let bundle = Bundle {
                id: id.clone(),
                category: def.category.clone(),
                name: def.name.to_string(),
                color: parse_hex_color(def.color),
                badge_color: parse_hex_color(def.badge_color),
                icon: def.icon.clone(),
                threads: Vec::new(),
                unread_count: 0,
                newest_date: chrono::Utc::now(),
                visibility: def.default_visibility.clone(),
                throttle: def.default_throttle.clone(),
            };
            store.insert_bundle(&bundle)?;
            tracing::info!(bundle_name = def.name, "created system bundle");
        }

        ids.push(id);
    }

    Ok(ids)
}

/// Parse a CSS hex colour string like "#d23f31" into the core Color type.
fn parse_hex_color(hex: &str) -> inboxly_core::Color {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    inboxly_core::Color { r, g, b, a: 255 }
}
```

**Tests:** Unit tests in `system_bundles.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_bundle_ids_are_deterministic() {
        let id1 = system_bundle_id(&BundleCategory::Social);
        let id2 = system_bundle_id(&BundleCategory::Social);
        assert_eq!(id1, id2);
    }

    #[test]
    fn all_categories_have_unique_ids() {
        let ids: Vec<_> = SYSTEM_BUNDLES.iter()
            .map(|d| system_bundle_id(&d.category))
            .collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "all system bundle IDs must be unique");
    }

    #[test]
    fn parse_hex_colors() {
        let color = parse_hex_color("#d23f31");
        assert_eq!(color.r, 0xd2);
        assert_eq!(color.g, 0x3f);
        assert_eq!(color.b, 0x31);
        assert_eq!(color.a, 255);
    }

    #[test]
    fn eight_system_bundles_defined() {
        assert_eq!(SYSTEM_BUNDLES.len(), 8);
    }
}
```

**Commit:** `feat(bundler): system bundles with BigTop colours (M12)`

---

### Step 5: Implement `heuristics.rs` — header matching engine

**Action:** Implement `CompiledRule`, the rule compiler, and the `evaluate()` function that takes an `EmailMeta` + headers and returns `Option<BundleCategory>`.

**File:** `inboxly-bundler/src/heuristics.rs`

```rust
use std::collections::HashMap;
use regex::Regex;
use inboxly_core::{BundleCategory, EmailMeta};
use crate::rules_toml::{HeuristicRule, RuleField, RuleOp};

/// A HeuristicRule with its regex pre-compiled for fast evaluation.
pub(crate) struct CompiledRule {
    pub name: String,
    pub category: BundleCategory,
    pub priority: i32,
    pub field: RuleField,
    pub operator: RuleOp,
    pub value: String,
    /// Compiled regex — populated for Matches and DomainGlob operators.
    pub regex: Option<Regex>,
}

/// Compile a list of HeuristicRules into CompiledRules.
/// Rules are pre-sorted by priority descending (from load_rules).
/// Returns error if any regex pattern is invalid.
pub(crate) fn compile_rules(rules: Vec<HeuristicRule>) -> crate::Result<Vec<CompiledRule>> {
    rules.into_iter().map(compile_one).collect()
}

fn compile_one(rule: HeuristicRule) -> crate::Result<CompiledRule> {
    let regex = match &rule.operator {
        RuleOp::Matches => {
            Some(Regex::new(&rule.value).map_err(|e| crate::BundlerError::InvalidRegex {
                rule_name: rule.name.clone(),
                source: e,
            })?)
        }
        RuleOp::DomainGlob => {
            // Convert glob pattern to regex:
            // "*.amazon.*" -> "^.*\.amazon\..*$"
            let pattern = glob_to_regex(&rule.value);
            Some(Regex::new(&pattern).map_err(|e| crate::BundlerError::InvalidRegex {
                rule_name: rule.name.clone(),
                source: e,
            })?)
        }
        _ => None,
    };

    Ok(CompiledRule {
        name: rule.name,
        category: rule.category,
        priority: rule.priority,
        field: rule.field,
        operator: rule.operator,
        value: rule.value,
        regex,
    })
}

/// Convert a domain glob pattern to a case-insensitive regex.
/// "*.amazon.*" -> "(?i)^.*\.amazon\..*$"
/// "paypal.*"   -> "(?i)^paypal\..*$"
fn glob_to_regex(glob: &str) -> String {
    let mut regex = String::from("(?i)^");
    for ch in glob.chars() {
        match ch {
            '*' => regex.push_str(".*"),
            '.' => regex.push_str("\\."),
            '?' => regex.push('.'),
            c => regex.push(c),
        }
    }
    regex.push('$');
    regex
}

/// Extract the domain from an email address.
/// "user@example.com" -> "example.com"
/// "Name <user@sub.example.com>" -> "sub.example.com"
fn extract_domain(from: &str) -> Option<&str> {
    // Handle "Name <address>" format
    let addr = if let Some(start) = from.rfind('<') {
        let end = from.rfind('>')?;
        &from[start + 1..end]
    } else {
        from
    };
    addr.rsplit_once('@').map(|(_, domain)| domain)
}

/// Evaluate all compiled rules against an email's metadata and headers.
/// Returns the category of the first matching rule, or None if no rule matches.
///
/// `headers` is the full set of email headers (from EmailContent.headers or
/// parsed from the .eml file during initial sync).
pub fn evaluate(
    rules: &[CompiledRule],
    email: &EmailMeta,
    headers: &HashMap<String, String>,
) -> Option<BundleCategory> {
    // Rules are pre-sorted by priority descending — first match wins
    for rule in rules {
        if matches_rule(rule, email, headers) {
            tracing::debug!(
                rule = rule.name,
                category = ?rule.category,
                email_id = %email.id,
                "header heuristic matched"
            );
            return Some(rule.category.clone());
        }
    }
    None
}

fn matches_rule(
    rule: &CompiledRule,
    email: &EmailMeta,
    headers: &HashMap<String, String>,
) -> bool {
    match &rule.field {
        RuleField::From => {
            let from_str = format!("{}", email.from);
            match_value(&from_str, &rule.operator, &rule.value, rule.regex.as_ref())
        }
        RuleField::Header(header_name) => {
            match &rule.operator {
                RuleOp::Present => {
                    // Case-insensitive header lookup
                    headers.keys().any(|k| k.eq_ignore_ascii_case(header_name))
                }
                RuleOp::PresentWithout => {
                    // value format: "present_header|absent_header"
                    if let Some((present, absent)) = rule.value.split_once('|') {
                        let has_present = headers.keys().any(|k| k.eq_ignore_ascii_case(present));
                        let has_absent = headers.keys().any(|k| k.eq_ignore_ascii_case(absent));
                        has_present && !has_absent
                    } else {
                        false
                    }
                }
                _ => {
                    // Get header value (case-insensitive lookup)
                    let header_val = headers.iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(header_name))
                        .map(|(_, v)| v.as_str())
                        .unwrap_or("");
                    if header_val.is_empty() {
                        return false;
                    }
                    match_value(header_val, &rule.operator, &rule.value, rule.regex.as_ref())
                }
            }
        }
        RuleField::Subject => {
            match_value(&email.subject, &rule.operator, &rule.value, rule.regex.as_ref())
        }
        RuleField::SenderDomain => {
            let from_str = format!("{}", email.from);
            if let Some(domain) = extract_domain(&from_str) {
                match_value(domain, &rule.operator, &rule.value, rule.regex.as_ref())
            } else {
                false
            }
        }
    }
}

fn match_value(haystack: &str, op: &RuleOp, value: &str, regex: Option<&Regex>) -> bool {
    match op {
        RuleOp::Contains => haystack.to_lowercase().contains(&value.to_lowercase()),
        RuleOp::Equals => haystack.eq_ignore_ascii_case(value),
        RuleOp::Matches => regex.map_or(false, |r| r.is_match(haystack)),
        RuleOp::DomainGlob => regex.map_or(false, |r| r.is_match(haystack)),
        RuleOp::Present | RuleOp::PresentWithout => {
            // These are handled at the field level, not the value level
            false
        }
    }
}
```

**Tests:** Unit tests in `heuristics.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use inboxly_core::Contact;

    fn make_email(from_address: &str, subject: &str) -> EmailMeta {
        EmailMeta {
            from: Contact {
                name: String::new(),
                address: from_address.to_string(),
            },
            subject: subject.to_string(),
            // ... other fields with test defaults
            ..EmailMeta::test_default()
        }
    }

    #[test]
    fn extract_domain_plain_address() {
        assert_eq!(extract_domain("user@example.com"), Some("example.com"));
    }

    #[test]
    fn extract_domain_with_name() {
        assert_eq!(
            extract_domain("John Doe <john@sub.example.com>"),
            Some("sub.example.com")
        );
    }

    #[test]
    fn glob_to_regex_wildcard() {
        let re = glob_to_regex("*.amazon.*");
        let compiled = Regex::new(&re).unwrap();
        assert!(compiled.is_match("email.amazon.com"));
        assert!(compiled.is_match("ship.amazon.ca"));
        assert!(!compiled.is_match("notamazon.com"));
    }

    #[test]
    fn list_id_matches_forums() {
        let rules_toml = crate::rules_toml::DEFAULT_RULES_TOML;
        let rules = crate::rules_toml::parse_rules(rules_toml).unwrap();
        let compiled = compile_rules(rules).unwrap();

        let email = make_email("user@lists.example.com", "Re: Discussion");
        let mut headers = HashMap::new();
        headers.insert("List-Id".to_string(), "<dev.lists.example.com>".to_string());

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Forums));
    }

    #[test]
    fn list_unsubscribe_without_list_id_matches_promos() {
        let rules = crate::rules_toml::parse_rules(crate::rules_toml::DEFAULT_RULES_TOML).unwrap();
        let compiled = compile_rules(rules).unwrap();

        let email = make_email("marketing@store.com", "50% off sale!");
        let mut headers = HashMap::new();
        headers.insert("List-Unsubscribe".to_string(), "<mailto:unsub@store.com>".to_string());
        // No List-Id header

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Promos));
    }

    #[test]
    fn facebook_sender_matches_social() {
        let rules = crate::rules_toml::parse_rules(crate::rules_toml::DEFAULT_RULES_TOML).unwrap();
        let compiled = compile_rules(rules).unwrap();

        let email = make_email("notification@facebookmail.com", "You have a new friend request");
        let headers = HashMap::new();

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Social));
    }

    #[test]
    fn amazon_sender_matches_purchases() {
        let rules = crate::rules_toml::parse_rules(crate::rules_toml::DEFAULT_RULES_TOML).unwrap();
        let compiled = compile_rules(rules).unwrap();

        let email = make_email("ship-confirm@ship.amazon.ca", "Your order has shipped");
        let headers = HashMap::new();

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Purchases));
    }

    #[test]
    fn paypal_sender_matches_finance() {
        let rules = crate::rules_toml::parse_rules(crate::rules_toml::DEFAULT_RULES_TOML).unwrap();
        let compiled = compile_rules(rules).unwrap();

        let email = make_email("service@paypal.com", "You sent a payment");
        let headers = HashMap::new();

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Finance));
    }

    #[test]
    fn precedence_bulk_matches_low_priority() {
        let rules = crate::rules_toml::parse_rules(crate::rules_toml::DEFAULT_RULES_TOML).unwrap();
        let compiled = compile_rules(rules).unwrap();

        let email = make_email("system@random.com", "Automated report");
        let mut headers = HashMap::new();
        headers.insert("Precedence".to_string(), "bulk".to_string());

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::LowPriority));
    }

    #[test]
    fn mailchimp_mailer_matches_promos() {
        let rules = crate::rules_toml::parse_rules(crate::rules_toml::DEFAULT_RULES_TOML).unwrap();
        let compiled = compile_rules(rules).unwrap();

        let email = make_email("newsletter@store.com", "Weekly deals");
        let mut headers = HashMap::new();
        headers.insert("X-Mailer".to_string(), "Mailchimp 2.0".to_string());

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Promos));
    }

    #[test]
    fn unknown_sender_returns_none() {
        let rules = crate::rules_toml::parse_rules(crate::rules_toml::DEFAULT_RULES_TOML).unwrap();
        let compiled = compile_rules(rules).unwrap();

        let email = make_email("friend@personal.com", "Hey, lunch tomorrow?");
        let headers = HashMap::new();

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, None);
    }

    #[test]
    fn higher_priority_rule_wins() {
        // If an email has both List-Id (Forums, priority 50) and is from github.com
        // (Social, priority 55), Social should win because it has higher priority.
        let rules = crate::rules_toml::parse_rules(crate::rules_toml::DEFAULT_RULES_TOML).unwrap();
        let compiled = compile_rules(rules).unwrap();

        let email = make_email("notifications@github.com", "New issue opened");
        let mut headers = HashMap::new();
        headers.insert("List-Id".to_string(), "<repo.github.com>".to_string());

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Social));
    }

    #[test]
    fn booking_sender_matches_travel() {
        let rules = crate::rules_toml::parse_rules(crate::rules_toml::DEFAULT_RULES_TOML).unwrap();
        let compiled = compile_rules(rules).unwrap();

        let email = make_email("noreply@booking.com", "Your reservation is confirmed");
        let headers = HashMap::new();

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Travel));
    }

    #[test]
    fn noreply_matches_updates() {
        let rules = crate::rules_toml::parse_rules(crate::rules_toml::DEFAULT_RULES_TOML).unwrap();
        let compiled = compile_rules(rules).unwrap();

        let email = make_email("noreply@someservice.com", "Your account was updated");
        let headers = HashMap::new();

        let result = evaluate(&compiled, &email, &headers);
        assert_eq!(result, Some(BundleCategory::Updates));
    }
}
```

**Commit:** `feat(bundler): header heuristic matching engine (M12)`

---

### Step 6: Implement `Bundler` struct and public API in `lib.rs`

**Action:** Add the `Bundler` struct that holds compiled rules and provides `categorise()` and `categorise_all()` methods. Wire everything together.

**File:** `inboxly-bundler/src/lib.rs` — extend the file from Step 2.

Add after the error types:

```rust
use std::collections::HashMap;
use std::path::PathBuf;

use inboxly_core::{BundleCategory, BundleId, EmailId, EmailMeta, ThreadId};
use inboxly_store::Store;

use crate::heuristics::{self, CompiledRule};
use crate::rules_toml;
use crate::system_bundles;

/// The main bundler engine. Holds compiled heuristic rules and a reference to the store.
pub struct Bundler {
    /// Pre-compiled heuristic rules, sorted by priority descending.
    rules: Vec<CompiledRule>,
    /// Map from BundleCategory to BundleId (system bundles).
    category_to_bundle: HashMap<BundleCategory, BundleId>,
}

impl Bundler {
    /// Create a new Bundler with default rules plus optional user overrides.
    ///
    /// `user_config_path` — path to user's `heuristics.toml`, typically
    /// `~/.config/inboxly/heuristics.toml`. Pass `None` to use only defaults.
    pub fn new(user_config_path: Option<&std::path::Path>) -> Result<Self> {
        let raw_rules = rules_toml::load_rules(user_config_path)?;
        let rules = heuristics::compile_rules(raw_rules)?;

        // Build category -> bundle_id map for system bundles
        let category_to_bundle: HashMap<BundleCategory, BundleId> =
            system_bundles::SYSTEM_BUNDLES
                .iter()
                .map(|def| (def.category.clone(), system_bundles::system_bundle_id(&def.category)))
                .collect();

        Ok(Self {
            rules,
            category_to_bundle,
        })
    }

    /// Categorise a single email using header heuristics (Layer 1).
    ///
    /// Returns `Some((BundleCategory, BundleId))` if a rule matched, `None` otherwise.
    /// The caller is responsible for checking higher-priority layers (user rules,
    /// sender learning) before falling through to this method.
    ///
    /// `headers` — the email's full headers (e.g., from EmailContent.headers or
    /// parsed during sync). Must include headers like List-Id, List-Unsubscribe,
    /// X-Mailer, Precedence for heuristics to fire.
    pub fn categorise(
        &self,
        email: &EmailMeta,
        headers: &HashMap<String, String>,
    ) -> Option<(BundleCategory, BundleId)> {
        let category = heuristics::evaluate(&self.rules, email, headers)?;
        let bundle_id = self.category_to_bundle.get(&category)?.clone();
        Some((category, bundle_id))
    }

    /// Categorise all uncategorised threads in the store.
    ///
    /// Iterates threads where `thread_state.bundle_id` is NULL, loads the newest
    /// email's headers for each, runs the heuristic pipeline, and writes the
    /// bundle assignment back to `thread_state`.
    ///
    /// Returns the number of threads that were categorised.
    pub fn categorise_all(&self, store: &Store) -> Result<u32> {
        let uncategorised_threads = store.get_uncategorised_thread_ids()?;
        let mut categorised_count = 0u32;

        for thread_id in &uncategorised_threads {
            // Get the newest email in the thread for categorisation
            let Some(email) = store.get_newest_email_in_thread(thread_id)? else {
                continue;
            };

            // Load headers for this email
            let headers = store.get_email_headers(&email.id)?;

            if let Some((_category, bundle_id)) = self.categorise(&email, &headers) {
                store.set_thread_bundle(thread_id, Some(&bundle_id))?;
                categorised_count += 1;
            }
        }

        tracing::info!(
            total = uncategorised_threads.len(),
            categorised = categorised_count,
            "batch categorisation complete"
        );

        Ok(categorised_count)
    }

    /// Categorise a single thread by its newest email.
    /// Convenience wrapper that loads the email and headers from the store.
    /// Writes the bundle assignment to thread_state if a match is found.
    ///
    /// Returns the assigned BundleCategory if categorised, None otherwise.
    pub fn categorise_thread(
        &self,
        store: &Store,
        thread_id: &ThreadId,
    ) -> Result<Option<BundleCategory>> {
        let Some(email) = store.get_newest_email_in_thread(thread_id)? else {
            return Ok(None);
        };

        let headers = store.get_email_headers(&email.id)?;

        if let Some((category, bundle_id)) = self.categorise(&email, &headers) {
            store.set_thread_bundle(thread_id, Some(&bundle_id))?;
            Ok(Some(category))
        } else {
            Ok(None)
        }
    }

    /// Get the BundleId for a given system category.
    pub fn bundle_id_for_category(&self, category: &BundleCategory) -> Option<&BundleId> {
        self.category_to_bundle.get(category)
    }

    /// Return the number of compiled heuristic rules.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}
```

**Tests:** None beyond what is already in `heuristics.rs` — the `Bundler` struct is a thin wrapper. Integration tests in the next step cover the full flow.

**Commit:** `feat(bundler): Bundler struct with categorise and categorise_all API (M12)`

---

### Step 7: Write integration tests

**Action:** Write end-to-end integration tests that exercise the full flow: create bundler, ensure system bundles, categorise emails, verify assignments.

**File:** `inboxly-bundler/tests/integration.rs`

```rust
//! Integration tests for the bundler crate.
//!
//! These tests use an in-memory SQLite store and fixture email data
//! to verify the full categorisation pipeline.

use std::collections::HashMap;
use inboxly_bundler::{Bundler, system_bundles};
use inboxly_core::*;
use inboxly_store::Store;

/// Create an in-memory store for testing.
fn test_store() -> Store {
    Store::open_in_memory().expect("failed to create in-memory store")
}

/// Create a test EmailMeta with the given from address and subject.
fn fixture_email(from_addr: &str, subject: &str) -> EmailMeta {
    EmailMeta {
        from: Contact {
            name: String::new(),
            address: from_addr.to_string(),
        },
        subject: subject.to_string(),
        ..EmailMeta::test_default()
    }
}

#[test]
fn ensure_system_bundles_creates_all_eight() {
    let store = test_store();
    let ids = system_bundles::ensure_system_bundles(&store).unwrap();
    assert_eq!(ids.len(), 8);

    // Verify each bundle exists in the store
    for id in &ids {
        let bundle = store.get_bundle(id).unwrap();
        assert!(bundle.is_some(), "bundle {id:?} should exist in store");
    }
}

#[test]
fn ensure_system_bundles_is_idempotent() {
    let store = test_store();

    let ids1 = system_bundles::ensure_system_bundles(&store).unwrap();
    let ids2 = system_bundles::ensure_system_bundles(&store).unwrap();

    assert_eq!(ids1, ids2, "bundle IDs should be identical across calls");

    // Should still only have 8 bundles total
    let all_bundles = store.list_bundles().unwrap();
    assert_eq!(all_bundles.len(), 8);
}

#[test]
fn categorise_facebook_email_as_social() {
    let bundler = Bundler::new(None).unwrap();
    let email = fixture_email("notification@facebookmail.com", "You have a new message");
    let headers = HashMap::new();

    let result = bundler.categorise(&email, &headers);
    assert!(result.is_some());
    let (category, _bundle_id) = result.unwrap();
    assert_eq!(category, BundleCategory::Social);
}

#[test]
fn categorise_mailchimp_as_promos() {
    let bundler = Bundler::new(None).unwrap();
    let email = fixture_email("deals@store.com", "Weekly newsletter");
    let mut headers = HashMap::new();
    headers.insert("X-Mailer".to_string(), "Mailchimp v3.0".to_string());

    let result = bundler.categorise(&email, &headers);
    assert!(result.is_some());
    let (category, _) = result.unwrap();
    assert_eq!(category, BundleCategory::Promos);
}

#[test]
fn categorise_mailing_list_as_forums() {
    let bundler = Bundler::new(None).unwrap();
    let email = fixture_email("user@lists.example.com", "Re: RFC discussion");
    let mut headers = HashMap::new();
    headers.insert("List-Id".to_string(), "<dev.lists.example.com>".to_string());

    let result = bundler.categorise(&email, &headers);
    assert!(result.is_some());
    let (category, _) = result.unwrap();
    assert_eq!(category, BundleCategory::Forums);
}

#[test]
fn categorise_personal_email_returns_none() {
    let bundler = Bundler::new(None).unwrap();
    let email = fixture_email("friend@gmail.com", "Hey, want to grab coffee?");
    let headers = HashMap::new();

    let result = bundler.categorise(&email, &headers);
    assert!(result.is_none(), "personal emails should not be categorised");
}

#[test]
fn categorise_all_assigns_bundles_in_store() {
    let store = test_store();
    let bundler = Bundler::new(None).unwrap();
    system_bundles::ensure_system_bundles(&store).unwrap();

    // Insert test data: 3 threads with different senders
    let emails = vec![
        ("notifications@github.com", "New PR review requested"),
        ("ship-confirm@ship.amazon.ca", "Your package has shipped"),
        ("friend@personal.com", "Dinner plans"),
    ];

    for (addr, subject) in &emails {
        let email = fixture_email(addr, subject);
        store.insert_email(&email).unwrap();
        store.ensure_thread_state(&email.thread_id).unwrap();
    }

    let categorised = bundler.categorise_all(&store).unwrap();

    // GitHub -> Social, Amazon -> Purchases, personal -> uncategorised
    assert_eq!(categorised, 2, "should categorise 2 out of 3 threads");
}

#[test]
fn categorise_thread_writes_bundle_assignment() {
    let store = test_store();
    let bundler = Bundler::new(None).unwrap();
    system_bundles::ensure_system_bundles(&store).unwrap();

    let email = fixture_email("noreply@booking.com", "Your reservation");
    store.insert_email(&email).unwrap();
    store.ensure_thread_state(&email.thread_id).unwrap();

    let result = bundler.categorise_thread(&store, &email.thread_id).unwrap();
    assert_eq!(result, Some(BundleCategory::Travel));

    // Verify it was persisted
    let state = store.get_thread_state(&email.thread_id).unwrap().unwrap();
    assert!(state.bundle_id.is_some());
}

#[test]
fn bundler_reports_rule_count() {
    let bundler = Bundler::new(None).unwrap();
    assert!(bundler.rule_count() >= 20, "should have at least 20 default rules");
}

#[test]
fn all_system_categories_have_bundle_ids() {
    let bundler = Bundler::new(None).unwrap();
    let categories = vec![
        BundleCategory::Social,
        BundleCategory::Promos,
        BundleCategory::Updates,
        BundleCategory::Finance,
        BundleCategory::Purchases,
        BundleCategory::Travel,
        BundleCategory::Forums,
        BundleCategory::LowPriority,
    ];

    for cat in categories {
        assert!(
            bundler.bundle_id_for_category(&cat).is_some(),
            "missing bundle ID for category {cat:?}"
        );
    }
}
```

**Tests:** `cargo test -p inboxly-bundler` — all tests pass.

**Commit:** `test(bundler): integration tests for categorisation pipeline (M12)`

---

### Step 8: Document public API and add crate-level docs

**Action:** Add rustdoc comments to all public items. Ensure `cargo doc -p inboxly-bundler --no-deps` produces clean output.

**File:** Enhance doc comments on all public items in `lib.rs`, `heuristics.rs`, `rules_toml.rs`, `system_bundles.rs`. The exact doc strings are provided in the code blocks above — verify they are present and add any missing ones.

Add to `lib.rs` at the top:

```rust
//! # inboxly-bundler
//!
//! Email categorisation engine for Inboxly. Implements a three-layer system:
//!
//! 1. **User-defined rules** (highest priority) — explicit sender/header rules (M13)
//! 2. **Sender learning** (medium priority) — learned from user actions (M13)
//! 3. **Header heuristics** (lowest priority) — zero-config pattern matching (this milestone)
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use inboxly_bundler::{Bundler, system_bundles};
//! use inboxly_store::Store;
//!
//! let store = Store::open("~/.local/share/inboxly/data.db")?;
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
//! ```
```

**Tests:** `cargo doc -p inboxly-bundler --no-deps` succeeds with no warnings.

**Commit:** `docs(bundler): rustdoc for public API (M12)`

---

### Step 9: Final verification

**Action:** Run the full test and lint suite.

```bash
cargo test -p inboxly-bundler
cargo clippy -p inboxly-bundler -- -D warnings
cargo doc -p inboxly-bundler --no-deps
```

All three must pass with zero warnings. Fix any issues found.

**Commit:** `chore(bundler): clippy and doc clean-up (M12)` (only if fixes were needed)

---

## Store API Requirements

This milestone assumes the following methods exist on `inboxly_store::Store` (from M3/M4). If they do not exist when M12 is implemented, they must be added to `inboxly-store` as part of this milestone:

| Method | Signature | Purpose |
|--------|-----------|---------|
| `get_bundle` | `(&self, id: &BundleId) -> Result<Option<Bundle>>` | Check if a bundle exists |
| `insert_bundle` | `(&self, bundle: &Bundle) -> Result<()>` | Create a new bundle |
| `list_bundles` | `(&self) -> Result<Vec<Bundle>>` | List all bundles |
| `get_uncategorised_thread_ids` | `(&self) -> Result<Vec<ThreadId>>` | Threads where `thread_state.bundle_id IS NULL` |
| `get_newest_email_in_thread` | `(&self, thread_id: &ThreadId) -> Result<Option<EmailMeta>>` | Newest email by date in a thread |
| `get_email_headers` | `(&self, email_id: &EmailId) -> Result<HashMap<String, String>>` | Parse headers from the `.eml` file on disk |
| `set_thread_bundle` | `(&self, thread_id: &ThreadId, bundle_id: Option<&BundleId>) -> Result<()>` | Update `thread_state.bundle_id` |
| `ensure_thread_state` | `(&self, thread_id: &ThreadId) -> Result<()>` | Create `thread_state` row if not exists |
| `get_thread_state` | `(&self, thread_id: &ThreadId) -> Result<Option<ThreadState>>` | Read thread state |
| `insert_email` | `(&self, email: &EmailMeta) -> Result<()>` | Insert email metadata |

## Core Type Requirements

This milestone assumes the following exist in `inboxly-core` (from M1):

| Type | Notes |
|------|-------|
| `BundleCategory` | Enum with `Social, Promos, Updates, Finance, Purchases, Travel, Forums, LowPriority, Saved, Custom(String)`. Must derive `Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize`. |
| `BundleId(Uuid)` | Must derive `Clone, Debug, PartialEq, Eq, Hash`. |
| `BundleIcon` | Enum with variants matching system bundles: `Social, Promos, Updates, Finance, Purchases, Travel, Forums, LowPriority`. Must derive `Clone, Debug`. |
| `BundleVisibility` | Enum: `Bundled, Unbundled, SkipInbox`. Must derive `Clone, Debug`. |
| `BundleThrottle` | Enum: `Immediate, Daily, Weekly`. Must derive `Clone, Debug`. |
| `Bundle` | Struct with all fields from the spec. Must derive `Clone, Debug`. |
| `EmailMeta` | Struct from spec. Must have `test_default()` constructor for test fixtures. `Contact` must implement `Display`. |
| `ThreadId(Uuid)` | Must derive `Clone, Debug, PartialEq, Eq, Hash`. |
| `EmailId(String)` | Must derive `Clone, Debug, PartialEq, Eq, Hash`. |
| `ThreadState` | Struct with `bundle_id: Option<BundleId>` field. |
| `Contact` | Struct with `name: String, address: String`. Must implement `Display` (formatting as `"name <address>"` or just `"address"` if name is empty). |
| `Color` | Struct with `r: u8, g: u8, b: u8, a: u8`. |

## Summary

| Step | Action | Files | Tests |
|------|--------|-------|-------|
| 1 | Scaffold crate | `Cargo.toml`, workspace root | `cargo check` |
| 2 | Error types | `src/lib.rs` | `cargo check` |
| 3 | TOML rule definitions | `src/rules_toml.rs`, `src/default_rules.toml` | 4 unit tests |
| 4 | System bundles | `src/system_bundles.rs` | 4 unit tests |
| 5 | Header matching engine | `src/heuristics.rs` | 11 unit tests |
| 6 | Bundler struct + API | `src/lib.rs` (extended) | covered by integration |
| 7 | Integration tests | `tests/integration.rs` | 9 integration tests |
| 8 | Documentation | all `src/*.rs` | `cargo doc` clean |
| 9 | Final verification | — | clippy + test + doc |

**Total: 9 steps, ~28 tests, 5 source files + 1 TOML data file.**
