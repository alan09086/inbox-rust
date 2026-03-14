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

**M12 complete** — Bundler Header Heuristics:
- Automatic email categorisation using header-based heuristics (zero-config, 25 default rules)
- 8 system bundles with BigTop colours: Social, Promos, Updates, Finance, Purchases, Travel, Forums, Low Priority
- TOML-defined rules with user override support (`~/.config/inboxly/heuristics.toml`)
- Priority-ordered matching: higher priority rules evaluated first, first match wins
- Compiled regex engine for pattern matching (domain globs, header values, subject patterns)
- Deterministic UUID v5 system bundle IDs (stable across reinstalls)
- Batch `categorise_all()` and single `categorise_thread()` APIs for integration with sync pipeline
- 437 tests passing, 0 clippy warnings

## Licence

GPL-3.0-only
