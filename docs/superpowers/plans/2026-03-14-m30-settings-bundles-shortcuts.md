# M30: Settings — Bundles + Notifications + Shortcuts — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Implement the three remaining settings tabs — Bundles (reorder, throttle, visibility), Notifications (toggles + per-bundle selection), and Keyboard Shortcuts (runtime-remappable bindings replacing the compile-time `Shortcuts` struct) — completing the full settings view.

**Architecture:** The Bundles tab renders a reorderable list of `BundleRow` entries from the store, with inline throttle editing via `PopupMenu` and visibility toggles. All mutations go through `Store::update_bundle()`. The Notifications tab reads/writes three settings keys via `Store::get_setting()`/`set_setting()`. The Keyboard Shortcuts tab introduces `ShortcutAction` (enum) and `ShortcutMap` (HashMap-backed, serialised to JSON), replacing the compile-time `Shortcuts` struct. Customisations persist to the `shortcuts` settings key; only non-default bindings are stored.

**Tech Stack:** Rust, iced 0.14 (`advanced` feature), serde_json, inboxly-store (settings + bundles APIs), inboxly-core (BundleThrottle, BundleCategory), inboxly-bundler (system_bundles)

**Prerequisites:**
- M29 complete — Settings view framework exists: `ActiveView::Settings` variant, `SettingsTab` enum (General, Accounts, Bundles, Notifications, Shortcuts, DataStorage), settings sidebar with tab navigation, content area scaffold, back-arrow toolbar. General, Accounts, and Data & Storage tabs are implemented.
- M26 complete — `PopupMenu` widget available in `inboxly-ui::widgets` for throttle dropdown.
- M3 complete — SQLite `settings` table with `Store::get_setting()`/`set_setting()` API.
- M14 complete — `BundleThrottle` enum with serde JSON roundtrip, `BundleRow` CRUD in store.

---

> **Codebase Reality Check (pre-implementation gap analysis):**
>
> 1. **`Shortcuts` struct** (`inboxly-ui/src/keyboard.rs`) uses `&'static str` constants. It is referenced **only** in its own test module — no other file imports `Shortcuts::*`. This makes the migration safe: replace the struct, update the one test file, done.
>
> 2. **`BundleRow.throttle`** is stored as a raw JSON `String` in SQLite. The `BundleThrottle` enum in `inboxly-core/src/throttle.rs` has full serde support (`#[serde(tag = "mode")]`). Parsing/serialising between the two is `serde_json::from_str`/`to_string`.
>
> 3. **`BundleRow.visibility`** is a `String` column. Current values are `"Bundled"` (set by `ensure_system_bundles`). The spec uses a toggle (visible/hidden in nav drawer). We will use `"visible"` and `"hidden"` as canonical values.
>
> 4. **`Store::update_bundle()`** updates the full row. For reorder, we must call it N times (once per row with new `sort_order`). A dedicated `update_bundle_sort_orders()` batch method would be more efficient but is optional — start simple, optimise if profiling shows issues.
>
> 5. **`ActiveView`** enum currently has `Inbox`, `Snoozed`, `Done`. M29 should add `Settings`. If M29 is not yet implemented when this plan executes, the implementer must verify the Settings variant exists and adjust accordingly.
>
> 6. **No `PopupMenu` in codebase yet** (M26). If M26 is not complete, throttle editing must use a simpler approach (e.g., a button that cycles through modes) and be retrofitted when `PopupMenu` lands. The plan assumes M26 is done.

## Task 1: Add `ShortcutAction` enum and `ShortcutMap` to `inboxly-ui`

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/keyboard.rs` (rewrite)

Replace the existing `Shortcuts` struct with a runtime-configurable shortcut system. The new module defines a `ShortcutAction` enum (one variant per bindable action) and a `ShortcutMap` that wraps `HashMap<ShortcutAction, String>` with defaults, serialisation, and lookup.

```rust
//! Runtime keyboard shortcut bindings.
//!
//! Replaces the compile-time `Shortcuts` struct with a `ShortcutMap` backed
//! by `HashMap<ShortcutAction, String>`. Custom bindings persist as JSON in
//! the SQLite settings table (key: `shortcuts`). Missing keys fall back to
//! defaults matching the original Google Inbox keybindings.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Every bindable keyboard action in the application.
///
/// Variants are serialised as lowercase snake_case via serde rename.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutAction {
    /// Archive / mark done.
    Done,
    /// Pin/unpin toggle.
    Pin,
    /// Open snooze picker.
    Snooze,
    /// Focus search bar.
    Search,
    /// Open compose view.
    Compose,
    /// Refresh / manual sync.
    Refresh,
    /// Select next thread.
    Next,
    /// Select previous thread.
    Previous,
    /// Open selected thread.
    Open,
    /// Undo last action.
    Undo,
    /// Navigate to Inbox view.
    GoInbox,
    /// Navigate to Snoozed view.
    GoSnoozed,
    /// Navigate to Done view.
    GoDone,
    /// Reply to thread.
    Reply,
    /// Reply all.
    ReplyAll,
    /// Forward thread.
    Forward,
    /// Show shortcut help overlay.
    Help,
    /// Close menu / go back.
    Escape,
}

