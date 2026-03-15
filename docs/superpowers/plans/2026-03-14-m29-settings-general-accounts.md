# M29: Settings View — General + Accounts + Data & Storage — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Build the settings view framework (sidebar + content area layout, toolbar changes, navigation messages) and implement three of the six settings tabs: General, Accounts, and Data & Storage.

**Architecture:** The settings view replaces the main content area when `ActiveView::Settings` is active. The nav drawer is hidden and the toolbar changes to neutral grey `#455a64` with a back arrow replacing the hamburger. A 240px sidebar provides tab navigation within settings. Each tab renders its own content panel (640px max-width, scrollable). Settings persist to either the SQLite settings table (via `SettingsReader`/`SettingsWriter` traits) or to `AppConfig.toml` (for account and snooze data).

**Tech Stack:** Rust, iced 0.14, inboxly-core (`AppConfig`, `AccountConfig`, `SnoozePresets`, `ThemePreference`), inboxly-store (`Store`, settings table), inboxly-ui (views, theme)

**Prerequisites:**
- M27 complete — `ActiveView::Settings` variant exists, gear icon in toolbar navigates to it
- M26 complete — `PopupMenu` widget available (used for account remove confirmation)
- `SettingsReader` / `SettingsWriter` traits exist in `inboxly-ui/src/theme/mod.rs`
- `Store::get_setting` / `Store::set_setting` exist in `inboxly-store/src/settings.rs`
- `AppConfig` with `accounts: Vec<AccountConfig>` and `snooze: SnoozePresets` exists in `inboxly-core/src/config.rs`
- `Store::list_accounts` / `Store::delete_account` / `Store::update_account` exist in `inboxly-store/src/accounts.rs`
- `SearchIndex::rebuild` exists in `inboxly-store/src/search/mod.rs`

**Branch:** `m29-settings-general-accounts`

---

## Gap Analysis

Before implementation, the worker MUST verify these assumptions against the actual codebase. Known gaps and uncertainties:

1. **`ActiveView::Settings` variant** — the spec says M27 adds this. If M27 is not yet implemented, the worker must add `Settings` to the `ActiveView` enum in `inboxly-ui/src/theme/mod.rs` as part of Task 1. Check: does `ActiveView` have a `Settings` variant?

2. **`SettingsReader`/`SettingsWriter` are NOT implemented on `Store`** — the traits exist in `inboxly-ui/src/theme/mod.rs` but `Store` (in `inboxly-store`) does not implement them. The Store has its own `get_setting`/`set_setting` methods returning `Result<_, StoreError>`, while the traits return `Result<_, Box<dyn Error>>`. Task 2 bridges this gap.

3. **`PopupMenu` widget** — M26 adds this. If not yet available, the account removal confirmation can use a simple two-button row as a temporary fallback. The worker should check if `inboxly-ui/src/widgets/popup_menu.rs` exists.

4. **`Inboxly::new` signature** — currently `fn new() -> (Self, Task<Message>)`. The Iced application builder calls this as the init function. No changes needed to the binary crate.

5. **`Paths` struct** — `inboxly_core::config::Paths` provides `cache_dir`, `data_dir`, `search_index_dir()`, `database_file()`. These are needed for Data & Storage tab size calculations.

6. **`view_toolbar` in `toolbar.rs`** — currently takes `&Inboxly` and uses `app.active_view.toolbar_color()` for the background. The Settings view needs a neutral grey toolbar with back arrow instead of hamburger and "Settings" title. The toolbar function must be updated to handle this case.

7. **Nav drawer visibility** — `app.drawer_open` controls this. When entering Settings, we set `drawer_open = false` and remember the previous state. When leaving, we restore it.

---

## Task 1: Add `ActiveView::Settings` and `SettingsTab` enum

**Prerequisite check:** If M27 already added `ActiveView::Settings`, skip the `ActiveView` edit and only add `SettingsTab`.

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/mod.rs` (edit)

Add `Settings` to the `ActiveView` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ActiveView {
    #[default]
    Inbox,
    Snoozed,
    Done,
    Settings,
}

impl ActiveView {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Inbox => "Inbox",
            Self::Snoozed => "Snoozed",
            Self::Done => "Done",
            Self::Settings => "Settings",
        }
    }

    pub fn toolbar_color(&self) -> Color {
        match self {
            Self::Inbox => color_from_hex(0x42, 0x85, 0xf4),
            Self::Snoozed => color_from_hex(0xef, 0x6c, 0x00),
            Self::Done => color_from_hex(0x0f, 0x9d, 0x58),
            Self::Settings => color_from_hex(0x45, 0x5a, 0x64), // neutral grey
        }
    }

    pub fn toolbar_color_themed(&self, theme: &InboxlyTheme) -> Color {
        match self {
            Self::Inbox => theme.colors.toolbar_inbox,
            Self::Snoozed => theme.colors.toolbar_snoozed,
            Self::Done => theme.colors.toolbar_done,
            Self::Settings => color_from_hex(0x45, 0x5a, 0x64), // same grey in both themes
        }
    }
}
```

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/settings_view.rs` (new file)

Define the `SettingsTab` enum:

```rust
/// The six tabs in the settings sidebar.
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
```

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/mod.rs` (edit)

Add `pub mod settings_view;`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add ActiveView::Settings variant and SettingsTab enum`

---

## Task 2: Implement `SettingsReader`/`SettingsWriter` for `Store`

The traits are defined in `inboxly-ui`, but `inboxly-store` cannot depend on `inboxly-ui` (circular). Move the traits to `inboxly-core` (which both crates depend on) or have `inboxly-ui` provide a blanket adapter. The cleanest approach: the traits stay in `inboxly-ui` and we add a wrapper struct.

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/settings_view.rs` (append)

Add a wrapper that adapts `Store`'s methods to the `SettingsReader`/`SettingsWriter` traits:

```rust
use crate::theme::{SettingsReader, SettingsWriter};

/// Adapter that wraps an `inboxly_store::Store` reference and implements
/// the `SettingsReader`/`SettingsWriter` traits from `inboxly-ui`.
///
/// This avoids a circular dependency between `inboxly-store` and `inboxly-ui`.
pub struct StoreSettingsAdapter<'a> {
    pub store: &'a inboxly_store::Store,
}

impl SettingsReader for StoreSettingsAdapter<'_> {
    fn get_setting(&self, key: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
        self.store.get_setting(key).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}

impl SettingsWriter for StoreSettingsAdapter<'_> {
    fn set_setting(&self, key: &str, value: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.store.set_setting(key, value).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}
```

