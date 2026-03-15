# Inboxly QoL: Menus, Settings & Dropdowns — Design Spec

## Overview

This spec covers the quality-of-life layer that transforms Inboxly from a functional prototype into a polished desktop email client. It introduces three interconnected systems: a reusable popup menu primitive, a full settings view, and contextual actions throughout the UI.

**Status**: Post-v1 (v0.25.0). All 25 backend+UI milestones complete. This is the first post-v1 feature spec.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Popup rendering | Iced `Widget::overlay()` trait | Framework-native layering, no z-index hacks, supports nested menus |
| Dismiss behaviour | Click-away + toggle + item selection | Standard desktop convention — all three dismiss paths |
| Settings access | Gear icon in toolbar | Faster access than burying in nav drawer; Inbox-style modernisation |
| Account switcher | Top of nav drawer, inline expansion | Matches original Inbox by Google pattern |
| Context menus | Right-click on email rows | Desktop-native interaction; same actions as overflow menu |
| Settings layout | Sidebar nav + content area (replaces main content) | Scales to 6 settings categories without overwhelming the UI |

## System 1: PopupMenu Widget

A reusable Iced widget that renders a dropdown/context menu as an overlay. Every dropdown, overflow menu, and context menu in the app uses this single primitive.

### Architecture

```
PopupMenu<Message>
├── trigger: Element<Message>      -- the button/area that opens the menu
├── items: Vec<MenuItem<Message>>   -- menu entries
├── is_open: bool                   -- controlled by parent state
├── anchor: PopupAnchor             -- where the menu positions relative to trigger
└── on_dismiss: Message             -- sent when click-away or item selected
```

### MenuItem Enum

```rust
enum MenuItem<Message> {
    Action {
        label: String,
        icon: Option<char>,       // Unicode icon character
        message: Message,
        style: MenuItemStyle,     // Normal or Destructive (red text)
    },
    Separator,
    Submenu {
        label: String,
        icon: Option<char>,
        items: Vec<MenuItem<Message>>,
    },
}

enum MenuItemStyle {
    Normal,
    Destructive,  // Red text — for Report Spam, Block Sender, etc.
}

enum PopupAnchor {
    BelowRight,   // Default — opens below trigger, right-aligned
    BelowLeft,    // Opens below trigger, left-aligned
    AtCursor,     // Context menu — opens at mouse position
}
```

### Overlay Implementation

The `PopupMenu` widget implements Iced's `Widget::overlay()` method:

1. When `is_open == false`, returns `None` (no overlay).
2. When `is_open == true`, returns an `overlay::Element` that renders:
   - An invisible full-screen backdrop that captures clicks (sends `on_dismiss`)
   - The menu card positioned relative to the trigger (or at cursor for context menus)
   - Menu items with hover highlighting
3. Clicking a menu item sends the item's `message` AND `on_dismiss`.
4. Pressing Escape sends `on_dismiss`.

### Visual Spec

- **Menu card**: white background, `border-radius: 10px`, `box-shadow: 0 6px 24px rgba(0,0,0,0.18)`, `border: 1px solid #e0e0e0`
- **Menu item**: `padding: 12px 18px`, `font-size: 15px`, icon 22px wide left-aligned
- **Hover**: `background: #f5f5f5`
- **Destructive hover**: `background: #fbe9e7`, `color: #ef5350`
- **Separator**: `1px solid #e8e8e8` with `2px` vertical margin
- **Menu width**: 260px (overflow/context), auto-width for shorter menus
- **Dark theme**: surface colour background, appropriate text/hover colours from `ThemeColors`

## System 2: Toolbar Integration

### Gear Icon

- Position: right side of toolbar, between search bar and account avatar
- Icon: gear/cog character (Unicode ⚙ or custom SVG)
- Behaviour: clicking navigates to `ActiveView::Settings` (not a dropdown — opens full settings view)
- Toolbar changes to neutral `#455a64` grey when Settings view is active
- Hamburger icon replaced with back arrow `←` that returns to previous view

### Three-Dot Overflow on Email Rows