impl ShortcutAction {
    /// All action variants in display order.
    pub const ALL: &'static [Self] = &[
        Self::Done,
        Self::Pin,
        Self::Snooze,
        Self::Search,
        Self::Compose,
        Self::Refresh,
        Self::Next,
        Self::Previous,
        Self::Open,
        Self::Undo,
        Self::GoInbox,
        Self::GoSnoozed,
        Self::GoDone,
        Self::Reply,
        Self::ReplyAll,
        Self::Forward,
        Self::Help,
        Self::Escape,
    ];

    /// Human-readable label for the settings table.
    pub fn label(self) -> &'static str {
        match self {
            Self::Done => "Done (archive)",
            Self::Pin => "Pin / unpin",
            Self::Snooze => "Snooze",
            Self::Search => "Search",
            Self::Compose => "Compose",
            Self::Refresh => "Refresh",
            Self::Next => "Next thread",
            Self::Previous => "Previous thread",
            Self::Open => "Open thread",
            Self::Undo => "Undo",
            Self::GoInbox => "Go to Inbox",
            Self::GoSnoozed => "Go to Snoozed",
            Self::GoDone => "Go to Done",
            Self::Reply => "Reply",
            Self::ReplyAll => "Reply All",
            Self::Forward => "Forward",
            Self::Help => "Shortcut help",
            Self::Escape => "Close / back",
        }
    }
}

/// Runtime-configurable shortcut map.
///
/// Wraps a `HashMap<ShortcutAction, String>` where values are human-readable
/// key descriptions (e.g. `"e"`, `"Ctrl+Z"`, `"g i"`, `"Shift+R"`).
///
/// Only non-default overrides are serialised to JSON. On load, missing keys
/// fall back to [`Self::defaults()`].
#[derive(Debug, Clone)]
pub struct ShortcutMap {
    bindings: HashMap<ShortcutAction, String>,
}

impl ShortcutMap {
    /// Construct a map with all default bindings.
    ///
    /// These match the original `Shortcuts` constants plus new additions
    /// from the QoL spec.
    #[must_use]
    pub fn defaults() -> Self {
        let mut bindings = HashMap::with_capacity(ShortcutAction::ALL.len());
        bindings.insert(ShortcutAction::Done, "e".to_string());
        bindings.insert(ShortcutAction::Pin, "=".to_string());
        bindings.insert(ShortcutAction::Snooze, "b".to_string());
        bindings.insert(ShortcutAction::Search, "/".to_string());
        bindings.insert(ShortcutAction::Compose, "c".to_string());
        bindings.insert(ShortcutAction::Refresh, "r".to_string());
        bindings.insert(ShortcutAction::Next, "j".to_string());
        bindings.insert(ShortcutAction::Previous, "k".to_string());
        bindings.insert(ShortcutAction::Open, "o".to_string());
        bindings.insert(ShortcutAction::Undo, "Ctrl+Z".to_string());
        bindings.insert(ShortcutAction::GoInbox, "g i".to_string());
        bindings.insert(ShortcutAction::GoSnoozed, "g s".to_string());
        bindings.insert(ShortcutAction::GoDone, "g d".to_string());
        bindings.insert(ShortcutAction::Reply, "Shift+R".to_string());
        bindings.insert(ShortcutAction::ReplyAll, "Shift+A".to_string());
        bindings.insert(ShortcutAction::Forward, "f".to_string());
        bindings.insert(ShortcutAction::Help, "?".to_string());
        bindings.insert(ShortcutAction::Escape, "Esc".to_string());
        Self { bindings }
    }

    /// Get the binding for an action. Always returns a value (defaults
    /// are populated at construction time).
    #[must_use]
    pub fn get(&self, action: ShortcutAction) -> &str {
        self.bindings
            .get(&action)
            .map(String::as_str)
            .unwrap_or("")
    }

    /// Set a custom binding for an action.
    pub fn set(&mut self, action: ShortcutAction, binding: String) {
        self.bindings.insert(action, binding);
    }

    /// Reset an action to its default binding.
    pub fn reset(&mut self, action: ShortcutAction) {
        let defaults = Self::defaults();
        if let Some(default_val) = defaults.bindings.get(&action) {
            self.bindings.insert(action, default_val.clone());
        }
    }

    /// Check if the current binding for an action differs from the default.
    #[must_use]
    pub fn is_customised(&self, action: ShortcutAction) -> bool {
        let defaults = Self::defaults();
        self.bindings.get(&action) != defaults.bindings.get(&action)
    }

    /// Serialise only non-default overrides to JSON for settings storage.
    ///
    /// # Errors
    ///
    /// Returns `serde_json::Error` if serialisation fails (should not happen
    /// with string values).
    pub fn to_overrides_json(&self) -> Result<String, serde_json::Error> {
        let defaults = Self::defaults();
        let overrides: HashMap<ShortcutAction, &String> = self
            .bindings
            .iter()
            .filter(|(action, binding)| {
                defaults
                    .bindings
                    .get(action)
                    .map_or(true, |default| default != *binding)
            })
            .map(|(action, binding)| (*action, binding))
            .collect();
        serde_json::to_string(&overrides)
    }

    /// Load from a JSON overrides string, merging over defaults.
    ///
    /// Invalid JSON or unknown keys are silently ignored (defaults used).
    #[must_use]
    pub fn from_overrides_json(json: &str) -> Self {
        let mut map = Self::defaults();
        if let Ok(overrides) = serde_json::from_str::<HashMap<ShortcutAction, String>>(json) {
            for (action, binding) in overrides {
                map.bindings.insert(action, binding);
            }
        }
        map
    }

