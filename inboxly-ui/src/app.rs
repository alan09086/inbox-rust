//! Core application state machine -- no framework dependencies.

use std::collections::HashSet;

use inboxly_core::config::{AccountConfig, AppConfig, AuthMethod, Paths, ThemePreference};
use inboxly_core::offline::OfflineAction;
use inboxly_store::{BundleRow, Store};

use crate::feed::{self, FeedSection};
use crate::keyboard::{ShortcutAction, ShortcutMap};
use crate::nav::{NavBundleCategory, NavTarget, default_bundle_categories};
use crate::theme::{ActiveView, InboxlyTheme, SettingsReader};
use crate::undo::{UndoAction, UndoState};

/// A 2D point (replaces `iced::Point`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ORIGIN: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Settings sidebar tab (moved from `settings_view.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SettingsTab {
    #[default]
    General,
    Accounts,
    Bundles,
    Notifications,
    KeyboardShortcuts,
    DataStorage,
}

impl SettingsTab {
    /// Display label for the sidebar.
    pub fn label(&self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Accounts => "Accounts",
            Self::Bundles => "Bundles",
            Self::Notifications => "Notifications",
            Self::KeyboardShortcuts => "Keyboard Shortcuts",
            Self::DataStorage => "Data & Storage",
        }
    }

    /// All tabs in display order.
    pub fn all() -> &'static [Self] {
        &[
            Self::General,
            Self::Accounts,
            Self::Bundles,
            Self::Notifications,
            Self::KeyboardShortcuts,
            Self::DataStorage,
        ]
    }
}

/// Adapter that wraps a [`Store`] reference and implements
/// [`SettingsReader`] from `crate::theme`.
///
/// This avoids a circular dependency between `inboxly-store` and `inboxly-ui`.
pub struct StoreSettingsAdapter<'a> {
    pub store: &'a Store,
}

impl SettingsReader for StoreSettingsAdapter<'_> {
    fn get_setting(&self, key: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
        self.store
            .get_setting(key)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}

/// Top-level application state.
pub struct Inboxly {
    /// Currently active primary view (drives toolbar colour).
    pub active_view: ActiveView,
    /// View to return to when leaving Settings (back arrow).
    pub previous_view: ActiveView,
    /// Currently selected nav target (may be a primary view, folder, or bundle).
    pub active_nav: NavTarget,
    /// Whether the nav drawer is visible (toggled by hamburger).
    pub drawer_open: bool,
    /// Bundle categories shown in the nav drawer.
    pub bundle_categories: Vec<NavBundleCategory>,
    /// Configured email accounts (loaded from AppConfig on startup).
    pub accounts: Vec<inboxly_core::config::AccountConfig>,
    /// Index of the currently active account.
    pub active_account_index: usize,
    /// Whether the account switcher dropdown is expanded.
    pub account_switcher_open: bool,
    /// Active theme (light or dark, with full BigTop tokens).
    pub theme: InboxlyTheme,
    /// SQLite store for querying threads (None until wired from binary).
    pub store: Option<Store>,
    /// Pre-built feed sections for the inbox view.
    pub feed_sections: Vec<FeedSection>,
    /// Undo state for timed undo of inbox actions.
    pub undo_state: UndoState,
    /// Thread ID whose overflow (three-dot) menu is currently open.
    pub overflow_menu_thread: Option<String>,
    /// Cursor position where the overflow menu was triggered (popup anchor).
    pub overflow_menu_position: Point,
    /// Thread ID whose right-click context menu is currently open.
    pub context_menu_thread: Option<String>,
    /// Cursor position where the context menu was triggered.
    pub context_menu_position: Point,
    /// Sender address of the thread whose menu is open (for BlockSender, CreateRuleFromSender).
    /// Shared between overflow and context menus — they are mutually exclusive.
    pub menu_thread_sender: Option<String>,

    // -- Settings state --
    /// Active settings tab (only relevant when active_view == Settings).
    pub settings_tab: SettingsTab,
    /// Whether the drawer was open before entering settings.
    pub drawer_was_open: bool,
    /// Loaded AppConfig (for accounts + snooze presets editing).
    pub config: AppConfig,

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
    pub settings_bundles: Vec<BundleRow>,
    /// Bundle whose throttle popup is currently open.
    pub throttle_popup_bundle_id: Option<String>,

    // -- Feed interaction state --
    /// Set of bundle IDs that are currently expanded in the inbox feed.
    pub expanded_bundles: HashSet<String>,
    /// Thread ID whose snooze date-picker popup is currently open.
    pub snooze_picker_thread: Option<String>,
    /// Cursor position where the snooze picker was triggered (popup anchor).
    pub snooze_picker_position: Point,
}

/// IMAP folder destinations for the "Move to..." action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveDestination {
    Inbox,
    Trash,
    Spam,
}

/// All messages the application can receive.
// ThemeChanged carries InboxlyTheme which has grown with additional Color fields;
// boxing would change the API surface and is heavy-handed for a UI message enum.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum Message {
    /// User clicked a nav item.
    Navigate(NavTarget),
    /// User toggled the hamburger menu.
    ToggleDrawer,
    /// Search bar text changed (placeholder for now).
    SearchChanged(String),
    /// User toggled the theme (light <-> dark).
    ThemeToggled,
    /// Async system theme detection completed.
    ThemeChanged(InboxlyTheme),
    /// Reload the inbox feed from the store.
    ReloadFeed,
    /// Mark a thread as Done (archive).
    MarkDone(String),
    /// Toggle pin state for a thread.
    TogglePin(String),
    /// Sweep: mark all unpinned threads in the current section as Done.
    Sweep,
    /// User pressed Undo on the snackbar.
    Undo,
    /// Undo timer expired -- commit the action.
    UndoExpired,
    /// Snooze a thread until the given UTC timestamp.
    SnoozeThread {
        thread_id: String,
        until: chrono::DateTime<chrono::Utc>,
    },
    /// Open the overflow (three-dot) menu for a specific thread.
    OpenOverflowMenu {
        thread_id: String,
        sender_address: String,
        position: Point,
    },
    /// Close the overflow menu.
    CloseOverflowMenu,
    /// Open the right-click context menu for a thread at a cursor position.
    OpenContextMenu {
        thread_id: String,
        sender_address: String,
        position: Point,
    },
    /// Close the right-click context menu.
    CloseContextMenu,
    /// Toggle the account switcher dropdown in the nav drawer.
    ToggleAccountSwitcher,
    /// Switch to the account at the given index.
    SwitchAccount(usize),
    /// Navigate to Settings view (gear icon).
    NavigateToSettings,
    /// Navigate back from Settings to previous view.
    NavigateBack,
    /// Settings sidebar tab clicked.
    SettingsTabChanged(SettingsTab),

    // -- General tab --
    /// Theme chip button clicked.
    SetThemePreference(ThemePreference),
    /// Default view dropdown changed.
    SetDefaultView(String),
    /// Undo timeout dropdown changed.
    SetUndoTimeout(u32),
    /// Snooze morning hour changed.
    SetSnoozeMorningHour(String),
    /// Snooze afternoon hour changed.
    SetSnoozeAfternoonHour(String),
    /// Snooze evening hour changed.
    SetSnoozeEveningHour(String),
    /// Snooze weekend day changed.
    SetSnoozeWeekendDay(String),

    // -- Accounts tab --
    /// Open the add-account form.
    AddAccountStart,
    /// Open the edit form for account at index.
    EditAccountStart(usize),
    /// Cancel add/edit form.
    AccountFormCancel,
    /// Save the add/edit form.
    AccountFormSave,
    /// Account form field changed.
    AccountFormEmailChanged(String),
    AccountFormDisplayNameChanged(String),
    AccountFormProviderChanged(String),
    AccountFormAuthMethodChanged(String),
    AccountFormImapHostChanged(String),
    AccountFormImapPortChanged(String),
    AccountFormSmtpHostChanged(String),
    AccountFormSmtpPortChanged(String),
    /// Show removal confirmation for account at index.
    RemoveAccountConfirm(usize),
    /// Dismiss removal confirmation.
    RemoveAccountCancel,
    /// Execute account removal.
    RemoveAccountExecute(usize),

    // -- Data & Storage tab --
    /// Clear cache button pressed.
    ClearCache,
    /// Rebuild search index button pressed.
    RebuildSearchIndex,
    /// Export data button pressed.
    ExportData,
    /// Async: data sizes calculated.
    DataSizesLoaded {
        db_size: String,
        index_size: String,
        maildir_size: String,
        last_sync: String,
    },

    // -- Keyboard shortcuts tab --
    /// Shortcuts loaded from store.
    ShortcutsLoaded(ShortcutMap),
    /// User set a new binding for an action.
    SetShortcut {
        action: ShortcutAction,
        binding: String,
    },
    /// Reset an action to its default binding.
    ResetShortcut(ShortcutAction),
    /// Begin capturing a new key for the given action.
    StartCapture(ShortcutAction),
    /// Cancel capture mode.
    CancelCapture,

    // -- Notifications tab --
    /// Toggle desktop notifications on/off.
    ToggleNotifications,
    /// Toggle notification sound on/off.
    ToggleNotificationSound,
    /// Set which bundles trigger notifications.
    SetNotificationBundles(Vec<String>),

    // -- Bundles tab --
    /// Bundles loaded from store.
    BundlesLoaded(Vec<BundleRow>),
    /// Toggle a bundle's visibility (visible/hidden).
    ToggleBundleVisibility(String),
    /// Set a bundle's throttle JSON.
    SetBundleThrottle {
        bundle_id: String,
        throttle_json: String,
    },
    /// Reorder bundles by their IDs (new order).
    ReorderBundles(Vec<String>),
    /// Open/close the throttle configuration popup.
    ToggleThrottlePopup(Option<String>),

    /// Move thread to a folder.
    MoveTo {
        thread_id: String,
        destination: MoveDestination,
    },
    /// Mark thread as read or unread.
    MarkReadState {
        thread_id: String,
        read: bool,
    },
    /// Mute a thread.
    MuteThread(String),
    /// Reply to a thread.
    Reply(String),
    /// Reply all to a thread.
    ReplyAll(String),
    /// Forward a thread.
    Forward(String),
    /// Add thread to a bundle category.
    AddToBundle {
        thread_id: String,
        category: String,
    },
    /// Create a rule from sender (stub -- shows "Coming soon" toast).
    CreateRuleFromSender(String),
    /// Block the sender.
    BlockSender {
        thread_id: String,
        sender_address: String,
    },
    /// Report thread as spam.
    ReportSpam(String),

    // -- Feed interaction --
    /// Toggle the expanded/collapsed state of a bundle row in the inbox feed.
    ToggleBundleExpand(String),
    /// Open the snooze date-picker for a thread at a cursor position.
    OpenSnoozePicker {
        thread_id: String,
        position: Point,
    },
    /// Close the snooze date-picker popup.
    CloseSnoozePicker,
}

