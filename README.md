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

**M13 complete** — Bundler User Rules + Sender Learning:
- Four-layer categorisation pipeline: user rules > sender learning > header heuristics > uncategorised
- User-defined rules with Contains, Equals, Matches (regex), and Domain operators across From, To, Subject, Header, and Body fields
- Pre-compiled regex caching for efficient repeated evaluation
- Sender affinity learning from user moves with exponential confidence decay (90-day half-life)
- Confidence threshold (0.6): sender learning only fires with sufficient evidence
- Override penalty: moving to a different bundle penalises the old affinity
- Custom bundle creation with BundleStore trait (name, colour, visibility, throttle)
- Re-categorisation on user move: updates both thread bundle and sender affinity
- `BundlerEngine::categorise()` unifies all four layers with clear precedence
- RuleStore and AffinityStore traits for testable persistence abstraction
- 496 tests passing, 0 clippy warnings

## Licence

GPL-3.0-only
