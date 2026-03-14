# Changelog

All notable changes to this project will be documented in this file.

## [0.16.0] - 2026-03-14

### Added

- **Theme system**: `InboxlyTheme` struct wrapping `ThemeColors` with light/dark constructors and Iced `Theme::Custom` integration
- **Light/dark colour tokens**: `ThemeColors` with 11 BigTop colour tokens (background, surface, text, divider, 3 toolbar colours), `const fn` constructors
- **System theme detection**: `query_system_color_scheme()` queries freedesktop portal D-Bus (`org.freedesktop.appearance.color-scheme`) with fallback to light
- **Theme resolution**: `from_preference()`, `from_settings()`, `from_system()` -- respects `ThemePreference` (System/Light/Dark) from config or settings store
- **Settings abstraction**: `SettingsReader`/`SettingsWriter` traits for theme persistence without direct SQLite dependency
- **Theme toggle**: `toggle()`, `save_preference()`, `reset_to_system()` on `InboxlyTheme`
- **Bundle category colours**: 8 `BundleCategoryColor` constants with title+badge pairs, `for_category()` typed lookup using `BundleCategory`
- **Avatar letter tile palette**: 27-colour `AVATAR_COLORS` array (A-Z + default), `for_letter()` lookup
- **Expanded dimensions**: 16 constants from BigTop APK (toolbar, nav drawer, avatar, list items, section headers, FAB, snooze picker, compose, dividers)
- **Expanded typography**: 20 constants (sizes + weights for toolbar, email title, author, snippet, timestamp, section header, badge, nav item, compose)
- **Async startup detection**: `Task::perform` fires D-Bus query on startup when preference is System
- **App integration**: `ThemeToggled` and `ThemeChanged(InboxlyTheme)` messages, theme field on `Inboxly` app struct
- **Theme module restructure**: Converted `theme.rs` flat file to `theme/` directory with 7 submodules (colors, bundle_colors, avatar_colors, dimensions, typography, system, mod)
- **Backward-compatible API**: All M15 exports preserved (`ActiveView`, `color_from_hex`, layout constants, `category_color`)
- 79 new tests (655 total): colour tokens (23), bundle colours (11), avatar colours (7), dimensions (9), typography (9), InboxlyTheme (12), app integration (4), backward compat (4)

### Dependencies

- Added `zbus 5` (D-Bus system theme detection)
- Added `tracing 0.1` (theme detection logging)
- Added `thiserror` and `tokio` to `inboxly-ui`

## [0.15.0] - 2026-03-14

### Added

- **Iced desktop shell**: Application window using Iced 0.14 with elm-architecture (Model -> Message -> Update -> View)
- **Navigation drawer**: 264dp white sidebar with account switcher (avatar + email + account count), primary nav (Inbox/Snoozed/Done), secondary nav (Drafts/Sent/Reminders/Trash/Spam), and 8 bundle categories with BigTop coloured dots
- **Toolbar**: 56dp coloured bar with hamburger toggle, view title, search placeholder, and account avatar circle. Colour changes by view: blue (Inbox), orange (Snoozed), green (Done)
- **View switching**: NavTarget routing system -- primary views change toolbar colour, secondary nav and bundle categories update content without changing toolbar
- **Theme system**: `ActiveView` enum, `color_from_hex()`, `category_color()`, layout constants (from BigTop APK), and typography sizes
- **Nav types**: `NavSection`, `NavBundleCategory`, `NavTarget` for unified navigation handling
- **Iced 0.14 workspace dependency**: Added to workspace Cargo.toml with `advanced` feature
- 19 new tests (576 total): state management (11), theme colours and layout constants (8)

## [0.14.0] - 2026-03-14

### Added

