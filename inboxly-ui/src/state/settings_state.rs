//! Settings drawer/panel state grouped into a single sub-struct.
//!
//! Extracted from `Inboxly` in M35a to stop the top-level struct from
//! ballooning as compose state lands in M35b. Field names drop the
//! `settings_` prefix when it would be redundant with the namespace
//! (e.g. `settings_bundles` → `bundles`).
//!
//! The default values must match the pre-refactor `Inboxly::default()`
//! exactly — this is a behaviour-preserving refactor.

use inboxly_core::config::{AccountConfig, ThemePreference};
use inboxly_store::BundleRow;

use crate::app::{SettingsTab, new_empty_account_form};
use crate::keyboard::{ShortcutAction, ShortcutMap};

/// State backing the Settings drawer and panel.
pub struct SettingsState {
    /// Active settings tab (only relevant when `active_view == Settings`).
    pub tab: SettingsTab,
    /// Whether the nav drawer was open before entering settings.
    pub drawer_was_open: bool,

    // -- General tab state --
    /// Current theme preference (System/Light/Dark).
    pub theme_preference: ThemePreference,
    /// Default view preference ("inbox", "snoozed", "done").
    pub default_view: String,
    /// Undo timeout in seconds.
    pub undo_timeout_secs: u32,

    // -- Accounts tab state --
    /// Index of the account currently being edited (None = no edit form open).
    pub editing_account_index: Option<usize>,
    /// Scratch account for the add/edit form.
    pub account_form: AccountConfig,
    /// Whether we're adding a new account (vs editing existing).
    pub adding_account: bool,
    /// Index of account pending removal confirmation (None = no confirmation shown).
    pub removing_account_index: Option<usize>,

    // -- Data & Storage tab state --
    /// Cached database size string (e.g., "42.3 MB").
    pub db_size_display: String,
    /// Cached search index size string.
    pub index_size_display: String,
    /// Cached maildir size string.
    pub maildir_size_display: String,
    /// Last full sync timestamp display string.
    pub last_sync_display: String,
    /// Status message shown after an action (e.g., "Cache cleared", "Rebuilding...").
    pub data_action_status: Option<String>,

    // -- Keyboard shortcuts state --
    /// Runtime keyboard shortcut bindings.
    pub shortcuts: ShortcutMap,
    /// Action currently being re-bound (user is pressing a new key).
    pub capturing_shortcut: Option<ShortcutAction>,

    // -- Notification settings state --
    /// Whether desktop notifications are enabled.
    pub notifications_enabled: bool,
    /// Whether notification sound is enabled.
    pub notification_sound: bool,
    /// Which bundles trigger notifications (["all"] = all bundles).
    pub notification_bundles: Vec<String>,

    // -- Bundle settings state --
    /// All bundles loaded from the store (for the Bundles settings tab).
    pub bundles: Vec<BundleRow>,
    /// Bundle whose throttle popup is currently open.
    pub throttle_popup_bundle_id: Option<String>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsState {
    /// Create settings state with defaults matching the pre-M35a shape.
    pub fn new() -> Self {
        Self {
            tab: SettingsTab::General,
            drawer_was_open: true,
            theme_preference: ThemePreference::System,
            default_view: "inbox".to_owned(),
            undo_timeout_secs: 7,
            editing_account_index: None,
            account_form: new_empty_account_form(),
            adding_account: false,
            removing_account_index: None,
            db_size_display: String::new(),
            index_size_display: String::new(),
            maildir_size_display: String::new(),
            last_sync_display: "Never".to_owned(),
            data_action_status: None,
            shortcuts: ShortcutMap::defaults(),
            capturing_shortcut: None,
            notifications_enabled: true,
            notification_sound: true,
            notification_bundles: vec!["all".to_string()],
            bundles: Vec::new(),
            throttle_popup_bundle_id: None,
        }
    }
}
