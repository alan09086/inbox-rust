# M28: Account Switcher — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Implement the account switcher at the top of the nav drawer. Clicking the account header expands an inline list of all configured accounts, allowing the user to switch between them or navigate to Settings to add a new one. Switching accounts reloads the inbox feed for the selected account.

**Crates:** `inboxly-ui` (primary), `inboxly-core` (read-only dependency on `AccountConfig`)

**Branch:** `m28-account-switcher`

**Prereqs:** M26 (PopupMenu widget), M27 (gear icon + `ActiveView::Settings`)

**Spec ref:** QoL Menus & Settings Design Spec, System 4: Account Switcher (lines 143-178)

**Tech Stack:** Rust, iced 0.14

---

> **Codebase Context (gap analysis):**
>
> 1. **Existing `account_switcher()` function** in `inboxly-ui/src/nav.rs` (line 168) renders a static collapsed view with `email` and `account_count` params. It does NOT support expansion, account listing, or switching. This function must be replaced.
> 2. **Existing `Inboxly` fields** in `inboxly-ui/src/app.rs`: `account_email: String` and `account_count: u32`. These are mock placeholders. They must be replaced with `accounts: Vec<AccountConfig>`, `active_account_index: usize`, and `account_switcher_open: bool`.
> 3. **`AccountConfig`** is in `inboxly-core/src/config.rs` with fields: `email`, `display_name`, `provider`, `auth_method`, `imap_host`, `imap_port`, `smtp_host`, `smtp_port`. The UI displays `email` and `display_name`.
> 4. **`AppConfig`** has `pub accounts: Vec<AccountConfig>`. The binary crate (`inboxly/src/main.rs`) currently calls `Inboxly::new()` which uses `Self::default()` — it does NOT load `AppConfig`. The plan must wire config loading.
> 5. **Avatar widget** exists at `inboxly-ui/src/widgets/avatar.rs` with `avatar_circle(letter, color_index)` returning a 40dp circle. The spec calls for 44px avatars in the switcher header — we will use a custom size variant.
> 6. **`Store::list_accounts()`** exists in `inboxly-store/src/accounts.rs` and returns `Vec<AccountRow>`. However, the account switcher should source accounts from `AppConfig` (TOML), not the SQLite store, since `AppConfig` is the authoritative source for account configuration. The store's `accounts` table is for sync metadata.
> 7. **`Message` enum** in `app.rs` does not have `ToggleAccountSwitcher` or `SwitchAccount` variants.
> 8. **Nav drawer rendering** in `nav.rs::view_drawer()` calls `account_switcher()` then pushes a divider. The expanded account list must be inserted between the header and the divider when `account_switcher_open` is true.
> 9. **Spec says 44px avatar** in the switcher header but the existing `AVATAR_DIAMETER` constant is 40dp. The switcher header uses a slightly larger avatar — add `ACCOUNT_SWITCHER_AVATAR` constant (44.0).
> 10. **Theme colours needed:** `#e8f0fe` (active account background — blue tint), checkmark for active account. These are not currently in `ThemeColors` but can be constructed inline with `color_from_hex`.

---

## Task 1: Add account switcher dimension constants

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/dimensions.rs` (modify)

Add the account switcher-specific layout constants at the end of the file, before the `#[cfg(test)]` module.

```rust
// -- Account Switcher --

/// Avatar diameter in the account switcher header (larger than standard 40dp).
pub const ACCOUNT_SWITCHER_AVATAR: f32 = 44.0;

/// Height of each account row in the expanded account list.
pub const ACCOUNT_ROW_HEIGHT: f32 = 56.0;
```

Also add two tests to the existing test module:

```rust
    #[test]
    fn account_switcher_avatar_is_44dp() {
        assert_eq!(ACCOUNT_SWITCHER_AVATAR, 44.0);
    }

    #[test]
    fn account_row_height_is_56dp() {
        assert_eq!(ACCOUNT_ROW_HEIGHT, 56.0);
    }
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- theme::dimensions
```

**Commit:** `feat(ui): add account switcher dimension constants`

---

## Task 2: Add `ToggleAccountSwitcher` and `SwitchAccount` messages

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify)

Add two new variants to the `Message` enum:

```rust
    /// Toggle the account switcher dropdown in the nav drawer.
    ToggleAccountSwitcher,
    /// Switch to the account at the given index in the accounts list.
    SwitchAccount(usize),
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add ToggleAccountSwitcher and SwitchAccount messages`

