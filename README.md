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

**M9 complete** — Incremental sync + IDLE push notifications:
- Phase 1 (M7): Batched header sync with crash recovery and progress events
- Phase 2 (M8): Background RFC822 body download to Maildir with tantivy indexing
- Incremental sync (M9): UIDNEXT-based new message detection, CONDSTORE flag sync (CHANGEDSINCE), non-CONDSTORE 30-day fallback, deleted message detection
- IDLE push (M9): Real-time server notifications with 29-minute timeout, exponential backoff reconnect, cancellation support
- Per-account sync loop with IDLE on INBOX + 5-minute periodic catch-up for other folders
- Multi-account SyncManager with start/stop/stop_all lifecycle control
- Polling fallback for servers without IDLE support
- On-demand single-email fetch for immediate display
- Resume capability — restart picks up where it left off
- Offline action queue with 9 action types and IMAP replay on reconnect
- 275 tests passing, 0 clippy warnings

## Licence

GPL-3.0-only