impl Default for Inboxly {
    fn default() -> Self {
        Self {
            active_view: ActiveView::Inbox,
            previous_view: ActiveView::Inbox,
            active_nav: NavTarget::View(ActiveView::Inbox),
            drawer_open: true,
            bundle_categories: default_bundle_categories(),
            accounts: Vec::new(),
            active_account_index: 0,
            account_switcher_open: false,
            theme: InboxlyTheme::light(),
            store: None,
            feed_sections: Vec::new(),
            undo_state: UndoState::new(),
            overflow_menu_thread: None,
            overflow_menu_position: Point::ORIGIN,
            context_menu_thread: None,
            context_menu_position: Point::ORIGIN,
            menu_thread_sender: None,
            // Settings state
            settings_tab: SettingsTab::General,
            drawer_was_open: true,
            config: AppConfig::default(),
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
            // Keyboard shortcuts
            shortcuts: ShortcutMap::defaults(),
            capturing_shortcut: None,
            // Notifications
            notifications_enabled: true,
            notification_sound: true,
            notification_bundles: vec!["all".to_string()],
            // Bundle settings
            settings_bundles: Vec::new(),
            throttle_popup_bundle_id: None,
            // Feed interaction
            expanded_bundles: HashSet::new(),
            snooze_picker_thread: None,
            snooze_picker_position: Point::ORIGIN,
        }
    }
}

/// Create an empty account form with sensible defaults.
fn new_empty_account_form() -> AccountConfig {
    AccountConfig {
        email: String::new(),
        display_name: String::new(),
        provider: "generic".to_owned(),
        auth_method: AuthMethod::Password,
        imap_host: String::new(),
        imap_port: 993,
        smtp_host: String::new(),
        smtp_port: 587,
    }
}

/// Calculate the total size of a directory in bytes (recursive).
fn dir_size(path: &std::path::Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    dir_size_recursive(path)
}

fn dir_size_recursive(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                } else if meta.is_dir() {
                    total += dir_size_recursive(&entry.path());
                }
            }
        }
    }
    total
}

/// Format bytes as a human-readable size string.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

impl Inboxly {
    /// Create the app with initial state.
    ///
    /// Theme should be resolved before calling this (via
    /// `InboxlyTheme::from_system()`) since zbus requires Tokio.
    pub fn new() -> Self {
        let mut app = Self {
            theme: InboxlyTheme::from_system(),
            ..Self::default()
        };
        app.reload_feed();
        app
    }

    /// Create the app with pre-loaded account configs.
    pub fn with_accounts(accounts: Vec<inboxly_core::config::AccountConfig>) -> Self {
        Self {
            accounts,
            ..Self::default()
        }
    }

    /// Create the app with a store instance (called from binary crate).
    pub fn with_store(store: Store) -> Self {
        let mut app = Self {
            store: Some(store),
            theme: InboxlyTheme::from_system(),
            ..Self::default()
        };
        app.reload_feed();
        app
    }

    /// Returns the currently active account config.
    pub fn active_account(&self) -> Option<&inboxly_core::config::AccountConfig> {
        self.accounts.get(self.active_account_index)
    }

    /// Returns the email address of the active account.
    pub fn active_email(&self) -> &str {
        self.active_account()
            .map(|a| a.email.as_str())
            .unwrap_or("No account")
    }

    /// Returns the display name of the active account.
    pub fn active_display_name(&self) -> &str {
        self.active_account()
            .map(|a| {
                if a.display_name.is_empty() {
                    a.email.as_str()
                } else {
                    a.display_name.as_str()
                }
            })
            .unwrap_or("No account")
    }

