# M31: Store Trait Integration + IMAP Action Execution — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make UI actions (archive, mark read, move, trash, spam) actually sync to the IMAP server, and wire the bundler engine to the real Store.

**Architecture:** Two parallel workstreams: (1) Move bundler store traits from `inboxly-bundler` to `inboxly-core` to break the circular dependency, then implement them on `Store`. (2) Wire UI action handlers to enqueue `OfflineAction`s and call the existing `replay_offline_queue()` at the right lifecycle points. Add archive/all-mail folder resolution for provider-specific archive behaviour.

**Tech Stack:** Rust, rusqlite, async-imap, tokio, serde, chrono, uuid

**Spec:** `docs/superpowers/specs/2026-04-06-inboxly-v2-full-client-design.md` — M31 section

---

## File Structure

### New Files
| File | Responsibility |
|------|---------------|
| `inboxly-core/src/store_traits/mod.rs` | Module root — re-exports all store traits |
| `inboxly-core/src/store_traits/rule_store.rs` | `RuleStore` trait, `RuleStoreError`, `CreateRuleParams`, `UpdateRuleParams`, `BundleRule`, `RuleId`, `UserRuleField`, `UserRuleOp` |
| `inboxly-core/src/store_traits/affinity_store.rs` | `AffinityStore` trait, `AffinityStoreError`, `SenderAffinity` |
| `inboxly-core/src/store_traits/bundle_store.rs` | `BundleStore` trait, `BundleStoreError`, `CreateBundleParams`, `UpdateBundleParams`, `BundleInfo` |
| `inboxly-store/src/trait_impls/mod.rs` | Module root for trait impl blocks |
| `inboxly-store/src/trait_impls/rule_store_impl.rs` | `impl RuleStore for Store` — delegates to `bundle_rules.rs` SQL methods with type conversion |
| `inboxly-store/src/trait_impls/affinity_store_impl.rs` | `impl AffinityStore for Store` — delegates to `sender_affinity.rs` SQL methods |
| `inboxly-store/src/trait_impls/bundle_store_impl.rs` | `impl BundleStore for Store` — delegates to `bundles.rs` SQL methods |
| `inboxly-store/tests/rule_store_trait.rs` | Integration tests for `RuleStore` impl |
| `inboxly-store/tests/affinity_store_trait.rs` | Integration tests for `AffinityStore` impl |
| `inboxly-store/tests/bundle_store_trait.rs` | Integration tests for `BundleStore` impl |
| `inboxly-store/tests/action_enqueue.rs` | Tests for offline action enqueue from UI-equivalent operations |

### Modified Files
| File | Change |
|------|--------|
| `inboxly-core/src/lib.rs` | Add `pub mod store_traits;` |
| `inboxly-core/Cargo.toml` | Add `chrono`, `uuid` to dependencies (if not already present — needed by `SenderAffinity` and `BundleInfo`) |
| `inboxly-bundler/src/rule_store.rs` | Replace local type/trait definitions with re-exports from `inboxly-core::store_traits` |
| `inboxly-bundler/src/affinity.rs` | Replace local `AffinityStore` trait + `SenderAffinity` with re-exports from core |
| `inboxly-bundler/src/custom_bundle.rs` | Replace local `BundleStore` trait + types with re-exports from core |
| `inboxly-store/src/lib.rs` | Add `mod trait_impls;` |
| `inboxly-store/Cargo.toml` | Ensure `inboxly-core` dependency includes the store_traits feature (likely already present) |
| `inboxly-imap/src/folders.rs` | Add `archive` field to `WellKnownFolders`, resolve `[Gmail]/All Mail` / `Archive` |
| `inboxly-imap/src/sync_loop.rs` | Call `replay_offline_queue()` on sync startup and IDLE reconnect |
| `inboxly-ui/src/app.rs` | Wire `MarkDone`, `MarkReadState`, `MoveTo`, `ReportSpam`, `MuteThread`, `BlockSender` handlers to enqueue offline actions |

### Deleted Files
| File | Reason |
|------|--------|
| `inboxly-core/src/traits.rs` | Aspirational traits — never implemented, superseded by store_traits module |

---

## Task 1: Move RuleStore Trait to inboxly-core

**Files:**
- Create: `inboxly-core/src/store_traits/mod.rs`
- Create: `inboxly-core/src/store_traits/rule_store.rs`
- Modify: `inboxly-core/src/lib.rs`
- Modify: `inboxly-core/Cargo.toml` (if `uuid` not already a dependency)

This task moves the `RuleStore` trait and all associated types from `inboxly-bundler/src/rule_store.rs` to `inboxly-core/src/store_traits/rule_store.rs`. The bundler currently owns these definitions, but the Store (in `inboxly-store`) needs to implement them. Since `inboxly-store` cannot depend on `inboxly-bundler` (reverse dep exists), we move the trait to the shared foundation crate `inboxly-core`.

- [ ] **Step 1: Create the store_traits module in inboxly-core**

Create `inboxly-core/src/store_traits/mod.rs`:

```rust
//! Store trait definitions for cross-crate implementation.
//!
//! These traits define the storage interface that `inboxly-store::Store`
//! implements and `inboxly-bundler::BundlerEngine` consumes. They live
//! in `inboxly-core` to avoid circular dependencies between the two crates.

mod rule_store;
mod affinity_store;
mod bundle_store;

pub use rule_store::*;
pub use affinity_store::*;
pub use bundle_store::*;
```

Create placeholder files so it compiles:

`inboxly-core/src/store_traits/affinity_store.rs`:
```rust
// Placeholder — populated in Task 2
```

`inboxly-core/src/store_traits/bundle_store.rs`:
```rust
// Placeholder — populated in Task 3
```

Add to `inboxly-core/src/lib.rs`:
```rust
pub mod store_traits;
```

- [ ] **Step 2: Copy RuleStore types to inboxly-core**

Read `inboxly-bundler/src/rule_store.rs` to get the exact type definitions. Copy the following to `inboxly-core/src/store_traits/rule_store.rs`:

- `RuleId` (newtype wrapper)
- `UserRuleField` enum
- `UserRuleOp` enum
- `BundleRule` struct
- `CreateRuleParams` struct
- `UpdateRuleParams` struct
- `RuleStoreError` enum
- `RuleStore` trait

Adapt imports to use `inboxly-core` types (e.g., `uuid::Uuid`). The file should look like:

```rust
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique identifier for a user-created bundle rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RuleId(pub String);

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ... (copy remaining types exactly from inboxly-bundler/src/rule_store.rs lines 1-134)
// Keep all derives, doc comments, and method impls intact.
```

**Important:** Copy the EXACT definitions from the bundler source. Do not paraphrase or simplify.

- [ ] **Step 3: Verify inboxly-core compiles**

Run: `cargo check -p inboxly-core`
Expected: Compiles with 0 errors. May have unused import warnings — that's fine, they resolve when bundler switches over.

- [ ] **Step 4: Commit**

```bash
git add inboxly-core/src/store_traits/ inboxly-core/src/lib.rs
git commit -m "refactor(core): add store_traits module with RuleStore trait

Move RuleStore trait, RuleId, BundleRule, CreateRuleParams, UpdateRuleParams,
UserRuleField, UserRuleOp, and RuleStoreError from inboxly-bundler to
inboxly-core to break circular dependency. Bundler will re-export in next task."
```

---

## Task 2: Move AffinityStore Trait to inboxly-core

**Files:**
- Modify: `inboxly-core/src/store_traits/affinity_store.rs`

- [ ] **Step 1: Copy AffinityStore types to inboxly-core**

Read `inboxly-bundler/src/affinity.rs` lines 1-152. Copy to `inboxly-core/src/store_traits/affinity_store.rs`:

