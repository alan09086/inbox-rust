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

/// Convert an iced keyboard key + modifiers into a shortcut string.
///
/// Returns strings like `"e"`, `"Ctrl+Z"`, `"Shift+R"`, `"Esc"`.
/// Returns `None` for modifier-only presses or unrecognised keys.
pub fn key_event_to_shortcut_string(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
) -> Option<String> {
    use iced::keyboard::{Key, key::Named};

    let key_str = match key {
        Key::Named(named) => match named {
            Named::Escape => "Esc".to_owned(),
            Named::Enter => "Enter".to_owned(),
            Named::Tab => "Tab".to_owned(),
            Named::Space => "Space".to_owned(),
            Named::Backspace => "Backspace".to_owned(),
            Named::Delete => "Delete".to_owned(),
            Named::ArrowUp => "Up".to_owned(),
            Named::ArrowDown => "Down".to_owned(),
            Named::ArrowLeft => "Left".to_owned(),
            Named::ArrowRight => "Right".to_owned(),
            Named::Home => "Home".to_owned(),
            Named::End => "End".to_owned(),
            Named::PageUp => "PageUp".to_owned(),
            Named::PageDown => "PageDown".to_owned(),
            Named::F1 => "F1".to_owned(),
            Named::F2 => "F2".to_owned(),
            Named::F3 => "F3".to_owned(),
            Named::F4 => "F4".to_owned(),
            Named::F5 => "F5".to_owned(),
            Named::F6 => "F6".to_owned(),
            Named::F7 => "F7".to_owned(),
            Named::F8 => "F8".to_owned(),
            Named::F9 => "F9".to_owned(),
            Named::F10 => "F10".to_owned(),
            Named::F11 => "F11".to_owned(),
            Named::F12 => "F12".to_owned(),
            // Modifier-only presses -- ignore
            Named::Shift | Named::Control | Named::Alt | Named::Super => return None,
            _ => return None,
        },
        Key::Character(c) => {
            let s = c.to_string();
            if s.is_empty() {
                return None;
            }
            // For Shift+letter, keep the uppercase form
            if modifiers.shift() && s.len() == 1 && s.chars().next().unwrap().is_ascii_alphabetic()
            {
                // We include "Shift+" prefix explicitly
                s.to_uppercase()
            } else {
                s
            }
        }
        Key::Unidentified => return None,
    };

    // Build modifier prefix (Ctrl+Alt+Shift+)
    let mut parts = Vec::new();
    if modifiers.control() {
        parts.push("Ctrl");
    }
    if modifiers.alt() {
        parts.push("Alt");
    }
    // Only add Shift prefix for non-character keys or when combined with Ctrl/Alt.
    // For plain Shift+letter, we already have the uppercase letter.
    if modifiers.shift() && (modifiers.control() || modifiers.alt() || matches!(key, Key::Named(_)))
    {
        parts.push("Shift");
    }

    if parts.is_empty() {
        Some(key_str)
    } else {
        parts.push(&key_str);
        Some(parts.join("+"))
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

    // -- key_event_to_shortcut_string tests --

    #[test]
    fn key_event_plain_character() {
        use iced::keyboard::{Key, Modifiers};
        let result =
            super::key_event_to_shortcut_string(&Key::Character("e".into()), Modifiers::empty());
        assert_eq!(result, Some("e".to_owned()));
    }

    #[test]
    fn key_event_ctrl_z() {
        use iced::keyboard::{Key, Modifiers};
        let result =
            super::key_event_to_shortcut_string(&Key::Character("z".into()), Modifiers::CTRL);
        assert_eq!(result, Some("Ctrl+z".to_owned()));
    }

    #[test]
    fn key_event_escape() {
        use iced::keyboard::{Key, Modifiers, key::Named};
        let result =
            super::key_event_to_shortcut_string(&Key::Named(Named::Escape), Modifiers::empty());
        assert_eq!(result, Some("Esc".to_owned()));
    }

    #[test]
    fn key_event_shift_letter() {
        use iced::keyboard::{Key, Modifiers};
        let result =
            super::key_event_to_shortcut_string(&Key::Character("R".into()), Modifiers::SHIFT);
        // Shift+letter alone: uppercase letter (no "Shift+" prefix)
        assert_eq!(result, Some("R".to_owned()));
    }

    #[test]
    fn key_event_ctrl_shift_letter() {
        use iced::keyboard::{Key, Modifiers};
        let mods = Modifiers::CTRL | Modifiers::SHIFT;
        let result = super::key_event_to_shortcut_string(&Key::Character("A".into()), mods);
        assert_eq!(result, Some("Ctrl+Shift+A".to_owned()));
    }

    #[test]
    fn key_event_modifier_only_returns_none() {
        use iced::keyboard::{Key, Modifiers, key::Named};
        let result =
            super::key_event_to_shortcut_string(&Key::Named(Named::Shift), Modifiers::SHIFT);
        assert!(result.is_none());
    }

    #[test]
    fn key_event_f_key() {
        use iced::keyboard::{Key, Modifiers, key::Named};
        let result =
            super::key_event_to_shortcut_string(&Key::Named(Named::F5), Modifiers::empty());
        assert_eq!(result, Some("F5".to_owned()));
    }

    #[test]
    fn key_event_unidentified_returns_none() {
        use iced::keyboard::{Key, Modifiers};
        let result = super::key_event_to_shortcut_string(&Key::Unidentified, Modifiers::empty());
        assert!(result.is_none());
    }
}