    /// Handle a message and mutate state accordingly.
    pub fn update(&mut self, message: Message) {
        match message {
            Message::Navigate(target) => {
                if let NavTarget::View(view) = &target {
                    self.active_view = *view;
                }
                self.active_nav = target;
                self.account_switcher_open = false;
            }
            Message::ToggleAccountSwitcher => {
                self.account_switcher_open = !self.account_switcher_open;
            }
            Message::SwitchAccount(index) => {
                if index < self.accounts.len() {
                    self.active_account_index = index;
                    self.account_switcher_open = false;
                    self.reload_feed();
                } else {
                    tracing::warn!(
                        "SwitchAccount index {} out of bounds (have {} accounts)",
                        index,
                        self.accounts.len()
                    );
                }
            }
            Message::ToggleDrawer => {
                self.drawer_open = !self.drawer_open;
            }
            Message::SearchChanged(_query) => {
                // Placeholder -- search is M24.
            }
            Message::ThemeToggled => {
                self.theme = self.theme.toggle();
            }
            Message::ThemeChanged(new_theme) => {
                self.theme = new_theme;
            }
            Message::ReloadFeed => {
                self.reload_feed();
            }
            Message::MarkDone(thread_id) => {
                if let Some(ref store) = self.store {
                    if let Err(e) = store.get_or_create_thread_state(&thread_id) {
                        tracing::warn!("failed to ensure thread state: {e}");
                    }
                    if let Err(e) = store.set_thread_done(&thread_id, true) {
                        tracing::warn!("failed to mark done: {e}");
                    }
                }
                // Enqueue IMAP archive actions for all emails in thread.
                self.enqueue_thread_actions(&thread_id, |account_id, folder, imap_uid| {
                    OfflineAction::MarkDone {
                        account_id: account_id.to_string(),
                        folder: folder.to_string(),
                        imap_uid,
                    }
                });
                self.undo_state.push(UndoAction::MarkDone { thread_id });
                self.reload_feed();
            }
            Message::TogglePin(thread_id) => {
                let was_pinned = self
                    .store
                    .as_ref()
                    .and_then(|store| store.get_thread_state(&thread_id).ok().map(|s| s.pinned));
                let was_pinned = was_pinned.unwrap_or(false);

                if let Some(ref store) = self.store {
                    if let Err(e) = store.get_or_create_thread_state(&thread_id) {
                        tracing::warn!("failed to ensure thread state: {e}");
                    }
                    if let Err(e) = store.set_thread_pinned(&thread_id, !was_pinned) {
                        tracing::warn!("failed to toggle pin: {e}");
                    }
                }
                self.undo_state.push(UndoAction::TogglePin {
                    thread_id,
                    was_pinned,
                });
                self.reload_feed();
            }
            Message::Sweep => {
                // Mark all unpinned, non-done threads as done.
                let mut swept = Vec::new();
                if let Some(ref store) = self.store
                    && let Ok(threads) = store.query_inbox_threads()
                {
                    for thread in threads {
                        if !thread.pinned {
                            if let Err(e) = store.get_or_create_thread_state(&thread.id) {
                                tracing::warn!("sweep: failed to ensure state: {e}");
                                continue;
                            }
                            if let Err(e) = store.set_thread_done(&thread.id, true) {
                                tracing::warn!("sweep: failed to mark done: {e}");
                                continue;
                            }
                            swept.push(thread.id);
                        }
                    }
                }
                if !swept.is_empty() {
                    self.undo_state
                        .push(UndoAction::Sweep { thread_ids: swept });
                }
                self.reload_feed();
            }
            Message::Undo => {
                if let Some(action) = self.undo_state.take() {
                    if let Some(ref store) = self.store {
                        match action {
                            UndoAction::MarkDone { thread_id } => {
                                let _ = store.set_thread_done(&thread_id, false);
                            }
                            UndoAction::TogglePin {
                                thread_id,
                                was_pinned,
                            } => {
                                let _ = store.set_thread_pinned(&thread_id, was_pinned);
                            }
                            UndoAction::Sweep { thread_ids } => {
                                for tid in &thread_ids {
                                    let _ = store.set_thread_done(tid, false);
                                }
                            }
                        }
                    }
                    self.reload_feed();
                }
            }
            Message::UndoExpired => {
                self.undo_state.clear();
            }
            Message::SnoozeThread { thread_id, until } => {
                if let Some(ref store) = self.store {
                    if let Err(e) = store.get_or_create_thread_state(&thread_id) {
                        tracing::warn!("failed to ensure thread state for snooze: {e}");
                    }
                    if let Err(e) =
                        store.set_thread_snoozed(&thread_id, Some(until.timestamp()), None)
                    {
                        tracing::warn!("failed to snooze thread: {e}");
                    }
                }
                self.reload_feed();
                self.snooze_picker_thread = None;
            }
            Message::OpenOverflowMenu {
                thread_id,
                sender_address,
                position,
            } => {
                self.context_menu_thread = None;
                self.overflow_menu_thread = Some(thread_id);
                self.overflow_menu_position = position;
                self.menu_thread_sender = Some(sender_address);
            }
            Message::CloseOverflowMenu => {
                self.overflow_menu_thread = None;
                self.menu_thread_sender = None;
            }
            Message::OpenContextMenu {
                thread_id,
                sender_address,
                position,
            } => {
                self.overflow_menu_thread = None;
                self.context_menu_thread = Some(thread_id);
                self.context_menu_position = position;
                self.menu_thread_sender = Some(sender_address);
            }
            Message::CloseContextMenu => {
                self.context_menu_thread = None;
                self.menu_thread_sender = None;
            }
            Message::NavigateToSettings => {
                self.previous_view = self.active_view;
                self.drawer_was_open = self.drawer_open;
                self.active_view = ActiveView::Settings;
                self.active_nav = NavTarget::View(ActiveView::Settings);
                self.drawer_open = false;
                self.settings_tab = SettingsTab::General;

                // Load current settings from store
                if let Some(ref store) = self.store {
                    let adapter = StoreSettingsAdapter { store };
                    // Theme preference
                    self.theme_preference = adapter
                        .get_setting("theme")
                        .ok()
                        .flatten()
                        .map(|v| match v.as_str() {
                            "light" => ThemePreference::Light,
                            "dark" => ThemePreference::Dark,
                            _ => ThemePreference::System,
                        })
                        .unwrap_or(ThemePreference::System);
                    // Default view
                    self.default_view = adapter
                        .get_setting("default_view")
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "inbox".to_owned());
                    // Undo timeout
                    self.undo_timeout_secs = adapter
                        .get_setting("undo_timeout_secs")
                        .ok()
                        .flatten()
                        .and_then(|v| v.parse::<u32>().ok())
                        .unwrap_or(7);
                    // Shortcuts
                    self.shortcuts = store
                        .get_setting("shortcuts")
                        .ok()
                        .flatten()
                        .map(|json| ShortcutMap::from_overrides_json(&json))
                        .unwrap_or_else(ShortcutMap::defaults);

                    // Notification settings
                    self.notifications_enabled = store
                        .get_setting("notifications_enabled")
                        .ok()
                        .flatten()
                        .map(|v| v != "false")
                        .unwrap_or(true);
                    self.notification_sound = store
                        .get_setting("notification_sound")
                        .ok()
                        .flatten()
                        .map(|v| v != "false")
                        .unwrap_or(true);
                    self.notification_bundles = store
                        .get_setting("notification_bundles")
                        .ok()
                        .flatten()
                        .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
                        .unwrap_or_else(|| vec!["all".to_string()]);

                    // Bundle settings
                    match store.list_bundle_rows() {
                        Ok(bundles) => self.settings_bundles = bundles,
                        Err(e) => tracing::warn!("failed to load bundles for settings: {e}"),
                    }
                }
                // Load config for snooze presets and accounts
                if let Ok(config) = AppConfig::load() {
                    self.config = config;
                }
            }
            Message::NavigateBack => {
                self.active_view = self.previous_view;
                self.active_nav = NavTarget::View(self.previous_view);
                self.drawer_open = self.drawer_was_open;
            }
            Message::SettingsTabChanged(tab) => {
                self.settings_tab = tab;
                // Reset edit state when switching tabs
                self.editing_account_index = None;
                self.adding_account = false;
                self.removing_account_index = None;
                self.data_action_status = None;

                // Load sizes when entering Data & Storage tab
                if tab == SettingsTab::DataStorage {
                    if let Ok(config) = AppConfig::load()
                        && let Some(paths) = Paths::resolve_with_config(&config)
                    {
                        self.db_size_display = format_size(
                            paths
                                .database_file()
                                .metadata()
                                .map(|m| m.len())
                                .unwrap_or(0),
                        );
                        self.index_size_display = format_size(dir_size(&paths.search_index_dir()));
                        self.maildir_size_display = format_size(dir_size(&paths.maildir_root()));
                    }
                    // Last sync from store
                    if let Some(ref store) = self.store {
                        self.last_sync_display = store
                            .get_setting("last_full_sync")
                            .ok()
                            .flatten()
                            .unwrap_or_else(|| "Never".to_owned());
                    }
                }
            }

            // -- General tab handlers --
            Message::SetThemePreference(pref) => {
                self.theme_preference = pref;
                // Persist to settings store
                if let Some(ref store) = self.store {
                    let value = match pref {
                        ThemePreference::System => "system",
                        ThemePreference::Light => "light",
                        ThemePreference::Dark => "dark",
                    };
                    if let Err(e) = store.set_setting("theme", value) {
                        tracing::warn!("failed to persist theme preference: {e}");
                    }
                }
                // Apply immediately
                self.theme = InboxlyTheme::from_preference(pref);
            }

            Message::SetDefaultView(view) => {
                self.default_view = view.clone();
                if let Some(ref store) = self.store
                    && let Err(e) = store.set_setting("default_view", &view)
                {
                    tracing::warn!("failed to persist default view: {e}");
                }
            }

            Message::SetUndoTimeout(secs) => {
                self.undo_timeout_secs = secs;
                if let Some(ref store) = self.store
                    && let Err(e) = store.set_setting("undo_timeout_secs", &secs.to_string())
                {
                    tracing::warn!("failed to persist undo timeout: {e}");
                }
            }

            Message::SetSnoozeMorningHour(val) => {
                if let Ok(hour) = val.parse::<u8>()
                    && hour <= 23
                {
                    self.config.snooze.morning_hour = hour;
                    if let Err(e) = self.config.save() {
                        tracing::warn!("failed to save config: {e}");
                    }
                }
            }

            Message::SetSnoozeAfternoonHour(val) => {
                if let Ok(hour) = val.parse::<u8>()
                    && hour <= 23
                {
                    self.config.snooze.afternoon_hour = hour;
                    if let Err(e) = self.config.save() {
                        tracing::warn!("failed to save config: {e}");
                    }
                }
            }

            Message::SetSnoozeEveningHour(val) => {
                if let Ok(hour) = val.parse::<u8>()
                    && hour <= 23
                {
                    self.config.snooze.evening_hour = hour;
                    if let Err(e) = self.config.save() {
                        tracing::warn!("failed to save config: {e}");
                    }
                }
            }

            Message::SetSnoozeWeekendDay(val) => {
                if let Ok(day) = val.parse::<u8>()
                    && day <= 6
                {
                    self.config.snooze.weekend_day = day;
                    if let Err(e) = self.config.save() {
                        tracing::warn!("failed to save config: {e}");
                    }
                }
            }

            // -- Accounts tab handlers --
            Message::AddAccountStart => {
                self.adding_account = true;
                self.editing_account_index = None;
                self.removing_account_index = None;
                self.account_form = new_empty_account_form();
            }

            Message::EditAccountStart(index) => {
                if let Some(account) = self.config.accounts.get(index) {
                    self.account_form = account.clone();
                    self.editing_account_index = Some(index);
                    self.adding_account = false;
                    self.removing_account_index = None;
                }
            }

            Message::AccountFormCancel => {
                self.editing_account_index = None;
                self.adding_account = false;
            }

            Message::AccountFormSave => {
                // Validate form
                let form = &self.account_form;
                if form.email.is_empty()
                    || !form.email.contains('@')
                    || form.imap_host.is_empty()
                    || form.smtp_host.is_empty()
                {
                    // Invalid -- do nothing (UI should show inline validation hints)
                    return;
                }

                if self.adding_account {
                    self.config.accounts.push(self.account_form.clone());
                } else if let Some(index) = self.editing_account_index
                    && let Some(account) = self.config.accounts.get_mut(index)
                {
                    *account = self.account_form.clone();
                }

                if let Err(e) = self.config.save() {
                    tracing::warn!("failed to save config after account update: {e}");
                }
                self.editing_account_index = None;
                self.adding_account = false;
            }

            Message::AccountFormEmailChanged(v) => self.account_form.email = v,
            Message::AccountFormDisplayNameChanged(v) => self.account_form.display_name = v,
            Message::AccountFormProviderChanged(v) => self.account_form.provider = v,
            Message::AccountFormAuthMethodChanged(v) => {
                self.account_form.auth_method = match v.as_str() {
                    "oauth2" => AuthMethod::OAuth2,
                    "app_password" => AuthMethod::AppPassword,
                    _ => AuthMethod::Password,
                };
            }
            Message::AccountFormImapHostChanged(v) => self.account_form.imap_host = v,
            Message::AccountFormImapPortChanged(v) => {
                if let Ok(port) = v.parse::<u16>() {
                    self.account_form.imap_port = port;
                }
            }
            Message::AccountFormSmtpHostChanged(v) => self.account_form.smtp_host = v,
            Message::AccountFormSmtpPortChanged(v) => {
                if let Ok(port) = v.parse::<u16>() {
                    self.account_form.smtp_port = port;
                }
            }

            Message::RemoveAccountConfirm(index) => {
                self.removing_account_index = Some(index);
            }

            Message::RemoveAccountCancel => {
                self.removing_account_index = None;
            }

            Message::RemoveAccountExecute(index) => {
                // Prevent removing the active account
                if index == self.active_account_index {
                    tracing::warn!(
                        "cannot remove the active account (index {index}); switch accounts first"
                    );
                } else if index < self.config.accounts.len() {
                    self.config.accounts.remove(index);
                    // Adjust active_account_index if needed
                    if self.active_account_index > index {
                        self.active_account_index = self.active_account_index.saturating_sub(1);
                    }
                    if let Err(e) = self.config.save() {
                        tracing::warn!("failed to save config after account removal: {e}");
                    }
                }
                self.removing_account_index = None;
            }

            // -- Data & Storage tab handlers --
            Message::ClearCache => {
                tracing::info!("cache clear requested");
                if let Ok(config) = AppConfig::load()
                    && let Some(paths) = Paths::resolve_with_config(&config)
                {
                    if paths.cache_dir.exists() {
                        if let Err(e) = std::fs::remove_dir_all(&paths.cache_dir) {
                            tracing::warn!("failed to clear cache: {e}");
                            self.data_action_status = Some(format!("Failed to clear cache: {e}"));
                        } else {
                            let _ = std::fs::create_dir_all(&paths.cache_dir);
                            self.data_action_status = Some("Cache cleared".to_owned());
                        }
                    } else {
                        self.data_action_status = Some("No cache to clear".to_owned());
                    }
                }
            }

            Message::RebuildSearchIndex => {
                self.data_action_status = Some("Rebuilding search index...".to_owned());
                tracing::info!("search index rebuild requested (stub)");
            }

            Message::ExportData => {
                self.data_action_status = Some("Coming soon".to_owned());
                tracing::info!("data export requested (stub)");
            }

            Message::DataSizesLoaded {
                db_size,
                index_size,
                maildir_size,
                last_sync,
            } => {
                self.db_size_display = db_size;
                self.index_size_display = index_size;
                self.maildir_size_display = maildir_size;
                self.last_sync_display = last_sync;
            }

            // -- Keyboard shortcuts handlers --
            Message::ShortcutsLoaded(map) => {
                self.shortcuts = map;
            }
            Message::SetShortcut { action, binding } => {
                self.shortcuts.set(action, binding);
                self.capturing_shortcut = None;
                if let Some(ref store) = self.store {
                    let json = self.shortcuts.to_overrides_json();
                    if let Err(e) = store.set_setting("shortcuts", &json) {
                        tracing::warn!("failed to persist shortcuts: {e}");
                    }
                }
            }
            Message::ResetShortcut(action) => {
                self.shortcuts.reset(action);
                if let Some(ref store) = self.store {
                    let json = self.shortcuts.to_overrides_json();
                    if let Err(e) = store.set_setting("shortcuts", &json) {
                        tracing::warn!("failed to persist shortcuts: {e}");
                    }
                }
            }
            Message::StartCapture(action) => {
                self.capturing_shortcut = Some(action);
            }
            Message::CancelCapture => {
                self.capturing_shortcut = None;
            }

            // -- Notification settings handlers --
            Message::ToggleNotifications => {
                self.notifications_enabled = !self.notifications_enabled;
                if let Some(ref store) = self.store {
                    let val = if self.notifications_enabled {
                        "true"
                    } else {
                        "false"
                    };
                    if let Err(e) = store.set_setting("notifications_enabled", val) {
                        tracing::warn!("failed to persist notifications_enabled: {e}");
                    }
                }
            }
            Message::ToggleNotificationSound => {
                self.notification_sound = !self.notification_sound;
                if let Some(ref store) = self.store {
                    let val = if self.notification_sound {
                        "true"
                    } else {
                        "false"
                    };
                    if let Err(e) = store.set_setting("notification_sound", val) {
                        tracing::warn!("failed to persist notification_sound: {e}");
                    }
                }
            }
            Message::SetNotificationBundles(bundles) => {
                self.notification_bundles = bundles;
                if let Some(ref store) = self.store {
                    let json = serde_json::to_string(&self.notification_bundles)
                        .unwrap_or_else(|_| r#"["all"]"#.to_owned());
                    if let Err(e) = store.set_setting("notification_bundles", &json) {
                        tracing::warn!("failed to persist notification_bundles: {e}");
                    }
                }
            }

            // -- Bundle settings handlers --
            Message::BundlesLoaded(bundles) => {
                self.settings_bundles = bundles;
            }
            Message::ToggleBundleVisibility(bundle_id) => {
                if let Some(bundle) = self.settings_bundles.iter_mut().find(|b| b.id == bundle_id) {
                    bundle.visibility = if bundle.visibility == "visible" {
                        "hidden".to_owned()
                    } else {
                        "visible".to_owned()
                    };
                    if let Some(ref store) = self.store
                        && let Err(e) = store.update_bundle_row(bundle)
                    {
                        tracing::warn!("failed to persist bundle visibility: {e}");
                    }
                }
            }
            Message::SetBundleThrottle {
                bundle_id,
                throttle_json,
            } => {
                if let Some(bundle) = self.settings_bundles.iter_mut().find(|b| b.id == bundle_id) {
                    bundle.throttle = throttle_json;
                    if let Some(ref store) = self.store
                        && let Err(e) = store.update_bundle_row(bundle)
                    {
                        tracing::warn!("failed to persist bundle throttle: {e}");
                    }
                }
            }
            Message::ReorderBundles(ids) => {
                // Reassign sort_order and persist each bundle.
                for (order, id) in ids.iter().enumerate() {
                    if let Some(bundle) = self.settings_bundles.iter_mut().find(|b| &b.id == id) {
                        bundle.sort_order = order as i64;
                        if let Some(ref store) = self.store
                            && let Err(e) = store.update_bundle_row(bundle)
                        {
                            tracing::warn!("failed to persist bundle reorder: {e}");
                        }
                    }
                }
                // Re-sort the local list.
                self.settings_bundles.sort_by_key(|b| b.sort_order);
            }
            Message::ToggleThrottlePopup(bundle_id) => {
                self.throttle_popup_bundle_id = bundle_id;
            }

            Message::MoveTo {
                thread_id,
                destination,
            } => {
                tracing::info!("move thread {thread_id} to {destination:?}");
                // Enqueue IMAP move actions for all emails in thread.
                match destination {
                    MoveDestination::Trash => {
                        self.enqueue_thread_actions(
                            &thread_id,
                            |account_id, folder, imap_uid| OfflineAction::MoveToTrash {
                                account_id: account_id.to_string(),
                                folder: folder.to_string(),
                                imap_uid,
                            },
                        );
                    }
                    MoveDestination::Inbox => {
                        let to = "INBOX".to_string();
                        self.enqueue_thread_actions(
                            &thread_id,
                            |account_id, folder, imap_uid| OfflineAction::MoveToFolder {
                                account_id: account_id.to_string(),
                                from_folder: folder.to_string(),
                                to_folder: to.clone(),
                                imap_uid,
                            },
                        );
                    }
                    MoveDestination::Spam => {
                        let to = "Spam".to_string();
                        self.enqueue_thread_actions(
                            &thread_id,
                            |account_id, folder, imap_uid| OfflineAction::MoveToFolder {
                                account_id: account_id.to_string(),
                                from_folder: folder.to_string(),
                                to_folder: to.clone(),
                                imap_uid,
                            },
                        );
                    }
                }
                self.close_menus();
            }
            Message::MarkReadState { thread_id, read } => {
                tracing::info!("mark thread {thread_id} read={read}");
                // Enqueue IMAP read/unread actions for all emails in thread.
                if read {
                    self.enqueue_thread_actions(
                        &thread_id,
                        |account_id, folder, imap_uid| OfflineAction::MarkRead {
                            account_id: account_id.to_string(),
                            folder: folder.to_string(),
                            imap_uid,
                        },
                    );
                } else {
                    self.enqueue_thread_actions(
                        &thread_id,
                        |account_id, folder, imap_uid| OfflineAction::MarkUnread {
                            account_id: account_id.to_string(),
                            folder: folder.to_string(),
                            imap_uid,
                        },
                    );
                }
                self.close_menus();
            }
            Message::MuteThread(thread_id) => {
                tracing::info!("mute thread {thread_id}");
                self.close_menus();
            }
            Message::Reply(thread_id) => {
                tracing::info!("reply to thread {thread_id}");
                self.close_menus();
            }
            Message::ReplyAll(thread_id) => {
                tracing::info!("reply all to thread {thread_id}");
                self.close_menus();
            }
            Message::Forward(thread_id) => {
                tracing::info!("forward thread {thread_id}");
                self.close_menus();
            }
            Message::AddToBundle {
                thread_id,
                category,
            } => {
                tracing::info!("add thread {thread_id} to bundle {category}");
                self.close_menus();
            }
            Message::CreateRuleFromSender(sender) => {
                tracing::info!("create rule from sender: {sender} (coming soon)");
                self.close_menus();
            }
            Message::BlockSender {
                thread_id,
                sender_address,
            } => {
                tracing::info!("block sender {sender_address} (thread {thread_id})");
                self.close_menus();
            }
            Message::ReportSpam(thread_id) => {
                tracing::info!("report spam: thread {thread_id}");
                // Enqueue IMAP move-to-spam actions for all emails in thread.
                let spam_folder = "Spam".to_string();
                self.enqueue_thread_actions(
                    &thread_id,
                    |account_id, folder, imap_uid| OfflineAction::MoveToFolder {
                        account_id: account_id.to_string(),
                        from_folder: folder.to_string(),
                        to_folder: spam_folder.clone(),
                        imap_uid,
                    },
                );
                self.close_menus();
            }
            Message::ToggleBundleExpand(id) => {
                if self.expanded_bundles.contains(&id) {
                    self.expanded_bundles.remove(&id);
                } else {
                    self.expanded_bundles.insert(id);
                }
            }
            Message::OpenSnoozePicker {
                thread_id,
                position,
            } => {
                self.close_menus();
                self.snooze_picker_thread = Some(thread_id);
                self.snooze_picker_position = position;
            }
            Message::CloseSnoozePicker => {
                self.snooze_picker_thread = None;
            }
        }
    }