**Note:** `inboxly-ui` already depends on `inboxly-store` (for `Store` in `app.rs`), so this import is valid.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add StoreSettingsAdapter bridging Store to SettingsReader/Writer traits`

---

## Task 3: Add settings state to `Inboxly` app struct and new messages

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (edit)

Add new fields to `Inboxly`:

```rust
use crate::views::settings_view::SettingsTab;
use inboxly_core::config::{AccountConfig, AppConfig, SnoozePresets, ThemePreference};

pub struct Inboxly {
    // ... existing fields ...

    /// Active settings tab (only relevant when active_view == Settings).
    pub settings_tab: SettingsTab,
    /// The view to restore when leaving settings (back arrow).
    pub previous_view: ActiveView,
    /// Whether the drawer was open before entering settings.
    pub drawer_was_open: bool,
    /// Loaded AppConfig (for accounts + snooze presets editing).
    pub config: AppConfig,

    // -- General tab state --
    /// Current theme preference (System/Light/Dark chip selection).
    pub theme_preference: ThemePreference,
    /// Default view preference.
    pub default_view: String,       // "inbox", "snoozed", "done"
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
}
```

Add new message variants:

```rust
pub enum Message {
    // ... existing variants ...

    // -- Settings navigation --
    /// Open settings view (gear icon pressed).
    OpenSettings,
    /// Back arrow pressed in settings toolbar -- return to previous view.
    CloseSettings,
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
    /// Async: cache sizes calculated.
    DataSizesLoaded {
        db_size: String,
        index_size: String,
        maildir_size: String,
        last_sync: String,
    },
}
```

Update `Default` impl for `Inboxly` with initial values for new fields (all defaults: `settings_tab: SettingsTab::General`, `previous_view: ActiveView::Inbox`, `drawer_was_open: true`, `config: AppConfig::default()`, etc.).

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add settings state fields and message variants to Inboxly`

---

## Task 4: Implement settings navigation in `update()`

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (edit — add match arms in `update`)

```rust
Message::OpenSettings => {
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
            .and_then(|v| match v.as_str() {
                "light" => Some(ThemePreference::Light),
                "dark" => Some(ThemePreference::Dark),
                "system" | _ => Some(ThemePreference::System),
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
    }
    // Load config for snooze presets and accounts
    if let Ok(config) = AppConfig::load() {
        self.config = config;
    }
}

Message::CloseSettings => {
    self.active_view = self.previous_view;
    self.active_nav = NavTarget::View(self.previous_view);
    self.drawer_open = self.drawer_was_open;
    // Apply theme from settings
    self.theme = InboxlyTheme::from_preference(self.theme_preference);
}

Message::SettingsTabChanged(tab) => {
    self.settings_tab = tab;
    // Reset edit state when switching tabs
    self.editing_account_index = None;
    self.adding_account = false;
    self.removing_account_index = None;
    self.data_action_status = None;
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement settings open/close/tab navigation in update()`

---

## Task 5: Implement General tab update handlers

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (edit — add match arms)

```rust
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
    if let Some(ref store) = self.store {
        if let Err(e) = store.set_setting("default_view", &view) {
            tracing::warn!("failed to persist default view: {e}");
        }
    }
}

Message::SetUndoTimeout(secs) => {
    self.undo_timeout_secs = secs;
    if let Some(ref store) = self.store {
        if let Err(e) = store.set_setting("undo_timeout_secs", &secs.to_string()) {
            tracing::warn!("failed to persist undo timeout: {e}");
        }
    }
}

Message::SetSnoozeMorningHour(val) => {
    if let Ok(hour) = val.parse::<u8>() {
        if hour <= 23 {
            self.config.snooze.morning_hour = hour;
            if let Err(e) = self.config.save() {
                tracing::warn!("failed to save config: {e}");
            }
        }
    }
}

// SetSnoozeAfternoonHour, SetSnoozeEveningHour, SetSnoozeWeekendDay follow same pattern
// Parse, validate range, update self.config.snooze.*, save config
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement General settings tab update handlers`

---

## Task 6: Implement Accounts tab update handlers

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (edit — add match arms)

```rust
Message::AddAccountStart => {
    self.adding_account = true;
    self.editing_account_index = None;
    self.account_form = AccountConfig {
        email: String::new(),
        display_name: String::new(),
        provider: "generic".to_owned(),
        auth_method: inboxly_core::config::AuthMethod::Password,
        imap_host: String::new(),
        imap_port: 993,
        smtp_host: String::new(),
        smtp_port: 587,
    };
}

Message::EditAccountStart(index) => {
    if let Some(account) = self.config.accounts.get(index) {
        self.account_form = account.clone();
        self.editing_account_index = Some(index);
        self.adding_account = false;
    }
}

Message::AccountFormCancel => {
    self.editing_account_index = None;
    self.adding_account = false;
}

Message::AccountFormSave => {
    // Validate form
    let form = &self.account_form;
    if form.email.is_empty() || !form.email.contains('@')
        || form.imap_host.is_empty() || form.smtp_host.is_empty()
    {
        // Invalid — do nothing (UI should show inline validation hints)
        return Task::none();
    }

    if self.adding_account {
        self.config.accounts.push(self.account_form.clone());
    } else if let Some(index) = self.editing_account_index {
        if let Some(account) = self.config.accounts.get_mut(index) {
            *account = self.account_form.clone();
        }
    }

    if let Err(e) = self.config.save() {
        tracing::warn!("failed to save config after account update: {e}");
    }
    self.editing_account_index = None;
    self.adding_account = false;
}

// Account form field changed handlers (each updates the corresponding field on self.account_form)
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
    if index < self.config.accounts.len() {
        self.config.accounts.remove(index);
        if let Err(e) = self.config.save() {
            tracing::warn!("failed to save config after account removal: {e}");
        }
    }
    self.removing_account_index = None;
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement Accounts tab update handlers`

---