    /// Look up which action (if any) is bound to the given key string.
    #[must_use]
    pub fn action_for_key(&self, key: &str) -> Option<ShortcutAction> {
        self.bindings
            .iter()
            .find(|(_, binding)| binding.as_str() == key)
            .map(|(action, _)| *action)
    }

    /// Returns an iterator over all (action, binding) pairs in display order.
    pub fn iter_display_order(&self) -> impl Iterator<Item = (ShortcutAction, &str)> {
        ShortcutAction::ALL
            .iter()
            .map(move |action| (*action, self.get(*action)))
    }
}

impl Default for ShortcutMap {
    fn default() -> Self {
        Self::defaults()
    }
}
```

**Delete** the old `Shortcuts` struct and its tests entirely. The new tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_cover_all_actions() {
        let map = ShortcutMap::defaults();
        for action in ShortcutAction::ALL {
            assert!(
                !map.get(*action).is_empty(),
                "action {action:?} has no default binding"
            );
        }
    }

    #[test]
    fn get_returns_correct_default() {
        let map = ShortcutMap::defaults();
        assert_eq!(map.get(ShortcutAction::Done), "e");
        assert_eq!(map.get(ShortcutAction::Undo), "Ctrl+Z");
        assert_eq!(map.get(ShortcutAction::GoInbox), "g i");
        assert_eq!(map.get(ShortcutAction::Reply), "Shift+R");
    }

    #[test]
    fn set_overrides_default() {
        let mut map = ShortcutMap::defaults();
        map.set(ShortcutAction::Done, "d".to_string());
        assert_eq!(map.get(ShortcutAction::Done), "d");
        assert!(map.is_customised(ShortcutAction::Done));
    }

    #[test]
    fn reset_restores_default() {
        let mut map = ShortcutMap::defaults();
        map.set(ShortcutAction::Done, "d".to_string());
        map.reset(ShortcutAction::Done);
        assert_eq!(map.get(ShortcutAction::Done), "e");
        assert!(!map.is_customised(ShortcutAction::Done));
    }

    #[test]
    fn overrides_json_only_stores_changes() {
        let mut map = ShortcutMap::defaults();
        map.set(ShortcutAction::Done, "d".to_string());
        let json = map.to_overrides_json().expect("serialise");
        assert!(json.contains("\"d\""));
        // Should NOT contain default bindings.
        assert!(!json.contains("\"Ctrl+Z\""));
    }

    #[test]
    fn from_overrides_json_merges_over_defaults() {
        let json = r#"{"done":"d","pin":"p"}"#;
        let map = ShortcutMap::from_overrides_json(json);
        assert_eq!(map.get(ShortcutAction::Done), "d");
        assert_eq!(map.get(ShortcutAction::Pin), "p");
        // Non-overridden actions keep defaults.
        assert_eq!(map.get(ShortcutAction::Undo), "Ctrl+Z");
    }

    #[test]
    fn from_overrides_json_handles_invalid_json() {
        let map = ShortcutMap::from_overrides_json("not json");
        assert_eq!(map.get(ShortcutAction::Done), "e");
    }

    #[test]
    fn from_overrides_json_handles_empty_string() {
        let map = ShortcutMap::from_overrides_json("");
        assert_eq!(map.get(ShortcutAction::Done), "e");
    }

    #[test]
    fn action_for_key_finds_binding() {
        let map = ShortcutMap::defaults();
        assert_eq!(map.action_for_key("e"), Some(ShortcutAction::Done));
        assert_eq!(map.action_for_key("Ctrl+Z"), Some(ShortcutAction::Undo));
        assert_eq!(map.action_for_key("x"), None);
    }

    #[test]
    fn iter_display_order_covers_all_actions() {
        let map = ShortcutMap::defaults();
        let items: Vec<_> = map.iter_display_order().collect();
        assert_eq!(items.len(), ShortcutAction::ALL.len());
    }

    #[test]
    fn serde_roundtrip_action() {
        let json = serde_json::to_string(&ShortcutAction::GoInbox).expect("serialise");
        assert_eq!(json, "\"go_inbox\"");
        let decoded: ShortcutAction = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(decoded, ShortcutAction::GoInbox);
    }

    #[test]
    fn default_impl_matches_defaults() {
        let d1 = ShortcutMap::default();
        let d2 = ShortcutMap::defaults();
        for action in ShortcutAction::ALL {
            assert_eq!(d1.get(*action), d2.get(*action));
        }
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- keyboard && cargo clippy -p inboxly-ui -- -D warnings
```

**Commit:** `feat(ui): replace Shortcuts struct with runtime ShortcutMap`

---

