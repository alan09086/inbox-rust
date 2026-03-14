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

**M18 complete** -- Bundle Rows + Expand/Collapse:
- Bundle row widget: collapsed summary with category icon, name, unread badge, sender previews, timestamp
- Bundle icon widget: 40dp tinted circle with Unicode category symbols
- Mixed feed rendering: email rows + bundle rows sorted by date within sections
- Bundle summary query: aggregates threads by bundle_id with sender preview subquery
- Bundle thread expansion query for detailed view
- Unbundled thread filtering (only shows threads without bundle assignment individually)
- ToggleBundle message wired for expand/collapse interaction
- 678 tests passing, 0 clippy warnings

## Licence

GPL-3.0-only