## Task 7: Implement Data & Storage tab update handlers

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (edit — add match arms)

```rust
Message::ClearCache => {
    if let Ok(Some(paths)) = inboxly_core::config::AppConfig::load()
        .map(|c| inboxly_core::config::Paths::resolve_with_config(&c))
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
    // Note: full rebuild is async/heavy. For now, show status.
    // The actual rebuild requires SearchIndex::rebuild(path, source) which
    // needs a RebuildSource impl. This is a stub that clears the index.
    // Full implementation requires wiring SearchIndex into app state (future M).
    tracing::info!("search index rebuild requested");
}

Message::ExportData => {
    self.data_action_status = Some("Export not yet implemented".to_owned());
    tracing::info!("data export requested (stub)");
}

Message::DataSizesLoaded { db_size, index_size, maildir_size, last_sync } => {
    self.db_size_display = db_size;
    self.index_size_display = index_size;
    self.maildir_size_display = maildir_size;
    self.last_sync_display = last_sync;
}
```

Add a helper function to calculate directory sizes:

```rust
/// Calculate the total size of a directory in bytes.
fn dir_size(path: &std::path::Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
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
```

**Important:** Add `walkdir` as a dependency to `inboxly-ui/Cargo.toml`. If the worker prefers to avoid the extra dependency, they can use `std::fs::read_dir` with a recursive helper instead.

When entering the Data & Storage tab (in `SettingsTabChanged` handler), compute and load the sizes:

```rust
Message::SettingsTabChanged(tab) => {
    self.settings_tab = tab;
    // ... existing reset code ...
    if tab == SettingsTab::DataStorage {
        // Load sizes synchronously (fast for small dirs)
        if let Ok(config) = AppConfig::load() {
            if let Some(paths) = Paths::resolve_with_config(&config) {
                self.db_size_display = format_size(
                    paths.database_file().metadata().map(|m| m.len()).unwrap_or(0)
                );
                self.index_size_display = format_size(dir_size(&paths.search_index_dir()));
                self.maildir_size_display = format_size(dir_size(&paths.maildir_root()));
            }
        }
        // Last sync from store
        if let Some(ref store) = self.store {
            // Query the max(last_sync) from sync_state table
            self.last_sync_display = store
                .get_setting("last_full_sync")
                .ok()
                .flatten()
                .unwrap_or_else(|| "Never".to_owned());
        }
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement Data & Storage tab update handlers with size calculation`

---

## Task 8: Update toolbar for settings view

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/toolbar.rs` (edit)

Modify `view_toolbar` to handle `ActiveView::Settings`:

```rust
pub fn view_toolbar(app: &Inboxly) -> Element<'_, Message> {
    let is_settings = app.active_view == ActiveView::Settings;

    let toolbar_bg = if is_settings {
        color_from_hex(0x45, 0x5a, 0x64) // neutral grey
    } else {
        app.active_view.toolbar_color()
    };

    // Hamburger or back arrow
    let nav_button = if is_settings {
        button(text("\u{2190}").size(20.0).color(Color::WHITE)) // ← back arrow
            .on_press(Message::CloseSettings)
            .padding([8, 12])
            .style(move |_theme, _status| button::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                text_color: Color::WHITE,
                border: Border::default(),
                ..Default::default()
            })
    } else {
        button(text("\u{2630}").size(20.0).color(Color::WHITE)) // ☰ hamburger
            .on_press(Message::ToggleDrawer)
            .padding([8, 12])
            .style(move |_theme, _status| button::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                text_color: Color::WHITE,
                border: Border::default(),
                ..Default::default()
            })
    };

    // Title: "Settings" when in settings, view name otherwise
    let title = text(app.active_view.title())
        .size(TOOLBAR_TITLE_SIZE)
        .color(Color::WHITE);

    // Hide search bar and show simplified toolbar when in settings
    if is_settings {
        let toolbar_row = row![
            nav_button,
            title,
        ]
        .spacing(12)
        .padding([0.0, DEFAULT_PADDING])
        .align_y(Alignment::Center)
        .height(TOOLBAR_HEIGHT)
        .width(Length::Fill);

        container(toolbar_row)
            .width(Length::Fill)
            .height(TOOLBAR_HEIGHT)
            .style(move |_theme| container::Style {
                background: Some(Background::Color(toolbar_bg)),
                ..Default::default()
            })
            .into()
    } else {
        // ... existing toolbar rendering (search bar, avatar, gear icon) ...
    }
}
```

Also add a gear icon button to the non-settings toolbar (between search bar and avatar):

```rust
// Gear icon
let gear = button(text("\u{2699}").size(20.0).color(Color::WHITE)) // ⚙
    .on_press(Message::OpenSettings)
    .padding([8, 12])
    .style(move |_theme, _status| button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: Color::WHITE,
        border: Border::default(),
        ..Default::default()
    });
```

Insert `gear` into the toolbar row between the fill space and the avatar.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): update toolbar for settings view (grey bg, back arrow, gear icon)`

---

## Task 9: Implement the settings view layout (sidebar + content area)

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/settings_view.rs` (edit — add view function)

```rust
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::app::{Inboxly, Message};
use crate::theme::{
    DIVIDER_THICKNESS, color_from_hex, primary_text, secondary_text,
    surface_color, divider_color, selected_bg,
};

/// Settings sidebar width.
const SETTINGS_SIDEBAR_WIDTH: f32 = 240.0;
/// Settings content max width.
const SETTINGS_CONTENT_MAX_WIDTH: f32 = 640.0;
/// Settings content padding.
const SETTINGS_CONTENT_PADDING: f32 = 32.0;

/// Render the full settings view (sidebar + content area).
pub fn settings_view(app: &Inboxly) -> Element<'_, Message> {
    let sidebar = settings_sidebar(app);
    let content = settings_content(app);

    row![sidebar, content]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Render the settings sidebar (240px, 6 tabs).
fn settings_sidebar(app: &Inboxly) -> Element<'_, Message> {
    let mut sidebar = column![].width(SETTINGS_SIDEBAR_WIDTH);

    for tab in SettingsTab::all() {
        let is_active = app.settings_tab == *tab;
        sidebar = sidebar.push(settings_tab_item(tab.label(), *tab, is_active));
    }

    container(sidebar)
        .width(SETTINGS_SIDEBAR_WIDTH)
        .height(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Background::Color(surface_color())),
            ..Default::default()
        })
        .into()
}