## Task 2: Add `ShortcutMap` to `Inboxly` app state

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs`

Wire `ShortcutMap` into the application state so it is available for the settings tab UI and for event handling.

**Changes:**

1. Add `use crate::keyboard::{ShortcutAction, ShortcutMap};` to imports.

2. Add field to `Inboxly`:

```rust
/// Runtime keyboard shortcut bindings (customisable via Settings).
pub shortcuts: ShortcutMap,
```

3. In `Default for Inboxly`, initialise:

```rust
shortcuts: ShortcutMap::defaults(),
```

4. Add new `Message` variants:

```rust
/// Load shortcuts from store on startup.
ShortcutsLoaded(ShortcutMap),
/// User changed a shortcut binding in settings.
SetShortcut { action: ShortcutAction, binding: String },
/// User reset a shortcut to its default.
ResetShortcut(ShortcutAction),
```

5. Handle in `update()`:

```rust
Message::ShortcutsLoaded(map) => {
    self.shortcuts = map;
}
Message::SetShortcut { action, binding } => {
    self.shortcuts.set(action, binding);
    // Persist overrides to settings store.
    if let Some(ref store) = self.store {
        match self.shortcuts.to_overrides_json() {
            Ok(json) => {
                if let Err(e) = store.set_setting("shortcuts", &json) {
                    tracing::warn!("failed to persist shortcut overrides: {e}");
                }
            }
            Err(e) => tracing::warn!("failed to serialise shortcut overrides: {e}"),
        }
    }
}
Message::ResetShortcut(action) => {
    self.shortcuts.reset(action);
    if let Some(ref store) = self.store {
        match self.shortcuts.to_overrides_json() {
            Ok(json) => {
                if let Err(e) = store.set_setting("shortcuts", &json) {
                    tracing::warn!("failed to persist shortcut reset: {e}");
                }
            }
            Err(e) => tracing::warn!("failed to serialise shortcut overrides: {e}"),
        }
    }
}
```

**Note:** The startup path (where the store is connected) should load shortcuts via:

```rust
let shortcuts = match store.get_setting("shortcuts") {
    Ok(Some(json)) => ShortcutMap::from_overrides_json(&json),
    _ => ShortcutMap::defaults(),
};
```

This may already exist in an init/connect handler from M29. If not, add it wherever the store is first connected.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui && cargo test -p inboxly-ui
```

**Commit:** `feat(ui): wire ShortcutMap into app state with persistence`

---

## Task 3: Add notification settings keys to store and app state

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs`

Add notification-related state and messages to the app. The store already has generic `get_setting`/`set_setting` — we use three keys: `notifications_enabled`, `notification_sound`, `notification_bundles`.

**Changes to `Inboxly`:**

```rust
/// Whether desktop notifications are enabled.
pub notifications_enabled: bool,
/// Whether notification sound is enabled.
pub notification_sound: bool,
/// Which bundles trigger notifications. `["all"]` means all bundles.
/// Otherwise a vec of category names (e.g. `["Social", "Finance"]`).
pub notification_bundles: Vec<String>,
```

**Defaults (in `Default for Inboxly`):**

```rust
notifications_enabled: true,
notification_sound: true,
notification_bundles: vec!["all".to_string()],
```

**New `Message` variants:**

```rust
/// Toggle desktop notifications on/off.
ToggleNotifications,
/// Toggle notification sound on/off.
ToggleNotificationSound,
/// Set which bundles send notifications.
SetNotificationBundles(Vec<String>),
/// Notification settings loaded from store.
NotificationSettingsLoaded {
    enabled: bool,
    sound: bool,
    bundles: Vec<String>,
},
```

**Handler logic (in `update()`):**

```rust
Message::ToggleNotifications => {
    self.notifications_enabled = !self.notifications_enabled;
    if let Some(ref store) = self.store {
        let val = if self.notifications_enabled { "true" } else { "false" };
        if let Err(e) = store.set_setting("notifications_enabled", val) {
            tracing::warn!("failed to persist notifications_enabled: {e}");
        }
    }
}
Message::ToggleNotificationSound => {
    self.notification_sound = !self.notification_sound;
    if let Some(ref store) = self.store {
        let val = if self.notification_sound { "true" } else { "false" };
        if let Err(e) = store.set_setting("notification_sound", val) {
            tracing::warn!("failed to persist notification_sound: {e}");
        }
    }
}
Message::SetNotificationBundles(bundles) => {
    self.notification_bundles = bundles;
    if let Some(ref store) = self.store {
        match serde_json::to_string(&self.notification_bundles) {
            Ok(json) => {
                if let Err(e) = store.set_setting("notification_bundles", &json) {
                    tracing::warn!("failed to persist notification_bundles: {e}");
                }
            }
            Err(e) => tracing::warn!("failed to serialise notification_bundles: {e}"),
        }
    }
}
Message::NotificationSettingsLoaded { enabled, sound, bundles } => {
    self.notifications_enabled = enabled;
    self.notification_sound = sound;
    self.notification_bundles = bundles;
}
```

**Startup load** (wherever store is connected):

```rust
let notifications_enabled = store
    .get_setting("notifications_enabled")
    .ok()
    .flatten()
    .map_or(true, |v| v == "true");

let notification_sound = store
    .get_setting("notification_sound")
    .ok()
    .flatten()
    .map_or(true, |v| v == "true");

let notification_bundles = store
    .get_setting("notification_bundles")
    .ok()
    .flatten()
    .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
    .unwrap_or_else(|| vec!["all".to_string()]);
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui && cargo test -p inboxly-ui
```

**Commit:** `feat(ui): add notification settings state and persistence`

---

## Task 4: Add bundle settings state and messages

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs`

Add bundle-specific settings state. The bundles list is loaded from `Store::list_bundles()` at startup. Mutations update both the in-memory list and the store.

**Add to `Inboxly`:**

```rust
/// Bundle settings rows (loaded from store, used by Bundles settings tab).
pub settings_bundles: Vec<inboxly_store::BundleRow>,
```

**Default:**

```rust
settings_bundles: Vec::new(),
```

**New `Message` variants:**

```rust
/// Bundle list loaded from store.
BundlesLoaded(Vec<inboxly_store::BundleRow>),
/// User toggled a bundle's visibility.
ToggleBundleVisibility(String),  // bundle ID
/// User changed a bundle's throttle.
SetBundleThrottle { bundle_id: String, throttle_json: String },
/// User reordered bundles (Vec of bundle IDs in new order).
ReorderBundles(Vec<String>),
```

