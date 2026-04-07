//! Core application state machine -- no framework dependencies.

use std::collections::HashSet;
use std::sync::Arc;

use inboxly_core::config::{AccountConfig, AppConfig, AuthMethod, Paths, ThemePreference};
use inboxly_core::offline::OfflineAction;
use inboxly_core::{AttachmentDraft, ComposeMode, Contact, parse_address_list};
use inboxly_store::{BundleRow, Store};

use crate::feed::{self, FeedSection};
use crate::keyboard::{ShortcutAction, ShortcutMap};
use crate::nav::{NavBundleCategory, NavTarget, default_bundle_categories};
use crate::state::{ComposeSendState, ComposeState, MenuState, SettingsState, SnoozeState};
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
    pub store: Option<Arc<Store>>,
    /// Pre-built feed sections for the inbox view.
    pub feed_sections: Vec<FeedSection>,
    /// Intent: which thread the user wants opened. The actual loaded
    /// body data lives in a separate signal at App level (Issue 1.4)
    /// so per-write `Clone` of Inboxly doesn't drag body bytes around.
    pub open_thread_id: Option<String>,
    /// Unified thread reader facade (Issue 1.5). Wraps Store +
    /// MaildirStore so consumers don't need to plumb two handles.
    /// None in M34 since real sync isn't wired yet — the App-level
    /// bridge falls through to fallback_thread() when this is None.
    pub thread_reader: Option<Arc<inboxly_store::thread_reader::ThreadReader>>,
    /// Undo state for timed undo of inbox actions.
    pub undo_state: UndoState,
    /// Overflow + right-click context menu state.
    pub menus: MenuState,

    /// Loaded AppConfig (for accounts + snooze presets editing).
    pub config: AppConfig,

    /// Settings drawer / panel state (tabs, forms, notifications, etc.).
    pub settings: SettingsState,

    // -- Feed interaction state --
    /// Set of bundle IDs that are currently expanded in the inbox feed.
    pub expanded_bundles: HashSet<String>,
    /// Snooze date-picker popup state.
    pub snooze: SnoozeState,

    /// In-progress compose draft state (M35).
    pub compose: ComposeState,
}

/// IMAP folder destinations for the "Move to..." action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveDestination {
    Inbox,
    Trash,
    Spam,
}

/// Identifies which recipient list (To/Cc/Bcc) a Compose recipient action targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipientField {
    /// To header.
    To,
    /// Cc header.
    Cc,
    /// Bcc header (not visible to other recipients).
    Bcc,
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
    /// Open the full thread detail view for a thread ID.
    OpenThread(String),
    /// Close the open thread and return to the inbox feed.
    CloseThread,
    /// Open an external URL in the user's system browser. Dispatched
    /// from the thread detail view's link-click interceptor so
    /// `<a href>` clicks inside email bodies don't navigate the
    /// WebKitGTK webview away from the app (eng review Issue 1.2).
    OpenExternalUrl(String),
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

    // -- Compose (M35) --
    /// Open the compose view with a fresh blank draft. Eagerly assigns
    /// a UUID `draft_id` (Gemini G4) so the per-draft attachment
    /// directory can be created before the first auto-save tick.
    OpenCompose,
    /// M36 placeholder — open compose in reply mode. Logs a warning in
    /// M35b and otherwise does nothing.
    OpenComposeReply {
        /// Thread ID being replied to.
        thread_id: String,
        /// Reply mode (Reply, ReplyAll, Forward).
        mode: ComposeMode,
    },
    /// Close the compose view without sending. Restores `previous_view`
    /// as `active_view` and leaves the compose state intact.
    CloseCompose,
    /// Set the subject line.
    ComposeSubjectChanged(String),
    /// Set the To input field text (chips are added on Enter or comma).
    ComposeToInputChanged(String),
    /// Set the Cc input field text.
    ComposeCcInputChanged(String),
    /// Set the Bcc input field text.
    ComposeBccInputChanged(String),
    /// Add a contact to the recipient list. `field` selects To/Cc/Bcc.
    ComposeAddRecipient {
        /// Which recipient list to append to.
        field: RecipientField,
        /// Resolved contact to add.
        contact: Contact,
    },
    /// Remove a recipient by index.
    ComposeRemoveRecipient {
        /// Which recipient list to mutate.
        field: RecipientField,
        /// Index into the recipient `Vec`.
        index: usize,
    },
    /// Set the body Markdown source.
    ComposeBodyChanged(String),
    /// Toggle Cc/Bcc row visibility (collapsed by default).
    ComposeToggleCcBcc,
    /// Toggle between body textarea and Markdown preview.
    ComposeTogglePreview,
    /// Switch the FROM account (Issue 1.3 — account picker dropdown).
    ComposeFromChanged {
        /// Index into `Inboxly::accounts` selecting the FROM account.
        account_index: usize,
    },
    /// User clicked the attach button. Triggers the rfd file picker
    /// (Phase 11 bridge).
    ComposeAttachFile,
    /// Phase 11 bridge dispatches this with the picked attachment.
    ComposeAttachmentAdded(Arc<AttachmentDraft>),
    /// Phase 11 bridge dispatches this when the picked file would push
    /// total attachment size over the 20 MB limit.
    ComposeAttachmentTooLarge,
    /// Remove an attachment by index.
    ComposeRemoveAttachment(usize),
    /// User clicked Save. Phase 10 bridge handles SQLite/Maildir/IMAP writes.
    ComposeSaveDraft,
    /// Phase 10 bridge dispatches this on a 30 s timer when the draft is
    /// dirty. `generation` is the captured `save_generation` snapshot used
    /// for the stale-result guard (Issue 1.8).
    ComposeAutoSaveTick {
        /// `save_generation` value at the moment the save was triggered.
        generation: u64,
    },
    /// User clicked Send. Phase 12 bridge runs the SMTP pipeline.
    ComposeSendDraft,
    /// Phase 12 bridge dispatches this on send completion.
    ComposeSendComplete {
        /// True if the SMTP send succeeded.
        success: bool,
        /// Failure reason if `success == false`.
        error: Option<String>,
    },
    /// User dismissed the "Sent — dismiss?" overlay (Gemini G9).
    /// Clears compose state and returns to the previous view.
    ComposeDismissSentNotice,
    /// User clicked Discard. Resets compose state and closes the view.
    ComposeDiscardDraft,
}

/// Validate an external URL against M34's scheme allowlist for the
/// `OpenExternalUrl` handler.
///
/// Returns `Ok(())` if the URL parses cleanly and uses an allowed
/// scheme (`http`, `https`, or `mailto`), or `Err(reason)` describing
/// why it was rejected. The handler logs the rejection reason via
/// `tracing::warn!` and drops the URL.
///
/// **Why this is a free function, not a closure inside the handler:**
/// extracting the validation as a pure function lets us unit-test it
/// directly (no `Inboxly::default()`, no `app.update(Message::...)`,
/// no risk of accidentally hitting the side-effect-having
/// `open::that()` call). The post-M34 incident — where the original
/// handler tests dispatched real URLs and overnight test runs spawned
/// ~10 browser + ~10 kmail compose windows — was caused by tests
/// going through the handler. Pure-function tests can never do that.
///
/// Eng review Issue 2.2: defence-in-depth — the sanitiser already
/// strips `javascript:` / `data:` / etc. via ammonia's default scheme
/// allowlist BEFORE the link gets to the JS bridge, but this second
/// validation layer protects against future ammonia upgrades or
/// edge cases in the sentinel-rewrite logic that might let an unsafe
/// scheme through.
///
/// The leading `::` on `::url::Url` disambiguates the `url` crate from
/// the `url` parameter in the handler that calls this — defensive,
/// no shadowing today but future-proofs against silent breakage if a
/// local `url` binding is added.
pub fn validate_external_url(url: &str) -> Result<(), String> {
    let parsed = ::url::Url::parse(url).map_err(|e| format!("failed to parse {url:?}: {e}"))?;
    match parsed.scheme() {
        "http" | "https" | "mailto" => Ok(()),
        other => Err(format!("rejected scheme {other:?} for url {url:?}")),
    }
}

