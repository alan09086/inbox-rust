# 📬 Inboxly

A recreation of [Inbox by Google](https://en.wikipedia.org/wiki/Inbox_by_Gmail) as a native desktop email client, built in Rust with [Dioxus](https://dioxuslabs.com/).

> *Google killed Inbox in April 2019. Nothing has replaced its unique approach to email — bundles, highlights, snooze, sweep, and smart extraction. Inboxly brings it back as a fully local, privacy-respecting desktop application.*

## ✨ Features

### 📥 Inbox Feed
- **Date-grouped sections** — Pinned, Today, Yesterday, This Week, This Month, Earlier
- **Email rows** with sender avatar, subject, snippet, timestamp, attachment/unread indicators
- **Bundle rows** — collapsible groups (Social, Promos, Updates, Finance, etc.)
- **Inbox Zero** 🌞 — celebratory view when you've handled everything

### ✅ Triage Actions
- **Done** — archive threads with one click (keyboard: `e`)
- **Pin** 📌 — keep important threads at the top (keyboard: `=`)
- **Sweep** — bulk-archive all unpinned threads
- **7-second undo** — timed snackbar with one-click revert

### 🕐 Snooze
- **5 preset options** — Later Today, Tomorrow, This Weekend, Next Week, Someday
- **Computed UTC times** — respects time zones and weekend preferences
- **Snoozed view** — dedicated view for all snoozed threads

### ✉️ Compose
- **Full compose view** — To, Cc, Subject, Body fields (920dp max-width)
- **Reply modes** — New, Reply, Reply All, Forward
- **Send / Discard** — with confirmation

### 🔍 Search
- **Query parser** — `from:`, `to:`, `subject:`, `has:attachment`, `is:unread` operators
- **Result view** — empty state, no-results state, result list

### 📋 Menus & Context Actions
- **PopupMenu widget** — reusable dropdown/context menu overlay (custom Iced `Widget` + `Overlay`)
- **Overflow menu** (⋮) — appears on hover with Move to, Mark read/unread, Mute, Reply, Forward, Block sender, Report spam
- **Right-click context menu** — Done/Pin/Snooze quick actions + full overflow menu at cursor position
- **RightClickArea widget** — custom Iced widget for right-click event interception

### 👤 Account Switcher
- **Inline expansion** in nav drawer — 44px avatar, display name, email, chevron
- **Multi-account support** — switch between configured accounts with one click
- **Active account indicator** — blue highlight + checkmark
- **Add account** — navigates to Settings → Accounts

### ⚙️ Settings (6 tabs)

| Tab | Controls |
|-----|----------|
| 🎨 **General** | Theme (System/Light/Dark chips), Default View, Snooze Presets, Undo Timeout |
| 👤 **Accounts** | Account cards with avatar, add/edit/remove forms, active-deletion prevention |
| 📦 **Bundles** | Reorder (↑↓ arrows), throttle badge (Immediate/Daily/Weekly), visibility toggle |
| 🔔 **Notifications** | Desktop notification toggle, sound toggle, per-bundle selection |
| ⌨️ **Keyboard Shortcuts** | 18 remappable actions, click-to-capture UI, delta-only JSON persistence |
| 💾 **Data & Storage** | Clear cache, rebuild search index, export (stub), storage sizes, last sync |

### 🎨 Theming
- **Light & dark themes** — BigTop design tokens from original Google Inbox APK
- **System detection** — D-Bus `org.freedesktop.portal.Settings` for automatic theme
- **Live switching** — theme changes apply instantly from Settings
- **Themed toolbar** — blue (Inbox), orange (Snoozed), green (Done), grey (Settings)

### ⌨️ Keyboard Shortcuts
| Key | Action | Key | Action |
|-----|--------|-----|--------|
| `e` | Done (archive) | `c` | Compose |
| `=` | Pin / unpin | `r` | Refresh |
| `b` | Snooze | `/` | Search |
| `j` / `k` | Next / previous | `o` | Open thread |
| `Ctrl+Z` | Undo | `?` | Help |
| `g i` / `g s` / `g d` | Go to Inbox / Snoozed / Done | `Esc` | Close / back |
| `Shift+R` | Reply | `Shift+A` | Reply All |
| `f` | Forward | | |

All shortcuts are runtime-remappable via Settings → Keyboard Shortcuts.

## 🏗️ Architecture

8-crate Cargo workspace:

| Crate | Purpose |
|-------|---------|
| `inboxly-core` | 🧱 Core types, traits, and error definitions |
| `inboxly-imap` | 📡 IMAP sync engine and OAuth2 authentication |
| `inboxly-store` | 💾 Maildir, SQLite, and Tantivy storage layer |
| `inboxly-bundler` | 📦 Email categorisation engine |
| `inboxly-snooze` | 🕐 Snooze scheduler and reminder system |
| `inboxly-extract` | 🔍 Smart extraction and highlight detection |
| `inboxly-ui` | 🖥️ Iced-based desktop UI |
| `inboxly` | 🚀 Binary entry point |

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

### Key Design Patterns
- **Custom Iced widgets** — `PopupMenu` (Widget + Overlay), `RightClickArea` (event interception)
- **View-local message enums** — `InboxViewMessage` decouples views from app-level `Message`
- **Auto-save settings** — every control change persists immediately, no dirty-state tracking
- **Delta persistence** — `ShortcutMap` stores only non-default overrides as JSON
- **Store adapter pattern** — `StoreSettingsAdapter` bridges crate boundaries without circular deps

## 🚀 Building

```bash
# Build
cargo build --workspace

# Test (841 tests)
cargo test --workspace

# Lint
cargo clippy --workspace -- -D warnings

# Run
cargo run -p inboxly
```

Requires Rust edition 2024 (rustc 1.85+).

## 📊 Project Status

| Milestone | Version | Tests | Description |
|-----------|---------|-------|-------------|
| M1-M25 | v0.25.0 | 715 | 🏁 Full v1 UI scaffold |
| M26 | v0.26.0 | 756 | 📋 PopupMenu widget |
| M27 | v0.27.0 | 774 | 📋 Overflow + context menus |
| M28 | v0.28.0 | 790 | 👤 Account switcher |
| M29 | v0.29.0 | 808 | ⚙️ Settings framework (3 tabs) |
| M30 | v0.30.0 | 841 | ⚙️ Settings complete (6 tabs) |

**Current: v0.30.0** — 841 tests, 0 clippy warnings, 8-crate workspace

## 📄 Licence

GPL-3.0-only