- Position: appended after existing hover action buttons (Done ✓, Pin 📌, Snooze 🕐, **More ⋮**)
- Only visible on hover (same as existing action buttons)
- Clicking opens a `PopupMenu` anchored `BelowRight` of the trigger
- Menu items (grouped with separators):

  **Group 1 — Thread actions**:
  - Move to... (opens submenu with bundle categories)
  - Mark as read / Mark as unread (toggles based on current state)
  - Mute thread

  **Group 2 — Reply actions**:
  - Reply
  - Reply All
  - Forward

  **Group 3 — Organisation**:
  - Add to bundle... (opens submenu with bundle categories)
  - Create rule from sender

  **Group 4 — Safety** (destructive style):
  - Block sender
  - Report spam

## System 3: Right-Click Context Menu

- Triggered by right-click on any email row in the inbox feed
- Uses `PopupMenu` with `PopupAnchor::AtCursor`
- Menu items combine quick actions + overflow actions:

  **Group 1 — Quick actions** (same as hover buttons):
  - Done
  - Pin
  - Snooze

  **Groups 2-5**: Same as overflow menu (Move to, Mark read/unread, Mute, Reply/Reply All/Forward, Add to bundle, Create rule, Block sender, Report spam)

### Implementation Note

Iced does not natively support right-click events. This requires either:
- A custom `mouse::Event::ButtonPressed(Button::Right)` handler in the widget
- Or intercepting at the `Application::subscription()` level

The recommended approach is handling `mouse::Event` in the email row widget's `on_event` method, storing the cursor position, and toggling the context menu state.

## System 4: Account Switcher

### Location

Top of the nav drawer, above the navigation items.

### Collapsed State (default)

- Account avatar (44px circle, letter tile)
- Display name (bold, 15px)
- Email address (grey, 13px, truncated with ellipsis)
- Chevron `▼` indicating expandability

### Expanded State (on click)

- Same header but chevron rotated `▲`
- Below header, separated by a thin divider:
  - List of all configured accounts
  - Active account has blue `#e8f0fe` background and checkmark
  - Other accounts show avatar + email, clickable to switch
  - "Add account" row at bottom with `+` icon (navigates to Settings → Accounts)
- Click-away or selecting an account collapses the list
- Switching accounts reloads the inbox feed for that account

### App State Changes

```rust
// New fields in Inboxly struct:
pub account_switcher_open: bool,
pub accounts: Vec<AccountInfo>,      // loaded from AppConfig
pub active_account_index: usize,

// New messages:
Message::ToggleAccountSwitcher,
Message::SwitchAccount(usize),       // index into accounts vec
```

## System 5: Settings View

### Access

Gear icon in toolbar → navigates to `ActiveView::Settings`.

### Layout

- **Toolbar**: neutral grey `#455a64`, back arrow `←` replaces hamburger, title "Settings"
- **Sidebar**: 240px wide, white background, 6 tabs with active state (blue text + left border + light blue background)
- **Content area**: 640px max-width, 32px padding, scrollable

### Nav drawer behaviour

The nav drawer is hidden when the Settings view is active. The back arrow returns to the previously active view and restores the nav drawer.

### Tab: General

| Setting | Control | Persists To |
|---------|---------|-------------|
| Theme | 3 chip buttons (System / Light / Dark) | `AppConfig.theme` + settings store |
| Default View | Dropdown (Inbox / Snoozed / Done) | settings store |
| Snooze Presets | 4 form fields: Morning time, Afternoon time, Evening time, Weekend day dropdown | `AppConfig.snooze` |
| Undo Timeout | Dropdown (3s / 5s / 7s / 10s / 15s) | settings store |

### Tab: Accounts

- List of account cards, each showing:
  - Avatar (letter tile, 48px)
  - Email address (bold, 17px)
  - Provider + auth method + last sync time (grey, 14px)
  - Edit button (opens inline edit form with IMAP/SMTP fields)
  - Remove button (red, with confirmation dialog)
- "+ Add Account" button (blue chip) at top-right
- Add/Edit form fields: email, display name, provider (Gmail/Fastmail/Generic), auth method (Password/OAuth2/App Password), IMAP host/port, SMTP host/port

### Tab: Bundles