/// Single settings tab button in the sidebar.
fn settings_tab_item(label: &str, tab: SettingsTab, is_active: bool) -> Element<'_, Message> {
    let bg = if is_active {
        color_from_hex(0xe8, 0xf0, 0xfe) // light blue bg
    } else {
        surface_color()
    };

    let label_color = if is_active {
        color_from_hex(0x42, 0x85, 0xf4) // blue text
    } else {
        primary_text()
    };

    // Active indicator: 3px left border
    let left_border = if is_active {
        container(Space::new().width(3.0).height(Length::Fill))
            .style(|_theme| container::Style {
                background: Some(Background::Color(color_from_hex(0x42, 0x85, 0xf4))),
                ..Default::default()
            })
    } else {
        container(Space::new().width(3.0).height(Length::Fill))
    };

    let label_widget = text(label.to_string())
        .size(14.0)
        .color(label_color);

    let content = row![left_border, container(label_widget).padding([12.0, 16.0])]
        .align_y(Alignment::Center)
        .height(44.0)
        .width(Length::Fill);

    button(content)
        .on_press(Message::SettingsTabChanged(tab))
        .width(Length::Fill)
        .style(move |_theme, _status| button::Style {
            background: Some(Background::Color(bg)),
            border: Border::default(),
            ..Default::default()
        })
        .into()
}