/// Validate that the given input string parses as exactly one email address.
///
/// Pure function (no `Inboxly` dependency) so it is unit-testable in
/// isolation and so the compose view's input handler can call it without
/// going through the full state machine. Reuses
/// [`inboxly_core::parse_address_list`], which already handles the
/// RFC 5322 forms (`"Name" <addr>`, `addr`, etc.).
///
/// Returns `Ok(Contact)` for valid input, `Err(message)` for invalid input.
///
/// # Errors
///
/// - the input is empty or whitespace-only
/// - the input cannot be parsed as a single address
/// - the input parses to more than one address
/// - the parsed address is missing an `@`
pub fn validate_smtp_recipient(input: &str) -> Result<Contact, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("recipient is empty".to_string());
    }
    let parsed = parse_address_list(trimmed);
    if parsed.is_empty() {
        return Err(format!("could not parse {trimmed:?} as an email address"));
    }
    if parsed.len() > 1 {
        return Err(format!(
            "expected one address, got {} in {trimmed:?}",
            parsed.len()
        ));
    }
    // parse_address_list already enforces a non-empty address with `@`,
    // but we double-check defensively so future parser changes can't
    // sneak an empty address through.
    let Some(pa) = parsed.into_iter().next() else {
        return Err(format!("could not parse {trimmed:?} as an email address"));
    };
    if pa.address.is_empty() || !pa.address.contains('@') {
        return Err(format!("{trimmed:?} is not a valid email address"));
    }
    Ok(Contact::new(pa.name.unwrap_or_default(), pa.address))
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
            open_thread_id: None,
            thread_reader: None,
            undo_state: UndoState::new(),
            menus: MenuState::new(),
            config: AppConfig::default(),
            settings: SettingsState::new(),
            expanded_bundles: HashSet::new(),
            snooze: SnoozeState::new(),
            compose: ComposeState::new(),
        }
    }
}