    /// Window title.
    pub fn title(&self) -> String {
        format!("Inboxly -- {}", self.active_view.title())
    }

    /// Reload the feed from the store (synchronous, fast).
    fn reload_feed(&mut self) {
        if let Some(ref store) = self.store {
            match feed::build_feed(store) {
                Ok(sections) => self.feed_sections = sections,
                Err(e) => {
                    tracing::warn!("failed to load inbox feed: {e}");
                    self.feed_sections = Vec::new();
                }
            }
        }
        // Prune expanded bundles that are no longer in the feed.
        let active_bundle_ids: HashSet<String> = self
            .feed_sections
            .iter()
            .flat_map(|s| s.items.iter())
            .filter_map(|e| match e {
                crate::feed::FeedEntry::Bundle(b) => Some(b.bundle_id.clone()),
                _ => None,
            })
            .collect();
        self.expanded_bundles
            .retain(|id| active_bundle_ids.contains(id));
    }

    /// Clear both menus (overflow + context) and their shared sender field.
    ///
    /// Every message handler that resolves a thread action should call this
    /// so the three-field menu-state invariant stays self-enforcing — new
    /// handlers can't accidentally clear only two of the three fields.
    fn close_menus(&mut self) {
        self.overflow_menu_thread = None;
        self.context_menu_thread = None;
        self.menu_thread_sender = None;
    }