- `SenderAffinity` struct (with all methods: `effective_confidence`, `is_confident`, `reinforce`, `penalize`)
- `AffinityStoreError` enum
- `AffinityStore` trait
- All constants (`CONFIDENCE_THRESHOLD`, `CONFIDENCE_INCREMENT`, `CONFIDENCE_MAX`, `CONFIDENCE_OVERRIDE_PENALTY`, `CONFIDENCE_HALF_LIFE_DAYS`)

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Half-life for confidence decay in days.
pub const CONFIDENCE_HALF_LIFE_DAYS: f64 = 90.0;
pub const CONFIDENCE_THRESHOLD: f64 = 0.6;
pub const CONFIDENCE_INCREMENT: f64 = 0.2;
pub const CONFIDENCE_MAX: f64 = 1.0;
pub const CONFIDENCE_OVERRIDE_PENALTY: f64 = 0.3;

// ... (copy remaining types exactly from inboxly-bundler/src/affinity.rs)
```

- [ ] **Step 2: Verify inboxly-core compiles**

Run: `cargo check -p inboxly-core`
Expected: Compiles with 0 errors.

- [ ] **Step 3: Commit**

```bash
git add inboxly-core/src/store_traits/affinity_store.rs
git commit -m "refactor(core): add AffinityStore trait to store_traits

Move AffinityStore trait, SenderAffinity, AffinityStoreError, and confidence
constants from inboxly-bundler to inboxly-core."
```

---

## Task 3: Move BundleStore Trait to inboxly-core

**Files:**
- Modify: `inboxly-core/src/store_traits/bundle_store.rs`

- [ ] **Step 1: Copy BundleStore types to inboxly-core**

Read `inboxly-bundler/src/custom_bundle.rs` lines 1-121. Copy to `inboxly-core/src/store_traits/bundle_store.rs`:

- `CreateBundleParams` struct
- `UpdateBundleParams` struct
- `BundleInfo` struct
- `BundleStoreError` enum
- `BundleStore` trait

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::bundle::{BundleVisibility, BundleThrottle};  // Already in inboxly-core

// ... (copy remaining types exactly from inboxly-bundler/src/custom_bundle.rs)
```