---

## Task 3: Replace mock account fields with account state

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify)

Replace the existing mock fields in the `Inboxly` struct:

**Remove:**
```rust
    /// Mock account info for the account switcher.
    pub account_email: String,
    /// Number of accounts (for the account switcher display).
    pub account_count: u32,
```

**Add:**
```rust
    /// Configured email accounts (loaded from AppConfig on startup).
    pub accounts: Vec<inboxly_core::config::AccountConfig>,
    /// Index of the currently active account in the `accounts` vec.
    pub active_account_index: usize,
    /// Whether the account switcher dropdown is expanded.
    pub account_switcher_open: bool,
```

**Add convenience methods** to `impl Inboxly`:

```rust
    /// Returns the currently active account config, or `None` if no accounts
    /// are configured.
    pub fn active_account(&self) -> Option<&inboxly_core::config::AccountConfig> {
        self.accounts.get(self.active_account_index)
    }

    /// Returns the email address of the active account, or a placeholder
    /// if no accounts are configured.
    pub fn active_email(&self) -> &str {
        self.active_account()
            .map(|a| a.email.as_str())
            .unwrap_or("No account")
    }

    /// Returns the display name of the active account, falling back to
    /// the email address if no display name is set.
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
```

**Update `Default` impl** to use the new fields:

```rust
    // Replace:
    account_email: "user@example.com".into(),
    account_count: 1,

    // With:
    accounts: Vec::new(),
    active_account_index: 0,
    account_switcher_open: false,
```

**Update `toolbar.rs`** references to `app.account_email` — change the avatar letter extraction to use `app.active_email()`:

```rust
    let avatar_letter = app
        .active_email()
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
```

**Update `nav.rs`** — the `account_switcher()` call in `view_drawer()` currently passes `&app.account_email` and `app.account_count`. This will be replaced in Task 5, but for now update the call signature to pass the `Inboxly` reference directly:

```rust
    // In view_drawer(), change:
    drawer = drawer.push(account_switcher(&app.account_email, app.account_count));
    // To:
    drawer = drawer.push(account_switcher_header(app));
```

And rename/refactor the `account_switcher` function to `account_switcher_header` taking `&Inboxly` (this is a temporary bridge — Task 5 replaces it fully). Update the function to use `app.active_email()` and `app.accounts.len()`.

