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

**M10 complete** — References-based email threading algorithm:
- Simplified JWZ threading: groups emails by `References[0]` (thread root), no subject-based grouping
- Placeholder threads for orphaned replies that resolve when root email arrives
- Thread unification: merges placeholder threads when root emails arrive, handles cross-thread merges
- Thread metadata aggregation: subject (oldest), snippet (newest), dates, counts, attachment flags
- Batch threading: processes unthreaded emails oldest-first to minimize placeholders, chunked for large mailboxes
- Full thread rebuild: wipe and reconstruct all threads from scratch (for algorithm updates or integrity repair)
- Self-referencing protection prevents infinite loops from broken mailers
- Schema migration v3: `root_message_id` column on threads table for placeholder tracking
- 350 tests passing, 0 clippy warnings

## Licence

GPL-3.0-only