**Handler logic:**

```rust
Message::BundlesLoaded(bundles) => {
    self.settings_bundles = bundles;
}
Message::ToggleBundleVisibility(bundle_id) => {
    if let Some(bundle) = self.settings_bundles.iter_mut().find(|b| b.id == bundle_id) {
        bundle.visibility = if bundle.visibility == "visible" {
            "hidden".to_string()
        } else {
            "visible".to_string()
        };
        if let Some(ref store) = self.store {
            if let Err(e) = store.update_bundle(bundle) {
                tracing::warn!("failed to update bundle visibility: {e}");
            }
        }
    }
}
Message::SetBundleThrottle { bundle_id, throttle_json } => {
    if let Some(bundle) = self.settings_bundles.iter_mut().find(|b| b.id == bundle_id) {
        bundle.throttle = throttle_json;
        if let Some(ref store) = self.store {
            if let Err(e) = store.update_bundle(bundle) {
                tracing::warn!("failed to update bundle throttle: {e}");
            }
        }
    }
}
Message::ReorderBundles(ordered_ids) => {
    // Reassign sort_order based on position in the new order.
    for (i, id) in ordered_ids.iter().enumerate() {
        if let Some(bundle) = self.settings_bundles.iter_mut().find(|b| &b.id == id) {
            bundle.sort_order = i as i64;
        }
    }
    // Re-sort the in-memory vec.
    self.settings_bundles.sort_by_key(|b| b.sort_order);
    // Persist all sort_orders.
    if let Some(ref store) = self.store {
        for bundle in &self.settings_bundles {
            if let Err(e) = store.update_bundle(bundle) {
                tracing::warn!("failed to persist bundle sort order: {e}");
            }
        }
    }
}
```

**Startup load:**

```rust
let settings_bundles = store.list_bundles().unwrap_or_default();
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui && cargo test -p inboxly-ui
```

**Commit:** `feat(ui): add bundle settings state and reorder/throttle messages`

---

## Task 5: Implement Bundles settings tab view

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/settings_bundles.rs` (new file)

Render the Bundles tab content. This is a scrollable list of bundle rows, each showing: drag handle, category icon (coloured circle), name, throttle badge, and visibility toggle.

**Key design points:**

1. **Drag reorder** — Iced 0.14 does not have a built-in drag-and-drop list widget. Use up/down arrow buttons (`▲`/`▼`) as a pragmatic alternative. Each row gets move-up and move-down buttons that swap `sort_order` with the adjacent row and emit `Message::ReorderBundles`.

2. **Throttle badge** — A coloured pill button. Clicking it emits a message that opens a `PopupMenu` (from M26) with throttle options. The pill colour encodes the current mode:
   - Green `#0f9d58` for Immediate
   - Orange `#ef6c00` for Daily
   - Blue `#4285f4` for Weekly

3. **Visibility toggle** — A simple toggle/checkbox. `"visible"` shows in nav drawer, `"hidden"` does not.

```rust
//! Bundles settings tab — reorder, throttle, and visibility.

use iced::widget::{button, checkbox, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

use inboxly_store::BundleRow;

use crate::app::Message;
use crate::theme::InboxlyTheme;

/// Render the Bundles settings tab content.
///
/// Each bundle row shows: reorder arrows, icon (coloured dot), name,
/// throttle badge (pill), and visibility toggle.
pub fn bundles_settings_tab<'a>(
    bundles: &'a [BundleRow],
    theme: &'a InboxlyTheme,
) -> Element<'a, Message> {
    // ... column of bundle_row() calls for each BundleRow
}

/// Render a single bundle row.
fn bundle_row<'a>(
    bundle: &'a BundleRow,
    index: usize,
    total: usize,
    theme: &'a InboxlyTheme,
) -> Element<'a, Message> {
    // Row: [▲ ▼] [● colour dot] [Name text] [Throttle pill button] [Visibility toggle]
}

/// Format the throttle value as a short label for the pill badge.
///
/// Parses the JSON throttle string and returns e.g. "Immediate",
/// "Daily @ 5:00 PM", "Weekly Mon @ 8:00 AM".
fn throttle_label(throttle_json: &str) -> String {
    // Parse BundleThrottle from JSON, format as display string.
    // Falls back to "Immediate" on parse error.
}

/// Determine the pill colour for a throttle mode.
fn throttle_pill_color(throttle_json: &str) -> iced::Color {
    // Green for Immediate, Orange for Daily, Blue for Weekly.
}
```

**Register in views:**

Add `pub mod settings_bundles;` to `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/mod.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement bundles settings tab view`

---

## Task 6: Implement throttle editor popup

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/settings_bundles.rs` (extend)

**Also modifies:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs`

Add state and messages for the throttle editor. When the user clicks a throttle badge, a `PopupMenu` appears with three options: Immediate, Daily (with submenu for time), Weekly (with submenu for day + time).

**New app state in `Inboxly`:**

```rust
/// Which bundle's throttle popup is currently open (None = all closed).
pub throttle_popup_bundle_id: Option<String>,
```

**New `Message` variants:**

```rust
/// Open/close the throttle popup for a bundle.
ToggleThrottlePopup(Option<String>),
```

**Throttle menu structure (using PopupMenu from M26):**