**Update existing tests** in `app.rs` that reference `account_email` or `account_count` — these fields no longer exist. Replace any assertions against them with assertions against the new fields.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui && cargo check -p inboxly
```

**Commit:** `refactor(ui): replace mock account fields with Vec<AccountConfig> and active index`

---

## Task 4: Handle new messages in `update()`

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify)

Add match arms to the `update()` method for the two new messages:

```rust
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
```

**Add tests** to the existing test module in `app.rs`:

```rust
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
    fn switch_account_changes_active_index() {
        let mut app = Inboxly::default();
        app.accounts = vec![
            inboxly_core::config::AccountConfig {
                email: "first@example.com".into(),
                display_name: "First".into(),
                provider: "generic".into(),
                auth_method: Default::default(),
                imap_host: "imap.example.com".into(),
                imap_port: 993,
                smtp_host: "smtp.example.com".into(),
                smtp_port: 587,
            },
            inboxly_core::config::AccountConfig {
                email: "second@example.com".into(),
                display_name: "Second".into(),
                provider: "generic".into(),
                auth_method: Default::default(),
                imap_host: "imap.example.com".into(),
                imap_port: 993,
                smtp_host: "smtp.example.com".into(),
                smtp_port: 587,
            },
        ];
        let _ = app.update(Message::SwitchAccount(1));
        assert_eq!(app.active_account_index, 1);
        assert_eq!(app.active_email(), "second@example.com");
    }

    #[test]
    fn switch_account_closes_switcher() {
        let mut app = Inboxly::default();
        app.accounts = vec![inboxly_core::config::AccountConfig {
            email: "test@example.com".into(),
            display_name: String::new(),
            provider: "generic".into(),
            auth_method: Default::default(),
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            smtp_host: "smtp.example.com".into(),
            smtp_port: 587,
        }];
        app.account_switcher_open = true;
        let _ = app.update(Message::SwitchAccount(0));
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
    fn active_display_name_falls_back_to_email() {
        let mut app = Inboxly::default();
        app.accounts = vec![inboxly_core::config::AccountConfig {
            email: "test@example.com".into(),
            display_name: String::new(),
            provider: "generic".into(),
            auth_method: Default::default(),
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            smtp_host: "smtp.example.com".into(),
            smtp_port: 587,
        }];
        assert_eq!(app.active_display_name(), "test@example.com");
    }
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- app::tests
```

**Commit:** `feat(ui): handle ToggleAccountSwitcher and SwitchAccount messages`

---

## Task 5: Rewrite account switcher UI (collapsed + expanded states)

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/nav.rs` (modify)

Replace the existing `account_switcher` (or `account_switcher_header`) function with a full account switcher implementation that handles both collapsed and expanded states. This is the core UI task.

**Add imports** at the top of `nav.rs`:

```rust
use crate::theme::dimensions::{ACCOUNT_ROW_HEIGHT, ACCOUNT_SWITCHER_AVATAR};
```

**Replace the account switcher function** with two functions:

### `account_switcher_header` — the always-visible header

Renders the collapsed header row. Clicking it sends `Message::ToggleAccountSwitcher`.

```rust
/// Render the account switcher header (always visible at top of drawer).
///
/// Shows: [44px avatar] [display name (bold 15px)] [chevron]
///                       [email (grey 13px, truncated)]
fn account_switcher_header(app: &Inboxly) -> Element<'_, Message> {
    let display_name = app.active_display_name().to_string();
    let email = app.active_email().to_string();
    let first_char = email.chars().next().unwrap_or('?');

    // 44px avatar circle (letter tile)
    let avatar = {
        let bg_color = crate::theme::avatar_colors::for_letter(first_char);
        let letter = first_char.to_uppercase().to_string();
        container(
            text(letter)
                .size(20.0)
                .color(Color::WHITE)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
        )
        .width(ACCOUNT_SWITCHER_AVATAR)
        .height(ACCOUNT_SWITCHER_AVATAR)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(bg_color)),
            border: Border {
                radius: (ACCOUNT_SWITCHER_AVATAR / 2.0).into(),
                ..Default::default()
            },
            ..Default::default()
        })
    };

    // Display name (bold, 15px)
    let name_text = text(display_name)
        .size(15.0)
        .color(primary_text());

    // Email (grey, 13px) — truncated via container width constraint
    let email_text = text(email)
        .size(13.0)
        .color(secondary_text());

    let info_col = column![name_text, email_text].spacing(2);

    // Chevron indicator
    let chevron = if app.account_switcher_open {
        "\u{25B2}" // ▲
    } else {
        "\u{25BC}" // ▼
    };
    let chevron_text = text(chevron)
        .size(10.0)
        .color(secondary_text());

    let header_row = row![avatar, info_col, Space::new().width(Length::Fill), chevron_text]
        .spacing(12)
        .align_y(Alignment::Center)
        .padding(DEFAULT_PADDING);

    button(
        container(header_row)
            .width(Length::Fill),
    )
    .on_press(Message::ToggleAccountSwitcher)
    .width(Length::Fill)
    .style(move |_theme, _status| button::Style {
        background: Some(Background::Color(surface_color())),
        text_color: primary_text(),
        border: Border::default(),
        ..Default::default()
    })
    .into()
}
```

### `account_list` — the expanded account list

Only rendered when `app.account_switcher_open` is true. Shows each account with its avatar and email, the active account highlighted with a blue background and checkmark, plus an "Add account" row at the bottom.

```rust
/// Render the expanded account list (shown below header when switcher is open).
fn account_list(app: &Inboxly) -> Element<'_, Message> {
    let active_bg = color_from_hex(0xe8, 0xf0, 0xfe); // #e8f0fe — spec blue tint

    let mut list = column![].width(Length::Fill);

    for (index, account) in app.accounts.iter().enumerate() {
        let is_active = index == app.active_account_index;
        let bg = if is_active { active_bg } else { surface_color() };
        let first_char = account.email.chars().next().unwrap_or('?');

        // 40px avatar (standard size in the list)
        let avatar_color = crate::theme::avatar_colors::for_letter(first_char);
        let avatar = container(
            text(first_char.to_uppercase().to_string())
                .size(16.0)
                .color(Color::WHITE)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
        )
        .width(AVATAR_DIAMETER)
        .height(AVATAR_DIAMETER)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(avatar_color)),
            border: Border {
                radius: (AVATAR_DIAMETER / 2.0).into(),
                ..Default::default()
            },
            ..Default::default()
        });

        // Email text
        let email_text = text(account.email.clone())
            .size(14.0)
            .color(primary_text());

        // Checkmark for active account
        let mut account_row = row![avatar, email_text]
            .spacing(12)
            .align_y(Alignment::Center);

        if is_active {
            let check = text("\u{2713}") // ✓
                .size(16.0)
                .color(color_from_hex(0x42, 0x85, 0xf4));
            account_row = account_row.push(Space::new().width(Length::Fill));
            account_row = account_row.push(check);
        }

        let row_btn = button(
            container(account_row)
                .padding([0.0, DEFAULT_PADDING])
                .height(ACCOUNT_ROW_HEIGHT)
                .width(Length::Fill)
                .align_y(iced::alignment::Vertical::Center),
        )
        .on_press(Message::SwitchAccount(index))
        .width(Length::Fill)
        .style(move |_theme, _status| button::Style {
            background: Some(Background::Color(bg)),
            text_color: primary_text(),
            border: Border::default(),
            ..Default::default()
        });

        list = list.push(row_btn);
    }

    // "Add account" row — navigates to Settings (M27 provides ActiveView::Settings)
    let add_icon = text("+")
        .size(20.0)
        .color(color_from_hex(0x42, 0x85, 0xf4));
    let add_label = text("Add account")
        .size(14.0)
        .color(color_from_hex(0x42, 0x85, 0xf4));
    let add_row = row![add_icon, add_label]
        .spacing(12)
        .align_y(Alignment::Center);

    let add_btn = button(
        container(add_row)
            .padding([0.0, DEFAULT_PADDING])
            .height(ACCOUNT_ROW_HEIGHT)
            .width(Length::Fill)
            .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(Message::Navigate(NavTarget::View(ActiveView::Settings)))
    .width(Length::Fill)
    .style(move |_theme, _status| button::Style {
        background: Some(Background::Color(surface_color())),
        text_color: primary_text(),
        border: Border::default(),
        ..Default::default()
    });

    list = list.push(add_btn);

    list.into()
}
```

**Note:** The `"Add account"` row sends `Message::Navigate(NavTarget::View(ActiveView::Settings))`. This requires `ActiveView::Settings` to exist (delivered by M27). If M27 is not yet merged when implementing, use a placeholder message or `Message::ToggleAccountSwitcher` (just closes the switcher) and add a `// TODO: navigate to Settings when M27 lands` comment.

### Update `view_drawer()` to use the new functions

Replace the existing account switcher section in `view_drawer()`:

```rust
    // Account switcher
    drawer = drawer.push(account_switcher_header(app));
    if app.account_switcher_open {
        drawer = drawer.push(account_list(app));
    }
    drawer = drawer.push(divider());
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui && cargo check -p inboxly
```

**Commit:** `feat(ui): implement account switcher with collapsed/expanded states`

---

## Task 6: Dismiss account switcher on click-away

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify)