- Reorderable list of bundle categories, each showing:
  - Drag handle (☰)
  - Category icon (coloured circle)
  - Category name (16px, medium weight)
  - Throttle badge (pill: green "Immediate", orange "Daily @ 5 PM", blue "Daily @ 9 AM", etc.)
  - Visibility toggle (on/off — controls nav drawer visibility)
- Clicking throttle badge opens a `PopupMenu` with throttle options:
  - Immediate
  - Daily → submenu with time picker
  - Weekly → submenu with day + time picker
- Drag to reorder changes bundle order in nav drawer and feed

### Tab: Notifications

| Setting | Control | Persists To |
|---------|---------|-------------|
| Desktop notifications | Toggle | settings store |
| Sound | Toggle | settings store |
| Notify for | Multi-select checkboxes (All / Primary only / Bundles: per-category) | settings store |

### Tab: Keyboard Shortcuts

- Two-column table: Action → Shortcut
- Editable: click a shortcut cell, press new key combination, confirm
- Default shortcuts:
  - `e` — Done (archive)
  - `p` — Pin/unpin
  - `s` — Snooze
  - `/` — Focus search
  - `c` — Compose
  - `r` — Reply
  - `a` — Reply All
  - `f` — Forward
  - `j` / `k` — Next / previous thread
  - `?` — Show shortcut help
  - `Esc` — Close menu / back

### Tab: Data & Storage

| Setting | Control | Description |
|---------|---------|-------------|
| Clear cache | Button | Removes cached data, re-syncs on next launch |
| Rebuild search index | Button | Rebuilds Tantivy index from Maildir |
| Export data | Button | Exports mailbox as mbox or EML archive |
| Database size | Read-only | Shows SQLite + Tantivy + Maildir sizes |
| Last full sync | Read-only | Timestamp of last completed sync |

## Persistence

### Settings Store

Settings that don't belong in `AppConfig.toml` (runtime preferences) persist via the existing `SettingsReader`/`SettingsWriter` traits backed by the SQLite `settings` table.

| Key | Type | Default |
|-----|------|---------|
| `theme` | String (`system`/`light`/`dark`) | `system` |
| `default_view` | String (`inbox`/`snoozed`/`done`) | `inbox` |
| `undo_timeout_secs` | String (integer) | `7` |
| `notifications_enabled` | String (`true`/`false`) | `true` |
| `notification_sound` | String (`true`/`false`) | `true` |
| `notification_bundles` | String (JSON array of category names) | `["all"]` |

### AppConfig.toml

Account configuration and snooze presets persist to `~/.config/inboxly/config.toml` via the existing `AppConfig` serialisation.

Bundle order and visibility persist to the SQLite `bundles` table (existing `display_order` and `visible` columns, to be added if not present).

Keyboard shortcuts persist to a `shortcuts` settings key as JSON.

## New Crate Dependencies

None. All UI work uses existing Iced widgets. The `PopupMenu` is a custom widget built from Iced primitives (`overlay`, `container`, `column`, `mouse_area`).

## Testing Strategy

### Unit Tests
- `PopupMenu` state management (open/close/dismiss)
- `MenuItem` rendering (icon + label, separator, destructive style)
- Settings persistence round-trip (write → read → verify)
- Account switcher state transitions
- Keyboard shortcut parser (key combo string ↔ enum)

### Integration Tests
- Settings view navigation (gear → settings → back)
- Theme change via settings persists across app restart
- Account add/edit/remove updates `AppConfig`
- Bundle reorder persists to store
- Context menu actions trigger correct messages

## Migration

No schema migration needed. The `settings` table already exists with a generic key-value schema. Bundle `display_order` and `visible` columns should be verified (add via migration if missing).

## Dark Theme

All new UI elements must respect `ThemeColors`:
- Menu card background → `surface`
- Menu item text → `text_primary`
- Menu hover → slightly lighter/darker than `surface`
- Destructive text → stays `#ef5350` in both themes
- Settings sidebar → `surface` background
- Settings content → `background`
- Form controls → `surface` background, `text_primary` values, `divider` borders
- Chips → active uses toolbar colour, inactive uses `surface`
