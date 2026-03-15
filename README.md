# Inboxly

A recreation of [Inbox by Google](https://en.wikipedia.org/wiki/Inbox_by_Gmail) as a native desktop email client, built in Rust with [Iced](https://iced.rs/).

## Why

Google killed Inbox in April 2019. Nothing has replaced its unique approach to email: bundles, highlights, snooze, sweep, and smart extraction. Inboxly aims to bring it back as a fully local, privacy-respecting desktop application.

## Architecture

8-crate Cargo workspace:

| Crate | Purpose |
|-------|---------|
| `inboxly-core` | Core types, traits, and error definitions |
| `inboxly-imap` | IMAP sync engine and OAuth2 authentication |
| `inboxly-store` | Maildir, SQLite, and Tantivy storage layer |
| `inboxly-bundler` | Email categorisation engine |
| `inboxly-snooze` | Snooze scheduler and reminder system |
| `inboxly-extract` | Smart extraction and highlight detection |
| `inboxly-ui` | Iced-based desktop UI |
| `inboxly` | Binary entry point |

### Dependency Graph

```
inboxly-core (foundation — zero internal deps)
  ├── inboxly-imap
  ├── inboxly-store
  ├── inboxly-extract
  ├── inboxly-bundler (+ inboxly-store)
  ├── inboxly-snooze (+ inboxly-store)
  └── inboxly-ui (all subcrates)
        └── inboxly (binary)
```

## Building

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

Requires Rust edition 2024 (rustc 1.85+).

## Status

**v0.30.0 / M30 complete** -- Full UI scaffold with QoL menus and settings:

### v1 Foundation (M1-M25)
- Theme system: light/dark with BigTop design tokens, D-Bus system detection
- Inbox feed: date-grouped sections, email rows, bundle rows, mixed rendering
- Triage actions: Done, Pin, Sweep with 7-second timed undo
- Swipe gestures: state tracking, hover action buttons (Done/Pin/Snooze)
- Snooze picker: 5 preset options with computed UTC times
- Reminders: reminder row widget, Speed Dial FAB (Compose + Reminder)
- Compose view: To/Cc/Subject/Body fields, Send/Discard, reply modes
- Search: query parser (from:/to:/subject:/has:/is: operators), result view
- Inbox Zero celebration view

### QoL Menus & Settings (M26-M30)
- **PopupMenu widget** (M26): Reusable dropdown/context menu overlay using Iced 0.14's `advanced` Widget + Overlay traits
- **Overflow + context menus** (M27): Three-dot overflow menu on email rows, right-click context menu with `RightClickArea` widget, gear icon for Settings
- **Account switcher** (M28): Multi-account support in nav drawer with inline expansion, switch between accounts, load from `AppConfig`
- **Settings view** (M29-M30): Full 6-tab settings with sidebar navigation:
  - General: theme preference (System/Light/Dark), default view, snooze presets, undo timeout
  - Accounts: add/edit/remove accounts with inline forms, active-deletion prevention
  - Bundles: reorder with arrows, throttle badge (Immediate/Daily/Weekly), visibility toggle
  - Notifications: desktop notification toggle, sound toggle, per-bundle selection
  - Keyboard Shortcuts: runtime-remappable bindings (ShortcutMap), click-to-capture UI, delta persistence
  - Data & Storage: clear cache, rebuild index, export (stub), storage sizes

841 tests passing, 0 clippy warnings

## Licence

GPL-3.0-only