When the user navigates (clicks any nav item) or switches accounts, the account switcher should close. Add dismiss logic to the existing `Message::Navigate` handler:

```rust
            Message::Navigate(target) => {
                // Close account switcher on any navigation
                self.account_switcher_open = false;

                if let NavTarget::View(view) = &target {
                    self.active_view = *view;
                }
                self.active_nav = target;
            }
```

The `SwitchAccount` handler already sets `self.account_switcher_open = false` (Task 4).

For full click-away dismiss (clicking the content area when the switcher is open), add handling in the `view()` method. When `account_switcher_open` is true, wrap the content area in a `mouse_area` that sends `ToggleAccountSwitcher` on press:

```rust
    // In view(), after building content_area, before building body:
    let content_area: Element<Message> = if self.account_switcher_open {
        iced::widget::mouse_area(content_area)
            .on_press(Message::ToggleAccountSwitcher)
            .into()
    } else {
        content_area
    };
```

**Add import** at the top of `app.rs` if not already present: `mouse_area` is available from `iced::widget`.

**Add test:**

```rust
    #[test]
    fn navigate_closes_account_switcher() {
        let mut app = Inboxly::default();
        app.account_switcher_open = true;
        let _ = app.update(Message::Navigate(NavTarget::View(ActiveView::Inbox)));
        assert!(!app.account_switcher_open);
    }
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- app::tests
```