**Note:** `BundleVisibility` and `BundleThrottle` should already exist in `inboxly-core` (they're part of the bundle configuration model). Verify by checking `inboxly-core/src/bundle.rs` or similar. If they're in the bundler, they need to move to core too.

- [ ] **Step 2: Verify inboxly-core compiles**

Run: `cargo check -p inboxly-core`
Expected: Compiles with 0 errors.

- [ ] **Step 3: Commit**

```bash
git add inboxly-core/src/store_traits/bundle_store.rs
git commit -m "refactor(core): add BundleStore trait to store_traits

Move BundleStore trait, BundleInfo, CreateBundleParams, UpdateBundleParams,
and BundleStoreError from inboxly-bundler to inboxly-core."
```

---

## Task 4: Update inboxly-bundler to Re-export from Core

**Files:**
- Modify: `inboxly-bundler/src/rule_store.rs`
- Modify: `inboxly-bundler/src/affinity.rs`
- Modify: `inboxly-bundler/src/custom_bundle.rs`
- Modify: `inboxly-bundler/src/lib.rs` (if it re-exports these modules)
- Modify: `inboxly-bundler/Cargo.toml` (if needed)

This task replaces the local definitions in `inboxly-bundler` with re-exports from `inboxly-core::store_traits`. All downstream consumers of the bundler see the same types — zero API change.

- [ ] **Step 1: Replace rule_store.rs definitions with re-exports**

Read `inboxly-bundler/src/rule_store.rs` to identify which types are defined locally vs imported. Replace the local definitions with:

```rust
// Re-export from inboxly-core — canonical definitions live there
// to allow inboxly-store to implement these traits without circular deps.
pub use inboxly_core::store_traits::{
    BundleRule, CreateRuleParams, RuleId, RuleStore, RuleStoreError,
    UpdateRuleParams, UserRuleField, UserRuleOp,
};

// Any bundler-specific code that USES these types (not defines them) stays here.
// For example, rule evaluation logic, rule matching, etc.
```

Keep any implementation code that uses these types (e.g., rule evaluation functions). Only remove the type/trait **definitions**.

- [ ] **Step 2: Replace affinity.rs definitions with re-exports**

```rust
pub use inboxly_core::store_traits::{
    AffinityStore, AffinityStoreError, SenderAffinity,
    CONFIDENCE_HALF_LIFE_DAYS, CONFIDENCE_INCREMENT, CONFIDENCE_MAX,
    CONFIDENCE_OVERRIDE_PENALTY, CONFIDENCE_THRESHOLD,
};

// Keep any bundler-specific affinity logic (scoring algorithms, etc.)
```

- [ ] **Step 3: Replace custom_bundle.rs definitions with re-exports**

```rust
pub use inboxly_core::store_traits::{
    BundleInfo, BundleStore, BundleStoreError, CreateBundleParams, UpdateBundleParams,
};

// Keep any bundler-specific bundle logic
```

- [ ] **Step 4: Verify entire workspace compiles**

Run: `cargo check --workspace`
Expected: Compiles with 0 errors. All bundler tests still reference the same types via re-exports.

- [ ] **Step 5: Run bundler tests to verify nothing broke**

Run: `cargo test -p inboxly-bundler`
Expected: All existing bundler tests pass.

- [ ] **Step 6: Commit**

```bash
git add inboxly-bundler/src/rule_store.rs inboxly-bundler/src/affinity.rs inboxly-bundler/src/custom_bundle.rs
git commit -m "refactor(bundler): re-export store traits from inboxly-core

Replace local trait/type definitions with re-exports from
inboxly_core::store_traits. Zero API change for consumers."
```

---

## Task 5: Implement RuleStore for Store

**Files:**
- Create: `inboxly-store/src/trait_impls/mod.rs`
- Create: `inboxly-store/src/trait_impls/rule_store_impl.rs`
- Modify: `inboxly-store/src/lib.rs`
- Create: `inboxly-store/tests/rule_store_trait.rs`

The `Store` already has SQL methods in `bundle_rules.rs` that operate on `BundleRuleRow`. This task bridges the gap: convert between `BundleRuleRow` (storage layer) and `BundleRule`/`CreateRuleParams` (trait layer).

- [ ] **Step 1: Write failing tests for RuleStore**

Create `inboxly-store/tests/rule_store_trait.rs`:

```rust
use inboxly_core::store_traits::{
    CreateRuleParams, RuleStore, RuleStoreError, UpdateRuleParams, UserRuleField, UserRuleOp,
};
use inboxly_store::Store;
use uuid::Uuid;

fn test_store() -> Store {
    Store::open_in_memory().expect("open in-memory store")
}

#[test]
fn create_and_get_rule() {
    let store = test_store();
    let bundle_id = Uuid::new_v4();
    let params = CreateRuleParams {
        bundle_id,
        field: UserRuleField::From,
        operator: UserRuleOp::Contains,
        value: "newsletter@example.com".to_string(),
        priority: 10,
    };
    let rule = store.create_rule(params).expect("create rule");
    assert_eq!(rule.bundle_id, bundle_id);
    assert_eq!(rule.field, UserRuleField::From);
    assert_eq!(rule.operator, UserRuleOp::Contains);
    assert_eq!(rule.value, "newsletter@example.com");
    assert_eq!(rule.priority, 10);

    let fetched = store.get_rule(rule.id.clone()).expect("get rule");
    assert_eq!(fetched.id, rule.id);
    assert_eq!(fetched.bundle_id, bundle_id);
}

#[test]
fn list_rules_and_filter_by_bundle() {
    let store = test_store();
    let bundle_a = Uuid::new_v4();
    let bundle_b = Uuid::new_v4();

    store.create_rule(CreateRuleParams {
        bundle_id: bundle_a,
        field: UserRuleField::From,
        operator: UserRuleOp::Contains,
        value: "a@example.com".to_string(),
        priority: 1,
    }).unwrap();
    store.create_rule(CreateRuleParams {
        bundle_id: bundle_b,
        field: UserRuleField::Subject,
        operator: UserRuleOp::Equals,
        value: "invoice".to_string(),
        priority: 2,
    }).unwrap();

    let all = store.list_rules().expect("list all");
    assert_eq!(all.len(), 2);

    let for_a = store.list_rules_for_bundle(bundle_a).expect("list for a");
    assert_eq!(for_a.len(), 1);
    assert_eq!(for_a[0].value, "a@example.com");
}

#[test]
fn update_rule() {
    let store = test_store();
    let rule = store.create_rule(CreateRuleParams {
        bundle_id: Uuid::new_v4(),
        field: UserRuleField::From,
        operator: UserRuleOp::Contains,
        value: "old@example.com".to_string(),
        priority: 1,
    }).unwrap();

    let updated = store.update_rule(rule.id.clone(), UpdateRuleParams {
        value: Some("new@example.com".to_string()),
        priority: Some(99),
        ..Default::default()
    }).expect("update rule");

    assert_eq!(updated.value, "new@example.com");
    assert_eq!(updated.priority, 99);
}

#[test]
fn delete_rule() {
    let store = test_store();
    let rule = store.create_rule(CreateRuleParams {
        bundle_id: Uuid::new_v4(),
        field: UserRuleField::From,
        operator: UserRuleOp::Contains,
        value: "delete-me@example.com".to_string(),
        priority: 1,
    }).unwrap();

    store.delete_rule(rule.id.clone()).expect("delete");
    let result = store.get_rule(rule.id);
    assert!(matches!(result, Err(RuleStoreError::NotFound(_))));
}

#[test]
fn get_nonexistent_rule_returns_not_found() {
    let store = test_store();
    let result = store.get_rule(inboxly_core::store_traits::RuleId("nonexistent".to_string()));
    assert!(matches!(result, Err(RuleStoreError::NotFound(_))));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p inboxly-store --test rule_store_trait`
Expected: Compilation error — `RuleStore` not implemented for `Store`.

- [ ] **Step 3: Create trait_impls module and implement RuleStore**

Create `inboxly-store/src/trait_impls/mod.rs`:

```rust
mod rule_store_impl;
mod affinity_store_impl;
mod bundle_store_impl;
```

Create placeholder files:

`inboxly-store/src/trait_impls/affinity_store_impl.rs`:
```rust
// Populated in Task 6
```

`inboxly-store/src/trait_impls/bundle_store_impl.rs`:
```rust
// Populated in Task 7
```

Add to `inboxly-store/src/lib.rs`:
```rust
mod trait_impls;
```

Create `inboxly-store/src/trait_impls/rule_store_impl.rs`:

```rust
use inboxly_core::store_traits::{
    BundleRule, CreateRuleParams, RuleId, RuleStore, RuleStoreError,
    UpdateRuleParams, UserRuleField, UserRuleOp,
};
use crate::bundle_rules::BundleRuleRow;
use crate::Store;
use uuid::Uuid;

/// Convert a `BundleRuleRow` (SQL layer) to a `BundleRule` (trait layer).
fn row_to_rule(row: BundleRuleRow) -> Result<BundleRule, RuleStoreError> {
    let field = match row.field.as_str() {
        "from" => UserRuleField::From,
        "to" => UserRuleField::To,
        "subject" => UserRuleField::Subject,
        "body" => UserRuleField::Body,
        other => return Err(RuleStoreError::InvalidField(other.to_string())),
    };
    let operator = match row.operator.as_str() {
        "contains" => UserRuleOp::Contains,
        "equals" => UserRuleOp::Equals,
        "starts_with" => UserRuleOp::StartsWith,
        "regex" => UserRuleOp::Regex,
        other => return Err(RuleStoreError::InvalidField(format!("unknown operator: {other}"))),
    };
    let bundle_id = Uuid::parse_str(&row.bundle_id)
        .map_err(|e| RuleStoreError::Database(format!("invalid bundle_id UUID: {e}")))?;

    Ok(BundleRule {
        id: RuleId(row.id),
        bundle_id,
        field,
        operator,
        value: row.value,
        priority: row.priority,
    })
}

/// Convert trait-layer field/operator enums to SQL string representations.
fn field_to_str(field: &UserRuleField) -> &'static str {
    match field {
        UserRuleField::From => "from",
        UserRuleField::To => "to",
        UserRuleField::Subject => "subject",
        UserRuleField::Body => "body",
    }
}

fn op_to_str(op: &UserRuleOp) -> &'static str {
    match op {
        UserRuleOp::Contains => "contains",
        UserRuleOp::Equals => "equals",
        UserRuleOp::StartsWith => "starts_with",
        UserRuleOp::Regex => "regex",
    }
}

impl RuleStore for Store {
    fn create_rule(&self, params: CreateRuleParams) -> Result<BundleRule, RuleStoreError> {
        let id = Uuid::new_v4().to_string();
        let row = BundleRuleRow {
            id: id.clone(),
            bundle_id: params.bundle_id.to_string(),
            field: field_to_str(&params.field).to_string(),
            operator: op_to_str(&params.operator).to_string(),
            value: params.value,
            priority: params.priority,
        };
        self.insert_bundle_rule(&row)
            .map_err(|e| RuleStoreError::Database(e.to_string()))?;
        self.get_rule(RuleId(id))
    }

    fn get_rule(&self, id: RuleId) -> Result<BundleRule, RuleStoreError> {
        let row = self.get_bundle_rule(&id.0)
            .map_err(|e| {
                // rusqlite returns QueryReturnedNoRows for missing rows
                if e.to_string().contains("no rows") || e.to_string().contains("QueryReturnedNoRows") {
                    RuleStoreError::NotFound(id.clone())
                } else {
                    RuleStoreError::Database(e.to_string())
                }
            })?;
        row_to_rule(row)
    }

    fn list_rules(&self) -> Result<Vec<BundleRule>, RuleStoreError> {
        let rows = self.get_all_bundle_rules()
            .map_err(|e| RuleStoreError::Database(e.to_string()))?;
        rows.into_iter().map(row_to_rule).collect()
    }

    fn list_rules_for_bundle(&self, bundle_id: Uuid) -> Result<Vec<BundleRule>, RuleStoreError> {
        let rows = self.get_rules_for_bundle(&bundle_id.to_string())
            .map_err(|e| RuleStoreError::Database(e.to_string()))?;
        rows.into_iter().map(row_to_rule).collect()
    }

    fn update_rule(&self, id: RuleId, params: UpdateRuleParams) -> Result<BundleRule, RuleStoreError> {
        // Fetch existing rule first
        let mut row = self.get_bundle_rule(&id.0)
            .map_err(|e| {
                if e.to_string().contains("no rows") || e.to_string().contains("QueryReturnedNoRows") {
                    RuleStoreError::NotFound(id.clone())
                } else {
                    RuleStoreError::Database(e.to_string())
                }
            })?;

        // Apply partial updates
        if let Some(bundle_id) = params.bundle_id {
            row.bundle_id = bundle_id.to_string();
        }
        if let Some(field) = &params.field {
            row.field = field_to_str(field).to_string();
        }
        if let Some(op) = &params.operator {
            row.operator = op_to_str(op).to_string();
        }
        if let Some(value) = params.value {
            row.value = value;
        }
        if let Some(priority) = params.priority {
            row.priority = priority;
        }

        self.update_bundle_rule(&row)
            .map_err(|e| RuleStoreError::Database(e.to_string()))?;
        row_to_rule(row)
    }

    fn delete_rule(&self, id: RuleId) -> Result<(), RuleStoreError> {
        // Verify it exists first
        let _ = self.get_rule(id.clone())?;
        self.delete_bundle_rule(&id.0)
            .map_err(|e| RuleStoreError::Database(e.to_string()))
    }
}
```

**Important:** Verify exact field names of `BundleRuleRow`, `BundleRule`, and enum variant names by reading the source files. The conversion functions above are based on the exploration report — adjust if actual code differs.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p inboxly-store --test rule_store_trait`
Expected: All 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add inboxly-store/src/trait_impls/ inboxly-store/src/lib.rs inboxly-store/tests/rule_store_trait.rs
git commit -m "feat(store): implement RuleStore trait for Store

Bridges bundle_rules.rs SQL methods to the RuleStore trait interface.
Converts between BundleRuleRow (storage) and BundleRule (trait) types.
5 integration tests cover CRUD + not-found error handling."
```

---

## Task 6: Implement AffinityStore for Store

**Files:**
- Modify: `inboxly-store/src/trait_impls/affinity_store_impl.rs`
- Create: `inboxly-store/tests/affinity_store_trait.rs`

- [ ] **Step 1: Write failing tests**

Create `inboxly-store/tests/affinity_store_trait.rs`:

```rust
use chrono::Utc;
use inboxly_core::store_traits::{AffinityStore, SenderAffinity};
use inboxly_store::Store;

fn test_store() -> Store {
    Store::open_in_memory().expect("open in-memory store")
}

#[test]
fn record_and_get_affinity() {
    let store = test_store();
    let now = Utc::now();
    let affinity = store
        .record_affinity("alice@example.com", "example.com", "social", now)
        .expect("record affinity");
    assert_eq!(affinity.sender_address, "alice@example.com");
    assert_eq!(affinity.sender_domain, "example.com");
    assert_eq!(affinity.bundle_category, "social");
    assert!(affinity.confidence > 0.0);

    let fetched = store
        .get_affinity("alice@example.com")
        .expect("get affinity");
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().bundle_category, "social");
}

#[test]
fn get_nonexistent_affinity_returns_none() {
    let store = test_store();
    let result = store.get_affinity("nobody@example.com").expect("get affinity");
    assert!(result.is_none());
}

#[test]
fn list_affinities() {
    let store = test_store();
    let now = Utc::now();
    store.record_affinity("a@example.com", "example.com", "social", now).unwrap();
    store.record_affinity("b@other.com", "other.com", "promos", now).unwrap();

    let all = store.list_affinities().expect("list");
    assert_eq!(all.len(), 2);
}

#[test]
fn delete_affinity() {
    let store = test_store();
    let now = Utc::now();
    store.record_affinity("delete-me@example.com", "example.com", "social", now).unwrap();
    store.delete_affinity("delete-me@example.com").expect("delete");
    let result = store.get_affinity("delete-me@example.com").expect("get");
    assert!(result.is_none());
}

#[test]
fn record_affinity_upserts_on_duplicate() {
    let store = test_store();
    let now = Utc::now();
    store.record_affinity("dup@example.com", "example.com", "social", now).unwrap();
    let updated = store.record_affinity("dup@example.com", "example.com", "promos", now).unwrap();
    // Should update, not create a second row
    assert_eq!(updated.bundle_category, "promos");
    let all = store.list_affinities().unwrap();
    // May be 1 (true upsert) or 2 (separate rows per category) — depends on existing upsert logic.
    // Verify against actual sender_affinity.rs upsert_sender_affinity behaviour.
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p inboxly-store --test affinity_store_trait`
Expected: Compilation error — `AffinityStore` not implemented for `Store`.

- [ ] **Step 3: Implement AffinityStore for Store**

Write `inboxly-store/src/trait_impls/affinity_store_impl.rs`:

```rust
use chrono::{DateTime, Utc};
use inboxly_core::store_traits::{
    AffinityStore, AffinityStoreError, SenderAffinity, CONFIDENCE_INCREMENT,
};
use crate::sender_affinity::SenderAffinityRow;
use crate::Store;

fn row_to_affinity(row: SenderAffinityRow) -> SenderAffinity {
    SenderAffinity {
        sender_domain: row.sender_domain,
        sender_address: row.sender_address,
        bundle_category: row.bundle_category,
        confidence: row.confidence,
        learned_at: DateTime::from_timestamp(row.learned_at, 0)
            .unwrap_or_else(|| Utc::now()),
    }
}

impl AffinityStore for Store {
    fn get_affinity(&self, sender_address: &str) -> Result<Option<SenderAffinity>, AffinityStoreError> {
        self.get_sender_affinity(sender_address)
            .map(|opt| opt.map(row_to_affinity))
            .map_err(|e| AffinityStoreError::Database(e.to_string()))
    }

    fn record_affinity(
        &self,
        sender_address: &str,
        sender_domain: &str,
        bundle_category: &str,
        now: DateTime<Utc>,
    ) -> Result<SenderAffinity, AffinityStoreError> {
        let row = SenderAffinityRow {
            sender_domain: sender_domain.to_string(),
            sender_address: sender_address.to_string(),
            bundle_category: bundle_category.to_string(),
            confidence: CONFIDENCE_INCREMENT,
            learned_at: now.timestamp(),
        };
        self.upsert_sender_affinity(&row)
            .map_err(|e| AffinityStoreError::Database(e.to_string()))?;

        // Re-fetch to return the stored state (upsert may have modified confidence)
        self.get_affinity(sender_address)?
            .ok_or_else(|| AffinityStoreError::Database("affinity not found after upsert".to_string()))
    }

    fn list_affinities(&self) -> Result<Vec<SenderAffinity>, AffinityStoreError> {
        // The Store may not have a list_all method — check sender_affinity.rs.
        // If not, we need to add one. For now, use get_affinities_by_domain
        // with a broader query, or add a list_all_sender_affinities method.
        //
        // Check actual available methods on Store and adapt.
        // If Store has no list-all, add: SELECT * FROM sender_affinity
        // to sender_affinity.rs first.
        todo!("Verify Store has a list-all method; if not, add one to sender_affinity.rs")
    }

    fn delete_affinity(&self, sender_address: &str) -> Result<(), AffinityStoreError> {
        // The existing delete_sender_affinity takes (sender_address, bundle_category).
        // The trait takes only sender_address — delete ALL affinities for this sender.
        // May need a new SQL method: DELETE FROM sender_affinity WHERE sender_address = ?
        //
        // Check actual method signature and adapt.
        todo!("Verify delete method signature; may need a new delete-by-address method")
    }
}
```

**Important implementation notes:**
1. Read `sender_affinity.rs` to verify `delete_sender_affinity` signature — if it requires `bundle_category`, add a `delete_sender_affinity_by_address` method that deletes all rows for that address.
2. Read `sender_affinity.rs` to verify if there's a list-all method. If not, add `list_all_sender_affinities(&self) -> Result<Vec<SenderAffinityRow>>` with `SELECT * FROM sender_affinity`.
3. Replace the `todo!()` calls with actual implementations once you've verified the available methods.

- [ ] **Step 4: Add any missing SQL methods to sender_affinity.rs**

If `list_all` or `delete_by_address` don't exist, add them to `inboxly-store/src/sender_affinity.rs`:

```rust
/// List all sender affinities.
pub fn list_all_sender_affinities(&self) -> Result<Vec<SenderAffinityRow>> {
    let mut stmt = self.conn().prepare(
        "SELECT sender_domain, sender_address, bundle_category, confidence, learned_at
         FROM sender_affinity"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(SenderAffinityRow {
            sender_domain: row.get(0)?,
            sender_address: row.get(1)?,
            bundle_category: row.get(2)?,
            confidence: row.get(3)?,
            learned_at: row.get(4)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Delete all affinities for a sender address (regardless of bundle_category).
pub fn delete_sender_affinity_by_address(&self, sender_address: &str) -> Result<()> {
    self.conn().execute(
        "DELETE FROM sender_affinity WHERE sender_address = ?1",
        [sender_address],
    )?;
    Ok(())
}
```

Then update the `AffinityStore` impl to call these methods instead of the `todo!()` stubs.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p inboxly-store --test affinity_store_trait`
Expected: All 5 tests pass.

- [ ] **Step 6: Commit**

```bash
git add inboxly-store/src/trait_impls/affinity_store_impl.rs inboxly-store/src/sender_affinity.rs inboxly-store/tests/affinity_store_trait.rs
git commit -m "feat(store): implement AffinityStore trait for Store

Bridges sender_affinity.rs SQL methods to the AffinityStore trait.
Adds list_all and delete_by_address SQL helpers where needed.
5 integration tests cover record/get/list/delete/upsert."
```

---

## Task 7: Implement BundleStore for Store

**Files:**
- Modify: `inboxly-store/src/trait_impls/bundle_store_impl.rs`
- Create: `inboxly-store/tests/bundle_store_trait.rs`

- [ ] **Step 1: Write failing tests**

Create `inboxly-store/tests/bundle_store_trait.rs`:

```rust
use inboxly_core::store_traits::{BundleStore, BundleStoreError, CreateBundleParams, UpdateBundleParams};
use inboxly_core::bundle::{BundleVisibility, BundleThrottle};  // Verify import path
use inboxly_store::Store;

fn test_store() -> Store {
    Store::open_in_memory().expect("open in-memory store")
}

#[test]
fn create_and_list_bundles() {
    let store = test_store();
    let id = store.create_bundle(CreateBundleParams {
        name: "My Custom Bundle".to_string(),
        color: "#FF5722".to_string(),
        badge_color: "#E64A19".to_string(),
        visibility: BundleVisibility::Visible,
        throttle: BundleThrottle::Immediate,
    }).expect("create bundle");

    let bundles = store.list_bundles().expect("list bundles");
    // May include built-in bundles — find ours by ID
    let found = bundles.iter().find(|b| b.id == id);
    assert!(found.is_some());
    let bundle = found.unwrap();
    assert_eq!(bundle.name, "My Custom Bundle");
    assert!(bundle.is_custom);
}

#[test]
fn update_bundle() {
    let store = test_store();
    let id = store.create_bundle(CreateBundleParams {
        name: "Old Name".to_string(),
        color: "#000000".to_string(),
        badge_color: "#000000".to_string(),
        visibility: BundleVisibility::Visible,
        throttle: BundleThrottle::Immediate,
    }).unwrap();

    store.update_bundle(id, UpdateBundleParams {
        name: Some("New Name".to_string()),
        sort_order: Some(42),
        ..Default::default()
    }).expect("update bundle");

    let bundles = store.list_bundles().unwrap();
    let updated = bundles.iter().find(|b| b.id == id).unwrap();
    assert_eq!(updated.name, "New Name");
    assert_eq!(updated.sort_order, 42);
}

#[test]
fn delete_bundle() {
    let store = test_store();
    let id = store.create_bundle(CreateBundleParams {
        name: "Delete Me".to_string(),
        color: "#000000".to_string(),
        badge_color: "#000000".to_string(),
        visibility: BundleVisibility::Visible,
        throttle: BundleThrottle::Immediate,
    }).unwrap();

    store.delete_bundle(id).expect("delete");
    let bundles = store.list_bundles().unwrap();
    assert!(bundles.iter().all(|b| b.id != id));
}

#[test]
fn delete_nonexistent_bundle_returns_not_found() {
    let store = test_store();
    let result = store.delete_bundle(uuid::Uuid::new_v4());
    assert!(matches!(result, Err(BundleStoreError::NotFound(_))));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p inboxly-store --test bundle_store_trait`
Expected: Compilation error — `BundleStore` not implemented for `Store`.

- [ ] **Step 3: Implement BundleStore for Store**

Write `inboxly-store/src/trait_impls/bundle_store_impl.rs`:

```rust
use inboxly_core::store_traits::{
    BundleInfo, BundleStore, BundleStoreError, CreateBundleParams, UpdateBundleParams,
};
use inboxly_core::bundle::{BundleVisibility, BundleThrottle};
use crate::bundles::BundleRow;
use crate::Store;
use uuid::Uuid;

fn row_to_info(row: BundleRow) -> Result<BundleInfo, BundleStoreError> {
    let id = Uuid::parse_str(&row.id)
        .map_err(|e| BundleStoreError::Database(format!("invalid UUID: {e}")))?;
    let visibility = serde_json::from_str::<BundleVisibility>(&format!("\"{}\"", row.visibility))
        .unwrap_or(BundleVisibility::Visible);
    let throttle = serde_json::from_str::<BundleThrottle>(&format!("\"{}\"", row.throttle))
        .unwrap_or(BundleThrottle::Immediate);

    Ok(BundleInfo {
        id,
        name: row.name,
        category: row.category,
        color: row.color,
        badge_color: row.badge_color,
        visibility,
        throttle,
        is_custom: !row.category.starts_with("system:"),  // Verify convention
        sort_order: row.sort_order,
    })
}

impl BundleStore for Store {
    fn create_bundle(&self, params: CreateBundleParams) -> Result<Uuid, BundleStoreError> {
        let id = Uuid::new_v4();
        let row = BundleRow {
            id: id.to_string(),
            category: format!("custom:{}", params.name.to_lowercase().replace(' ', "_")),
            name: params.name,
            color: params.color,
            badge_color: params.badge_color,
            visibility: serde_json::to_string(&params.visibility)
                .unwrap_or_else(|_| "visible".to_string())
                .trim_matches('"').to_string(),
            throttle: serde_json::to_string(&params.throttle)
                .unwrap_or_else(|_| "immediate".to_string())
                .trim_matches('"').to_string(),
            sort_order: 999,  // Append to end; user can reorder later
        };
        self.insert_bundle(&row)
            .map_err(|e| BundleStoreError::Database(e.to_string()))?;
        Ok(id)
    }

    fn update_bundle(&self, id: Uuid, params: UpdateBundleParams) -> Result<(), BundleStoreError> {
        let mut row = self.get_bundle(&id.to_string())
            .map_err(|e| {
                if e.to_string().contains("no rows") || e.to_string().contains("QueryReturnedNoRows") {
                    BundleStoreError::NotFound(id)
                } else {
                    BundleStoreError::Database(e.to_string())
                }
            })?;

        if let Some(name) = params.name { row.name = name; }
        if let Some(color) = params.color { row.color = color; }
        if let Some(badge_color) = params.badge_color { row.badge_color = badge_color; }
        if let Some(visibility) = params.visibility {
            row.visibility = serde_json::to_string(&visibility)
                .unwrap_or_else(|_| "visible".to_string())
                .trim_matches('"').to_string();
        }
        if let Some(throttle) = params.throttle {
            row.throttle = serde_json::to_string(&throttle)
                .unwrap_or_else(|_| "immediate".to_string())
                .trim_matches('"').to_string();
        }
        if let Some(sort_order) = params.sort_order { row.sort_order = sort_order; }

        self.update_bundle(&row)
            .map_err(|e| BundleStoreError::Database(e.to_string()))
    }

    fn delete_bundle(&self, id: Uuid) -> Result<(), BundleStoreError> {
        // Verify exists first
        self.get_bundle(&id.to_string())
            .map_err(|e| {
                if e.to_string().contains("no rows") || e.to_string().contains("QueryReturnedNoRows") {
                    BundleStoreError::NotFound(id)
                } else {
                    BundleStoreError::Database(e.to_string())
                }
            })?;
        self.delete_bundle_row(&id.to_string())
            .map_err(|e| BundleStoreError::Database(e.to_string()))
    }

    fn list_bundles(&self) -> Result<Vec<BundleInfo>, BundleStoreError> {
        let rows = self.list_bundle_rows()
            .map_err(|e| BundleStoreError::Database(e.to_string()))?;
        rows.into_iter().map(row_to_info).collect()
    }
}
```

**Important:** The Store already has methods named `list_bundles`, `delete_bundle`, `update_bundle` — these will conflict with the trait method names. You may need to rename the existing SQL methods (e.g., `list_bundle_rows`, `delete_bundle_row`, `update_bundle_row`) or use fully-qualified syntax. Read the actual method names in `bundles.rs` and resolve naming conflicts before implementing.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p inboxly-store --test bundle_store_trait`
Expected: All 4 tests pass.

- [ ] **Step 5: Run full workspace tests**

Run: `cargo test --workspace`
Expected: All 841+ tests pass. No regressions from the trait move.

- [ ] **Step 6: Commit**

```bash
git add inboxly-store/src/trait_impls/bundle_store_impl.rs inboxly-store/tests/bundle_store_trait.rs
git commit -m "feat(store): implement BundleStore trait for Store

Bridges bundles.rs SQL methods to the BundleStore trait interface.
Handles BundleRow <-> BundleInfo conversion with visibility/throttle
enum serialization. 4 integration tests cover create/update/delete/not-found."
```

---

## Task 8: Add Archive Folder to WellKnownFolders

**Files:**
- Modify: `inboxly-imap/src/folders.rs`

The `MarkDone` action needs to move emails to the archive folder, which is provider-specific: `[Gmail]/All Mail` for Gmail, `Archive` for Outlook.

- [ ] **Step 1: Write failing test**

Add to the existing folder test file (or create inline test module in `folders.rs`):

```rust
#[test]
fn resolve_gmail_archive_folder() {
    // Gmail's archive is [Gmail]/All Mail
    let folders = vec![
        test_folder("INBOX", vec!["\\Inbox"]),
        test_folder("[Gmail]/All Mail", vec!["\\All"]),
        test_folder("[Gmail]/Sent Mail", vec!["\\Sent"]),
        test_folder("[Gmail]/Trash", vec!["\\Trash"]),
    ];
    let wkf = map_well_known_folders(&folders);
    assert_eq!(wkf.archive.as_deref(), Some("[Gmail]/All Mail"));
}

#[test]
fn resolve_outlook_archive_folder() {
    // Outlook uses a plain "Archive" folder
    let folders = vec![
        test_folder("INBOX", vec!["\\Inbox"]),
        test_folder("Archive", vec!["\\Archive"]),
        test_folder("Sent", vec!["\\Sent"]),
        test_folder("Deleted Items", vec!["\\Trash"]),
    ];
    let wkf = map_well_known_folders(&folders);
    assert_eq!(wkf.archive.as_deref(), Some("Archive"));
}

#[test]
fn resolve_archive_by_name_heuristic() {
    // No SPECIAL-USE attributes — fall back to name matching
    let folders = vec![
        test_folder("INBOX", vec![]),
        test_folder("Archive", vec![]),
        test_folder("Sent", vec![]),
    ];
    let wkf = map_well_known_folders(&folders);
    assert_eq!(wkf.archive.as_deref(), Some("Archive"));
}
```

**Note:** Adjust `test_folder` helper to match the existing test infrastructure in `folders.rs`. Read the file to see how test folders are constructed.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p inboxly-imap folders::tests`
Expected: `archive` field not found on `WellKnownFolders`.

- [ ] **Step 3: Add archive field and resolution**

In `inboxly-imap/src/folders.rs`, add to `WellKnownFolders`:

```rust
pub struct WellKnownFolders {
    pub inbox: Option<String>,
    pub sent: Option<String>,
    pub drafts: Option<String>,
    pub trash: Option<String>,
    pub spam: Option<String>,
    pub archive: Option<String>,  // NEW: [Gmail]/All Mail or Archive
}
```

Add to `resolve_folder_role_by_name()`:

```rust
// Archive heuristic
"archive" | "[gmail]/all mail" | "all mail" => Some(FolderRole::Archive),
```

Add to `parse_special_use_attr()`:

```rust
"\\All" => Some(FolderRole::All),      // Gmail's All Mail
"\\Archive" => Some(FolderRole::Archive),  // Outlook/generic Archive
```

Add to `map_well_known_folders()`: resolve `archive` field from `FolderRole::All` or `FolderRole::Archive` (prefer `\Archive` over `\All` if both present, since `\All` on Gmail includes everything).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p inboxly-imap folders`
Expected: All folder tests pass including the 3 new archive tests.

- [ ] **Step 5: Commit**

```bash
git add inboxly-imap/src/folders.rs
git commit -m "feat(imap): add archive folder to WellKnownFolders

Resolves Gmail's [Gmail]/All Mail (\\All) and Outlook's Archive (\\Archive)
folders. Falls back to name heuristic ('archive', 'all mail'). Needed for
MarkDone action to move emails to the correct provider-specific archive."
```

---

## Task 9: Wire UI Action Handlers to Enqueue Offline Actions

**Files:**
- Modify: `inboxly-ui/src/app.rs`

Currently, handlers like `MarkDone` update local SQLite state but never enqueue an `OfflineAction` for IMAP sync. This task adds the enqueue call to each handler.

- [ ] **Step 1: Read app.rs to understand the current handler patterns**

Read `inboxly-ui/src/app.rs` lines 580-620 (MarkDone, TogglePin handlers) and lines 1150-1210 (MarkReadState, MoveTo, and other stub handlers). Also read how the Store is accessed — `self.store` is `Option<Store>` or similar.

Understand how to get the `account_id`, `folder`, and `imap_uid` for a thread. These are likely available via `Store::get_emails_for_thread(thread_id)` or similar — each email has `account_id`, `imap_folder`, and `imap_uid`.

- [ ] **Step 2: Add enqueue helper method to App**

Add a helper that looks up the email metadata and enqueues an action:

```rust
/// Enqueue an offline action for IMAP sync.
/// Looks up the first email in the thread to get account_id, folder, and imap_uid.
fn enqueue_action(&self, thread_id: &str, make_action: impl FnOnce(String, String, u32) -> OfflineAction) {
    let Some(ref store) = self.store else { return };
    // Get the most recent email in the thread to find its IMAP coordinates
    let emails = match store.get_emails_for_thread(thread_id) {
        Ok(emails) => emails,
        Err(e) => {
            tracing::warn!("failed to get emails for thread {thread_id}: {e}");
            return;
        }
    };
    let Some(email) = emails.first() else {
        tracing::warn!("no emails found for thread {thread_id}");
        return;
    };
    let action = make_action(
        email.account_id.clone(),
        email.imap_folder.clone(),
        email.imap_uid,
    );
    let payload = serde_json::to_string(&action).expect("serialize OfflineAction");
    if let Err(e) = store.enqueue_offline_action(action.variant_name(), &payload) {
        tracing::warn!("failed to enqueue offline action: {e}");
    }
}
```

**Important:** Verify exact method names by reading `app.rs` and the Store API. The helper above assumes `store.get_emails_for_thread()` exists and returns `Vec<EmailRow>` with `account_id`, `imap_folder`, `imap_uid` fields. Read the actual code to confirm.

- [ ] **Step 3: Wire MarkDone handler**

Update the existing `Message::MarkDone` handler (around line 584) to also enqueue:

```rust
Message::MarkDone(thread_id) => {
    if let Some(ref store) = self.store {
        if let Err(e) = store.get_or_create_thread_state(&thread_id) {
            tracing::warn!("failed to ensure thread state: {e}");
        }
        if let Err(e) = store.set_thread_done(&thread_id, true) {
            tracing::warn!("failed to mark done: {e}");
        }
    }
    // NEW: enqueue IMAP action
    self.enqueue_action(&thread_id, |account_id, folder, imap_uid| {
        OfflineAction::MarkDone { account_id, folder, imap_uid }
    });
    self.undo_state.push(UndoAction::MarkDone { thread_id });
    self.reload_feed();
}
```

- [ ] **Step 4: Wire MarkReadState handler**

Replace the stub (around line 1160):

```rust
Message::MarkReadState { thread_id, read } => {
    if let Some(ref store) = self.store {
        // Update local state
        if let Err(e) = store.get_or_create_thread_state(&thread_id) {
            tracing::warn!("failed to ensure thread state: {e}");
        }
        // Mark read state in local DB (verify method name)
        if let Err(e) = store.set_thread_read(&thread_id, read) {
            tracing::warn!("failed to set read state: {e}");
        }
    }
    // Enqueue IMAP action
    if read {
        self.enqueue_action(&thread_id, |account_id, folder, imap_uid| {
            OfflineAction::MarkRead { account_id, folder, imap_uid }
        });
    } else {
        self.enqueue_action(&thread_id, |account_id, folder, imap_uid| {
            OfflineAction::MarkUnread { account_id, folder, imap_uid }
        });
    }
    self.overflow_menu_thread = None;
    self.context_menu_thread = None;
    self.reload_feed();
}
```

**Note:** Verify that `store.set_thread_read()` exists. If not, you may need to update email flags via `store.update_email_flags()` or similar. Read the Store API.

- [ ] **Step 5: Wire MoveTo handler**

Replace the stub (around line 1152):

```rust
Message::MoveTo { thread_id, destination } => {
    // Enqueue the appropriate offline action based on destination
    match destination {
        MoveDestination::Trash => {
            self.enqueue_action(&thread_id, |account_id, folder, imap_uid| {
                OfflineAction::MoveToTrash { account_id, folder, imap_uid }
            });
        }
        MoveDestination::Folder(to_folder) => {
            // Need both from and to folders
            if let Some(ref store) = self.store {
                if let Ok(emails) = store.get_emails_for_thread(&thread_id) {
                    if let Some(email) = emails.first() {
                        let action = OfflineAction::MoveToFolder {
                            account_id: email.account_id.clone(),
                            from_folder: email.imap_folder.clone(),
                            to_folder,
                            imap_uid: email.imap_uid,
                        };
                        let payload = serde_json::to_string(&action).expect("serialize");
                        if let Err(e) = store.enqueue_offline_action(action.variant_name(), &payload) {
                            tracing::warn!("failed to enqueue move: {e}");
                        }
                    }
                }
            }
        }
    }
    self.overflow_menu_thread = None;
    self.context_menu_thread = None;
    self.reload_feed();
}
```

**Note:** Verify that `MoveDestination` enum exists and has these variants. Read the actual enum definition from `app.rs` message types.

- [ ] **Step 6: Wire ReportSpam handler**

Replace the stub:

```rust
Message::ReportSpam(thread_id) => {
    // Move to spam folder via MoveToFolder action
    if let Some(ref store) = self.store {
        if let Ok(emails) = store.get_emails_for_thread(&thread_id) {
            if let Some(email) = emails.first() {
                // Get the spam folder name from well_known_folders
                // or default to "Spam"
                let spam_folder = self.well_known_folders
                    .as_ref()
                    .and_then(|wkf| wkf.spam.clone())
                    .unwrap_or_else(|| "Spam".to_string());
                let action = OfflineAction::MoveToFolder {
                    account_id: email.account_id.clone(),
                    from_folder: email.imap_folder.clone(),
                    to_folder: spam_folder,
                    imap_uid: email.imap_uid,
                };
                let payload = serde_json::to_string(&action).expect("serialize");
                if let Err(e) = store.enqueue_offline_action(action.variant_name(), &payload) {
                    tracing::warn!("failed to enqueue spam report: {e}");
                }
            }
        }
    }
    self.overflow_menu_thread = None;
    self.context_menu_thread = None;
    self.reload_feed();
}
```

- [ ] **Step 7: Verify compilation**

Run: `cargo check -p inboxly-ui`
Expected: Compiles. Resolve any import issues (`use inboxly_core::offline::OfflineAction;`).

- [ ] **Step 8: Commit**

```bash
git add inboxly-ui/src/app.rs
git commit -m "feat(ui): wire action handlers to enqueue offline IMAP actions

MarkDone, MarkReadState, MoveTo, and ReportSpam now enqueue OfflineActions
to the SQLite offline_queue for IMAP sync. TogglePin remains local-only.
Previous handlers either only logged or only updated local DB state."
```

---

## Task 10: Call replay_offline_queue in Sync Loop

**Files:**
- Modify: `inboxly-imap/src/sync_loop.rs`

The `replay_offline_queue()` function exists in `offline_replay.rs` but is never called. This task wires it into the sync loop at two points: (1) on initial sync startup, and (2) on IDLE reconnect.

- [ ] **Step 1: Read sync_loop.rs to identify insertion points**

Read `inboxly-imap/src/sync_loop.rs`:
- Lines 73-137: Phase 1 initial catch-up — replay should happen BEFORE incremental sync
- Lines 297-481: IDLE task — the TODO at line 319 says "On reconnect, drain offline_queue"

- [ ] **Step 2: Add replay call before Phase 1 sync**

In `account_sync_loop()`, add the replay call before the incremental sync loop (around line 73):

```rust
// Replay any queued offline actions before syncing
// This ensures local UI actions (archive, mark read, etc.) are pushed
// to the server before we pull new state.
match offline_replay::replay_offline_queue(&session, &store).await {
    Ok(count) => {
        if count > 0 {
            tracing::info!("replayed {count} offline actions");
            let _ = event_tx.send(SyncEvent::OfflineQueueDrained { count }).await;
        }
    }
    Err(e) => {
        tracing::warn!("offline replay failed (will retry next sync): {e}");
    }
}
```

**Note:** `SyncEvent::OfflineQueueDrained` may not exist yet. If it doesn't, either add it to the `SyncEvent` enum or skip the event emission and just log.

- [ ] **Step 3: Add replay call on IDLE reconnect**

In the IDLE reconnect path (around line 319 where the TODO is), add:

```rust
// Drain offline queue on reconnect (resolves the M19 TODO)
match offline_replay::replay_offline_queue(&session, &store).await {
    Ok(count) => {
        if count > 0 {
            tracing::info!("replayed {count} offline actions on reconnect");
        }
    }
    Err(e) => {
        tracing::warn!("offline replay on reconnect failed: {e}");
    }
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p inboxly-imap`
Expected: Compiles. Verify import: `use crate::offline_replay;`

- [ ] **Step 5: Commit**

```bash
git add inboxly-imap/src/sync_loop.rs
git commit -m "feat(imap): call replay_offline_queue on sync start and IDLE reconnect

Wires the existing offline replay logic into the sync lifecycle:
1. Before Phase 1 incremental sync (push local changes before pulling)
2. On IDLE reconnect (resolves TODO at line 319)
Failed replays are logged and retried on next sync cycle."
```

---

## Task 11: Update MarkDone Replay for Provider-Specific Archive

**Files:**
- Modify: `inboxly-imap/src/offline_replay.rs`

The current `MarkDone` replay does `STORE +FLAGS (\Seen \Deleted)` + `EXPUNGE`. Per the spec, Gmail should `MOVE` to `[Gmail]/All Mail` and Outlook should `MOVE` to `Archive`.

- [ ] **Step 1: Write a test for provider-specific archive behaviour**

Add to existing offline_replay tests (or create a new test module):

```rust
#[test]
fn mark_done_maps_to_archive_move() {
    // Verify that resolve_archive_action returns the correct IMAP commands
    // based on the provider's archive folder
    let gmail_archive = Some("[Gmail]/All Mail".to_string());
    let action = resolve_archive_action("INBOX", 42, &gmail_archive);
    // Should produce: COPY to [Gmail]/All Mail, then STORE \Deleted, EXPUNGE
    // (Gmail doesn't actually need COPY since removing from INBOX leaves it in All Mail,
    // but the generic approach works for all providers)
    assert!(matches!(action, ArchiveStrategy::MoveToFolder { .. }));

    let no_archive = None;
    let action = resolve_archive_action("INBOX", 42, &no_archive);
    // Fallback: STORE \Deleted + EXPUNGE
    assert!(matches!(action, ArchiveStrategy::DeleteAndExpunge));
}
```

- [ ] **Step 2: Add provider-aware archive logic**

In `offline_replay.rs`, modify the `MarkDone` arm of `replay_single_action`:

```rust
OfflineAction::MarkDone { account_id, folder, imap_uid } => {
    let mut guard = session.lock().await;
    guard.select(&folder).await?;

    // If we know the archive folder, MOVE there instead of deleting
    if let Some(ref archive_folder) = well_known.archive {
        // Mark as read first
        guard.uid_store(imap_uid.to_string(), "+FLAGS (\\Seen)").await?;
        // Move to archive (COPY + DELETE + EXPUNGE, or MOVE if supported)
        guard.uid_copy(imap_uid.to_string(), archive_folder).await?;
        guard.uid_store(imap_uid.to_string(), "+FLAGS (\\Deleted)").await?;
        guard.expunge().await?;
    } else {
        // Fallback: mark read + delete + expunge
        guard.uid_store(imap_uid.to_string(), "+FLAGS (\\Seen \\Deleted)").await?;
        guard.expunge().await?;
    }
}
```

**Note:** This requires passing `WellKnownFolders` to `replay_single_action`. Update the function signature:

```rust
async fn replay_single_action<S>(
    session: &Arc<AsyncMutex<Session<S>>>,
    action: &OfflineAction,
    well_known: &WellKnownFolders,  // NEW parameter
) -> Result<(), ImapError>
```

And update `replay_offline_queue` to pass `well_known` through.

- [ ] **Step 3: Update replay_offline_queue signature**

```rust
pub async fn replay_offline_queue<S>(
    session: &Arc<AsyncMutex<Session<S>>>,
    store: &Store,
    well_known: &WellKnownFolders,  // NEW parameter
) -> Result<u64, ImapError>
```

Update callers in `sync_loop.rs` to pass the `well_known` folders.

- [ ] **Step 4: Verify compilation and run tests**

Run: `cargo test -p inboxly-imap`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add inboxly-imap/src/offline_replay.rs inboxly-imap/src/sync_loop.rs
git commit -m "feat(imap): provider-specific archive for MarkDone action

MarkDone now moves to [Gmail]/All Mail or Archive (Outlook) instead of
just deleting. Falls back to STORE \\Deleted + EXPUNGE when no archive
folder is resolved. Passes WellKnownFolders through replay pipeline."
```

---

## Task 12: Delete Aspirational traits.rs from inboxly-core

**Files:**
- Delete: `inboxly-core/src/traits.rs`
- Modify: `inboxly-core/src/lib.rs`

TODOS.md identifies `inboxly-core/src/traits.rs` as aspirational traits that were never implemented. Now that `store_traits/` is the canonical home, remove the old file.

- [ ] **Step 1: Verify traits.rs is not imported anywhere**

Run: `cargo check --workspace` first to confirm current state compiles.

Search for imports of `inboxly_core::traits` or `use crate::traits` across the workspace.

- [ ] **Step 2: Delete traits.rs and remove module declaration**

Delete `inboxly-core/src/traits.rs`.

In `inboxly-core/src/lib.rs`, remove:
```rust
pub mod traits;  // DELETE this line
```

- [ ] **Step 3: Verify workspace still compiles**

Run: `cargo check --workspace`
Expected: Compiles with 0 errors. If anything referenced the old traits, fix the imports to point to `store_traits`.

- [ ] **Step 4: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass (841+ existing + ~19 new from this milestone).

- [ ] **Step 5: Commit**

```bash
git add -A inboxly-core/src/traits.rs inboxly-core/src/lib.rs
git commit -m "refactor(core): remove aspirational traits.rs

Superseded by store_traits/ module which contains the real trait
definitions now implemented by inboxly-store::Store."
```

---

## Task 13: Final Integration Test + Verification

**Files:**
- Create: `inboxly-store/tests/action_enqueue.rs`

End-to-end test that verifies the full action flow: enqueue → serialize → deserialize → verify.

- [ ] **Step 1: Write integration test**

Create `inboxly-store/tests/action_enqueue.rs`:

```rust
use inboxly_core::offline::OfflineAction;
use inboxly_store::Store;

fn test_store() -> Store {
    Store::open_in_memory().expect("open in-memory store")
}

#[test]
fn enqueue_and_dequeue_mark_read() {
    let store = test_store();
    let action = OfflineAction::MarkRead {
        account_id: "acct-001".to_string(),
        folder: "INBOX".to_string(),
        imap_uid: 42,
    };
    let payload = serde_json::to_string(&action).expect("serialize");
    let id = store.enqueue_offline_action(action.variant_name(), &payload).expect("enqueue");

    let queue = store.get_offline_queue().expect("get queue");
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].action, "mark_read");

    let deserialized: OfflineAction = serde_json::from_str(&queue[0].payload_json).expect("deserialize");
    match deserialized {
        OfflineAction::MarkRead { account_id, folder, imap_uid } => {
            assert_eq!(account_id, "acct-001");
            assert_eq!(folder, "INBOX");
            assert_eq!(imap_uid, 42);
        }
        _ => panic!("expected MarkRead variant"),
    }

    store.dequeue_offline_action(id).expect("dequeue");
    assert_eq!(store.count_offline_queue().expect("count"), 0);
}