/// Create an empty account form with sensible defaults.
pub(crate) fn new_empty_account_form() -> AccountConfig {
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
    ///
    /// `Arc<Store>` is `!Send + !Sync` because `Store` holds a
    /// `rusqlite::Connection`. This is intentional per the M34 design —
    /// see `inboxly_store::thread_reader::ThreadReader` for the
    /// threading caveat. Dioxus's single-threaded executor handles this
    /// fine; the alternative (refactoring `Store` to be `Send + Sync`)
    /// is out of scope for M34, so we explicitly allow the lint here.
    #[allow(clippy::arc_with_non_send_sync)]
    pub fn with_store(store: Store) -> Self {
        let mut app = Self {
            store: Some(Arc::new(store)),
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
                    // F-2: switching accounts dismisses any open thread so the
                    // user doesn't see thread A's content while viewing account
                    // B's inbox once ThreadReader is wired (cross-account
                    // contamination). Mirrors the behaviour of switching to
                    // Settings / Done views.
                    self.open_thread_id = None;
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
                self.snooze.picker_thread = None;
            }
            Message::OpenOverflowMenu {
                thread_id,
                sender_address,
                position,
            } => {
                self.menus.context_thread = None;
                self.menus.overflow_thread = Some(thread_id);
                self.menus.overflow_position = position;
                self.menus.thread_sender = Some(sender_address);
            }
            Message::CloseOverflowMenu => {
                self.menus.overflow_thread = None;
                self.menus.thread_sender = None;
            }
            Message::OpenContextMenu {
                thread_id,
                sender_address,
                position,
            } => {
                self.menus.overflow_thread = None;
                self.menus.context_thread = Some(thread_id);
                self.menus.context_position = position;
                self.menus.thread_sender = Some(sender_address);
            }
            Message::CloseContextMenu => {
                self.menus.context_thread = None;
                self.menus.thread_sender = None;
            }
            Message::OpenThread(thread_id) => {
                self.open_thread_id = Some(thread_id);
                self.menus.close();
                // Phase 10 polish: opening a thread also dismisses the
                // account switcher. Without this, the row's onclick
                // (Phase 8) calls stop_propagation() and the existing
                // .content-area click handler that dismisses the
                // switcher never fires.
                self.account_switcher_open = false;
            }
            Message::CloseThread => {
                self.open_thread_id = None;
            }
            Message::OpenExternalUrl(url) => {
                // Thin wrapper: validate the URL via the pure helper, then
                // hand it to the system browser. The pure helper is unit-
                // tested separately (see `validate_external_url_*` tests
                // below) so the side-effect-having `open::that` call is
                // never reached from `cargo test`. The `#[cfg(not(test))]`
                // gate is defence-in-depth — see the post-M34 incident
                // where the original handler tests dispatched real URLs
                // through this branch and overnight test runs spawned ~10
                // browser + ~10 kmail compose windows. Belt and suspenders:
                // pure helper for unit testing AND cfg gate for any
                // future test that accidentally dispatches the message.
                match validate_external_url(&url) {
                    Ok(()) => {
                        #[cfg(not(test))]
                        if let Err(e) = open::that(&url) {
                            tracing::warn!("open::that({url}) failed: {e}");
                        }
                        #[cfg(test)]
                        tracing::debug!("OpenExternalUrl: would open {url} (skipped in test mode)");
                    }
                    Err(reason) => {
                        tracing::warn!("OpenExternalUrl: {reason}");
                    }
                }
            }
            Message::NavigateToSettings => {
                // Re-entry guard: if we're already in Settings, do NOT
                // overwrite previous_view (which would set it to Settings
                // and break NavigateBack — the back arrow would no-op
                // back to Settings instead of returning to the user's
                // prior view). M29-era bug surfaced during M34 manual
                // testing — clicking the gear icon while already in
                // Settings corrupted the previous_view and trapped the
                // user. The settings load still runs unconditionally
                // below so this remains a valid "reload settings"
                // trigger; only the navigation bookkeeping is gated.
                if self.active_view != ActiveView::Settings {
                    self.previous_view = self.active_view;
                    self.settings.drawer_was_open = self.drawer_open;
                }
                self.active_view = ActiveView::Settings;
                self.active_nav = NavTarget::View(ActiveView::Settings);
                self.drawer_open = false;
                self.settings.tab = SettingsTab::General;

                // Load current settings from store
                if let Some(ref store) = self.store {
                    let adapter = StoreSettingsAdapter { store };
                    // Theme preference
                    self.settings.theme_preference = adapter
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
                    self.settings.default_view = adapter
                        .get_setting("default_view")
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "inbox".to_owned());
                    // Undo timeout
                    self.settings.undo_timeout_secs = adapter
                        .get_setting("undo_timeout_secs")
                        .ok()
                        .flatten()
                        .and_then(|v| v.parse::<u32>().ok())
                        .unwrap_or(7);
                    // Shortcuts
                    self.settings.shortcuts = store
                        .get_setting("shortcuts")
                        .ok()
                        .flatten()
                        .map(|json| ShortcutMap::from_overrides_json(&json))
                        .unwrap_or_else(ShortcutMap::defaults);

                    // Notification settings
                    self.settings.notifications_enabled = store
                        .get_setting("notifications_enabled")
                        .ok()
                        .flatten()
                        .map(|v| v != "false")
                        .unwrap_or(true);
                    self.settings.notification_sound = store
                        .get_setting("notification_sound")
                        .ok()
                        .flatten()
                        .map(|v| v != "false")
                        .unwrap_or(true);
                    self.settings.notification_bundles = store
                        .get_setting("notification_bundles")
                        .ok()
                        .flatten()
                        .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
                        .unwrap_or_else(|| vec!["all".to_string()]);

                    // Bundle settings
                    match store.list_bundle_rows() {
                        Ok(bundles) => self.settings.bundles = bundles,
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
                self.drawer_open = self.settings.drawer_was_open;
            }
            Message::SettingsTabChanged(tab) => {
                self.settings.tab = tab;
                // Reset edit state when switching tabs
                self.settings.editing_account_index = None;
                self.settings.adding_account = false;
                self.settings.removing_account_index = None;
                self.settings.data_action_status = None;

                // Load sizes when entering Data & Storage tab
                if tab == SettingsTab::DataStorage {
                    if let Ok(config) = AppConfig::load()
                        && let Some(paths) = Paths::resolve_with_config(&config)
                    {
                        self.settings.db_size_display = format_size(
                            paths
                                .database_file()
                                .metadata()
                                .map(|m| m.len())
                                .unwrap_or(0),
                        );
                        self.settings.index_size_display =
                            format_size(dir_size(&paths.search_index_dir()));
                        self.settings.maildir_size_display =
                            format_size(dir_size(&paths.maildir_root()));
                    }
                    // Last sync from store
                    if let Some(ref store) = self.store {
                        self.settings.last_sync_display = store
                            .get_setting("last_full_sync")
                            .ok()
                            .flatten()
                            .unwrap_or_else(|| "Never".to_owned());
                    }
                }
            }

            // -- General tab handlers --
            Message::SetThemePreference(pref) => {
                self.settings.theme_preference = pref;
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
                self.settings.default_view = view.clone();
                if let Some(ref store) = self.store
                    && let Err(e) = store.set_setting("default_view", &view)
                {
                    tracing::warn!("failed to persist default view: {e}");
                }
            }

            Message::SetUndoTimeout(secs) => {
                self.settings.undo_timeout_secs = secs;
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
                self.settings.adding_account = true;
                self.settings.editing_account_index = None;
                self.settings.removing_account_index = None;
                self.settings.account_form = new_empty_account_form();
            }

            Message::EditAccountStart(index) => {
                if let Some(account) = self.config.accounts.get(index) {
                    self.settings.account_form = account.clone();
                    self.settings.editing_account_index = Some(index);
                    self.settings.adding_account = false;
                    self.settings.removing_account_index = None;
                }
            }

            Message::AccountFormCancel => {
                self.settings.editing_account_index = None;
                self.settings.adding_account = false;
            }

            Message::AccountFormSave => {
                // Validate form
                let form = &self.settings.account_form;
                if form.email.is_empty()
                    || !form.email.contains('@')
                    || form.imap_host.is_empty()
                    || form.smtp_host.is_empty()
                {
                    // Invalid -- do nothing (UI should show inline validation hints)
                    return;
                }

                if self.settings.adding_account {
                    self.config
                        .accounts
                        .push(self.settings.account_form.clone());
                } else if let Some(index) = self.settings.editing_account_index
                    && let Some(account) = self.config.accounts.get_mut(index)
                {
                    *account = self.settings.account_form.clone();
                }

                if let Err(e) = self.config.save() {
                    tracing::warn!("failed to save config after account update: {e}");
                }
                self.settings.editing_account_index = None;
                self.settings.adding_account = false;
            }

            Message::AccountFormEmailChanged(v) => self.settings.account_form.email = v,
            Message::AccountFormDisplayNameChanged(v) => {
                self.settings.account_form.display_name = v;
            }
            Message::AccountFormProviderChanged(v) => self.settings.account_form.provider = v,
            Message::AccountFormAuthMethodChanged(v) => {
                self.settings.account_form.auth_method = match v.as_str() {
                    "oauth2" => AuthMethod::OAuth2,
                    "app_password" => AuthMethod::AppPassword,
                    _ => AuthMethod::Password,
                };
            }
            Message::AccountFormImapHostChanged(v) => self.settings.account_form.imap_host = v,
            Message::AccountFormImapPortChanged(v) => {
                if let Ok(port) = v.parse::<u16>() {
                    self.settings.account_form.imap_port = port;
                }
            }
            Message::AccountFormSmtpHostChanged(v) => self.settings.account_form.smtp_host = v,
            Message::AccountFormSmtpPortChanged(v) => {
                if let Ok(port) = v.parse::<u16>() {
                    self.settings.account_form.smtp_port = port;
                }
            }

            Message::RemoveAccountConfirm(index) => {
                self.settings.removing_account_index = Some(index);
            }

            Message::RemoveAccountCancel => {
                self.settings.removing_account_index = None;
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
                self.settings.removing_account_index = None;
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
                            self.settings.data_action_status =
                                Some(format!("Failed to clear cache: {e}"));
                        } else {
                            let _ = std::fs::create_dir_all(&paths.cache_dir);
                            self.settings.data_action_status = Some("Cache cleared".to_owned());
                        }
                    } else {
                        self.settings.data_action_status = Some("No cache to clear".to_owned());
                    }
                }
            }

            Message::RebuildSearchIndex => {
                self.settings.data_action_status = Some("Rebuilding search index...".to_owned());
                tracing::info!("search index rebuild requested (stub)");
            }

            Message::ExportData => {
                self.settings.data_action_status = Some("Coming soon".to_owned());
                tracing::info!("data export requested (stub)");
            }

            Message::DataSizesLoaded {
                db_size,
                index_size,
                maildir_size,
                last_sync,
            } => {
                self.settings.db_size_display = db_size;
                self.settings.index_size_display = index_size;
                self.settings.maildir_size_display = maildir_size;
                self.settings.last_sync_display = last_sync;
            }

            // -- Keyboard shortcuts handlers --
            Message::ShortcutsLoaded(map) => {
                self.settings.shortcuts = map;
            }
            Message::SetShortcut { action, binding } => {
                self.settings.shortcuts.set(action, binding);
                self.settings.capturing_shortcut = None;
                if let Some(ref store) = self.store {
                    let json = self.settings.shortcuts.to_overrides_json();
                    if let Err(e) = store.set_setting("shortcuts", &json) {
                        tracing::warn!("failed to persist shortcuts: {e}");
                    }
                }
            }
            Message::ResetShortcut(action) => {
                self.settings.shortcuts.reset(action);
                if let Some(ref store) = self.store {
                    let json = self.settings.shortcuts.to_overrides_json();
                    if let Err(e) = store.set_setting("shortcuts", &json) {
                        tracing::warn!("failed to persist shortcuts: {e}");
                    }
                }
            }
            Message::StartCapture(action) => {
                self.settings.capturing_shortcut = Some(action);
            }
            Message::CancelCapture => {
                self.settings.capturing_shortcut = None;
            }

            // -- Notification settings handlers --
            Message::ToggleNotifications => {
                self.settings.notifications_enabled = !self.settings.notifications_enabled;
                if let Some(ref store) = self.store {
                    let val = if self.settings.notifications_enabled {
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
                self.settings.notification_sound = !self.settings.notification_sound;
                if let Some(ref store) = self.store {
                    let val = if self.settings.notification_sound {
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
                self.settings.notification_bundles = bundles;
                if let Some(ref store) = self.store {
                    let json = serde_json::to_string(&self.settings.notification_bundles)
                        .unwrap_or_else(|_| r#"["all"]"#.to_owned());
                    if let Err(e) = store.set_setting("notification_bundles", &json) {
                        tracing::warn!("failed to persist notification_bundles: {e}");
                    }
                }
            }

            // -- Bundle settings handlers --
            Message::BundlesLoaded(bundles) => {
                self.settings.bundles = bundles;
            }
            Message::ToggleBundleVisibility(bundle_id) => {
                if let Some(bundle) = self.settings.bundles.iter_mut().find(|b| b.id == bundle_id) {
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
                if let Some(bundle) = self.settings.bundles.iter_mut().find(|b| b.id == bundle_id) {
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
                    if let Some(bundle) = self.settings.bundles.iter_mut().find(|b| &b.id == id) {
                        bundle.sort_order = order as i64;
                        if let Some(ref store) = self.store
                            && let Err(e) = store.update_bundle_row(bundle)
                        {
                            tracing::warn!("failed to persist bundle reorder: {e}");
                        }
                    }
                }
                // Re-sort the local list.
                self.settings.bundles.sort_by_key(|b| b.sort_order);
            }
            Message::ToggleThrottlePopup(bundle_id) => {
                self.settings.throttle_popup_bundle_id = bundle_id;
            }

            Message::MoveTo {
                thread_id,
                destination,
            } => {
                tracing::info!("move thread {thread_id} to {destination:?}");
                // Enqueue IMAP move actions for all emails in thread.
                match destination {
                    MoveDestination::Trash => {
                        self.enqueue_thread_actions(&thread_id, |account_id, folder, imap_uid| {
                            OfflineAction::MoveToTrash {
                                account_id: account_id.to_string(),
                                folder: folder.to_string(),
                                imap_uid,
                            }
                        });
                    }
                    MoveDestination::Inbox => {
                        let to = "INBOX".to_string();
                        self.enqueue_thread_actions(&thread_id, |account_id, folder, imap_uid| {
                            OfflineAction::MoveToFolder {
                                account_id: account_id.to_string(),
                                from_folder: folder.to_string(),
                                to_folder: to.clone(),
                                imap_uid,
                            }
                        });
                    }
                    MoveDestination::Spam => {
                        let to = "Spam".to_string();
                        self.enqueue_thread_actions(&thread_id, |account_id, folder, imap_uid| {
                            OfflineAction::MoveToFolder {
                                account_id: account_id.to_string(),
                                from_folder: folder.to_string(),
                                to_folder: to.clone(),
                                imap_uid,
                            }
                        });
                    }
                }
                self.menus.close();
            }
            Message::MarkReadState { thread_id, read } => {
                tracing::info!("mark thread {thread_id} read={read}");
                // Enqueue IMAP read/unread actions for all emails in thread.
                if read {
                    self.enqueue_thread_actions(&thread_id, |account_id, folder, imap_uid| {
                        OfflineAction::MarkRead {
                            account_id: account_id.to_string(),
                            folder: folder.to_string(),
                            imap_uid,
                        }
                    });
                } else {
                    self.enqueue_thread_actions(&thread_id, |account_id, folder, imap_uid| {
                        OfflineAction::MarkUnread {
                            account_id: account_id.to_string(),
                            folder: folder.to_string(),
                            imap_uid,
                        }
                    });
                }
                self.menus.close();
            }
            Message::MuteThread(thread_id) => {
                tracing::info!("mute thread {thread_id}");
                self.menus.close();
            }
            Message::Reply(thread_id) => {
                tracing::info!("reply to thread {thread_id}");
                self.menus.close();
            }
            Message::ReplyAll(thread_id) => {
                tracing::info!("reply all to thread {thread_id}");
                self.menus.close();
            }
            Message::Forward(thread_id) => {
                tracing::info!("forward thread {thread_id}");
                self.menus.close();
            }
            Message::AddToBundle {
                thread_id,
                category,
            } => {
                tracing::info!("add thread {thread_id} to bundle {category}");
                self.menus.close();
            }
            Message::CreateRuleFromSender(sender) => {
                tracing::info!("create rule from sender: {sender} (coming soon)");
                self.menus.close();
            }
            Message::BlockSender {
                thread_id,
                sender_address,
            } => {
                tracing::info!("block sender {sender_address} (thread {thread_id})");
                self.menus.close();
            }
            Message::ReportSpam(thread_id) => {
                tracing::info!("report spam: thread {thread_id}");
                // Enqueue IMAP move-to-spam actions for all emails in thread.
                let spam_folder = "Spam".to_string();
                self.enqueue_thread_actions(&thread_id, |account_id, folder, imap_uid| {
                    OfflineAction::MoveToFolder {
                        account_id: account_id.to_string(),
                        from_folder: folder.to_string(),
                        to_folder: spam_folder.clone(),
                        imap_uid,
                    }
                });
                self.menus.close();
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
                self.menus.close();
                self.snooze.picker_thread = Some(thread_id);
                self.snooze.picker_position = position;
            }
            Message::CloseSnoozePicker => {
                self.snooze.picker_thread = None;
            }

            // -- Compose (M35) ----------------------------------------
            Message::OpenCompose => {
                // Issue 1.7: open_thread_id and other view state are NOT
                // touched. The thread stays open in the background;
                // navigating back to Inbox restores it.
                self.previous_view = self.active_view;
                self.active_view = ActiveView::Compose;
                self.active_nav = NavTarget::View(ActiveView::Compose);
                if self.compose.dirty {
                    // M35b accepts "single compose at a time" — opening
                    // a new compose discards any existing dirty state
                    // with a warning. M36+ can add a confirmation prompt
                    // or true multi-compose support.
                    tracing::warn!(
                        "OpenCompose called with dirty existing compose state -- discarding"
                    );
                }
                self.compose = ComposeState::default();
                self.compose.draft_id = Some(uuid::Uuid::new_v4().to_string());
                self.compose.from_account_index = self.active_account_index;
            }
            Message::OpenComposeReply { thread_id, mode } => {
                tracing::warn!(
                    "OpenComposeReply is M36 territory -- not implemented in M35b (thread_id={thread_id}, mode={mode:?})"
                );
            }
            Message::CloseCompose => {
                // Note: compose state is NOT cleared. If the user reopens
                // compose via OpenCompose, they get a blank one (per
                // OpenCompose's reset). Resuming the same draft will be
                // wired through the drafts list in M36+.
                self.active_view = self.previous_view;
                self.active_nav = NavTarget::View(self.previous_view);
            }
            Message::ComposeSubjectChanged(value) => {
                // Gemini G5: never clobber state during a send.
                if matches!(self.compose.send_state, ComposeSendState::Sending) {
                    tracing::warn!("ComposeSubjectChanged ignored during Sending state");
                } else {
                    self.compose.subject = value;
                    self.compose.mark_dirty();
                }
            }
            Message::ComposeToInputChanged(value) => {
                if matches!(self.compose.send_state, ComposeSendState::Sending) {
                    tracing::warn!("ComposeToInputChanged ignored during Sending state");
                } else {
                    self.compose.to_input = value;
                    self.compose.mark_dirty();
                }
            }
            Message::ComposeCcInputChanged(value) => {
                if matches!(self.compose.send_state, ComposeSendState::Sending) {
                    tracing::warn!("ComposeCcInputChanged ignored during Sending state");
                } else {
                    self.compose.cc_input = value;
                    self.compose.mark_dirty();
                }
            }
            Message::ComposeBccInputChanged(value) => {
                if matches!(self.compose.send_state, ComposeSendState::Sending) {
                    tracing::warn!("ComposeBccInputChanged ignored during Sending state");
                } else {
                    self.compose.bcc_input = value;
                    self.compose.mark_dirty();
                }
            }
            Message::ComposeAddRecipient { field, contact } => {
                if matches!(self.compose.send_state, ComposeSendState::Sending) {
                    tracing::warn!("ComposeAddRecipient ignored during Sending state");
                } else {
                    let target = match field {
                        RecipientField::To => &mut self.compose.to,
                        RecipientField::Cc => &mut self.compose.cc,
                        RecipientField::Bcc => &mut self.compose.bcc,
                    };
                    target.push(Arc::new(contact));
                    self.compose.mark_dirty();
                }
            }
            Message::ComposeRemoveRecipient { field, index } => {
                if matches!(self.compose.send_state, ComposeSendState::Sending) {
                    tracing::warn!("ComposeRemoveRecipient ignored during Sending state");
                } else {
                    let target = match field {
                        RecipientField::To => &mut self.compose.to,
                        RecipientField::Cc => &mut self.compose.cc,
                        RecipientField::Bcc => &mut self.compose.bcc,
                    };
                    if index < target.len() {
                        target.remove(index);
                        self.compose.mark_dirty();
                    } else {
                        tracing::warn!(
                            "ComposeRemoveRecipient index {index} out of bounds (len {})",
                            target.len()
                        );
                    }
                }
            }
            Message::ComposeBodyChanged(value) => {
                if matches!(self.compose.send_state, ComposeSendState::Sending) {
                    tracing::warn!("ComposeBodyChanged ignored during Sending state");
                } else {
                    self.compose.body_markdown = value;
                    self.compose.mark_dirty();
                }
            }
            Message::ComposeToggleCcBcc => {
                self.compose.show_cc_bcc = !self.compose.show_cc_bcc;
            }
            Message::ComposeTogglePreview => {
                self.compose.show_preview = !self.compose.show_preview;
            }
            Message::ComposeFromChanged { account_index } => {
                if matches!(self.compose.send_state, ComposeSendState::Sending) {
                    tracing::warn!("ComposeFromChanged ignored during Sending state");
                } else if account_index < self.accounts.len() {
                    self.compose.from_account_index = account_index;
                    self.compose.mark_dirty();
                } else {
                    tracing::warn!(
                        "ComposeFromChanged index {account_index} out of bounds (have {} accounts)",
                        self.accounts.len()
                    );
                }
            }
            Message::ComposeAttachFile => {
                // Phase 11 wires the rfd file picker bridge. Phase 6
                // accepts the message so the UI button has a stable
                // target; the actual picker dispatch lives outside the
                // pure state machine.
                tracing::debug!("ComposeAttachFile -- file picker bridge is M35 phase 11");
            }
            Message::ComposeAttachmentAdded(att) => {
                if matches!(self.compose.send_state, ComposeSendState::Sending) {
                    tracing::warn!("ComposeAttachmentAdded ignored during Sending state");
                } else {
                    self.compose.attachments.push(att);
                    self.compose.mark_dirty();
                }
            }
            Message::ComposeAttachmentTooLarge => {
                // Phase 8 surfaces a snackbar; Phase 6 only logs.
                tracing::warn!(
                    "ComposeAttachmentTooLarge -- file would push compose over the 20 MB limit"
                );
            }
            Message::ComposeRemoveAttachment(index) => {
                if matches!(self.compose.send_state, ComposeSendState::Sending) {
                    tracing::warn!("ComposeRemoveAttachment ignored during Sending state");
                } else if index < self.compose.attachments.len() {
                    self.compose.attachments.remove(index);
                    self.compose.mark_dirty();
                } else {
                    tracing::warn!(
                        "ComposeRemoveAttachment index {index} out of bounds (len {})",
                        self.compose.attachments.len()
                    );
                }
            }
            Message::ComposeSaveDraft => {
                // Phase 10 bridge handles SQLite/Maildir/IMAP writes.
                tracing::debug!("ComposeSaveDraft -- save bridge is M35 phase 10");
            }
            Message::ComposeAutoSaveTick { generation } => {
                // Issue 1.8 generation snapshot guard: the Phase 10
                // auto-save bridge dispatches this AFTER calling
                // Store::update_draft. Only clear `dirty` if the user
                // hasn't typed since the save was triggered, otherwise
                // the next tick must save again.
                if self.compose.save_generation == generation {
                    self.compose.dirty = false;
                }
            }
            Message::ComposeSendDraft => {
                if !self.compose.can_send() {
                    tracing::warn!("ComposeSendDraft ignored -- can_send() returned false");
                } else {
                    self.compose.send_state = ComposeSendState::Sending;
                }
            }
            Message::ComposeSendComplete { success, error } => {
                if success {
                    // Gemini G9 two-phase commit: don't clear compose
                    // state until the user dismisses the overlay.
                    self.compose.send_state = ComposeSendState::Sent {
                        dismiss_pending: true,
                    };
                } else {
                    self.compose.send_state = ComposeSendState::Failed {
                        error: error.unwrap_or_else(|| "send failed".to_string()),
                    };
                }
            }
            Message::ComposeDismissSentNotice => {
                if matches!(self.compose.send_state, ComposeSendState::Sent { .. }) {
                    self.compose = ComposeState::default();
                    self.active_view = self.previous_view;
                    self.active_nav = NavTarget::View(self.previous_view);
                } else {
                    tracing::warn!("ComposeDismissSentNotice ignored -- send_state is not Sent");
                }
            }
            Message::ComposeDiscardDraft => {
                self.compose = ComposeState::default();
                self.active_view = self.previous_view;
                self.active_nav = NavTarget::View(self.previous_view);
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
    fn navigate_to_settings_while_already_in_settings_preserves_previous_view() {
        // Regression test for the M29-era bug surfaced during M34 manual
        // testing: clicking the gear icon while already in Settings used
        // to overwrite previous_view = Settings, which trapped the user
        // because NavigateBack would no-op back to Settings.
        let mut app = Inboxly::default();
        app.active_view = ActiveView::Snoozed;
        // First NavigateToSettings — previous_view should be Snoozed.
        let _ = app.update(Message::NavigateToSettings);
        assert_eq!(app.previous_view, ActiveView::Snoozed);
        // Second NavigateToSettings (re-entry) — previous_view must
        // STILL be Snoozed, not Settings.
        let _ = app.update(Message::NavigateToSettings);
        assert_eq!(
            app.previous_view,
            ActiveView::Snoozed,
            "re-entry to Settings must not overwrite previous_view"
        );
        // And NavigateBack must actually return to Snoozed.
        let _ = app.update(Message::NavigateBack);
        assert_eq!(
            app.active_view,
            ActiveView::Snoozed,
            "NavigateBack should return to the original previous view"
        );
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
        assert_eq!(app.menus.overflow_thread, Some("t1".into()));
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
        assert_eq!(app.menus.overflow_thread, Some("t1".into()));
        assert_eq!(app.menus.overflow_position, pos);
        assert_eq!(app.menus.thread_sender, Some("sender@example.com".into()));
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
        assert_eq!(app.menus.context_thread, Some("t2".into()));
        assert_eq!(app.menus.context_position, pos);
        assert_eq!(app.menus.thread_sender, Some("ctx@example.com".into()));
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
        assert!(app.menus.overflow_thread.is_none());
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
        assert!(app.menus.thread_sender.is_none());
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
        assert!(app.menus.context_thread.is_none());
        assert!(app.menus.thread_sender.is_none());
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
        assert!(app.menus.overflow_thread.is_none());
        assert_eq!(app.menus.context_thread, Some("t2".into()));
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
        assert!(app.menus.overflow_thread.is_none());
        // sender reflects the context menu opener, not the overflow one
        assert_eq!(app.menus.thread_sender, Some("ctx@b.com".into()));
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
        assert!(app.menus.context_thread.is_none());
        assert_eq!(app.menus.overflow_thread, Some("t2".into()));
        assert_eq!(app.menus.thread_sender, Some("b@b.com".into()));
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
        assert!(app.menus.overflow_thread.is_none());
        assert!(app.menus.thread_sender.is_none());
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
        assert!(app.menus.context_thread.is_none());
        assert!(app.menus.thread_sender.is_none());
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
        assert!(app.menus.overflow_thread.is_none());
        assert!(app.menus.thread_sender.is_none());
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
        assert!(app.menus.context_thread.is_none());
        assert!(app.menus.thread_sender.is_none());
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
        assert_eq!(app.menus.thread_sender, Some("menu@sender.com".into()));
        let _ = app.update(Message::BlockSender {
            thread_id: "t1".into(),
            sender_address: "menu@sender.com".into(),
        });
        assert!(app.menus.context_thread.is_none());
        assert!(app.menus.thread_sender.is_none());
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
        assert_eq!(app.settings.tab, SettingsTab::General);
    }

    #[test]
    fn settings_tab_change() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::NavigateToSettings);
        let _ = app.update(Message::SettingsTabChanged(SettingsTab::Accounts));
        assert_eq!(app.settings.tab, SettingsTab::Accounts);
    }

    #[test]
    fn settings_tab_change_resets_edit_state() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::NavigateToSettings);
        // Set some edit state
        app.settings.adding_account = true;
        app.settings.editing_account_index = Some(0);
        app.settings.removing_account_index = Some(1);
        app.settings.data_action_status = Some("test".to_owned());
        // Switch tab
        let _ = app.update(Message::SettingsTabChanged(SettingsTab::General));
        assert!(!app.settings.adding_account);
        assert!(app.settings.editing_account_index.is_none());
        assert!(app.settings.removing_account_index.is_none());
        assert!(app.settings.data_action_status.is_none());
    }

    #[test]
    fn add_account_start_opens_form() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::AddAccountStart);
        assert!(app.settings.adding_account);
        assert!(app.settings.editing_account_index.is_none());
        assert!(app.settings.removing_account_index.is_none());
    }

    #[test]
    fn account_form_cancel_resets_state() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::AddAccountStart);
        assert!(app.settings.adding_account);
        let _ = app.update(Message::AccountFormCancel);
        assert!(!app.settings.adding_account);
        assert!(app.settings.editing_account_index.is_none());
    }

    #[test]
    fn account_form_field_updates() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::AccountFormEmailChanged("test@example.com".into()));
        assert_eq!(app.settings.account_form.email, "test@example.com");

        let _ = app.update(Message::AccountFormDisplayNameChanged("Test User".into()));
        assert_eq!(app.settings.account_form.display_name, "Test User");

        let _ = app.update(Message::AccountFormProviderChanged("gmail".into()));
        assert_eq!(app.settings.account_form.provider, "gmail");

        let _ = app.update(Message::AccountFormAuthMethodChanged("oauth2".into()));
        assert_eq!(
            app.settings.account_form.auth_method,
            inboxly_core::config::AuthMethod::OAuth2
        );

        let _ = app.update(Message::AccountFormImapHostChanged("imap.gmail.com".into()));
        assert_eq!(app.settings.account_form.imap_host, "imap.gmail.com");

        let _ = app.update(Message::AccountFormImapPortChanged("143".into()));
        assert_eq!(app.settings.account_form.imap_port, 143);

        let _ = app.update(Message::AccountFormSmtpHostChanged("smtp.gmail.com".into()));
        assert_eq!(app.settings.account_form.smtp_host, "smtp.gmail.com");

        let _ = app.update(Message::AccountFormSmtpPortChanged("465".into()));
        assert_eq!(app.settings.account_form.smtp_port, 465);
    }

    #[test]
    fn remove_account_confirm_and_cancel() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::RemoveAccountConfirm(1));
        assert_eq!(app.settings.removing_account_index, Some(1));
        let _ = app.update(Message::RemoveAccountCancel);
        assert!(app.settings.removing_account_index.is_none());
    }

    #[test]
    fn undo_timeout_values() {
        let mut app = Inboxly::default();
        for secs in [3, 5, 7, 10, 15] {
            let _ = app.update(Message::SetUndoTimeout(secs));
            assert_eq!(app.settings.undo_timeout_secs, secs);
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
        assert!(app.settings.notifications_enabled);
        let _ = app.update(Message::ToggleNotifications);
        assert!(!app.settings.notifications_enabled);
        let _ = app.update(Message::ToggleNotifications);
        assert!(app.settings.notifications_enabled);
    }

    #[test]
    fn toggle_notification_sound() {
        let mut app = Inboxly::default();
        assert!(app.settings.notification_sound);
        let _ = app.update(Message::ToggleNotificationSound);
        assert!(!app.settings.notification_sound);
        let _ = app.update(Message::ToggleNotificationSound);
        assert!(app.settings.notification_sound);
    }

    #[test]
    fn set_notification_bundles() {
        let mut app = Inboxly::default();
        assert_eq!(app.settings.notification_bundles, vec!["all".to_string()]);

        let _ = app.update(Message::SetNotificationBundles(vec![
            "Social".to_string(),
            "Finance".to_string(),
        ]));
        assert_eq!(app.settings.notification_bundles.len(), 2);
        assert!(
            app.settings
                .notification_bundles
                .contains(&"Social".to_string())
        );
        assert!(
            app.settings
                .notification_bundles
                .contains(&"Finance".to_string())
        );

        let _ = app.update(Message::SetNotificationBundles(vec!["primary".to_string()]));
        assert_eq!(
            app.settings.notification_bundles,
            vec!["primary".to_string()]
        );
    }

    #[test]
    fn toggle_bundle_visibility() {
        let mut app = Inboxly::default();
        app.settings.bundles.push(BundleRow {
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
        assert_eq!(app.settings.bundles[0].visibility, "hidden");

        let _ = app.update(Message::ToggleBundleVisibility("b1".to_string()));
        assert_eq!(app.settings.bundles[0].visibility, "visible");
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
        assert!(app.settings.capturing_shortcut.is_none());
        let _ = app.update(Message::StartCapture(ShortcutAction::Done));
        assert_eq!(app.settings.capturing_shortcut, Some(ShortcutAction::Done));
    }

    #[test]
    fn cancel_capture() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::StartCapture(ShortcutAction::Done));
        assert!(app.settings.capturing_shortcut.is_some());
        let _ = app.update(Message::CancelCapture);
        assert!(app.settings.capturing_shortcut.is_none());
    }

    #[test]
    fn set_shortcut() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::StartCapture(ShortcutAction::Done));
        let _ = app.update(Message::SetShortcut {
            action: ShortcutAction::Done,
            binding: "d".to_string(),
        });
        assert_eq!(app.settings.shortcuts.get(ShortcutAction::Done), "d");
        assert!(app.settings.capturing_shortcut.is_none());
        assert!(app.settings.shortcuts.is_customised(ShortcutAction::Done));
    }

    #[test]
    fn reset_shortcut() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::SetShortcut {
            action: ShortcutAction::Done,
            binding: "d".to_string(),
        });
        assert_eq!(app.settings.shortcuts.get(ShortcutAction::Done), "d");

        let _ = app.update(Message::ResetShortcut(ShortcutAction::Done));
        assert_eq!(app.settings.shortcuts.get(ShortcutAction::Done), "e"); // default
        assert!(!app.settings.shortcuts.is_customised(ShortcutAction::Done));
    }

    #[test]
    fn bundles_loaded_replaces_list() {
        let mut app = Inboxly::default();
        assert!(app.settings.bundles.is_empty());

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
        assert_eq!(app.settings.bundles.len(), 2);
    }

    #[test]
    fn reorder_bundles() {
        let mut app = Inboxly::default();
        app.settings.bundles = vec![
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
        assert_eq!(app.settings.bundles[0].id, "b2");
        assert_eq!(app.settings.bundles[0].sort_order, 0);
        assert_eq!(app.settings.bundles[1].id, "b1");
        assert_eq!(app.settings.bundles[1].sort_order, 1);
    }

    #[test]
    fn set_bundle_throttle() {
        let mut app = Inboxly::default();
        app.settings.bundles.push(BundleRow {
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
        assert_eq!(app.settings.bundles[0].throttle, daily_json);
    }

    #[test]
    fn toggle_throttle_popup() {
        let mut app = Inboxly::default();
        assert!(app.settings.throttle_popup_bundle_id.is_none());

        let _ = app.update(Message::ToggleThrottlePopup(Some("b1".to_string())));
        assert_eq!(
            app.settings.throttle_popup_bundle_id,
            Some("b1".to_string())
        );

        let _ = app.update(Message::ToggleThrottlePopup(None));
        assert!(app.settings.throttle_popup_bundle_id.is_none());
    }

    #[test]
    fn shortcuts_loaded_replaces_map() {
        let mut app = Inboxly::default();
        let mut custom_map = ShortcutMap::defaults();
        custom_map.set(ShortcutAction::Done, "x".to_owned());

        let _ = app.update(Message::ShortcutsLoaded(custom_map));
        assert_eq!(app.settings.shortcuts.get(ShortcutAction::Done), "x");
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
        assert_eq!(app.snooze.picker_thread, Some("t1".into()));
        assert_eq!(app.snooze.picker_position, pos);
    }

    #[test]
    fn close_snooze_picker_clears_state() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenSnoozePicker {
            thread_id: "t1".into(),
            position: Point::new(10.0, 20.0),
        });
        let _ = app.update(Message::CloseSnoozePicker);
        assert!(app.snooze.picker_thread.is_none());
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
        assert!(app.menus.overflow_thread.is_none());
        assert!(app.menus.context_thread.is_none());
        assert_eq!(app.snooze.picker_thread, Some("t1".into()));
    }

    #[test]
    fn snooze_thread_closes_picker() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenSnoozePicker {
            thread_id: "t1".into(),
            position: Point::ORIGIN,
        });
        assert_eq!(app.snooze.picker_thread, Some("t1".into()));
        let _ = app.update(Message::SnoozeThread {
            thread_id: "t1".into(),
            until: chrono::Utc::now() + chrono::Duration::hours(1),
        });
        assert!(
            app.snooze.picker_thread.is_none(),
            "SnoozeThread should close the picker"
        );
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

    // -- M33 Phase 10: InboxZero rendering gate --

    #[test]
    fn inbox_feed_sections_empty_by_default() {
        // ContentArea branches on feed_sections.is_empty() to show InboxZero.
        // Verify the default state satisfies this gate condition.
        let app = Inboxly::default();
        assert!(app.feed_sections.is_empty());
    }

    // -- M34 Phase 8: OpenThread / CloseThread lifecycle --

    #[test]
    fn open_thread_sets_open_thread_id_field() {
        let mut app = Inboxly::default();
        assert!(app.open_thread_id.is_none());
        let _ = app.update(Message::OpenThread("t1".into()));
        assert_eq!(app.open_thread_id.as_deref(), Some("t1"));
    }

    #[test]
    fn close_thread_clears_open_thread_id_field() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenThread("t1".into()));
        let _ = app.update(Message::CloseThread);
        assert!(app.open_thread_id.is_none());
    }

    #[test]
    fn open_thread_dismisses_open_menus() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenContextMenu {
            thread_id: "t1".into(),
            sender_address: "a@b.com".into(),
            position: Point::ORIGIN,
        });
        assert_eq!(app.menus.context_thread, Some("t1".into()));
        let _ = app.update(Message::OpenThread("t1".into()));
        // Opening a thread must dismiss any open menu (close_menus()
        // invariant from M33 Phase 7A).
        assert!(app.menus.context_thread.is_none());
        assert!(app.menus.thread_sender.is_none());
        // And it must record the intent.
        assert_eq!(app.open_thread_id.as_deref(), Some("t1"));
    }

    #[test]
    fn opening_thread_does_not_load_body_into_inboxly() {
        // Regression test for the Issue 1.4 design: dispatching OpenThread
        // must NOT cause any body data to be cloned into Inboxly. The
        // body lives in a separate signal that this test can't see —
        // we just verify Inboxly itself stays small.
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenThread("t1".into()));
        // The id is stored, but no LoadedThread or LoadedMessage anywhere
        // in Inboxly. (This is enforced by the type system: Inboxly has
        // no field of type LoadedThread.) The test exists to document
        // the contract for future contributors who might be tempted to
        // add an `Option<LoadedThread>` to Inboxly "for convenience".
        assert_eq!(app.open_thread_id.as_deref(), Some("t1"));
    }

    #[test]
    fn open_thread_dismisses_account_switcher() {
        // Phase 10 polish: opening a thread also dismisses the account
        // switcher. The row's onclick (Phase 8) calls stop_propagation()
        // to prevent bubbling, which would otherwise mean the existing
        // .content-area click handler that dismisses the switcher never
        // fires when a row is clicked. We compensate inside OpenThread.
        let mut app = Inboxly::default();
        app.account_switcher_open = true;
        let _ = app.update(Message::OpenThread("t1".into()));
        assert!(
            !app.account_switcher_open,
            "OpenThread must dismiss the account switcher"
        );
        assert_eq!(app.open_thread_id.as_deref(), Some("t1"));
    }

    #[test]
    fn switch_account_clears_open_thread_id() {
        // Final review finding F-2: switching accounts while a thread is
        // open must clear open_thread_id so the user doesn't see thread A
        // from account 1 while viewing account 2's inbox once ThreadReader
        // is wired (cross-account contamination). Without the clear, the
        // stale intent would survive the account swap and the bridge in
        // components::app would attempt to load it against the new
        // account's store.
        let mut app = Inboxly::with_accounts(vec![
            make_test_account("first@example.com", "First"),
            make_test_account("second@example.com", "Second"),
        ]);
        let _ = app.update(Message::OpenThread("t1".into()));
        assert_eq!(app.open_thread_id.as_deref(), Some("t1"));
        let _ = app.update(Message::SwitchAccount(1));
        assert_eq!(app.active_account_index, 1);
        assert!(
            app.open_thread_id.is_none(),
            "SwitchAccount must dismiss any open thread to avoid cross-account contamination"
        );
    }

    // -- M34 Phase 9: validate_external_url scheme allowlist tests --
    //
    // These tests exercise the pure validation function directly, NOT
    // the OpenExternalUrl handler. The handler is a thin wrapper that
    // calls validate_external_url and then open::that, so testing the
    // pure function gives us full coverage of the security-critical
    // logic without any risk of triggering open::that's side effect.
    //
    // The original Phase 9 tests went through the handler via
    // `app.update(Message::OpenExternalUrl(...))` which actually
    // launched the user's browser and mail client every time
    // `cargo test` ran — the post-M34 incident spawned ~10 example.com
    // windows and ~10 kmail compose windows overnight before the bug
    // was caught. Refactoring to pure-function tests removes any
    // chance of a future test (or a future refactor) accidentally
    // dispatching the handler with a real URL.

    #[test]
    fn validate_external_url_accepts_https() {
        assert!(super::validate_external_url("https://example.com").is_ok());
    }

    #[test]
    fn validate_external_url_accepts_http() {
        assert!(super::validate_external_url("http://example.com").is_ok());
    }

    #[test]
    fn validate_external_url_accepts_mailto() {
        // mailto: is on the allowlist for compose-from-link in M36+.
        // For M34 the handler still hands it to open::that() which
        // routes to the user's default mail client.
        assert!(super::validate_external_url("mailto:friend@example.com").is_ok());
    }

    #[test]
    fn validate_external_url_rejects_javascript_scheme() {
        // Eng review Issue 2.2 defence in depth: even if a javascript:
        // URL somehow slips past the sanitiser, validation must reject
        // it before the handler calls open::that(). Pin the allowlist.
        let result = super::validate_external_url("javascript:alert(1)");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("javascript"),
            "rejection should name the scheme: {err}"
        );
    }

    #[test]
    fn validate_external_url_rejects_file_scheme() {
        // file:// URLs in emails are typically attacks (path traversal,
        // SMB credential theft on Windows, etc.). The allowlist excludes
        // them by virtue of only listing http/https/mailto.
        let result = super::validate_external_url("file:///etc/passwd");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("file"),
            "rejection should name the scheme: {err}"
        );
    }

    #[test]
    fn validate_external_url_rejects_garbage_input() {
        // Malformed URLs return parse-error, not scheme-rejection.
        let result = super::validate_external_url("not a url at all !!!");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("failed to parse"),
            "garbage input should fail at parse, not scheme: {err}"
        );
    }

    // ========================================================================
    // M35 Phase 6 -- ComposeState state-machine tests
    // ========================================================================
    //
    // These cover the pure state-machine layer for the compose view: the
    // OpenCompose / CloseCompose / Compose*Changed handlers, the Gemini
    // G4 eager-UUID invariant, the Gemini G5 Sending-state guard, the
    // Gemini G9 two-phase commit dismiss flow, and the Issue 1.7
    // open-thread-preservation guarantee.

    fn test_account(email: &str) -> AccountConfig {
        AccountConfig {
            email: email.to_string(),
            display_name: String::new(),
            provider: "generic".to_owned(),
            auth_method: AuthMethod::Password,
            imap_host: "imap.example.com".to_string(),
            imap_port: 993,
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
        }
    }

    #[test]
    fn open_compose_initializes_state_and_active_view() {
        let mut app = Inboxly::default();
        app.accounts = vec![test_account("alan@example.com")];
        app.active_account_index = 0;

        let _ = app.update(Message::OpenCompose);

        assert_eq!(app.active_view, ActiveView::Compose);
        assert!(
            app.compose.draft_id.is_some(),
            "OpenCompose must eagerly assign a draft_id (Gemini G4)"
        );
        assert_eq!(app.compose.from_account_index, 0);
        assert_eq!(app.compose.send_state, ComposeSendState::Idle);
        assert!(!app.compose.dirty);
        assert!(app.compose.to.is_empty());
        assert!(app.compose.subject.is_empty());
    }

    #[test]
    fn open_compose_preserves_open_thread_id() {
        // Issue 1.7 regression: opening compose must NOT touch
        // open_thread_id. The thread stays open in the background;
        // navigating back to Inbox restores the user's reading position.
        let mut app = Inboxly::default();
        app.open_thread_id = Some("t1".to_string());

        let _ = app.update(Message::OpenCompose);

        assert_eq!(app.open_thread_id.as_deref(), Some("t1"));
        assert_eq!(app.active_view, ActiveView::Compose);
    }

    #[test]
    fn compose_subject_changed_marks_dirty_and_bumps_generation() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenCompose);
        let snapshot = app.compose.save_generation;
        assert!(!app.compose.dirty);

        let _ = app.update(Message::ComposeSubjectChanged("hello".to_string()));

        assert_eq!(app.compose.subject, "hello");
        assert!(app.compose.dirty);
        assert!(
            app.compose.save_generation > snapshot,
            "save_generation must bump on every field change (Issue 1.8)"
        );
    }

    #[test]
    fn compose_add_recipient_appends_to_correct_field() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenCompose);

        let _ = app.update(Message::ComposeAddRecipient {
            field: RecipientField::To,
            contact: Contact::new("Alan", "alan@example.com"),
        });
        let _ = app.update(Message::ComposeAddRecipient {
            field: RecipientField::Cc,
            contact: Contact::new("Bob", "bob@example.com"),
        });

        assert_eq!(app.compose.to.len(), 1);
        assert_eq!(app.compose.cc.len(), 1);
        assert!(app.compose.bcc.is_empty());
        assert_eq!(app.compose.to[0].address, "alan@example.com");
        assert_eq!(app.compose.cc[0].address, "bob@example.com");
        assert!(app.compose.dirty);
    }

    #[test]
    fn compose_remove_recipient_by_index() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenCompose);
        for (name, addr) in [
            ("Alpha", "a@example.com"),
            ("Beta", "b@example.com"),
            ("Gamma", "c@example.com"),
        ] {
            let _ = app.update(Message::ComposeAddRecipient {
                field: RecipientField::To,
                contact: Contact::new(name, addr),
            });
        }
        assert_eq!(app.compose.to.len(), 3);

        let _ = app.update(Message::ComposeRemoveRecipient {
            field: RecipientField::To,
            index: 1,
        });

        assert_eq!(app.compose.to.len(), 2);
        assert_eq!(app.compose.to[0].address, "a@example.com");
        assert_eq!(app.compose.to[1].address, "c@example.com");

        // Out-of-bounds removal must be a no-op (defensive bounds check).
        let _ = app.update(Message::ComposeRemoveRecipient {
            field: RecipientField::To,
            index: 99,
        });
        assert_eq!(app.compose.to.len(), 2);
    }

    #[test]
    fn compose_toggle_cc_bcc_and_preview() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenCompose);
        assert!(!app.compose.show_cc_bcc);
        assert!(!app.compose.show_preview);

        let _ = app.update(Message::ComposeToggleCcBcc);
        let _ = app.update(Message::ComposeTogglePreview);
        assert!(app.compose.show_cc_bcc);
        assert!(app.compose.show_preview);

        let _ = app.update(Message::ComposeToggleCcBcc);
        let _ = app.update(Message::ComposeTogglePreview);
        assert!(!app.compose.show_cc_bcc);
        assert!(!app.compose.show_preview);
    }

    #[test]
    fn compose_from_changed_switches_account_index() {
        let mut app = Inboxly::default();
        app.accounts = vec![
            test_account("alan@example.com"),
            test_account("alan@work.example.com"),
        ];
        let _ = app.update(Message::OpenCompose);
        assert_eq!(app.compose.from_account_index, 0);

        let _ = app.update(Message::ComposeFromChanged { account_index: 1 });
        assert_eq!(app.compose.from_account_index, 1);
        assert!(app.compose.dirty);

        // Out-of-bounds index must be ignored (defensive bounds check).
        let _ = app.update(Message::ComposeFromChanged { account_index: 99 });
        assert_eq!(app.compose.from_account_index, 1);
    }

    #[test]
    fn close_compose_preserves_open_thread_id() {
        // The CloseCompose handler must restore previous_view but never
        // mutate open_thread_id (mirrors Issue 1.7 for the close path).
        let mut app = Inboxly::default();
        app.open_thread_id = Some("t1".to_string());
        let _ = app.update(Message::OpenCompose);
        assert_eq!(app.active_view, ActiveView::Compose);

        let _ = app.update(Message::CloseCompose);

        assert_eq!(app.active_view, ActiveView::Inbox);
        assert_eq!(app.open_thread_id.as_deref(), Some("t1"));
    }

    #[test]
    fn compose_discard_draft_clears_state() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenCompose);
        let _ = app.update(Message::ComposeSubjectChanged("dirty subject".to_string()));
        let _ = app.update(Message::ComposeAddRecipient {
            field: RecipientField::To,
            contact: Contact::new("X", "x@example.com"),
        });
        assert!(app.compose.dirty);
        assert!(!app.compose.subject.is_empty());

        let _ = app.update(Message::ComposeDiscardDraft);

        assert_eq!(app.active_view, ActiveView::Inbox);
        assert!(app.compose.draft_id.is_none());
        assert!(app.compose.subject.is_empty());
        assert!(app.compose.to.is_empty());
        assert!(!app.compose.dirty);
        assert_eq!(app.compose.save_generation, 0);
        assert_eq!(app.compose.send_state, ComposeSendState::Idle);
    }

    #[test]
    fn validate_smtp_recipient_accepts_valid_addresses() {
        let plain = super::validate_smtp_recipient("alan@example.com")
            .expect("plain bare address should parse");
        assert_eq!(plain.address, "alan@example.com");

        let named = super::validate_smtp_recipient("Alan Gaudet <alan@example.com>")
            .expect("named address should parse");
        assert_eq!(named.address, "alan@example.com");
        assert_eq!(named.name, "Alan Gaudet");
    }

    #[test]
    fn validate_smtp_recipient_rejects_invalid() {
        // Empty input.
        assert!(super::validate_smtp_recipient("").is_err());
        assert!(super::validate_smtp_recipient("   ").is_err());

        // Garbage that doesn't contain an address.
        assert!(super::validate_smtp_recipient("not-an-email").is_err());

        // Two addresses on one line -- the validator only accepts one.
        let multi = super::validate_smtp_recipient("two@addresses.com, three@addresses.com");
        assert!(multi.is_err());
    }

    #[test]
    fn compose_attachment_too_large_does_not_add() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenCompose);
        assert!(app.compose.attachments.is_empty());

        let _ = app.update(Message::ComposeAttachmentTooLarge);

        // The handler is purely advisory in M35b -- it must NOT mutate
        // the attachments list. Phase 8 surfaces a snackbar.
        assert!(app.compose.attachments.is_empty());
        assert!(!app.compose.dirty);
    }

    #[test]
    fn compose_dismiss_sent_notice_clears_state_from_sent() {
        // Gemini G9 two-phase commit dismiss flow: from Sent state, the
        // dismiss message resets compose to default and returns to the
        // previous view.
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenCompose);
        let _ = app.update(Message::ComposeSubjectChanged("Outgoing".to_string()));
        // Manually park the state in Sent { dismiss_pending: true } as
        // the Phase 12 send bridge would.
        app.compose.send_state = ComposeSendState::Sent {
            dismiss_pending: true,
        };

        let _ = app.update(Message::ComposeDismissSentNotice);

        assert_eq!(app.compose.send_state, ComposeSendState::Idle);
        assert!(app.compose.draft_id.is_none());
        assert!(app.compose.subject.is_empty());
        assert!(!app.compose.dirty);
        assert_eq!(app.active_view, ActiveView::Inbox);
    }

    #[test]
    fn compose_subject_changed_blocked_during_sending() {
        // Gemini G5 isolation: while the SMTP send is in flight, every
        // field-change handler must refuse to mutate state. Otherwise
        // the user could type into the subject mid-send and the
        // committed-to-disk draft would diverge from what was actually
        // put on the wire.
        let mut app = Inboxly::default();
        let _ = app.update(Message::OpenCompose);
        let _ = app.update(Message::ComposeSubjectChanged("original".to_string()));
        app.compose.send_state = ComposeSendState::Sending;
        let snapshot_generation = app.compose.save_generation;

        let _ = app.update(Message::ComposeSubjectChanged("clobbered".to_string()));

        assert_eq!(
            app.compose.subject, "original",
            "subject must not change while Sending"
        );
        assert_eq!(
            app.compose.send_state,
            ComposeSendState::Sending,
            "send_state must remain Sending"
        );
        assert_eq!(
            app.compose.save_generation, snapshot_generation,
            "save_generation must not bump while Sending"
        );
    }
}
