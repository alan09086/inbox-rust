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
| M31 | v0.31.0 | — | 🧱 Store traits + action execution |
| M32 | v0.32.0 | 225 | 🔄 Iced 0.14 → Dioxus 0.7 framework conversion |
| M33 | v0.33.0 | 242 | 📥 Inbox feed + widgets restored on Dioxus |
| M33.1 | v0.33.1 | 870 | 🔧 Post-M33 polish (window title, dark mode) |
| M34 | v0.34.0 | 882 | 🧵 Thread detail view + HTML email rendering |
| M34.1 | v0.34.1 | 884 | 🔧 Post-M34 polish (test side effect, validate_external_url, Settings re-entry) |
| M35 | v0.35.0 | 961 | ✉️ SMTP engine + compose view + drafts (M35a refactor + M35b feature work) |
| M35.1 | v0.35.1 | 961 | 🔧 Post-M35 dogfooding polish (inline CSS, 1280×800 default, FAB hide, blur-to-chip) |
| **M36** | **v0.36.0** | **1072** | **✉️ Reply/ReplyAll/Forward + M35 cleanup (keyring, OAuth2 persistence, Sent write, explicit save)** |

**Current: v0.36.0** — 1072 tests, 0 clippy warnings, 8-crate workspace

### M36 highlights

M36 closes every M35.1 follow-up item and ships the
Reply / ReplyAll / Forward feature surface on top. Fourteen phases across
two sub-milestones under a `/plan-eng-review` pass (findings A1-A9) and a
Gemini outside-voice pass (findings G1-G8):

- **M36a (Phases 0-5)** — M35 cleanup: a real `keyring 3` secrets backend
  (replaces the `INBOXLY_SMTP_PASSWORD` env var), OAuth2 refresh token
  persistence with a rotation callback (A3), a local `.Sent/` write on
  every successful SMTP send + real IMAP `APPEND` via `WellKnownFolders`
  (A6), and an explicit Save Draft bridge with a three-layer persistence
  model (in-memory → SQLite → offline queue) plus a Navigate-with-compose
  auto-save guard (A8). New `inboxly` CLI subcommands: `oauth2-authorize`,
  `set-password`, `delete-credentials`.
- **M36b (Phases 6-13)** — Reply feature surface: pure helpers for
  subject normalization + References chain + Gmail-compatible quote
  formatting, a `compose_state_from_original` DRY helper + `ComposeMode`
  dispatch, a real `OpenComposeReply` handler with a body-fetch fallback
  for header-only local copies (G3) via new `ThreadReader::load_email`,
  forward attachment passthrough, Reply / ReplyAll / Forward buttons in
  the thread message footer, an inline compose panel with a layout
  toggle (Inline ↔ FullScreen), a quoted-original placeholder, and 11
  end-to-end state-machine integration tests.

**Known runtime gap preserved from M35:** the binary still doesn't
instantiate `Store` / `MaildirStore` / `ThreadReader` at startup. Phases
4, 5, and 8 use on-demand `MaildirStore` construction to sidestep this
for the automated test paths, but clicking Reply in the running binary
will surface `ComposeReplyFailed { reason: "thread_reader not wired" }`
until a follow-up wires the data layer. A dedicated **M36.1 polish
milestone** is scheduled to close this gap. See CHANGELOG.md for the
full phase-by-phase breakdown and the four documented scope reductions
(Phase 9 forward streaming, Phase 11 signal split, Phase 12 quoted
original expansion, Phase 5 IMAP draft replay).

### M35 highlights

M35 closes the read/write loop. After M34 users could read email via the
thread detail view; after M35 they can write it too. The milestone shipped
in two sub-milestones:

- **M35a** — behaviour-preserving god-object refactor (31 flat fields on
  `Inboxly` extracted into `SettingsState`, `MenuState`, `SnoozeState`)
- **M35b** — 14 phases covering lettre 0.11 API verification, core data
  types, SQLite drafts table, SMTP transport with dual message builders
  (Gemini G1 Bcc-not-in-headers invariant), retry logic with PII-redacted
  logging, IMAP APPEND helpers, sync-side Message-ID dedup (G8), offline
  replay via a `DraftSender` trait, ComposeState + 23 Message variants,
  CSS, the ComposeView Dioxus component, FAB wiring, 30 s auto-save
  bridge, rfd attachment picker, and the send bridge with two-phase
  commit dismiss overlay (G9) + AppendSent fallback (G6).

All six M35b limitations scheduled for M36 were closed in M36a (Phases
0-5): keyring-backed password secrets replace the `INBOXLY_SMTP_PASSWORD`
env var, OAuth2 refresh token persistence with rotation callback is live
end-to-end, `SmtpClient::send()` writes to the local Maildir `.Sent/`
atomically on every success, the real `AppendSent` replay handler
resolves the Sent folder via `WellKnownFolders`, the explicit Save Draft
bridge landed with a three-layer persistence model and a
Navigate-with-compose auto-save guard, and the toolbar Draft chip shows
unsaved / saving / saved state. Remaining deferrals after M36 are four
smaller scope reductions (all `TODO(post-M36)` in-tree): forward
attachment streaming, compose signal split, quoted-original expanded
preview, and IMAP APPEND for drafts. See CHANGELOG.md for the full
phase-by-phase breakdown.

## 📄 Licence

GPL-3.0-only