/// Route to the correct tab's content renderer.
fn settings_content(app: &Inboxly) -> Element<'_, Message> {
    let inner: Element<'_, Message> = match app.settings_tab {
        SettingsTab::General => general_tab(app),
        SettingsTab::Accounts => accounts_tab(app),
        SettingsTab::DataStorage => data_storage_tab(app),
        // Placeholder for future tabs
        _ => container(
            text(format!("{} — coming in M30", app.settings_tab.label()))
                .size(16.0)
                .color(secondary_text()),
        )
        .padding(SETTINGS_CONTENT_PADDING)
        .into(),
    };

    container(
        scrollable(
            container(inner)
                .max_width(SETTINGS_CONTENT_MAX_WIDTH)
                .padding(SETTINGS_CONTENT_PADDING),
        )
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement settings view layout with sidebar navigation`

---

## Task 10: Implement General tab content view

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/settings_view.rs` (append)

```rust
/// Render the General settings tab content.
fn general_tab(app: &Inboxly) -> Element<'_, Message> {
    let mut content = column![].spacing(24);

    // -- Theme section --
    content = content.push(section_header("Theme"));
    content = content.push(theme_chips(app.theme_preference));

    // -- Default View section --
    content = content.push(section_header("Default View"));
    content = content.push(default_view_selector(&app.default_view));

    // -- Snooze Presets section --
    content = content.push(section_header("Snooze Presets"));
    content = content.push(snooze_presets_form(&app.config.snooze));

    // -- Undo Timeout section --
    content = content.push(section_header("Undo Timeout"));
    content = content.push(undo_timeout_selector(app.undo_timeout_secs));

    content.into()
}

/// Section header text (bold, 16px).
fn section_header(label: &str) -> Element<'_, Message> {
    text(label.to_string())
        .size(16.0)
        .color(primary_text())
        // Note: Iced 0.14 may use .font() for weight. Worker should use
        // iced::Font { weight: Weight::Bold, .. } if available.
        .into()
}

/// Three chip buttons for theme selection (System / Light / Dark).
fn theme_chips(current: ThemePreference) -> Element<'_, Message> {
    let chips = row![
        theme_chip("System", ThemePreference::System, current == ThemePreference::System),
        theme_chip("Light", ThemePreference::Light, current == ThemePreference::Light),
        theme_chip("Dark", ThemePreference::Dark, current == ThemePreference::Dark),
    ]
    .spacing(8);

    chips.into()
}

/// Single theme chip button.
fn theme_chip(label: &str, pref: ThemePreference, is_active: bool) -> Element<'_, Message> {
    let bg = if is_active {
        color_from_hex(0x42, 0x85, 0xf4) // blue for active
    } else {
        surface_color()
    };
    let text_color = if is_active {
        Color::WHITE
    } else {
        primary_text()
    };
    let border_color = if is_active {
        color_from_hex(0x42, 0x85, 0xf4)
    } else {
        divider_color()
    };

    button(
        text(label.to_string())
            .size(14.0)
            .color(text_color)
    )
    .padding([8, 16])
    .on_press(Message::SetThemePreference(pref))
    .style(move |_theme, _status| button::Style {
        background: Some(Background::Color(bg)),
        text_color,
        border: Border {
            radius: 16.0.into(),
            width: 1.0,
            color: border_color,
        },
        ..Default::default()
    })
    .into()
}

/// Default view dropdown (rendered as 3 chip buttons for consistency).
fn default_view_selector(current: &str) -> Element<'_, Message> {
    let options = [("Inbox", "inbox"), ("Snoozed", "snoozed"), ("Done", "done")];
    let mut chips = row![].spacing(8);
    for (label, value) in options {
        let is_active = current == value;
        let bg = if is_active {
            color_from_hex(0x42, 0x85, 0xf4)
        } else {
            surface_color()
        };
        let text_color = if is_active { Color::WHITE } else { primary_text() };
        let border_color = if is_active {
            color_from_hex(0x42, 0x85, 0xf4)
        } else {
            divider_color()
        };
        chips = chips.push(
            button(text(label.to_string()).size(14.0).color(text_color))
                .padding([8, 16])
                .on_press(Message::SetDefaultView(value.to_owned()))
                .style(move |_theme, _status| button::Style {
                    background: Some(Background::Color(bg)),
                    text_color,
                    border: Border {
                        radius: 16.0.into(),
                        width: 1.0,
                        color: border_color,
                    },
                    ..Default::default()
                }),
        );
    }
    chips.into()
}

/// Snooze presets form (4 fields).
fn snooze_presets_form(presets: &SnoozePresets) -> Element<'_, Message> {
    let morning = labeled_input(
        "Morning",
        &presets.morning_hour.to_string(),
        "Hour (0-23)",
        Message::SetSnoozeMorningHour,
    );
    let afternoon = labeled_input(
        "Afternoon",
        &presets.afternoon_hour.to_string(),
        "Hour (0-23)",
        Message::SetSnoozeAfternoonHour,
    );
    let evening = labeled_input(
        "Evening",
        &presets.evening_hour.to_string(),
        "Hour (0-23)",
        Message::SetSnoozeEveningHour,
    );

    let weekend_labels = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let weekend_label = weekend_labels
        .get(presets.weekend_day as usize)
        .unwrap_or(&"Sat");
    let weekend = labeled_input(
        "Weekend day",
        &weekend_label.to_string(),
        "0=Mon .. 6=Sun",
        Message::SetSnoozeWeekendDay,
    );

    column![morning, afternoon, evening, weekend]
        .spacing(12)
        .into()
}

/// A labeled text input row: "Label: [input]"
fn labeled_input<'a>(
    label: &str,
    value: &str,
    placeholder: &str,
    on_change: fn(String) -> Message,
) -> Element<'a, Message> {
    row![
        text(label.to_string()).size(14.0).width(120.0),
        text_input(placeholder, value)
            .on_input(on_change)
            .width(200.0)
            .padding([6, 10]),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

/// Undo timeout selector (rendered as chip buttons).
fn undo_timeout_selector(current_secs: u32) -> Element<'_, Message> {
    let options: &[(u32, &str)] = &[
        (3, "3s"), (5, "5s"), (7, "7s"), (10, "10s"), (15, "15s"),
    ];
    let mut chips = row![].spacing(8);
    for (secs, label) in options {
        let is_active = current_secs == *secs;
        let bg = if is_active {
            color_from_hex(0x42, 0x85, 0xf4)
        } else {
            surface_color()
        };
        let text_color = if is_active { Color::WHITE } else { primary_text() };
        let border_color = if is_active {
            color_from_hex(0x42, 0x85, 0xf4)
        } else {
            divider_color()
        };
        let secs_val = *secs;
        chips = chips.push(
            button(text(label.to_string()).size(14.0).color(text_color))
                .padding([8, 16])
                .on_press(Message::SetUndoTimeout(secs_val))
                .style(move |_theme, _status| button::Style {
                    background: Some(Background::Color(bg)),
                    text_color,
                    border: Border {
                        radius: 16.0.into(),
                        width: 1.0,
                        color: border_color,
                    },
                    ..Default::default()
                }),
        );
    }
    chips.into()
}
```

**Note:** The worker must add `use iced::widget::text_input;` to the imports at the top of the file.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement General settings tab view (theme, default view, snooze, undo)`

---

## Task 11: Implement Accounts tab content view

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/settings_view.rs` (append)

```rust
/// Render the Accounts settings tab content.
fn accounts_tab(app: &Inboxly) -> Element<'_, Message> {
    let mut content = column![].spacing(16);

    // Header row with "+ Add Account" button
    let header = row![
        text("Accounts").size(16.0).color(primary_text()),
        Space::new().width(Length::Fill),
        button(
            text("+ Add Account").size(14.0).color(Color::WHITE)
        )
        .padding([8, 16])
        .on_press(Message::AddAccountStart)
        .style(|_theme, _status| button::Style {
            background: Some(Background::Color(color_from_hex(0x42, 0x85, 0xf4))),
            text_color: Color::WHITE,
            border: Border {
                radius: 16.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }),
    ]
    .align_y(Alignment::Center);
    content = content.push(header);

    // Add account form (if active)
    if app.adding_account {
        content = content.push(account_form(&app.account_form, None));
    }

    // Account cards
    for (index, account) in app.config.accounts.iter().enumerate() {
        if app.editing_account_index == Some(index) {
            // Show edit form instead of card
            content = content.push(account_form(&app.account_form, Some(index)));
        } else {
            content = content.push(account_card(account, index, app.removing_account_index));
        }
    }

    if app.config.accounts.is_empty() && !app.adding_account {
        content = content.push(
            container(
                text("No accounts configured").size(14.0).color(secondary_text()),
            )
            .padding(24),
        );
    }

    content.into()
}

/// Render a single account card.
fn account_card(
    account: &AccountConfig,
    index: usize,
    removing_index: Option<usize>,
) -> Element<'_, Message> {
    // Avatar (48px, first letter)
    let avatar_letter = account
        .email
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();

    let avatar = container(
        text(avatar_letter)
            .size(20.0)
            .color(Color::WHITE)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
    )
    .width(48.0)
    .height(48.0)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center)
    .style(|_theme| container::Style {
        background: Some(Background::Color(color_from_hex(0x42, 0x85, 0xf4))),
        border: Border {
            radius: 24.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });

    // Info column
    let auth_label = match account.auth_method {
        AuthMethod::Password => "Password",
        AuthMethod::OAuth2 => "OAuth2",
        AuthMethod::AppPassword => "App Password",
    };
    let info = column![
        text(account.email.clone()).size(17.0).color(primary_text()),
        text(format!("{} • {} • {}:{}", account.provider, auth_label, account.imap_host, account.imap_port))
            .size(14.0)
            .color(secondary_text()),
    ]
    .spacing(4);

    // Action buttons
    let edit_btn = button(text("Edit").size(13.0))
        .padding([6, 12])
        .on_press(Message::EditAccountStart(index));

    let remove_btn = button(
        text("Remove").size(13.0).color(color_from_hex(0xef, 0x53, 0x50)), // red
    )
    .padding([6, 12])
    .on_press(Message::RemoveAccountConfirm(index));

    let actions = row![edit_btn, remove_btn].spacing(8);

    let mut card = column![
        row![avatar, info, Space::new().width(Length::Fill), actions]
            .spacing(16)
            .align_y(Alignment::Center)
            .padding(16),
    ];

    // Removal confirmation overlay
    if removing_index == Some(index) {
        let confirm_row = container(
            row![
                text("Remove account?").size(14.0).color(primary_text()),
                Space::new().width(Length::Fill),
                button(text("Cancel").size(13.0))
                    .padding([6, 12])
                    .on_press(Message::RemoveAccountCancel),
                button(
                    text("Remove").size(13.0).color(color_from_hex(0xef, 0x53, 0x50)),
                )
                .padding([6, 12])
                .on_press(Message::RemoveAccountExecute(index)),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .padding(12),
        )
        .style(|_theme| container::Style {
            background: Some(Background::Color(color_from_hex(0xff, 0xeb, 0xee))), // light red bg
            ..Default::default()
        });
        card = card.push(confirm_row);
    }

    // Card container with border
    container(card)
        .width(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Background::Color(surface_color())),
            border: Border {
                radius: 8.0.into(),
                width: 1.0,
                color: divider_color(),
            },
            ..Default::default()
        })
        .into()
}

/// Render the add/edit account form.
fn account_form(form: &AccountConfig, editing_index: Option<usize>) -> Element<'_, Message> {
    let title = if editing_index.is_some() {
        "Edit Account"
    } else {
        "Add Account"
    };

    let form_content = column![
        text(title.to_string()).size(16.0).color(primary_text()),
        labeled_input("Email", &form.email, "user@example.com", Message::AccountFormEmailChanged),
        labeled_input("Display Name", &form.display_name, "Your Name", Message::AccountFormDisplayNameChanged),
        labeled_input("Provider", &form.provider, "gmail / fastmail / generic", Message::AccountFormProviderChanged),
        labeled_input("Auth Method", &format!("{:?}", form.auth_method).to_lowercase(), "password / oauth2 / app_password", Message::AccountFormAuthMethodChanged),
        labeled_input("IMAP Host", &form.imap_host, "imap.example.com", Message::AccountFormImapHostChanged),
        labeled_input("IMAP Port", &form.imap_port.to_string(), "993", Message::AccountFormImapPortChanged),
        labeled_input("SMTP Host", &form.smtp_host, "smtp.example.com", Message::AccountFormSmtpHostChanged),
        labeled_input("SMTP Port", &form.smtp_port.to_string(), "587", Message::AccountFormSmtpPortChanged),
        row![
            button(text("Cancel").size(14.0))
                .padding([8, 16])
                .on_press(Message::AccountFormCancel),
            button(text("Save").size(14.0).color(Color::WHITE))
                .padding([8, 16])
                .on_press(Message::AccountFormSave)
                .style(|_theme, _status| button::Style {
                    background: Some(Background::Color(color_from_hex(0x42, 0x85, 0xf4))),
                    text_color: Color::WHITE,
                    border: Border { radius: 4.0.into(), ..Default::default() },
                    ..Default::default()
                }),
        ]
        .spacing(12),
    ]
    .spacing(12)
    .padding(16);

    container(form_content)
        .width(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Background::Color(surface_color())),
            border: Border {
                radius: 8.0.into(),
                width: 1.0,
                color: color_from_hex(0x42, 0x85, 0xf4),
            },
            ..Default::default()
        })
        .into()
}
```

**Note:** The `AuthMethod` formatting in the form uses a debug-style lowercase. The worker should implement proper display: `Password` → `"password"`, `OAuth2` → `"oauth2"`, `AppPassword` → `"app_password"`. This aligns with the serde `rename_all = "snake_case"` on `AuthMethod`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement Accounts tab view (cards, add/edit form, remove confirmation)`

---

## Task 12: Implement Data & Storage tab content view

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/settings_view.rs` (append)

```rust
/// Render the Data & Storage settings tab content.
fn data_storage_tab(app: &Inboxly) -> Element<'_, Message> {
    let mut content = column![].spacing(24);

    content = content.push(section_header("Data & Storage"));

    // -- Action buttons section --
    content = content.push(
        column![
            action_button("Clear Cache", Message::ClearCache, false),
            action_button("Rebuild Search Index", Message::RebuildSearchIndex, false),
            action_button("Export Data", Message::ExportData, false),
        ]
        .spacing(8),
    );

    // Status message (if any)
    if let Some(ref status) = app.data_action_status {
        content = content.push(
            container(
                text(status.clone()).size(14.0).color(color_from_hex(0x0f, 0x9d, 0x58)), // green
            )
            .padding([8, 0]),
        );
    }

    // -- Storage info section --
    content = content.push(section_header("Storage"));
    content = content.push(
        column![
            info_row("Database (SQLite)", &app.db_size_display),
            info_row("Search Index (Tantivy)", &app.index_size_display),
            info_row("Mail Storage (Maildir)", &app.maildir_size_display),
        ]
        .spacing(8),
    );

    // -- Sync info --
    content = content.push(section_header("Sync"));
    content = content.push(info_row("Last Full Sync", &app.last_sync_display));

    content.into()
}

/// An action button (full-width, outline style).
fn action_button(label: &str, message: Message, destructive: bool) -> Element<'_, Message> {
    let text_color = if destructive {
        color_from_hex(0xef, 0x53, 0x50) // red
    } else {
        color_from_hex(0x42, 0x85, 0xf4) // blue
    };
    let border_color = text_color;

    button(text(label.to_string()).size(14.0).color(text_color))
        .padding([10, 16])
        .width(Length::Fill)
        .on_press(message)
        .style(move |_theme, _status| button::Style {
            background: Some(Background::Color(surface_color())),
            text_color,
            border: Border {
                radius: 8.0.into(),
                width: 1.0,
                color: border_color,
            },
            ..Default::default()
        })
        .into()
}

/// A read-only info row: "Label: value".
fn info_row<'a>(label: &str, value: &str) -> Element<'a, Message> {
    row![
        text(label.to_string()).size(14.0).color(secondary_text()).width(200.0),
        text(value.to_string()).size(14.0).color(primary_text()),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement Data & Storage tab view (actions, sizes, sync info)`

---

## Task 13: Wire settings view into `app.rs` view routing

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (edit — update `view()`)

Update the `view()` method to render the settings view when `active_view == Settings`:

```rust
pub fn view(&self) -> Element<'_, Message> {
    use crate::nav::view_drawer;
    use crate::toolbar::view_toolbar;
    use crate::views::settings_view::settings_view;

    let toolbar = view_toolbar(self);

    // Settings view replaces the entire body (no nav drawer)
    let body: Element<Message> = if self.active_view == ActiveView::Settings {
        settings_view(self)
    } else {
        let drawer = if self.drawer_open {
            Some(view_drawer(self))
        } else {
            None
        };

        let content_area: Element<Message> = if self.active_view == ActiveView::Inbox {
            inbox_view(
                &self.feed_sections,
                self.theme.colors.text_primary,
                self.theme.colors.text_secondary,
                self.theme.colors.surface,
                self.theme.colors.divider,
            )
            .map(Message::InboxView)
        } else {
            container(
                text(format!(
                    "{} -- content area placeholder",
                    self.active_view.title()
                ))
                .size(16.0),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(crate::theme::DEFAULT_PADDING)
            .into()
        };

        match drawer {
            Some(drawer_el) => row![drawer_el, content_area]
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            None => content_area,
        }
    };

    // ... rest of layout (undo snackbar) unchanged ...
}
```

**Important:** The nav drawer is NOT rendered when Settings is active. The toolbar shows the grey settings bar with back arrow.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): wire settings view into main app view routing`

---

## Task 14: Dark theme support for settings views

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/settings_view.rs` (edit)