**Commit:** `feat(ui): dismiss account switcher on navigation and click-away`

---

## Task 7: Load accounts from `AppConfig` on startup

**Files:**
- `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify)
- `/mnt/TempNVME/projects/inbox-rust/inboxly/src/main.rs` (modify)

### 7a. Add `with_accounts` constructor to `Inboxly`

In `app.rs`, add a new constructor that accepts a list of accounts:

```rust
    /// Create the app with accounts loaded from configuration.
    pub fn with_accounts(accounts: Vec<inboxly_core::config::AccountConfig>) -> (Self, Task<Message>) {
        let mut app = Self {
            accounts,
            theme: InboxlyTheme::from_system(),
            ..Self::default()
        };
        app.reload_feed();
        (app, Task::none())
    }

    /// Create the app with a store and accounts.
    pub fn with_store_and_accounts(
        store: Store,
        accounts: Vec<inboxly_core::config::AccountConfig>,
    ) -> (Self, Task<Message>) {
        let mut app = Self {
            store: Some(store),
            accounts,
            theme: InboxlyTheme::from_system(),
            ..Self::default()
        };
        app.reload_feed();
        (app, Task::none())
    }
```

### 7b. Load `AppConfig` in `main.rs`

Update the binary crate to load `AppConfig` and pass accounts to the UI:

```rust
use inboxly_core::config::AppConfig;
use inboxly_ui::app::Inboxly;

fn main() -> iced::Result {
    // Load config (falls back to defaults if file missing)
    let config = AppConfig::load().unwrap_or_else(|e| {
        eprintln!("warning: failed to load config: {e}, using defaults");
        AppConfig::default()
    });

    let accounts = config.accounts.clone();

    iced::application(
        move |_| Inboxly::with_accounts(accounts.clone()),
        Inboxly::update,
        Inboxly::view,
    )
    .title(Inboxly::title)
    .window_size(iced::Size::new(1280.0, 800.0))
    .theme(Inboxly::theme)
    .run()
}
```

**Important:** The `iced::application()` API takes a closure for initialization. The closure captures `accounts` by value. Check the exact Iced 0.14 `application()` signature — if it takes `fn() -> (Model, Task)` (not `Fn`), the accounts must be passed differently. The current code uses `Inboxly::new` as a function pointer; the new version needs a closure. If Iced 0.14's `application()` requires `Fn() -> (Self, Task)`, `move` closure works. If it requires `fn`, we need a different approach (e.g., a global or thread-local). Verify against Iced 0.14 docs.

**Alternative approach if `application()` does not accept closures:** Store config in a `OnceCell<AppConfig>` and have `Inboxly::new()` read from it:

```rust
// In app.rs:
use std::sync::OnceLock;

static APP_CONFIG: OnceLock<Vec<inboxly_core::config::AccountConfig>> = OnceLock::new();

impl Inboxly {
    /// Provide accounts configuration before creating the app.
    /// Must be called before `new()`.
    pub fn set_accounts(accounts: Vec<inboxly_core::config::AccountConfig>) {
        let _ = APP_CONFIG.set(accounts);
    }
}
```

Then in the `new()` method, load from `APP_CONFIG`. This is a fallback — prefer the closure approach if Iced supports it.

**Add test:**

```rust
    #[test]
    fn with_accounts_sets_accounts() {
        let accounts = vec![inboxly_core::config::AccountConfig {
            email: "test@example.com".into(),
            display_name: "Test".into(),
            provider: "generic".into(),
            auth_method: Default::default(),
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            smtp_host: "smtp.example.com".into(),
            smtp_port: 587,
        }];
        let (app, _) = Inboxly::with_accounts(accounts);
        assert_eq!(app.accounts.len(), 1);
        assert_eq!(app.active_account_index, 0);
        assert_eq!(app.active_email(), "test@example.com");
    }
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- app::tests && cargo check -p inboxly
```

**Commit:** `feat: load accounts from AppConfig and pass to UI on startup`

---

## Task 8: Tests for nav drawer rendering

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/nav.rs` (modify — add test module)