- **Bundle throttling**: `BundleThrottle` enum with `Immediate`, `Daily { delivery_time }`, and `Weekly { delivery_day, delivery_time }` variants in `inboxly-core/src/throttle.rs`
- **WeekdayWrapper**: Serializable wrapper around `chrono::Weekday` with lowercase string serde (monday, tuesday, etc.)
- **Throttle delivery windows**: `is_window_open()` and `next_window()` for computing when throttled bundles surface in the inbox feed
- **Store throttle CRUD**: `get_bundle_throttle()`, `set_bundle_throttle()`, `get_throttled_bundles()`, `get_currently_suppressed_bundle_ids()` in `inboxly-store/src/throttle.rs`
- **Throttle-aware thread queries**: `get_threads_excluding_bundles()` and `get_threads_throttled()` for inbox feed filtering
- **Schema migration v3->v4**: Converts plain-string throttle values (Immediate/Daily/Weekly) to JSON format with delivery times
- **Background throttle scheduler**: `spawn_throttle_scheduler()` in `inboxly-bundler/src/scheduler.rs` -- tokio task that checks windows every 60s and emits `ThrottleEvent::WindowOpened` events
- **Body re-evaluation**: `BundlerEngine::re_evaluate_with_body()` re-runs the four-layer pipeline when Phase 2 sync delivers message bodies
- **BundlerEvent types**: `ThrottleChanged`, `BundleChanged`, `ThrottleWindowOpened` in `inboxly-bundler/src/events.rs` for UI notification
- **ThrottleWindowOpened sync event**: Added to `SyncEvent` in `inboxly-imap/src/channel.rs`
- **Default throttle presets**: `default_throttle_for_category()` -- Promos daily 5 PM, Updates daily 9 AM, Forums daily noon, Low Priority weekly Monday 8 AM, Social/Finance/Travel/Purchases immediate
- **BundleCategory::as_str()**: Stable lowercase string key for settings storage
- **BundleId::FromStr**: Parse UUID strings into BundleId
- **Application scheduler wiring**: Basic tokio runtime with scheduler demo in `inboxly/src/main.rs`
- 49 new tests (557 total): throttle types (17), store CRUD (9), scheduler (3), body re-evaluation (4), events (3), presets (5), integration (8)

## [0.13.0] - 2026-03-14

### Added

- **User-defined bundle rules**: `UserRuleField` (From, To, Subject, Header, Body), `UserRuleOp` (Contains, Equals, Matches, Domain), `BundleRule` struct with priority-ordered first-match evaluation in `inboxly-bundler/src/user_rules.rs`
- **Pre-compiled regex caching**: `UserCompiledRule` wraps `BundleRule` with optional pre-compiled `Regex` for efficient repeated evaluation; invalid patterns gracefully return no-match
- **RuleMatchable trait**: Abstract email field access for rule matching, enabling pure-function testing with `MockEmail` test doubles
- **RuleStore trait**: CRUD abstraction for bundle rule persistence (create, get, list, list_by_bundle, update, delete) with regex validation on create/update; `MockRuleStore` for tests
- **Sender affinity learning**: `SenderAffinity` struct with exponential confidence decay (90-day half-life), `reinforce()` (+0.2 per user action, 5 actions to max), `penalize()` (-0.3 on override), threshold at 0.6
- **AffinityStore trait**: Persistence abstraction for sender learning (get, record with auto-reinforce/penalize, list, delete); `MockAffinityStore` for tests
- **Custom bundle creation**: `BundleStore` trait with `CreateBundleParams`, `UpdateBundleParams`, `BundleInfo`; `BundleStoreError` with built-in bundle protection; re-uses `BundleVisibility`/`BundleThrottle` from `inboxly-core`
- **Four-layer evaluation pipeline**: `BundlerEngine::categorise()` runs user rules > sender learning > header heuristics > uncategorised with `CategoriseResult` and `CategoriseSource` tracking which layer matched
- **`HeuristicMatch` bridge type**: Connects M12's heuristic engine output into M13's unified pipeline
- **Re-categorisation**: `process_move()` handles user manual moves, updating sender affinity and returning `MoveResult` with new/reinforced status
- **Shared test utilities**: `test_utils::fixtures` module with `MockEmail` and `make_rule` helper for consistent test doubles across modules
- 59 new tests (496 total): user rule matching (21), RuleStore CRUD (8), custom bundles (6), sender affinity (15), evaluation engine (6), pipeline integration (4), recategorise (3)

## [0.12.0] - 2026-03-14

### Added

