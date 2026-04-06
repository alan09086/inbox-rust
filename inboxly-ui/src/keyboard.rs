//! Keyboard shortcut definitions for the application.
//!
//! Provides a runtime-configurable shortcut map (`ShortcutMap`) keyed by
//! `ShortcutAction`. Defaults match the original Google Inbox conventions;
//! users can override individual bindings and persist only the deltas.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Every action that can be triggered by a keyboard shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutAction {
    /// Archive / Mark Done.
    Done,
    /// Pin toggle.
    Pin,
    /// Snooze.
    Snooze,
    /// Search.
    Search,
    /// Compose new email.
    Compose,
    /// Refresh.
    Refresh,
    /// Select next.
    Next,
    /// Select previous.
    Previous,
    /// Open thread.
    Open,
    /// Undo.
    Undo,
    /// Go to inbox.
    GoInbox,
    /// Go to snoozed.
    GoSnoozed,
    /// Go to done.
    GoDone,
    /// Reply to a thread.
    Reply,
    /// Reply all.
    ReplyAll,
    /// Forward a thread.
    Forward,
    /// Show keyboard shortcuts help.
    Help,
    /// Escape / dismiss.
    Escape,
}

impl ShortcutAction {
    /// All shortcut actions in display order.
    pub const ALL: [ShortcutAction; 18] = [
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

    /// Human-readable label for display in settings UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Done => "Mark Done",
            Self::Pin => "Pin Toggle",
            Self::Snooze => "Snooze",
            Self::Search => "Search",
            Self::Compose => "Compose",
            Self::Refresh => "Refresh",
            Self::Next => "Next",
            Self::Previous => "Previous",
            Self::Open => "Open",
            Self::Undo => "Undo",
            Self::GoInbox => "Go to Inbox",
            Self::GoSnoozed => "Go to Snoozed",
            Self::GoDone => "Go to Done",
            Self::Reply => "Reply",
            Self::ReplyAll => "Reply All",
            Self::Forward => "Forward",
            Self::Help => "Help",
            Self::Escape => "Escape",
        }
    }
}

/// Runtime keyboard shortcut map.
///
/// Stores a full set of action-to-binding mappings, with an internal
/// overlay so only user-customised bindings are persisted.
#[derive(Debug, Clone)]
pub struct ShortcutMap {
    /// Current bindings (defaults merged with overrides).
    bindings: HashMap<ShortcutAction, String>,
    /// Only the user-customised bindings (delta from defaults).
    overrides: HashMap<ShortcutAction, String>,
}

impl ShortcutMap {
    /// Create a map with all default bindings.
    pub fn defaults() -> Self {
        let mut bindings = HashMap::with_capacity(ShortcutAction::ALL.len());
        for (action, binding) in Self::default_pairs() {
            bindings.insert(action, binding.to_owned());
        }
        Self {
            bindings,
            overrides: HashMap::new(),
        }
    }

    /// Get the binding for an action.
    pub fn get(&self, action: ShortcutAction) -> &str {
        self.bindings.get(&action).map(String::as_str).unwrap_or("")
    }

    /// Override the binding for an action.
    pub fn set(&mut self, action: ShortcutAction, binding: String) {
        self.overrides.insert(action, binding.clone());
        self.bindings.insert(action, binding);
    }

    /// Reset an action to its default binding.
    pub fn reset(&mut self, action: ShortcutAction) {
        self.overrides.remove(&action);
        let default = Self::default_for(action);
        self.bindings.insert(action, default.to_owned());
    }

    /// Whether the given action has been customised from its default.
    pub fn is_customised(&self, action: ShortcutAction) -> bool {
        self.overrides.contains_key(&action)
    }

    /// Serialise only the overrides as a JSON string.
    ///
    /// Returns `"{}"` when there are no overrides.
    pub fn to_overrides_json(&self) -> String {
        serde_json::to_string(&self.overrides).unwrap_or_else(|_| "{}".to_owned())
    }

    /// Load overrides from a JSON string, merging over defaults.
    ///
    /// Invalid JSON or an empty string both produce a clean defaults map.
    pub fn from_overrides_json(json: &str) -> Self {
        let mut map = Self::defaults();
        if let Ok(overrides) = serde_json::from_str::<HashMap<ShortcutAction, String>>(json) {
            for (action, binding) in overrides {
                map.bindings.insert(action, binding.clone());
                map.overrides.insert(action, binding);
            }
        }
        map
    }

    /// Find the action bound to the given key string, if any.
    pub fn action_for_key(&self, key: &str) -> Option<ShortcutAction> {
        self.bindings
            .iter()
            .find(|(_, v)| v.as_str() == key)
            .map(|(k, _)| *k)
    }

    /// Iterate over all actions in display order with their current bindings.
    pub fn iter_display_order(&self) -> impl Iterator<Item = (ShortcutAction, &str)> {
        ShortcutAction::ALL.iter().map(|a| (*a, self.get(*a)))
    }