#[test]
fn enqueue_mark_done_and_move_to_folder() {
    let store = test_store();

    let action1 = OfflineAction::MarkDone {
        account_id: "acct-001".to_string(),
        folder: "INBOX".to_string(),
        imap_uid: 10,
    };
    let action2 = OfflineAction::MoveToFolder {
        account_id: "acct-001".to_string(),
        from_folder: "INBOX".to_string(),
        to_folder: "Archive".to_string(),
        imap_uid: 20,
    };

    let payload1 = serde_json::to_string(&action1).expect("serialize");
    let payload2 = serde_json::to_string(&action2).expect("serialize");
    store.enqueue_offline_action(action1.variant_name(), &payload1).unwrap();
    store.enqueue_offline_action(action2.variant_name(), &payload2).unwrap();

    let queue = store.get_offline_queue().unwrap();
    assert_eq!(queue.len(), 2);
    // FIFO order
    assert_eq!(queue[0].action, "mark_done");
    assert_eq!(queue[1].action, "move_to_folder");
}

#[test]
fn all_offline_action_variants_serialize_roundtrip() {
    let actions = vec![
        OfflineAction::MarkRead { account_id: "a".into(), folder: "f".into(), imap_uid: 1 },
        OfflineAction::MarkUnread { account_id: "a".into(), folder: "f".into(), imap_uid: 2 },
        OfflineAction::Star { account_id: "a".into(), folder: "f".into(), imap_uid: 3 },
        OfflineAction::Unstar { account_id: "a".into(), folder: "f".into(), imap_uid: 4 },
        OfflineAction::MarkDone { account_id: "a".into(), folder: "f".into(), imap_uid: 5 },
        OfflineAction::MoveToTrash { account_id: "a".into(), folder: "f".into(), imap_uid: 6 },
        OfflineAction::MoveToFolder {
            account_id: "a".into(), from_folder: "f".into(), to_folder: "t".into(), imap_uid: 7,
        },
        OfflineAction::MarkAnswered { account_id: "a".into(), folder: "f".into(), imap_uid: 8 },
        OfflineAction::SendDraft { account_id: "a".into(), draft_maildir_path: "/tmp/draft".into() },
    ];

    let store = test_store();
    for action in &actions {
        let payload = serde_json::to_string(action).expect("serialize");
        store.enqueue_offline_action(action.variant_name(), &payload).expect("enqueue");
    }

    let queue = store.get_offline_queue().unwrap();
    assert_eq!(queue.len(), 9);

    for (row, original) in queue.iter().zip(actions.iter()) {
        let deserialized: OfflineAction = serde_json::from_str(&row.payload_json).expect("deserialize");
        assert_eq!(
            serde_json::to_string(&deserialized).unwrap(),
            serde_json::to_string(original).unwrap(),
        );
    }
}
```

- [ ] **Step 2: Run all tests**

Run: `cargo test --workspace`
Expected: All tests pass. Count should be ~860+ (841 existing + ~19 new).

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: 0 warnings.

- [ ] **Step 4: Commit**

```bash
git add inboxly-store/tests/action_enqueue.rs
git commit -m "test(store): add offline action enqueue integration tests

Verifies serialize/deserialize roundtrip for all 9 OfflineAction variants,
FIFO queue ordering, and enqueue/dequeue lifecycle."
```

- [ ] **Step 5: Final commit — bump version**

Update version in root `Cargo.toml`:

```toml
[workspace.package]
version = "0.31.0"
```

```bash
git add Cargo.toml
git commit -m "chore: bump version to v0.31.0

M31: Store trait integration + IMAP action execution.
- RuleStore, AffinityStore, BundleStore traits moved to inboxly-core
- All three traits implemented on inboxly-store::Store
- UI action handlers enqueue OfflineActions for IMAP sync
- replay_offline_queue called on sync start and IDLE reconnect
- Provider-specific archive folder (Gmail All Mail / Outlook Archive)
- Aspirational traits.rs deleted
- ~19 new tests"
```
