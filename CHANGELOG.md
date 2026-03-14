# Changelog

All notable changes to this project will be documented in this file.

## [0.6.0] - 2026-03-14

### Added

- **IMAP crate**: Full `inboxly-imap` crate with async connection, auth, and folder management
- **TLS**: Implicit TLS (port 993) and STARTTLS upgrade with rustls + webpki roots
- **Connection**: ImapConnection with capability detection (IDLE, CONDSTORE, COMPRESS, etc.)
- **Password auth**: LOGIN command with credential redaction in Debug output
- **OAuth2**: Gmail PKCE authorization code flow with loopback HTTP server, token refresh
- **XOAUTH2**: SASL mechanism for IMAP AUTHENTICATE with base64-encoded bearer token
- **Folder listing**: LIST with SPECIAL-USE attribute parsing (RFC 6154) and name-based fallback heuristics
- **Auth dispatcher**: Routes Password/AppPassword to LOGIN, OAuth2 to XOAUTH2
- **Connection pool**: Semaphore-gated concurrency (default 3), exponential backoff retry, NOOP health checks
- **Sync channels**: SyncEvent (9 variants) and UiCommand (4 variants) via tokio mpsc
- 35 new IMAP tests (TLS, connection, auth, OAuth2, XOAUTH2, folders, pool, channels, integration)

## [0.5.0] - 2026-03-14

### Added

- **Tantivy search index**: Full-text search engine in `inboxly-store`
- **Search schema**: 9 indexed fields (email_id, from, to, subject, body_text, date, account_id, bundle_category, has_attachment)
- **Document conversion**: EmailMeta + body text to tantivy Document with facet encoding
- **SearchIndex**: create/open/open_or_create lifecycle, in-memory for tests
- **Indexing**: add_email, batch_index, remove_email, update_email with atomic commit
- **Query builders**: term, phrase, multi-field, facet filter, date range, has:attachment
- **BM25 + recency boost**: Exponential decay scoring (60-day half-life, 2x max boost)
- **SearchHit**: Structured results with score, email_id, subject, from, date
- **Rebuild**: Abstract RebuildSource trait for full index reconstruction
- **Clear/destroy**: Delete all documents or entire index directory
- 26 new search integration tests

## [0.4.0] - 2026-03-14

### Added

- **Maildir operations**: Full Maildir++ filesystem layer in `inboxly-store`
- **MaildirStore**: Folder initialization with standard IMAP folder mapping (INBOX, Sent, Drafts, Trash, Jstrk, Archive)
- **Flag encoding**: Bidirectional IMAP ↔ Maildir flag conversion (DFPRST suffix format)
- **Atomic writes**: store_new (tmp→new), deliver (new→cur), store_cur with flag suffixes
- **Flag updates**: set_flags, add_flags, remove_flags via filename rename
- **Email parsing**: parse_email_meta (lightweight headers+snippet) and read_email_content (full body+attachments)
- **Message operations**: list_messages, count_messages, delete_message, move_message, copy_message
- **Disaster recovery**: scan_folder, scan_all, rebuild_emails_from_maildir for SQLite reconstruction
- **Test fixtures**: 4 RFC 5322 .eml files (simple, multipart, attachment, reply)
- 20 new tests (5 flag unit tests + 15 maildir integration tests)

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