Add unit tests for the account switcher state logic. Since Iced widgets don't have a simple test harness for visual rendering, test the state transitions and data flow rather than pixel output.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Inboxly, Message};

    #[test]
    fn account_switcher_starts_collapsed() {
        let app = Inboxly::default();
        assert!(!app.account_switcher_open);
    }

    #[test]
    fn account_switcher_toggles() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::ToggleAccountSwitcher);
        assert!(app.account_switcher_open);
        let _ = app.update(Message::ToggleAccountSwitcher);
        assert!(!app.account_switcher_open);
    }

    #[test]
    fn active_email_with_multiple_accounts() {
        let mut app = Inboxly::default();
        app.accounts = vec![
            inboxly_core::config::AccountConfig {
                email: "alice@example.com".into(),
                display_name: "Alice".into(),
                provider: "generic".into(),
                auth_method: Default::default(),
                imap_host: "imap.example.com".into(),
                imap_port: 993,
                smtp_host: "smtp.example.com".into(),
                smtp_port: 587,
            },
            inboxly_core::config::AccountConfig {
                email: "bob@example.com".into(),
                display_name: "Bob".into(),
                provider: "generic".into(),
                auth_method: Default::default(),
                imap_host: "imap.example.com".into(),
                imap_port: 993,
                smtp_host: "smtp.example.com".into(),
                smtp_port: 587,
            },
        ];
        assert_eq!(app.active_email(), "alice@example.com");
        let _ = app.update(Message::SwitchAccount(1));
        assert_eq!(app.active_email(), "bob@example.com");
        assert_eq!(app.active_display_name(), "Bob");
    }

    #[test]
    fn switch_account_reloads_feed() {
        // With no store, reload_feed is a no-op, but it should not panic.
        let mut app = Inboxly::default();
        app.accounts = vec![
            inboxly_core::config::AccountConfig {
                email: "a@example.com".into(),
                display_name: String::new(),
                provider: "generic".into(),
                auth_method: Default::default(),
                imap_host: "imap.example.com".into(),
                imap_port: 993,
                smtp_host: "smtp.example.com".into(),
                smtp_port: 587,
            },
            inboxly_core::config::AccountConfig {
                email: "b@example.com".into(),
                display_name: String::new(),
                provider: "generic".into(),
                auth_method: Default::default(),
                imap_host: "imap.example.com".into(),
                imap_port: 993,
                smtp_host: "smtp.example.com".into(),
                smtp_port: 587,
            },
        ];
        let _ = app.update(Message::SwitchAccount(1));
        assert_eq!(app.active_account_index, 1);
        // Feed sections are empty (no store), but no panic.
        assert!(app.feed_sections.is_empty());
    }

    #[test]
    fn switch_to_same_account_still_closes_switcher() {
        let mut app = Inboxly::default();
        app.accounts = vec![inboxly_core::config::AccountConfig {
            email: "test@example.com".into(),
            display_name: String::new(),
            provider: "generic".into(),
            auth_method: Default::default(),
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            smtp_host: "smtp.example.com".into(),
            smtp_port: 587,
        }];
        app.account_switcher_open = true;
        let _ = app.update(Message::SwitchAccount(0));
        assert!(!app.account_switcher_open);
        assert_eq!(app.active_account_index, 0);
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- nav::tests
```

**Commit:** `test(ui): add account switcher state transition tests`

---

## Final verification

After all tasks are complete, run the full verification suite:

```bash
cd /mnt/TempNVME/projects/inbox-rust && \
    cargo fmt --check --all && \
    cargo clippy --workspace -- -D warnings && \
    cargo test --workspace && \
    cargo build
```

**Expected outcomes:**
- All existing tests still pass (no regressions from removing `account_email`/`account_count`)
- New tests pass: ~12 new tests across `app::tests`, `nav::tests`, `dimensions::tests`
- No clippy warnings
- Binary compiles and runs, showing the account switcher in the nav drawer

---

## Summary

| Task | Scope | Files | New Tests |
|------|-------|-------|-----------|
| 1 | Dimension constants | `theme/dimensions.rs` | 2 |
| 2 | New message variants | `app.rs` | 0 |
| 3 | Replace mock fields with `Vec<AccountConfig>` + index + bool | `app.rs`, `nav.rs`, `toolbar.rs` | 0 (existing tests updated) |
| 4 | Handle messages in `update()` | `app.rs` | 6 |
| 5 | Account switcher UI (header + expanded list) | `nav.rs` | 0 |
| 6 | Click-away dismiss | `app.rs` | 1 |
| 7 | Load accounts from `AppConfig` on startup | `app.rs`, `main.rs` | 1 |
| 8 | Nav drawer state tests | `nav.rs` | 5 |
| **Total** | | **5 files** | **15 tests** |
