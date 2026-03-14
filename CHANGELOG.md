# Changelog

All notable changes to this project will be documented in this file.

## [0.3.0] - 2026-03-14

### Added

- **SQLite store**: Full `inboxly-store` crate with rusqlite (bundled SQLite, WAL mode)
- **Schema migration**: v0→v1 migration creating 13 tables with indexes and foreign keys
- **Store struct**: `open`/`open_in_memory`/`transaction`/`rebuild` API
- **13 CRUD modules**: accounts, emails, threads, thread_state, sync_state, contacts, bundles, bundle_rules, sender_affinity, reminders, highlights, settings, offline_queue
- **Email operations**: UID lookup, flag updates, thread reassignment, max UID query
- **Thread operations**: upsert, pagination, account-scoped listing
- **Thread state**: pin/done/snooze/bundle assignment with filtered queries
- **Contact cache**: upsert with COALESCE for display names, prefix search
- **Bundle rules**: priority-ordered rule evaluation for the bundler engine
- **Sender affinity**: confidence-based learned categorisation with domain fallback
- **Reminders**: time-based and location-based queries
- **Offline queue**: FIFO action replay for reconnect scenarios
- **Database rebuild**: Drop and recreate all tables for Maildir recovery
- 18 new integration tests (accounts, emails, threads, thread_state, sync_state, contacts, bundles, bundle_rules, sender_affinity, reminders, highlights, settings, offline_queue, transactions, rebuild)

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