    /// The default binding for a single action.
    fn default_for(action: ShortcutAction) -> &'static str {
        match action {
            ShortcutAction::Done => "e",
            ShortcutAction::Pin => "=",
            ShortcutAction::Snooze => "b",
            ShortcutAction::Search => "/",
            ShortcutAction::Compose => "c",
            ShortcutAction::Refresh => "r",
            ShortcutAction::Next => "j",
            ShortcutAction::Previous => "k",
            ShortcutAction::Open => "o",
            ShortcutAction::Undo => "Ctrl+Z",
            ShortcutAction::GoInbox => "g i",
            ShortcutAction::GoSnoozed => "g s",
            ShortcutAction::GoDone => "g d",
            ShortcutAction::Reply => "Shift+R",
            ShortcutAction::ReplyAll => "Shift+A",
            ShortcutAction::Forward => "f",
            ShortcutAction::Help => "?",
            ShortcutAction::Escape => "Esc",
        }
    }

    /// All (action, default-binding) pairs.
    fn default_pairs() -> [(ShortcutAction, &'static str); 18] {
        ShortcutAction::ALL.map(|a| (a, Self::default_for(a)))
    }
}

impl Default for ShortcutMap {
    fn default() -> Self {
        Self::defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_cover_all_actions() {
        let map = ShortcutMap::defaults();
        for action in ShortcutAction::ALL {
            assert!(!map.get(action).is_empty(), "no default for {action:?}");
        }
    }

    #[test]
    fn get_returns_correct_default() {
        let map = ShortcutMap::defaults();
        assert_eq!(map.get(ShortcutAction::Done), "e");
        assert_eq!(map.get(ShortcutAction::Pin), "=");
        assert_eq!(map.get(ShortcutAction::Snooze), "b");
        assert_eq!(map.get(ShortcutAction::Search), "/");
        assert_eq!(map.get(ShortcutAction::Compose), "c");
        assert_eq!(map.get(ShortcutAction::Refresh), "r");
        assert_eq!(map.get(ShortcutAction::Next), "j");
        assert_eq!(map.get(ShortcutAction::Previous), "k");
        assert_eq!(map.get(ShortcutAction::Open), "o");
        assert_eq!(map.get(ShortcutAction::Undo), "Ctrl+Z");
        assert_eq!(map.get(ShortcutAction::GoInbox), "g i");
        assert_eq!(map.get(ShortcutAction::GoSnoozed), "g s");
        assert_eq!(map.get(ShortcutAction::GoDone), "g d");
        assert_eq!(map.get(ShortcutAction::Reply), "Shift+R");
        assert_eq!(map.get(ShortcutAction::ReplyAll), "Shift+A");
        assert_eq!(map.get(ShortcutAction::Forward), "f");
        assert_eq!(map.get(ShortcutAction::Help), "?");
        assert_eq!(map.get(ShortcutAction::Escape), "Esc");
    }

    #[test]
    fn set_overrides_default() {
        let mut map = ShortcutMap::defaults();
        map.set(ShortcutAction::Done, "d".to_owned());
        assert_eq!(map.get(ShortcutAction::Done), "d");
        assert!(map.is_customised(ShortcutAction::Done));
    }

    #[test]
    fn reset_restores_default() {
        let mut map = ShortcutMap::defaults();
        map.set(ShortcutAction::Done, "d".to_owned());
        map.reset(ShortcutAction::Done);
        assert_eq!(map.get(ShortcutAction::Done), "e");
        assert!(!map.is_customised(ShortcutAction::Done));
    }

    #[test]
    fn overrides_json_only_stores_changes() {
        let mut map = ShortcutMap::defaults();
        let json = map.to_overrides_json();
        assert_eq!(json, "{}");

        map.set(ShortcutAction::Done, "d".to_owned());
        let json = map.to_overrides_json();
        assert!(json.contains("done"));
        assert!(json.contains("\"d\""));
        // Should NOT contain non-overridden keys.
        assert!(!json.contains("pin"));
    }

    #[test]
    fn from_overrides_json_merges_over_defaults() {
        let json = r#"{"done":"d","pin":"+"}"#;
        let map = ShortcutMap::from_overrides_json(json);
        assert_eq!(map.get(ShortcutAction::Done), "d");
        assert_eq!(map.get(ShortcutAction::Pin), "+");
        // Non-overridden should still have defaults.
        assert_eq!(map.get(ShortcutAction::Compose), "c");
    }

    #[test]
    fn from_overrides_json_handles_invalid_json() {
        let map = ShortcutMap::from_overrides_json("not json at all");
        // Should silently fall back to defaults.
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
        assert_eq!(map.action_for_key("nonexistent"), None);
    }

    #[test]
    fn iter_display_order_covers_all_actions() {
        let map = ShortcutMap::defaults();
        let items: Vec<_> = map.iter_display_order().collect();
        assert_eq!(items.len(), 18);
        assert_eq!(items[0].0, ShortcutAction::Done);
        assert_eq!(items[17].0, ShortcutAction::Escape);
    }

    #[test]
    fn serde_roundtrip_action() {
        let action = ShortcutAction::ReplyAll;
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(json, "\"reply_all\"");
        let back: ShortcutAction = serde_json::from_str(&json).unwrap();
        assert_eq!(back, action);
    }

    #[test]
    fn default_impl_matches_defaults() {
        let from_default = ShortcutMap::default();
        let from_fn = ShortcutMap::defaults();
        for action in ShortcutAction::ALL {
            assert_eq!(from_default.get(action), from_fn.get(action));
        }
    }

}