```
PopupMenu
├── Action: "Immediate" → SetBundleThrottle { ..., throttle_json: immediate_json }
├── Submenu: "Daily"
│   ├── Action: "6:00 AM" → SetBundleThrottle { ..., throttle_json: daily_6am }
│   ├── Action: "9:00 AM" → SetBundleThrottle { ..., throttle_json: daily_9am }
│   ├── Action: "12:00 PM" → SetBundleThrottle { ..., throttle_json: daily_noon }
│   ├── Action: "5:00 PM" → SetBundleThrottle { ..., throttle_json: daily_5pm }
│   └── Action: "9:00 PM" → SetBundleThrottle { ..., throttle_json: daily_9pm }
└── Submenu: "Weekly"
    ├── Action: "Monday 8 AM" → SetBundleThrottle { ..., throttle_json: weekly_mon_8am }
    ├── Action: "Tuesday 8 AM" → ...
    ├── ...
    └── Action: "Sunday 8 AM" → ...
```

**Generating throttle JSON values:**

```rust
use inboxly_core::throttle::{BundleThrottle, WeekdayWrapper};
use chrono::{NaiveTime, Weekday};

fn immediate_json() -> String {
    serde_json::to_string(&BundleThrottle::Immediate).expect("serialise immediate")
}

fn daily_json(hour: u32, minute: u32) -> String {
    let throttle = BundleThrottle::Daily {
        delivery_time: NaiveTime::from_hms_opt(hour, minute, 0).expect("valid time"),
    };
    serde_json::to_string(&throttle).expect("serialise daily")
}

fn weekly_json(day: Weekday, hour: u32, minute: u32) -> String {
    let throttle = BundleThrottle::Weekly {
        delivery_day: WeekdayWrapper(day),
        delivery_time: NaiveTime::from_hms_opt(hour, minute, 0).expect("valid time"),
    };
    serde_json::to_string(&throttle).expect("serialise weekly")
}
```

**Important dependency note:** If `PopupMenu` (M26) is not yet available, implement the throttle editor as a simple cycling button that rotates through `Immediate → Daily 9 AM → Weekly Monday 8 AM → Immediate`. Add a `// TODO: Replace with PopupMenu when M26 lands` comment. The message flow (`SetBundleThrottle`) stays the same either way.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui && cargo test -p inboxly-ui
```

**Commit:** `feat(ui): add throttle editor popup for bundle settings`

---

## Task 7: Implement Notifications settings tab view

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/settings_notifications.rs` (new file)

Render the Notifications tab with three controls:

1. **Desktop notifications** — A labelled toggle (checkbox). Bound to `notifications_enabled`.
2. **Sound** — A labelled toggle (checkbox). Bound to `notification_sound`. Greyed out when notifications are disabled.
3. **Notify for** — A set of checkboxes:
   - "All mail" (exclusive — if checked, unchecks individual bundles)
   - "Primary only" (exclusive with "All")
   - One checkbox per system bundle category (Social, Promos, Updates, Finance, Purchases, Travel, Forums, Low Priority)

The "Notify for" state maps to the `notification_bundles` vec:
- `["all"]` → "All mail" checked
- `["primary"]` → "Primary only" checked
- `["Social", "Finance"]` → those specific checkboxes checked

```rust
//! Notifications settings tab.

use iced::widget::{checkbox, column, container, row, text, Space};
use iced::{Element, Length};

use crate::app::Message;
use crate::theme::InboxlyTheme;

/// System bundle category names for the per-bundle notification checkboxes.
const BUNDLE_CATEGORIES: &[&str] = &[
    "Social", "Promos", "Updates", "Finance",
    "Purchases", "Travel", "Forums", "LowPriority",
];

/// Render the Notifications settings tab content.
pub fn notifications_settings_tab<'a>(
    notifications_enabled: bool,
    notification_sound: bool,
    notification_bundles: &'a [String],
    theme: &'a InboxlyTheme,
) -> Element<'a, Message> {
    // Section: Desktop Notifications toggle
    // Section: Sound toggle (disabled when notifications off)
    // Section: "Notify for" checkboxes
}
```

**Register in views:**

Add `pub mod settings_notifications;` to `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/mod.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement notifications settings tab view`

---

