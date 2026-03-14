# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] - 2026-03-14

### Added

- **Config system**: TOML-based configuration at `~/.config/inboxly/config.toml`
- **AuthMethod**: Password, OAuth2, AppPassword authentication variants
- **AccountConfig**: Multi-account email settings with serde defaults (IMAP 993, SMTP 587)
- **SnoozePresets**: Configurable morning/afternoon/evening hours and weekend day
- **ThemePreference**: System/Light/Dark theme selection
- **AppConfig**: Top-level config with accounts, theme, directory overrides, snooze presets
- **Paths**: XDG path resolver for config/data/cache directories with config overrides
- **ConfigError**: Typed errors for I/O, parse, serialize, validation, and missing home dir
- **load/save**: First-run defaults on missing file, pretty TOML serialization
- **validate**: Account field validation, port range, snooze hour bounds
- 38 new config tests (serialization, defaults, validation, file I/O, XDG paths, realistic TOML)

## [0.1.0] - 2026-03-14

### Added

- **Workspace**: 8-crate Cargo workspace with centralised version and dependency management
- **Identity types**: `AccountId`, `EmailId`, `ThreadId`, `BundleId` with UUID/string newtypes
- **Contact types**: `Contact` with avatar letter generation
- **Attachment types**: `AttachmentMeta` (lightweight) and `Attachment` (with content bytes)
- **Email flags**: `EmailFlags` with IMAP semantics and bitmask conversion for SQLite
- **Email types**: `EmailMeta` (SQLite-resident metadata) and `EmailContent` (lazy-loaded body)
- **Thread type**: Conversation grouping with participant tracking and unread counts
- **Bundle types**: `Bundle`, `BundleCategory`, `BundleVisibility`, `BundleThrottle`, `BundleIcon`, `Color`
- **Highlight types**: `Highlight` enum (tracking, flight, hotel, event, payment) and `TripBundle`
- **Inbox types**: `InboxItem` (unified feed), `ThreadState`, `SnoozeInfo`, `SnoozeUntil`
- **Error types**: `InboxlyError` with thiserror, covering storage/IMAP/bundler/snooze/config errors
- **Trait definitions**: `Store`, `Bundler`, `Extractor` async trait interfaces
- **Inter-crate dependencies**: Full DAG wired per design spec
- **Integration test**: Imports and instantiates every public type (9 test cases)
- 50 tests total (41 unit + 9 integration)