- **Bundler header heuristics**: Automatic email categorisation using header-based pattern matching in new `inboxly-bundler` crate
- **25 default heuristic rules** covering 8 categories: Social, Promos, Updates, Finance, Purchases, Travel, Forums, Low Priority
- **TOML rule engine**: `HeuristicRule`, `RuleField`, `RuleOp` types with `parse_rules()` and `load_rules()` for user overrides at `~/.config/inboxly/heuristics.toml`
- **Compiled regex matching**: `CompiledRule` pre-compiles regex patterns at construction; domain glob-to-regex conversion; priority-ordered first-match-wins evaluation
- **System bundles**: 8 default bundles with BigTop colour palette, deterministic UUID v5 IDs (stable across reinstalls), idempotent `ensure_system_bundles()` for startup
- **`Bundler` struct**: Public API with `new()`, `categorise()`, `categorise_all()`, `categorise_thread()`, `bundle_id_for_category()`, `rule_count()`
- **`Contact` Display impl**: Formats as `"Name <address>"` or bare `"address"` when name is empty
- **`EmailMeta::test_default()`**: Test fixture constructor behind `test-helpers` feature flag
- **Store methods for bundler**: `get_uncategorised_thread_ids()`, `get_newest_email_in_thread()`, `load_email_headers()` for batch categorisation
- 45 new tests (437 total): TOML parsing (6), system bundles (6), heuristic matching (19), integration (11), core Display (2), doctest (1)

## [0.11.0] - 2026-03-14

### Added

- **Avatar colour palette**: 26-colour BigTop APK palette (`AvatarColor`, `AVATAR_PALETTE`, `AVATAR_COLOR_DEFAULT`) for consistent sender visual identification in `inboxly-core/src/contact.rs`
- **RFC 2822 address parsing**: `parse_address()` and `parse_address_list()` handle display name + angle bracket, quoted names with commas, bare addresses, and case normalisation
- **`ParsedAddress` type**: Intermediate parsed address with optional name and normalised address
- **`Contact::avatar_color()`**: Returns the palette colour for a contact's avatar letter
- **`ContactRow::from_address()`**: Constructor that automatically derives avatar letter (from display name or email local part) and palette index
- **`Store::list_all_contacts()`**: Returns all contacts ordered by most recently seen
- **`Store::extract_contacts_from_headers()`**: Parses From/To/Cc headers and upserts contacts into the database (for email ingest pipeline)
- **`Store::backfill_contacts_from_emails()`**: Batch-extracts contacts from all existing emails in the database (idempotent, for database rebuild or M11 migration)
- **Improved upsert logic**: `upsert_contact()` now preserves `avatar_color_index` when display name is NULL (prevents colour reset on bare-address updates)
- 42 new tests (392 total): palette lookups (7), contact creation (5), address parsing (9), ContactRow CRUD (6), header extraction (2), backfill (5), JSON parsing (2), integration pipeline (3), palette verification (1), deduplication (1)

## [0.10.0] - 2026-03-14

### Added

- **Threading module**: Full References-based email threading algorithm in `inboxly-store/src/threading/` (simplified JWZ, no subject-based grouping)
- **Header extraction**: Parse Message-ID, In-Reply-To, References with case-insensitive lookup, angle bracket stripping, bare ID support, header folding whitespace handling (`threading/headers.rs`)
- **Thread assignment**: Core algorithm using `References[0]` as thread root, with placeholder threads for orphaned replies (`threading/assign.rs`)
- **Placeholder tracking**: `is_placeholder_thread` and `list_placeholder_threads` helpers for diagnostics and re-threading
- **Thread unification**: Merges placeholder threads when root emails arrive, handles cross-thread merge scenarios (`threading/unify.rs`)
- **Metadata aggregation**: Recalculates subject (oldest), snippet (newest), dates, counts, attachment flag per thread; bulk refresh for account; participant extraction (`threading/metadata.rs`)
- **Batch threading**: Processes unthreaded emails oldest-first to minimize placeholders, chunked at 5000 for large mailboxes; targeted email ID batch threading (`threading/batch.rs`)
- **Thread rebuild**: Wipe and reconstruct all threads from scratch with proper FK cleanup (thread_state, highlights) (`threading/rebuild.rs`)
- **Self-referencing protection**: Prevents infinite loops when broken mailers put own Message-ID in References
- **Schema migration v3**: `root_message_id TEXT` column on threads table with index for placeholder tracking
- **ThreadRow.root_message_id**: New `Option<String>` field on `ThreadRow` for placeholder thread identification
- 75 new tests (350 total): header parsing (16), thread assignment (10), placeholder helpers (4), unification (4), metadata aggregation (6), batch threading (5), rebuild (4), edge cases (17), integration tests (13)

## [0.9.0] - 2026-03-14

### Added

