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

**M4 complete** — Maildir filesystem operations:
- Maildir++ folder structure with standard IMAP folder mapping
- Atomic email storage (tmp → new → cur), flag encoding/decoding
- .eml parsing to EmailMeta (lightweight) and EmailContent (lazy-loaded)
- Message listing, deletion, move, copy operations
- Disaster recovery: rebuild SQLite from Maildir scan
- 126 tests passing

## Licence

GPL-3.0-only