Update all view functions to use `ThemeColors` from the app's theme instead of hardcoded light-theme colors. The key changes:

1. Pass `&InboxlyTheme` (or the relevant colors) into each view function
2. Replace `surface_color()` calls with `app.theme.colors.surface`
3. Replace `primary_text()` calls with `app.theme.colors.text_primary`
4. Replace `secondary_text()` calls with `app.theme.colors.text_secondary`
5. Replace `divider_color()` calls with `app.theme.colors.divider`
6. Keep blue `#4285f4` and red `#ef5350` as constants (they don't change between themes, per spec)

The worker should refactor the function signatures to accept the theme colors or the entire `&Inboxly` where needed.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add dark theme support to settings views`

---

## Task 15: Unit tests for settings state management

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (append to `#[cfg(test)] mod tests`)

```rust
// -- M29 settings tests --

#[test]
fn open_settings_changes_view_and_hides_drawer() {
    let mut app = Inboxly::default();
    assert!(app.drawer_open);
    let _ = app.update(Message::OpenSettings);
    assert_eq!(app.active_view, ActiveView::Settings);
    assert!(!app.drawer_open);
    assert_eq!(app.previous_view, ActiveView::Inbox);
    assert!(app.drawer_was_open);
}

#[test]
fn close_settings_restores_previous_view() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::Navigate(NavTarget::View(ActiveView::Done)));
    let _ = app.update(Message::OpenSettings);
    assert_eq!(app.active_view, ActiveView::Settings);
    let _ = app.update(Message::CloseSettings);
    assert_eq!(app.active_view, ActiveView::Done);
}

#[test]
fn close_settings_restores_drawer_state() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::ToggleDrawer); // close drawer
    assert!(!app.drawer_open);
    let _ = app.update(Message::OpenSettings);
    let _ = app.update(Message::CloseSettings);
    assert!(!app.drawer_open); // restored to closed
}

#[test]
fn settings_tab_defaults_to_general() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::OpenSettings);
    assert_eq!(app.settings_tab, SettingsTab::General);
}

#[test]
fn settings_tab_change() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::OpenSettings);
    let _ = app.update(Message::SettingsTabChanged(SettingsTab::Accounts));
    assert_eq!(app.settings_tab, SettingsTab::Accounts);
}

#[test]
fn settings_tab_change_resets_edit_state() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::OpenSettings);
    app.editing_account_index = Some(0);
    app.removing_account_index = Some(1);
    let _ = app.update(Message::SettingsTabChanged(SettingsTab::General));
    assert_eq!(app.editing_account_index, None);
    assert_eq!(app.removing_account_index, None);
}

#[test]
fn settings_toolbar_color_is_grey() {
    let c = ActiveView::Settings.toolbar_color();
    // #455a64
    assert!((c.r - 0x45_f32 / 255.0).abs() < 0.01);
    assert!((c.g - 0x5a_f32 / 255.0).abs() < 0.01);
    assert!((c.b - 0x64_f32 / 255.0).abs() < 0.01);
}

#[test]
fn settings_title() {
    assert_eq!(ActiveView::Settings.title(), "Settings");
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui
```

**Commit:** `test(ui): add settings navigation state management tests`

---

## Task 16: Unit tests for SettingsTab enum

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/settings_view.rs` (append)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_tab_labels() {
        assert_eq!(SettingsTab::General.label(), "General");
        assert_eq!(SettingsTab::Accounts.label(), "Accounts");
        assert_eq!(SettingsTab::Bundles.label(), "Bundles");
        assert_eq!(SettingsTab::Notifications.label(), "Notifications");
        assert_eq!(SettingsTab::KeyboardShortcuts.label(), "Keyboard Shortcuts");
        assert_eq!(SettingsTab::DataStorage.label(), "Data & Storage");
    }

    #[test]
    fn settings_tab_all_returns_six() {
        assert_eq!(SettingsTab::all().len(), 6);
    }

    #[test]
    fn settings_tab_default_is_general() {
        assert_eq!(SettingsTab::default(), SettingsTab::General);
    }

    #[test]
    fn settings_tab_all_order() {
        let tabs = SettingsTab::all();
        assert_eq!(tabs[0], SettingsTab::General);
        assert_eq!(tabs[1], SettingsTab::Accounts);
        assert_eq!(tabs[5], SettingsTab::DataStorage);
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui
```

**Commit:** `test(ui): add SettingsTab enum unit tests`

---

## Task 17: Unit tests for accounts form state management

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (append to tests)

```rust
#[test]
fn add_account_start_opens_form() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::AddAccountStart);
    assert!(app.adding_account);
    assert_eq!(app.editing_account_index, None);
    assert!(app.account_form.email.is_empty());
}

#[test]
fn account_form_cancel_resets_state() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::AddAccountStart);
    let _ = app.update(Message::AccountFormCancel);
    assert!(!app.adding_account);
    assert_eq!(app.editing_account_index, None);
}

#[test]
fn account_form_field_updates() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::AddAccountStart);
    let _ = app.update(Message::AccountFormEmailChanged("test@test.com".into()));
    assert_eq!(app.account_form.email, "test@test.com");
    let _ = app.update(Message::AccountFormDisplayNameChanged("Test".into()));
    assert_eq!(app.account_form.display_name, "Test");
    let _ = app.update(Message::AccountFormImapPortChanged("143".into()));
    assert_eq!(app.account_form.imap_port, 143);
}

#[test]
fn remove_account_confirm_and_cancel() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::RemoveAccountConfirm(0));
    assert_eq!(app.removing_account_index, Some(0));
    let _ = app.update(Message::RemoveAccountCancel);
    assert_eq!(app.removing_account_index, None);
}

#[test]
fn undo_timeout_values() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::SetUndoTimeout(15));
    assert_eq!(app.undo_timeout_secs, 15);
}

#[test]
fn format_size_helper() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(512), "512 B");
    assert_eq!(format_size(1024), "1.0 KB");
    assert_eq!(format_size(1_048_576), "1.0 MB");
    assert_eq!(format_size(1_073_741_824), "1.0 GB");
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui
```

**Commit:** `test(ui): add accounts form and settings helper unit tests`

---

## Task 18: Unit tests for settings persistence round-trip

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/settings_view.rs` (append to tests)

```rust
#[test]
fn store_settings_adapter_read_write() {
    let store = inboxly_store::Store::open_in_memory().expect("in-memory store");
    let adapter = StoreSettingsAdapter { store: &store };

    // Write
    adapter.set_setting("theme", "dark").expect("set_setting");
    // Read
    let val = adapter.get_setting("theme").expect("get_setting");
    assert_eq!(val, Some("dark".to_owned()));
}

#[test]
fn store_settings_adapter_missing_key() {
    let store = inboxly_store::Store::open_in_memory().expect("in-memory store");
    let adapter = StoreSettingsAdapter { store: &store };
    let val = adapter.get_setting("nonexistent").expect("get_setting");
    assert_eq!(val, None);
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui
```

**Commit:** `test(ui): add StoreSettingsAdapter persistence round-trip tests`

---

## Task 19: Final integration verify — build, test, clippy

No new code in this task. This is a verification step.

**Run the full build + test + clippy pipeline:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check
```

Fix any warnings or errors. Common issues to expect:

- **Unused imports** — remove any that Clippy flags
- **Unreachable pattern** — the `_ => ...` wildcard in `settings_content` match will trigger if all `SettingsTab` variants are handled individually. Keep it for future tabs (Bundles, Notifications, Shortcuts from M30) but ensure Clippy doesn't warn.
- **Iced 0.14 API mismatches** — the code samples target the known API but the worker must verify against the actual version in the workspace `Cargo.toml`. Common issues:
  - `button::Style` fields may differ
  - `container::Style` may need different syntax
  - `text().color()` vs `text().style()`
  - `Space::new().width()` vs `Space::with_width()`
- **`walkdir` dependency** — if added in Task 7, ensure it compiles. Alternatively, implement `dir_size` with `std::fs::read_dir` recursion.
- **Thread safety** — `Store` is `!Send`. All settings reads/writes happen on the main thread (Iced update function), which is correct.

**Then manually launch the binary:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo run -p inboxly
```

**Visual checklist:**

- [ ] Gear icon visible in toolbar (between search and avatar)
- [ ] Clicking gear navigates to settings view
- [ ] Toolbar changes to grey `#455a64` with back arrow `←` and title "Settings"
- [ ] Nav drawer is hidden
- [ ] Settings sidebar shows 6 tabs (General active by default)
- [ ] General tab: theme chips (System/Light/Dark), default view chips, snooze fields, undo timeout chips
- [ ] Clicking a theme chip changes the theme immediately
- [ ] Accounts tab: account cards list (or empty state), Add Account button
- [ ] Add Account form: all 8 fields, Cancel/Save buttons
- [ ] Account card: avatar, email, provider info, Edit/Remove buttons
- [ ] Remove confirmation: red highlight bar with Cancel/Remove
- [ ] Data & Storage tab: Clear cache, Rebuild index, Export buttons, storage sizes, last sync
- [ ] Back arrow returns to previous view and restores nav drawer
- [ ] Switching tabs in sidebar works
- [ ] Tab switching resets account edit state
- [ ] Dark theme: all settings views render correctly with dark colors

**Commit:** `chore(ui): fix clippy warnings and verify settings view visual output`

---

## Summary

| Task | Description | Files | Tests |
|------|-------------|-------|-------|
| 1 | `ActiveView::Settings` + `SettingsTab` enum | `theme/mod.rs`, `views/settings_view.rs`, `views/mod.rs` | Task 16 |
| 2 | `StoreSettingsAdapter` | `views/settings_view.rs` | Task 18 |
| 3 | Settings state fields + messages in Inboxly | `app.rs` | Task 15 |
| 4 | Settings navigation handlers (open/close/tab) | `app.rs` | Task 15 |
| 5 | General tab update handlers | `app.rs` | Task 17 |
| 6 | Accounts tab update handlers | `app.rs` | Task 17 |
| 7 | Data & Storage tab update handlers | `app.rs` | Task 17 |
| 8 | Toolbar update (grey bg, back arrow, gear icon) | `toolbar.rs` | visual |
| 9 | Settings view layout (sidebar + content routing) | `views/settings_view.rs` | visual |
| 10 | General tab content view | `views/settings_view.rs` | visual |
| 11 | Accounts tab content view | `views/settings_view.rs` | visual |
| 12 | Data & Storage tab content view | `views/settings_view.rs` | visual |
| 13 | Wire settings view into app.rs view routing | `app.rs` | visual |
| 14 | Dark theme support | `views/settings_view.rs` | visual |
| 15 | Settings navigation tests | `app.rs` | 8 tests |
| 16 | SettingsTab enum tests | `views/settings_view.rs` | 4 tests |
| 17 | Accounts form + helper tests | `app.rs` | 6 tests |
| 18 | StoreSettingsAdapter persistence tests | `views/settings_view.rs` | 2 tests |
| 19 | Integration verify | — | build + clippy + visual |

**Total: 19 tasks, ~20 unit tests, 1 new source file (`views/settings_view.rs`), edits to 3 existing files (`app.rs`, `toolbar.rs`, `views/mod.rs`, `theme/mod.rs`).**

**New dependency (optional):** `walkdir` for directory size calculation. Can be avoided with `std::fs::read_dir` recursion.