- **Incremental sync**: UIDNEXT-based new message detection — fetches only what changed since last sync (`inboxly-imap/src/incremental.rs`)
- **CONDSTORE flag sync**: CHANGEDSINCE-based flag updates (RFC 4551) — only fetches messages with changed flags
- **Non-CONDSTORE fallback**: 30-day UID window for flag sync on servers without CONDSTORE
- **Deleted message detection**: UID comparison to detect server-side deletions within 30-day window
- **IDLE push sync**: Real-time server notifications with EXISTS/EXPUNGE/FETCH response parsing (`inboxly-imap/src/idle.rs`)
- **IDLE reconnect loop**: Exponential backoff (5s to 5min), configurable max failures (default 10)
- **Per-account sync loop**: IDLE on INBOX + 5-minute periodic catch-up for Sent/Drafts/Trash/Spam (`inboxly-imap/src/sync_loop.rs`)
- **Polling fallback**: 60-second polling for servers without IDLE support
- **SyncManager**: Multi-account lifecycle management (register/stop/stop_all) with master cancellation (`inboxly-imap/src/sync_manager.rs`)
- **AccountSyncConfig**: Structured config for sync loop parameters (avoids argument-count lint)
- **New SyncEvent variants**: `EmailsDeleted`, `IncrementalSyncComplete`, `SyncUpToDate`
- **New ImapError variants**: `UidValidityChanged`, `IdleInterrupted`, `IdleNotSupported`, `SyncCancelled`, `SyncNotRunning`, `NoSyncState`, `Protocol`, `Store`
- **Store extensions**: `get_uids_in_folder`, `get_uids_since`, `mark_email_deleted_by_uid`, `update_flags_by_uid`, `upsert_email`
- 32 new tests (275 total): UID set formatting (9), SQLite helpers (4), IDLE parsing (10), folder resolution (2), SyncManager lifecycle (6), channel/struct tests (1)

## [0.8.0] - 2026-03-14

### Added

- **Phase 2 body download**: Background RFC822 fetch to Maildir with tantivy indexing (`inboxly-imap/src/phase2.rs`)
- **Batch RFC822 FETCH**: Fetch bodies in batches of 500, newest-first for fastest UX (`inboxly-imap/src/body_fetch.rs`)
- **Body processing pipeline**: Maildir write + body text extraction + SQLite update (`inboxly-imap/src/body_processor.rs`)
- **On-demand body fetch**: Single-email fetch when user opens before Phase 2 reaches it (`inboxly-imap/src/on_demand.rs`)
- **Progress reporting**: `SyncEvent::BodyDownloadProgress`, `BodyFetched`, `BodyDownloadComplete`, `BodyDownloadError`
- **Resume capability**: `body_downloaded` column IS the checkpoint — restart picks up where it left off
- **Offline action queue**: `OfflineAction` enum (9 variants) with serde JSON serialization in `inboxly-core`
- **Offline replay**: Drain queue and replay actions against IMAP on reconnect (`inboxly-imap/src/offline_replay.rs`)
- **`FetchBodyOnDemand` command**: UI can request single-email body fetch via `UiCommand`
- **Schema migration v2**: `body_downloaded` column with partial index for efficient Phase 2 queries
- **Store methods**: `mark_body_downloaded`, `is_body_downloaded`, `get_maildir_path`, `count_emails_without_body`, `get_uids_without_body`, `get_email_id_by_uid`
- 19 new tests (243 total): body text extraction (7), offline action serde (3), offline queue integration (3), Phase 2 resume/ordering/progress (6)

## [0.7.0] - 2026-03-14

### Added

- **Sync engine**: Phase 1 initial sync in `inboxly-imap/src/sync/`
- **Batch processing**: Newest-first UID range splitting with configurable batch size
- **UIDVALIDITY**: State persistence, staleness detection, automatic folder invalidation on reset
- **Envelope parsing**: IMAP ENVELOPE to EmailRow conversion with RFC 2822 date handling
- **Batch insert**: Transactional SQLite insertion with ON CONFLICT IGNORE dedup
- **Threading**: Basic thread association via In-Reply-To/References header chains
- **Sync orchestrator**: `run_phase1_sync()` — full async pipeline (SELECT → batch FETCH → parse → insert → thread → persist state)
- **Crash recovery**: Per-batch state persistence enables resume from last completed batch
- **Progress events**: SyncEvent channel for real-time UI feedback (header count, batch progress)
- 37 new sync tests (batching, UID state, envelope parsing, store operations, threading, engine integration)

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