    /// Enqueue offline actions for all emails in a thread.
    ///
    /// Looks up every email belonging to `thread_id` and calls `make_action`
    /// for each one, serialising the resulting [`OfflineAction`] into the
    /// SQLite offline queue.  The entire batch is wrapped in a transaction
    /// for atomicity.
    fn enqueue_thread_actions(
        &self,
        thread_id: &str,
        make_action: impl Fn(&str, &str, u32) -> OfflineAction,
    ) {
        let Some(ref store) = self.store else { return };
        let emails = match store.get_emails_by_thread(thread_id) {
            Ok(emails) => emails,
            Err(e) => {
                tracing::warn!("failed to get emails for thread {thread_id}: {e}");
                return;
            }
        };
        if emails.is_empty() {
            tracing::warn!("no emails found for thread {thread_id}");
            return;
        }
        // Wrap in a transaction for atomicity.
        let conn = store.connection();
        if let Err(e) = conn.execute_batch("BEGIN") {
            tracing::warn!("failed to begin transaction for offline queue: {e}");
            return;
        }
        for email in &emails {
            let uid = email.imap_uid as u32;
            let action = make_action(&email.account_id, &email.imap_folder, uid);
            let payload = serde_json::to_string(&action).expect("serialize OfflineAction");
            if let Err(e) = store.enqueue_offline_action(action.variant_name(), &payload) {
                tracing::warn!("failed to enqueue offline action: {e}");
                let _ = conn.execute_batch("ROLLBACK");
                return;
            }
        }
        if let Err(e) = conn.execute_batch("COMMIT") {
            tracing::warn!("failed to commit offline queue transaction: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::NavSection;

    #[test]
    fn default_state_is_inbox() {
        let app = Inboxly::default();
        assert_eq!(app.active_view, ActiveView::Inbox);
        assert_eq!(app.active_nav, NavTarget::View(ActiveView::Inbox));
        assert!(app.drawer_open);
    }

    #[test]
    fn navigate_to_snoozed_changes_view_and_nav() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::Navigate(NavTarget::View(ActiveView::Snoozed)));
        assert_eq!(app.active_view, ActiveView::Snoozed);
        assert_eq!(app.active_nav, NavTarget::View(ActiveView::Snoozed));
    }

    #[test]
    fn navigate_to_done_changes_view_and_nav() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::Navigate(NavTarget::View(ActiveView::Done)));
        assert_eq!(app.active_view, ActiveView::Done);
        assert_eq!(app.active_nav, NavTarget::View(ActiveView::Done));
    }

    #[test]
    fn navigate_to_section_does_not_change_active_view() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::Navigate(NavTarget::Section(NavSection::Drafts)));
        assert_eq!(app.active_view, ActiveView::Inbox);
        assert_eq!(app.active_nav, NavTarget::Section(NavSection::Drafts));
    }

    #[test]
    fn navigate_to_bundle_category_does_not_change_active_view() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::Navigate(NavTarget::BundleCategory(
            "Social".into(),
        )));
        assert_eq!(app.active_view, ActiveView::Inbox);
        assert_eq!(app.active_nav, NavTarget::BundleCategory("Social".into()));
    }

    #[test]
    fn toggle_drawer() {
        let mut app = Inboxly::default();
        assert!(app.drawer_open);
        let _ = app.update(Message::ToggleDrawer);
        assert!(!app.drawer_open);
        let _ = app.update(Message::ToggleDrawer);
        assert!(app.drawer_open);
    }

    #[test]
    fn navigate_back_to_inbox_from_done() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::Navigate(NavTarget::View(ActiveView::Done)));
        assert_eq!(app.active_view, ActiveView::Done);
        let _ = app.update(Message::Navigate(NavTarget::View(ActiveView::Inbox)));
        assert_eq!(app.active_view, ActiveView::Inbox);
        assert_eq!(app.active_nav, NavTarget::View(ActiveView::Inbox));
    }

    #[test]
    fn toolbar_color_changes_with_view() {
        let inbox_color = ActiveView::Inbox.toolbar_color();
        let snoozed_color = ActiveView::Snoozed.toolbar_color();
        let done_color = ActiveView::Done.toolbar_color();

        assert_ne!(inbox_color, snoozed_color);
        assert_ne!(inbox_color, done_color);
        assert_ne!(snoozed_color, done_color);
    }

    #[test]
    fn view_titles() {
        assert_eq!(ActiveView::Inbox.title(), "Inbox");
        assert_eq!(ActiveView::Snoozed.title(), "Snoozed");
        assert_eq!(ActiveView::Done.title(), "Done");
        assert_eq!(ActiveView::Settings.title(), "Settings");
    }

    #[test]
    fn nav_section_labels() {
        assert_eq!(NavSection::Drafts.label(), "Drafts");
        assert_eq!(NavSection::Sent.label(), "Sent");
        assert_eq!(NavSection::Reminders.label(), "Reminders");
        assert_eq!(NavSection::Trash.label(), "Trash");
        assert_eq!(NavSection::Spam.label(), "Spam");
    }

    #[test]
    fn default_bundle_categories_has_eight_entries() {
        let cats = crate::nav::default_bundle_categories();
        assert_eq!(cats.len(), 8);
        assert_eq!(cats[0].name, "Social");
        assert_eq!(cats[7].name, "Low Priority");
    }

    // -- M16 theme tests --

    #[test]
    fn default_theme_is_light() {
        let app = Inboxly::default();
        assert!(!app.theme.colors.is_dark);
    }

    #[test]
    fn theme_toggle_changes_to_dark() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::ThemeToggled);
        assert!(app.theme.colors.is_dark);
    }

    #[test]
    fn theme_toggle_back_to_light() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::ThemeToggled);
        let _ = app.update(Message::ThemeToggled);
        assert!(!app.theme.colors.is_dark);
    }

    #[test]
    fn theme_changed_message_updates_theme() {
        let mut app = Inboxly::default();
        let dark = InboxlyTheme::dark();
        let _ = app.update(Message::ThemeChanged(dark));
        assert!(app.theme.colors.is_dark);
    }

    // -- M17 feed tests --

    #[test]
    fn default_feed_is_empty() {
        let app = Inboxly::default();
        assert!(app.feed_sections.is_empty());
    }

    #[test]
    fn reload_feed_with_store() {
        let store = Store::open_in_memory().expect("in-memory store");
        let app = Inboxly::with_store(store);
        assert!(app.feed_sections.is_empty());
    }

    #[test]
    fn reload_feed_message_does_not_panic() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::ReloadFeed);
        assert!(app.feed_sections.is_empty());
    }

    // -- M27 Settings nav, toolbar colour, FeedItem tests --

    #[test]
    fn navigate_to_settings_stores_previous_view() {
        let mut app = Inboxly::default();
        app.active_view = ActiveView::Snoozed;
        let _ = app.update(Message::NavigateToSettings);
        assert_eq!(app.active_view, ActiveView::Settings);
        assert_eq!(app.previous_view, ActiveView::Snoozed);
        assert!(!app.drawer_open);
    }

    #[test]
    fn navigate_back_from_settings_restores_view() {
        let mut app = Inboxly::default();
        app.active_view = ActiveView::Done;
        let _ = app.update(Message::NavigateToSettings);
        let _ = app.update(Message::NavigateBack);
        assert_eq!(app.active_view, ActiveView::Done);
        assert!(app.drawer_open);
    }

    #[test]
    fn settings_toolbar_distinct_from_all_views() {
        let settings_color = ActiveView::Settings.toolbar_color();
        assert_ne!(settings_color, ActiveView::Inbox.toolbar_color());
        assert_ne!(settings_color, ActiveView::Snoozed.toolbar_color());
        assert_ne!(settings_color, ActiveView::Done.toolbar_color());
    }

    // -- M27 menu state transition tests --

    #[test]
    fn open_overflow_menu_sets_state() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenOverflowMenu {
            thread_id: "t1".into(),
            sender_address: "a@b.com".into(),
            position: Point::ORIGIN,
        });
        assert_eq!(app.overflow_menu_thread, Some("t1".into()));
    }

    #[test]
    fn open_overflow_menu_sets_position_and_sender() {
        let mut app = Inboxly::default();
        let pos = Point::new(42.0, 100.0);
        let _ = app.update(Message::OpenOverflowMenu {
            thread_id: "t1".into(),
            sender_address: "sender@example.com".into(),
            position: pos,
        });
        assert_eq!(app.overflow_menu_thread, Some("t1".into()));
        assert_eq!(app.overflow_menu_position, pos);
        assert_eq!(app.menu_thread_sender, Some("sender@example.com".into()));
    }

    #[test]
    fn open_context_menu_sets_sender() {
        let mut app = Inboxly::default();
        let pos = Point::new(10.0, 20.0);
        let _ = app.update(Message::OpenContextMenu {
            thread_id: "t2".into(),
            sender_address: "ctx@example.com".into(),
            position: pos,
        });
        assert_eq!(app.context_menu_thread, Some("t2".into()));
        assert_eq!(app.context_menu_position, pos);
        assert_eq!(app.menu_thread_sender, Some("ctx@example.com".into()));
    }

    #[test]
    fn close_overflow_menu_clears_state() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenOverflowMenu {
            thread_id: "t1".into(),
            sender_address: "a@b.com".into(),
            position: Point::ORIGIN,
        });
        let _ = app.update(Message::CloseOverflowMenu);
        assert!(app.overflow_menu_thread.is_none());
    }

    #[test]
    fn close_overflow_menu_clears_sender() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenOverflowMenu {
            thread_id: "t1".into(),
            sender_address: "a@b.com".into(),
            position: Point::ORIGIN,
        });
        let _ = app.update(Message::CloseOverflowMenu);
        assert!(app.menu_thread_sender.is_none());
    }

    #[test]
    fn close_context_menu_clears_sender() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenContextMenu {
            thread_id: "t1".into(),
            sender_address: "a@b.com".into(),
            position: Point::ORIGIN,
        });
        let _ = app.update(Message::CloseContextMenu);
        assert!(app.context_menu_thread.is_none());
        assert!(app.menu_thread_sender.is_none());
    }

    #[test]
    fn open_context_menu_closes_overflow() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenOverflowMenu {
            thread_id: "t1".into(),
            sender_address: "a@b.com".into(),
            position: Point::ORIGIN,
        });
        let _ = app.update(Message::OpenContextMenu {
            thread_id: "t2".into(),
            sender_address: "ctx@b.com".into(),
            position: Point::new(100.0, 200.0),
        });
        assert!(app.overflow_menu_thread.is_none());
        assert_eq!(app.context_menu_thread, Some("t2".into()));
    }

    #[test]
    fn opening_context_menu_clears_overflow_sender() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenOverflowMenu {
            thread_id: "t1".into(),
            sender_address: "overflow@b.com".into(),
            position: Point::ORIGIN,
        });
        let _ = app.update(Message::OpenContextMenu {
            thread_id: "t2".into(),
            sender_address: "ctx@b.com".into(),
            position: Point::new(5.0, 10.0),
        });
        assert!(app.overflow_menu_thread.is_none());
        // sender reflects the context menu opener, not the overflow one
        assert_eq!(app.menu_thread_sender, Some("ctx@b.com".into()));
    }

    #[test]
    fn open_overflow_closes_context_menu() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenContextMenu {
            thread_id: "t1".into(),
            sender_address: "a@b.com".into(),
            position: Point::ORIGIN,
        });
        let _ = app.update(Message::OpenOverflowMenu {
            thread_id: "t2".into(),
            sender_address: "b@b.com".into(),
            position: Point::ORIGIN,
        });
        assert!(app.context_menu_thread.is_none());
        assert_eq!(app.overflow_menu_thread, Some("t2".into()));
        assert_eq!(app.menu_thread_sender, Some("b@b.com".into()));
    }

    #[test]
    fn thread_actions_close_menus() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenOverflowMenu {
            thread_id: "t1".into(),
            sender_address: "a@b.com".into(),
            position: Point::ORIGIN,
        });
        let _ = app.update(Message::MoveTo {
            thread_id: "t1".into(),
            destination: MoveDestination::Inbox,
        });
        assert!(app.overflow_menu_thread.is_none());
        assert!(app.menu_thread_sender.is_none());
    }

    #[test]
    fn thread_actions_do_not_panic() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::MoveTo {
            thread_id: "t1".into(),
            destination: MoveDestination::Trash,
        });
        let _ = app.update(Message::MarkReadState {
            thread_id: "t1".into(),
            read: true,
        });
        let _ = app.update(Message::MuteThread("t1".into()));
        let _ = app.update(Message::Reply("t1".into()));
        let _ = app.update(Message::ReplyAll("t1".into()));
        let _ = app.update(Message::Forward("t1".into()));
        let _ = app.update(Message::AddToBundle {
            thread_id: "t1".into(),
            category: "Social".into(),
        });
        let _ = app.update(Message::CreateRuleFromSender("a@b.com".into()));
        let _ = app.update(Message::BlockSender {
            thread_id: "t1".into(),
            sender_address: "a@b.com".into(),
        });
        let _ = app.update(Message::ReportSpam("t1".into()));
    }

    // -- M33 Phase 7B: menu action tests --

    #[test]
    fn context_menu_reply_dispatches_reply_and_closes() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenContextMenu {
            thread_id: "t1".into(),
            sender_address: "a@b.com".into(),
            position: Point::ORIGIN,
        });
        let _ = app.update(Message::Reply("t1".into()));
        assert!(app.context_menu_thread.is_none());
        assert!(app.menu_thread_sender.is_none());
    }

    #[test]
    fn overflow_menu_mark_read_closes_menu() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenOverflowMenu {
            thread_id: "t1".into(),
            sender_address: "a@b.com".into(),
            position: Point::ORIGIN,
        });
        let _ = app.update(Message::MarkReadState {
            thread_id: "t1".into(),
            read: true,
        });
        assert!(app.overflow_menu_thread.is_none());
        assert!(app.menu_thread_sender.is_none());
    }

    #[test]
    fn context_menu_add_to_bundle_closes_menu() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenContextMenu {
            thread_id: "t1".into(),
            sender_address: "a@b.com".into(),
            position: Point::ORIGIN,
        });
        let _ = app.update(Message::AddToBundle {
            thread_id: "t1".into(),
            category: "Social".into(),
        });
        assert!(app.context_menu_thread.is_none());
        assert!(app.menu_thread_sender.is_none());
    }

    #[test]
    fn context_menu_block_sender_uses_menu_sender() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenContextMenu {
            thread_id: "t1".into(),
            sender_address: "menu@sender.com".into(),
            position: Point::ORIGIN,
        });
        // Verify state captured the sender correctly BEFORE BlockSender fires.
        assert_eq!(app.menu_thread_sender, Some("menu@sender.com".into()));
        let _ = app.update(Message::BlockSender {
            thread_id: "t1".into(),
            sender_address: "menu@sender.com".into(),
        });
        assert!(app.context_menu_thread.is_none());
        assert!(app.menu_thread_sender.is_none());
    }

    // -- M28 account switcher data layer tests --

    fn make_test_account(email: &str, display_name: &str) -> inboxly_core::config::AccountConfig {
        inboxly_core::config::AccountConfig {
            email: email.to_string(),
            display_name: display_name.to_string(),
            provider: "generic".to_string(),
            auth_method: inboxly_core::config::AuthMethod::Password,
            imap_host: "imap.example.com".to_string(),
            imap_port: 993,
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
        }
    }

    fn make_test_account_no_name(email: &str) -> inboxly_core::config::AccountConfig {
        inboxly_core::config::AccountConfig {
            email: email.to_string(),
            display_name: String::new(),
            provider: "generic".to_string(),
            auth_method: inboxly_core::config::AuthMethod::Password,
            imap_host: "imap.example.com".to_string(),
            imap_port: 993,
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
        }
    }

    #[test]
    fn toggle_account_switcher() {
        let mut app = Inboxly::default();
        assert!(!app.account_switcher_open);
        let _ = app.update(Message::ToggleAccountSwitcher);
        assert!(app.account_switcher_open);
        let _ = app.update(Message::ToggleAccountSwitcher);
        assert!(!app.account_switcher_open);
    }

    #[test]
    fn switch_account_out_of_bounds_is_noop() {
        let mut app = Inboxly::default();
        assert_eq!(app.active_account_index, 0);
        let _ = app.update(Message::SwitchAccount(5));
        assert_eq!(app.active_account_index, 0);
    }

    #[test]
    fn active_email_with_no_accounts() {
        let app = Inboxly::default();
        assert_eq!(app.active_email(), "No account");
    }

    #[test]
    fn navigate_closes_account_switcher() {
        let mut app = Inboxly::default();
        app.account_switcher_open = true;
        let _ = app.update(Message::Navigate(NavTarget::View(ActiveView::Inbox)));
        assert!(!app.account_switcher_open);
    }

    #[test]
    fn switch_account_changes_active_index() {
        let mut app = Inboxly::default();
        app.accounts = vec![
            make_test_account("first@example.com", "First"),
            make_test_account("second@example.com", "Second"),
        ];
        let _ = app.update(Message::SwitchAccount(1));
        assert_eq!(app.active_account_index, 1);
        assert_eq!(app.active_email(), "second@example.com");
    }

    #[test]
    fn switch_account_closes_switcher() {
        let mut app = Inboxly::default();
        app.accounts = vec![make_test_account("test@example.com", "Test")];
        app.account_switcher_open = true;
        let _ = app.update(Message::SwitchAccount(0));
        assert!(!app.account_switcher_open);
    }

    #[test]
    fn active_display_name_falls_back_to_email() {
        let mut app = Inboxly::default();
        app.accounts = vec![make_test_account_no_name("test@example.com")];
        assert_eq!(app.active_display_name(), "test@example.com");
    }

    #[test]
    fn with_accounts_sets_accounts() {
        let accounts = vec![make_test_account("test@example.com", "Test")];
        let app = Inboxly::with_accounts(accounts);
        assert_eq!(app.accounts.len(), 1);
        assert_eq!(app.active_email(), "test@example.com");
    }

    // -- M29 Settings state management tests --

    #[test]
    fn open_settings_changes_view_and_hides_drawer() {
        let mut app = Inboxly::default();
        assert!(app.drawer_open);
        let _ = app.update(Message::NavigateToSettings);
        assert_eq!(app.active_view, ActiveView::Settings);
        assert!(!app.drawer_open);
    }

    #[test]
    fn close_settings_restores_previous_view() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::Navigate(NavTarget::View(ActiveView::Done)));
        let _ = app.update(Message::NavigateToSettings);
        assert_eq!(app.active_view, ActiveView::Settings);
        let _ = app.update(Message::NavigateBack);
        assert_eq!(app.active_view, ActiveView::Done);
    }

    #[test]
    fn close_settings_restores_drawer_state() {
        let mut app = Inboxly::default();
        // Start with drawer closed
        app.drawer_open = false;
        let _ = app.update(Message::NavigateToSettings);
        assert!(!app.drawer_open);
        let _ = app.update(Message::NavigateBack);
        // Restores to false (was closed before settings)
        assert!(!app.drawer_open);

        // Now with drawer open
        app.drawer_open = true;
        let _ = app.update(Message::NavigateToSettings);
        assert!(!app.drawer_open);
        let _ = app.update(Message::NavigateBack);
        assert!(app.drawer_open);
    }

    #[test]
    fn settings_tab_defaults_to_general() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::NavigateToSettings);
        assert_eq!(app.settings_tab, SettingsTab::General);
    }

    #[test]
    fn settings_tab_change() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::NavigateToSettings);
        let _ = app.update(Message::SettingsTabChanged(SettingsTab::Accounts));
        assert_eq!(app.settings_tab, SettingsTab::Accounts);
    }

    #[test]
    fn settings_tab_change_resets_edit_state() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::NavigateToSettings);
        // Set some edit state
        app.adding_account = true;
        app.editing_account_index = Some(0);
        app.removing_account_index = Some(1);
        app.data_action_status = Some("test".to_owned());
        // Switch tab
        let _ = app.update(Message::SettingsTabChanged(SettingsTab::General));
        assert!(!app.adding_account);
        assert!(app.editing_account_index.is_none());
        assert!(app.removing_account_index.is_none());
        assert!(app.data_action_status.is_none());
    }

    #[test]
    fn add_account_start_opens_form() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::AddAccountStart);
        assert!(app.adding_account);
        assert!(app.editing_account_index.is_none());
        assert!(app.removing_account_index.is_none());
    }

    #[test]
    fn account_form_cancel_resets_state() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::AddAccountStart);
        assert!(app.adding_account);
        let _ = app.update(Message::AccountFormCancel);
        assert!(!app.adding_account);
        assert!(app.editing_account_index.is_none());
    }

    #[test]
    fn account_form_field_updates() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::AccountFormEmailChanged("test@example.com".into()));
        assert_eq!(app.account_form.email, "test@example.com");

        let _ = app.update(Message::AccountFormDisplayNameChanged("Test User".into()));
        assert_eq!(app.account_form.display_name, "Test User");

        let _ = app.update(Message::AccountFormProviderChanged("gmail".into()));
        assert_eq!(app.account_form.provider, "gmail");

        let _ = app.update(Message::AccountFormAuthMethodChanged("oauth2".into()));
        assert_eq!(
            app.account_form.auth_method,
            inboxly_core::config::AuthMethod::OAuth2
        );

        let _ = app.update(Message::AccountFormImapHostChanged("imap.gmail.com".into()));
        assert_eq!(app.account_form.imap_host, "imap.gmail.com");

        let _ = app.update(Message::AccountFormImapPortChanged("143".into()));
        assert_eq!(app.account_form.imap_port, 143);

        let _ = app.update(Message::AccountFormSmtpHostChanged("smtp.gmail.com".into()));
        assert_eq!(app.account_form.smtp_host, "smtp.gmail.com");

        let _ = app.update(Message::AccountFormSmtpPortChanged("465".into()));
        assert_eq!(app.account_form.smtp_port, 465);
    }

    #[test]
    fn remove_account_confirm_and_cancel() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::RemoveAccountConfirm(1));
        assert_eq!(app.removing_account_index, Some(1));
        let _ = app.update(Message::RemoveAccountCancel);
        assert!(app.removing_account_index.is_none());
    }

    #[test]
    fn undo_timeout_values() {
        let mut app = Inboxly::default();
        for secs in [3, 5, 7, 10, 15] {
            let _ = app.update(Message::SetUndoTimeout(secs));
            assert_eq!(app.undo_timeout_secs, secs);
        }
    }

    #[test]
    fn format_size_helper() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    // -- M30 Batch 2: Notifications, bundles, and shortcuts settings tests --

    #[test]
    fn toggle_notifications() {
        let mut app = Inboxly::default();
        assert!(app.notifications_enabled);
        let _ = app.update(Message::ToggleNotifications);
        assert!(!app.notifications_enabled);
        let _ = app.update(Message::ToggleNotifications);
        assert!(app.notifications_enabled);
    }

    #[test]
    fn toggle_notification_sound() {
        let mut app = Inboxly::default();
        assert!(app.notification_sound);
        let _ = app.update(Message::ToggleNotificationSound);
        assert!(!app.notification_sound);
        let _ = app.update(Message::ToggleNotificationSound);
        assert!(app.notification_sound);
    }

    #[test]
    fn set_notification_bundles() {
        let mut app = Inboxly::default();
        assert_eq!(app.notification_bundles, vec!["all".to_string()]);

        let _ = app.update(Message::SetNotificationBundles(vec![
            "Social".to_string(),
            "Finance".to_string(),
        ]));
        assert_eq!(app.notification_bundles.len(), 2);
        assert!(app.notification_bundles.contains(&"Social".to_string()));
        assert!(app.notification_bundles.contains(&"Finance".to_string()));

        let _ = app.update(Message::SetNotificationBundles(vec!["primary".to_string()]));
        assert_eq!(app.notification_bundles, vec!["primary".to_string()]);
    }

    #[test]
    fn toggle_bundle_visibility() {
        let mut app = Inboxly::default();
        app.settings_bundles.push(BundleRow {
            id: "b1".to_string(),
            category: "Social".to_string(),
            name: "Social".to_string(),
            color: "#1DA1F2".to_string(),
            badge_color: "#1DA1F2".to_string(),
            visibility: "visible".to_string(),
            throttle: r#"{"mode":"Immediate"}"#.to_string(),
            sort_order: 0,
        });

        let _ = app.update(Message::ToggleBundleVisibility("b1".to_string()));
        assert_eq!(app.settings_bundles[0].visibility, "hidden");

        let _ = app.update(Message::ToggleBundleVisibility("b1".to_string()));
        assert_eq!(app.settings_bundles[0].visibility, "visible");
    }

    #[test]
    fn toggle_bundle_visibility_unknown_id_is_noop() {
        let mut app = Inboxly::default();
        // No bundles -- should not panic
        let _ = app.update(Message::ToggleBundleVisibility("nonexistent".to_string()));
    }

    #[test]
    fn start_capture() {
        let mut app = Inboxly::default();
        assert!(app.capturing_shortcut.is_none());
        let _ = app.update(Message::StartCapture(ShortcutAction::Done));
        assert_eq!(app.capturing_shortcut, Some(ShortcutAction::Done));
    }

    #[test]
    fn cancel_capture() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::StartCapture(ShortcutAction::Done));
        assert!(app.capturing_shortcut.is_some());
        let _ = app.update(Message::CancelCapture);
        assert!(app.capturing_shortcut.is_none());
    }

    #[test]
    fn set_shortcut() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::StartCapture(ShortcutAction::Done));
        let _ = app.update(Message::SetShortcut {
            action: ShortcutAction::Done,
            binding: "d".to_string(),
        });
        assert_eq!(app.shortcuts.get(ShortcutAction::Done), "d");
        assert!(app.capturing_shortcut.is_none());
        assert!(app.shortcuts.is_customised(ShortcutAction::Done));
    }

    #[test]
    fn reset_shortcut() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::SetShortcut {
            action: ShortcutAction::Done,
            binding: "d".to_string(),
        });
        assert_eq!(app.shortcuts.get(ShortcutAction::Done), "d");

        let _ = app.update(Message::ResetShortcut(ShortcutAction::Done));
        assert_eq!(app.shortcuts.get(ShortcutAction::Done), "e"); // default
        assert!(!app.shortcuts.is_customised(ShortcutAction::Done));
    }

    #[test]
    fn bundles_loaded_replaces_list() {
        let mut app = Inboxly::default();
        assert!(app.settings_bundles.is_empty());

        let bundles = vec![
            BundleRow {
                id: "b1".to_string(),
                category: "Social".to_string(),
                name: "Social".to_string(),
                color: "#1DA1F2".to_string(),
                badge_color: "#1DA1F2".to_string(),
                visibility: "visible".to_string(),
                throttle: r#"{"mode":"Immediate"}"#.to_string(),
                sort_order: 0,
            },
            BundleRow {
                id: "b2".to_string(),
                category: "Finance".to_string(),
                name: "Finance".to_string(),
                color: "#0f9d58".to_string(),
                badge_color: "#0f9d58".to_string(),
                visibility: "visible".to_string(),
                throttle: r#"{"mode":"Immediate"}"#.to_string(),
                sort_order: 1,
            },
        ];
        let _ = app.update(Message::BundlesLoaded(bundles));
        assert_eq!(app.settings_bundles.len(), 2);
    }

    #[test]
    fn reorder_bundles() {
        let mut app = Inboxly::default();
        app.settings_bundles = vec![
            BundleRow {
                id: "b1".to_string(),
                category: "Social".to_string(),
                name: "Social".to_string(),
                color: "#1DA1F2".to_string(),
                badge_color: "#1DA1F2".to_string(),
                visibility: "visible".to_string(),
                throttle: r#"{"mode":"Immediate"}"#.to_string(),
                sort_order: 0,
            },
            BundleRow {
                id: "b2".to_string(),
                category: "Finance".to_string(),
                name: "Finance".to_string(),
                color: "#0f9d58".to_string(),
                badge_color: "#0f9d58".to_string(),
                visibility: "visible".to_string(),
                throttle: r#"{"mode":"Immediate"}"#.to_string(),
                sort_order: 1,
            },
        ];

        // Reverse the order
        let _ = app.update(Message::ReorderBundles(vec![
            "b2".to_string(),
            "b1".to_string(),
        ]));
        assert_eq!(app.settings_bundles[0].id, "b2");
        assert_eq!(app.settings_bundles[0].sort_order, 0);
        assert_eq!(app.settings_bundles[1].id, "b1");
        assert_eq!(app.settings_bundles[1].sort_order, 1);
    }

    #[test]
    fn set_bundle_throttle() {
        let mut app = Inboxly::default();
        app.settings_bundles.push(BundleRow {
            id: "b1".to_string(),
            category: "Social".to_string(),
            name: "Social".to_string(),
            color: "#1DA1F2".to_string(),
            badge_color: "#1DA1F2".to_string(),
            visibility: "visible".to_string(),
            throttle: r#"{"mode":"Immediate"}"#.to_string(),
            sort_order: 0,
        });

        let daily_json = r#"{"mode":"Daily","delivery_time":"17:00:00"}"#.to_string();
        let _ = app.update(Message::SetBundleThrottle {
            bundle_id: "b1".to_string(),
            throttle_json: daily_json.clone(),
        });
        assert_eq!(app.settings_bundles[0].throttle, daily_json);
    }

    #[test]
    fn toggle_throttle_popup() {
        let mut app = Inboxly::default();
        assert!(app.throttle_popup_bundle_id.is_none());

        let _ = app.update(Message::ToggleThrottlePopup(Some("b1".to_string())));
        assert_eq!(app.throttle_popup_bundle_id, Some("b1".to_string()));

        let _ = app.update(Message::ToggleThrottlePopup(None));
        assert!(app.throttle_popup_bundle_id.is_none());
    }

    #[test]
    fn shortcuts_loaded_replaces_map() {
        let mut app = Inboxly::default();
        let mut custom_map = ShortcutMap::defaults();
        custom_map.set(ShortcutAction::Done, "x".to_owned());

        let _ = app.update(Message::ShortcutsLoaded(custom_map));
        assert_eq!(app.shortcuts.get(ShortcutAction::Done), "x");
    }

    // -- M33 bundle expand and snooze picker tests --

    #[test]
    fn toggle_bundle_expand_adds_and_removes() {
        let mut app = Inboxly::default();
        assert!(app.expanded_bundles.is_empty());

        // First toggle inserts the bundle ID.
        let _ = app.update(Message::ToggleBundleExpand("b1".into()));
        assert!(app.expanded_bundles.contains("b1"));

        // Second toggle removes it.
        let _ = app.update(Message::ToggleBundleExpand("b1".into()));
        assert!(!app.expanded_bundles.contains("b1"));
    }

    #[test]
    fn mark_done_pushes_undo_state() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::MarkDone("t1".into()));
        // MarkDone should push an undo action (no store in test, DB ops are skipped).
        assert!(app.undo_state.is_active());
        assert_eq!(
            app.undo_state.description().as_deref(),
            Some("Conversation marked done")
        );
    }

    #[test]
    fn toggle_pin_pushes_undo_state() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::TogglePin("t1".into()));
        // TogglePin with no store defaults was_pinned=false → "Pinned".
        assert!(app.undo_state.is_active());
        assert_eq!(app.undo_state.description().as_deref(), Some("Pinned"));
    }

    #[test]
    fn open_snooze_picker_sets_state() {
        let mut app = Inboxly::default();
        let pos = Point::new(42.0, 84.0);
        let _ = app.update(Message::OpenSnoozePicker {
            thread_id: "t1".into(),
            position: pos,
        });
        assert_eq!(app.snooze_picker_thread, Some("t1".into()));
        assert_eq!(app.snooze_picker_position, pos);
    }

    #[test]
    fn close_snooze_picker_clears_state() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenSnoozePicker {
            thread_id: "t1".into(),
            position: Point::new(10.0, 20.0),
        });
        let _ = app.update(Message::CloseSnoozePicker);
        assert!(app.snooze_picker_thread.is_none());
    }

    #[test]
    fn open_snooze_picker_closes_other_menus() {
        let mut app = Inboxly::default();
        // Prime both overflow and context menus.
        let _ = app.update(Message::OpenOverflowMenu {
            thread_id: "t0".into(),
            sender_address: "a@b.com".into(),
            position: Point::ORIGIN,
        });
        let _ = app.update(Message::OpenContextMenu {
            thread_id: "t0".into(),
            sender_address: "a@b.com".into(),
            position: Point::ORIGIN,
        });
        // Opening the snooze picker must close both.
        let _ = app.update(Message::OpenSnoozePicker {
            thread_id: "t1".into(),
            position: Point::new(5.0, 5.0),
        });
        assert!(app.overflow_menu_thread.is_none());
        assert!(app.context_menu_thread.is_none());
        assert_eq!(app.snooze_picker_thread, Some("t1".into()));
    }

    #[test]
    fn snooze_thread_closes_picker() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenSnoozePicker {
            thread_id: "t1".into(),
            position: Point::ORIGIN,
        });
        assert_eq!(app.snooze_picker_thread, Some("t1".into()));
        let _ = app.update(Message::SnoozeThread {
            thread_id: "t1".into(),
            until: chrono::Utc::now() + chrono::Duration::hours(1),
        });
        assert!(app.snooze_picker_thread.is_none(), "SnoozeThread should close the picker");
    }

    #[test]
    fn undo_message_reverses_mark_done() {
        let mut app = Inboxly::default();
        // Push a MarkDone undo action (no store in tests, DB ops are skipped).
        let _ = app.update(Message::MarkDone("t1".into()));
        assert!(app.undo_state.is_active());

        // Dispatching Undo should consume the pending action and clear state.
        let _ = app.update(Message::Undo);
        assert!(!app.undo_state.is_active());
    }

    #[test]
    fn undo_expired_clears_state() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::MarkDone("t1".into()));
        assert!(app.undo_state.is_active());

        // UndoExpired fires when the snackbar timer expires.
        let _ = app.update(Message::UndoExpired);
        assert!(!app.undo_state.is_active());
    }

    #[test]
    fn undo_state_reusable_after_expire() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::MarkDone("t1".into()));
        assert!(app.undo_state.is_active());
        let _ = app.update(Message::UndoExpired);
        assert!(!app.undo_state.is_active());
        let _ = app.update(Message::MarkDone("t2".into()));
        assert!(app.undo_state.is_active());
    }
}