## Task 8: Implement Keyboard Shortcuts settings tab view

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/settings_shortcuts.rs` (new file)

Render the Shortcuts tab as a two-column table: Action (left) and Shortcut (right).

**Table layout:**

- Header row: "Action" | "Shortcut" (bold, `text_secondary` colour)
- One row per `ShortcutAction` (via `ShortcutMap::iter_display_order()`)
- Action column: `action.label()` text
- Shortcut column: current binding in a clickable `button` with a keyboard-style visual (rounded rect, monospace font)

**Shortcut capture mode:**

Add state to `Inboxly`:

```rust
/// Which shortcut action is currently being captured (None = not capturing).
pub capturing_shortcut: Option<ShortcutAction>,
```

**New `Message` variants:**

```rust
/// Enter shortcut capture mode for an action.
StartCapture(ShortcutAction),
/// Cancel shortcut capture.
CancelCapture,
```

**Behaviour:**

1. Click a shortcut cell → `StartCapture(action)` → cell shows "Press key..." with a pulsing/highlighted style
2. User presses a key (or key combo) → The subscription handler converts the Iced key event to a string (e.g. `"e"`, `"Ctrl+Z"`, `"Shift+A"`) and emits `SetShortcut { action, binding }` + clears `capturing_shortcut`
3. Press Escape during capture → `CancelCapture` → restores previous binding
4. **Conflict detection:** If the new binding is already used by another action, show a brief warning text below the table (e.g. "Conflicts with: Done (e)"). Allow the override — the old action becomes unbound. This is standard behaviour in desktop apps.

**Shortcut cell rendering:**

```rust
fn shortcut_cell<'a>(
    action: ShortcutAction,
    binding: &str,
    is_capturing: bool,
    is_customised: bool,
    theme: &'a InboxlyTheme,
) -> Element<'a, Message> {
    // If capturing: "Press key..." text with highlighted background
    // If customised: binding text + small "Reset" link
    // Otherwise: binding text in monospace
}
```

**Register in views:**

Add `pub mod settings_shortcuts;` to `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/mod.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement keyboard shortcuts settings tab view`

---

## Task 9: Wire settings tabs into the settings view router

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (or wherever M29 placed the settings view router)

M29 should have created a settings content area that dispatches based on `SettingsTab`. This task wires the three new tabs into that router.

**Expected pattern (from M29):**

```rust
SettingsTab::Bundles => {
    settings_bundles::bundles_settings_tab(
        &self.settings_bundles,
        &self.theme,
    )
}
SettingsTab::Notifications => {
    settings_notifications::notifications_settings_tab(
        self.notifications_enabled,
        self.notification_sound,
        &self.notification_bundles,
        &self.theme,
    )
}
SettingsTab::Shortcuts => {
    settings_shortcuts::shortcuts_settings_tab(
        &self.shortcuts,
        self.capturing_shortcut,
        &self.theme,
    )
}
```

**Add imports:**

```rust
use crate::views::settings_bundles;
use crate::views::settings_notifications;
use crate::views::settings_shortcuts;
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui && cargo test -p inboxly-ui
```

**Commit:** `feat(ui): wire bundles, notifications, shortcuts tabs into settings router`

---

## Task 10: Keyboard event capture for shortcut editing

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (subscription handler)

When `capturing_shortcut` is `Some(action)`, the application's keyboard subscription must intercept the next key event, convert it to a shortcut string, and emit `SetShortcut`.

**Key-to-string conversion:**

```rust
/// Convert an Iced keyboard event to a shortcut string.
///
/// Returns `None` for modifier-only presses (Ctrl, Shift, Alt alone).
fn key_event_to_shortcut_string(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
) -> Option<String> {
    let key_name = match key {
        iced::keyboard::Key::Named(named) => {
            match named {
                iced::keyboard::key::Named::Escape => "Esc".to_string(),
                iced::keyboard::key::Named::Enter => "Enter".to_string(),
                iced::keyboard::key::Named::Tab => "Tab".to_string(),
                iced::keyboard::key::Named::Space => "Space".to_string(),
                iced::keyboard::key::Named::Backspace => "Backspace".to_string(),
                iced::keyboard::key::Named::Delete => "Delete".to_string(),
                iced::keyboard::key::Named::ArrowUp => "Up".to_string(),
                iced::keyboard::key::Named::ArrowDown => "Down".to_string(),
                iced::keyboard::key::Named::ArrowLeft => "Left".to_string(),
                iced::keyboard::key::Named::ArrowRight => "Right".to_string(),
                // Modifier-only keys — ignore.
                iced::keyboard::key::Named::Control
                | iced::keyboard::key::Named::Shift
                | iced::keyboard::key::Named::Alt
                | iced::keyboard::key::Named::Super => return None,
                other => format!("{other:?}"),
            }
        }
        iced::keyboard::Key::Character(c) => c.to_string(),
        iced::keyboard::Key::Unidentified => return None,
    };

    let mut parts = Vec::new();
    if modifiers.control() {
        parts.push("Ctrl");
    }
    if modifiers.alt() {
        parts.push("Alt");
    }
    if modifiers.shift() && key_name.len() > 1 {
        // Only prefix Shift for non-character keys.
        // For characters, Shift is implicit (e.g. "R" not "Shift+r").
        parts.push("Shift");
    }
    parts.push(&key_name);
    Some(parts.join("+"))
}
```

**In the subscription handler** (wherever keyboard events are processed — likely `subscription()` or a custom widget):

```rust
// If we are capturing a shortcut, intercept the key event.
if let Some(action) = self.capturing_shortcut {
    if let Some(binding) = key_event_to_shortcut_string(&key, modifiers) {
        if binding == "Esc" {
            // Cancel capture.
            return Task::done(Message::CancelCapture);
        }
        self.capturing_shortcut = None;
        return Task::done(Message::SetShortcut { action, binding });
    }
}
```

**Handle `CancelCapture` and `StartCapture` in `update()`:**

```rust
Message::StartCapture(action) => {
    self.capturing_shortcut = Some(action);
}
Message::CancelCapture => {
    self.capturing_shortcut = None;
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui && cargo test -p inboxly-ui
```

**Commit:** `feat(ui): add keyboard capture mode for shortcut editing`

---

## Task 11: Integration tests

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/integration.rs` (append) and `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/keyboard.rs` (already has unit tests from Task 1)

### Store integration tests (append to existing integration test file):

```rust
#[test]
fn bundle_visibility_toggle_persists() {
    let store = Store::open_in_memory().expect("open store");
    inboxly_bundler::system_bundles::ensure_system_bundles(&store).expect("ensure bundles");

    let mut bundles = store.list_bundles().expect("list");
    let bundle = &mut bundles[0];
    bundle.visibility = "hidden".to_string();
    store.update_bundle(bundle).expect("update visibility");

    let reloaded = store.get_bundle(&bundle.id).expect("get bundle");
    assert_eq!(reloaded.visibility, "hidden");
}

#[test]
fn bundle_throttle_update_persists() {
    let store = Store::open_in_memory().expect("open store");
    inboxly_bundler::system_bundles::ensure_system_bundles(&store).expect("ensure bundles");

    let mut bundles = store.list_bundles().expect("list");
    let bundle = &mut bundles[0]; // Social — currently Immediate
    let new_throttle = r#"{"mode":"Daily","delivery_time":"17:00:00"}"#;
    bundle.throttle = new_throttle.to_string();
    store.update_bundle(bundle).expect("update throttle");

    let reloaded = store.get_bundle(&bundle.id).expect("get bundle");
    assert_eq!(reloaded.throttle, new_throttle);
}

#[test]
fn bundle_reorder_persists() {
    let store = Store::open_in_memory().expect("open store");
    inboxly_bundler::system_bundles::ensure_system_bundles(&store).expect("ensure bundles");

    let mut bundles = store.list_bundles().expect("list");
    assert_eq!(bundles.len(), 8);

    // Swap first and second bundle sort_orders.
    bundles[0].sort_order = 1;
    bundles[1].sort_order = 0;
    store.update_bundle(&bundles[0]).expect("update 0");
    store.update_bundle(&bundles[1]).expect("update 1");

    let reloaded = store.list_bundles().expect("list after reorder");
    assert_eq!(reloaded[0].category, bundles[1].category);
    assert_eq!(reloaded[1].category, bundles[0].category);
}

#[test]
fn notification_settings_roundtrip() {
    let store = Store::open_in_memory().expect("open store");

    store.set_setting("notifications_enabled", "false").expect("set");
    store.set_setting("notification_sound", "false").expect("set");
    store.set_setting("notification_bundles", r#"["Social","Finance"]"#).expect("set");

    assert_eq!(
        store.get_setting("notifications_enabled").expect("get").as_deref(),
        Some("false")
    );
    assert_eq!(
        store.get_setting("notification_sound").expect("get").as_deref(),
        Some("false")
    );
    let bundles_json = store.get_setting("notification_bundles").expect("get").expect("some");
    let bundles: Vec<String> = serde_json::from_str(&bundles_json).expect("parse");
    assert_eq!(bundles, vec!["Social", "Finance"]);
}

#[test]
fn shortcut_overrides_roundtrip() {
    let store = Store::open_in_memory().expect("open store");

    let overrides = r#"{"done":"d","pin":"p"}"#;
    store.set_setting("shortcuts", overrides).expect("set");

    let loaded = store.get_setting("shortcuts").expect("get").expect("some");
    assert_eq!(loaded, overrides);
}
```

### UI keyboard module tests (already in Task 1, verify they all pass):

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- keyboard
```

**Verify all tests pass:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test --workspace && cargo clippy --workspace -- -D warnings
```

**Commit:** `test: add integration tests for bundle, notification, and shortcut settings`

---

## Task 12: Dark theme support for all new views

**File:** All three new view files (`settings_bundles.rs`, `settings_notifications.rs`, `settings_shortcuts.rs`)

Audit all new views to ensure they respect `ThemeColors`:

1. **Text colours** — All labels use `theme.colors.text_primary`, descriptions use `theme.colors.text_secondary`.
2. **Backgrounds** — Settings tab content uses `theme.colors.background`. Individual controls use `theme.colors.surface`.
3. **Dividers** — Row separators use `theme.colors.divider`.
4. **Throttle pills** — Green/orange/blue colours are intentionally theme-independent (they are semantic status indicators). However, the pill text should be white in both themes.
5. **Shortcut cells** — Monospace key indicators should have a `surface` background with a `divider` border, `text_primary` text.
6. **Capture mode highlight** — Use `theme.colors.surface_selected` background.

**Verify visually** if possible (run the app with `--theme dark`), otherwise verify by reading the view code and confirming every colour reference goes through `theme.colors.*`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo clippy --workspace -- -D warnings
```

**Commit:** `fix(ui): ensure dark theme compliance for all settings tabs`

---

## Final Verification

```bash
cd /mnt/TempNVME/projects/inbox-rust \
  && cargo fmt --check \
  && cargo clippy --workspace -- -D warnings \
  && cargo test --workspace \
  && cargo build --workspace
```

All four must pass before merging.

---

## File Summary

| File | Action |
|------|--------|
| `inboxly-ui/src/keyboard.rs` | **Rewrite** — delete `Shortcuts` struct, add `ShortcutAction` enum + `ShortcutMap` |
| `inboxly-ui/src/app.rs` | **Modify** — add shortcuts, notifications, bundles state + messages + handlers |
| `inboxly-ui/src/views/settings_bundles.rs` | **New** — Bundles tab view |
| `inboxly-ui/src/views/settings_notifications.rs` | **New** — Notifications tab view |
| `inboxly-ui/src/views/settings_shortcuts.rs` | **New** — Keyboard Shortcuts tab view |
| `inboxly-ui/src/views/mod.rs` | **Modify** — register three new view modules |
| `inboxly-store/tests/integration.rs` | **Modify** — add settings integration tests |

## Test Count Estimate

- Task 1 (keyboard.rs unit tests): **12 tests**
- Task 11 (store integration tests): **5 tests**
- Existing tests (must not regress): all current workspace tests

**Expected new tests: ~17**
