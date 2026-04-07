# Changelog

All notable changes to this project will be documented in this file.

## [0.35.0] - 2026-04-07

### Added (M35 — SMTP Engine + Compose View)

Closes the read/write loop. After M34, users could READ email via the thread
detail view; after M35, they can WRITE it too. Sixteen commits across two
sub-milestones (M35a refactor + M35b feature work) under a `/plan-eng-review`
audit (16 issues) and a Gemini outside-voice pass (9 additional findings, G1-G9).
All 25 findings were resolved interactively before implementation began.

#### M35a — God-object refactor (pre-requisite, behaviour-preserving)

Before adding ~14 new compose state fields, the 53-field `Inboxly` god-object
was refactored into three sub-structs so M35b's new fields could land in a
clean shape from day one. Isolated as its own sub-milestone to contain
regression risk (Gemini G7).

- `inboxly-ui/src/state/settings_state.rs` — `SettingsState` (21 fields):
  `theme_preference`, `default_view`, `undo_timeout_secs`, account form,
  storage display, keyboard shortcuts, notifications, bundles, settings tab.
- `inboxly-ui/src/state/menu_state.rs` — `MenuState` (5 fields): overflow +
  context menu state with `close()` helper replacing `Inboxly::close_menus()`.
- `inboxly-ui/src/state/snooze_state.rs` — `SnoozeState` (2 fields): picker
  thread + position.
- Every handler, component, and test updated to use the new dotted paths
  (`app.settings.theme_preference`, `app.menus.overflow_thread`, etc.).
  All 884 existing tests continued to pass — refactor was behaviour-preserving.

#### M35b — SMTP + Compose + Drafts (the new feature)

**Phase 0 — lettre 0.11 API verification.** Downloaded the crate and grepped
its source to confirm two API unknowns before writing any SMTP code. Documented
in `docs/superpowers/notes/2026-04-07-lettre-api-verification.md`:
- `Mechanism::Xoauth2` exists as a built-in enum variant with no feature gate.
  **Critical:** not in `DEFAULT_MECHANISMS` — must explicitly call
  `.authentication(vec![Mechanism::Xoauth2])` or Gmail rejects with 534-5.7.9.
- `MessageBuilder::bcc()` defaults to envelope-only via `drop_bcc: true`;
  `keep_bcc()` is the documented opt-out for the Sent-folder APPEND case.

**Phase 1 — Core data types + Markdown converter.** Added `DraftEmail`,
`ComposeMode` (with placeholder Reply/ReplyAll/Forward variants for M36),
`AttachmentDraft`, `AttachmentSource::Disk(PathBuf)` to `inboxly-core`.
New `inboxly-core/src/markdown.rs` with `markdown_to_html` and
`markdown_to_plaintext` via `pulldown-cmark 0.12`. Raw HTML in the source is
dropped at the parse layer via an `Event::Html`/`Event::InlineHtml` filter
(pulldown-cmark 0.12 dropped `Options::ENABLE_HTML` entirely, so the
event-filter pattern replaces it). Tables, strikethrough, tasklists enabled.
15 unit tests including the raw-HTML-dropped security invariant.

**Phase 2 — Drafts table + Store CRUD.** SQLite migration v4→v5 for the
`drafts` table (15 columns). `inboxly-store/src/drafts.rs` with `DraftRow`
+ `insert_draft`, `update_draft`, `get_draft`, `list_drafts`, `delete_draft`
Store methods. Message-ID uniqueness is the dedup invariant across the
three-layer persistence (SQLite, Maildir, IMAP Drafts). `references` SQL
column renamed to `references_header` to avoid the SQLite reserved word trap.
6 integration tests.

**Phase 3 — SMTP transport + dual message builders.** `inboxly-imap/src/smtp/`
with six new modules: `error.rs`, `retry.rs` (pure `should_retry` function),
`redact.rs` (SHA-256 recipient hash, body never logged), `message_builder.rs`
(TWO builders per Gemini G1: `build_rfc5322_for_smtp` with `drop_bcc: true`
for wire send, `build_rfc5322_for_sent_folder` with `.keep_bcc()` for the
Sent folder copy), `transport.rs` (SmtpSender), `draft_sender.rs`
(`DraftSender` trait for mocking). Also adds
`inboxly-imap/src/auth/shared_oauth2.rs` for cross-transport token sharing
(Issue 1.2). 23 new unit tests including both Gemini G1 Bcc invariants.

**Phase 4 — IMAP APPEND helpers + Maildir dedup.**
`inboxly-imap/src/append.rs` with `imap_append_draft` and `imap_append_sent`
using the `build_rfc5322_for_sent_folder` builder. `MaildirStore::has_message_id`
linear scan + 1 unit test.

**Phase 4b — Sync-side dedup wiring (Gemini G8).** New
`MaildirStore::find_message_id` returning `Option<PathBuf>`; `process_body()`
checks it for Drafts/Sent folders BEFORE writing, and on a match skips the
write and points the email row at the existing file. Prevents visible
duplicate drafts. 3 integration tests.

**Phase 5 — Offline replay (additive `SendDraftFull`).** New
`OfflineAction::SendDraftFull { draft: Box<DraftEmail> }` variant alongside
the legacy `SendDraft`. `DraftSender` trait abstraction with a
`MockDraftSender` for tests. `replay_offline_queue` now takes
`Option<&dyn DraftSender>`; existing callers pass `None`. 4 unit tests.

**Phase 6 — ComposeState + 23 Message variants + state-machine tests.**
`inboxly-ui/src/state/compose_state.rs` with `ComposeState` (17 fields
including `Arc<Contact>` recipients per Issue 4.2, eager `draft_id` per
Gemini G4) and `ComposeSendState` (two-phase commit per Gemini G9).
`ActiveView::Compose` added to the theme. 23 new Message variants. All
field-change handlers respect the `Sending` guard (Gemini G5). Pure
`validate_smtp_recipient` free function. 14 state-machine tests.

**Phase 7 — CSS.** Section 16 of `inboxly-ui/assets/main.css` with ~25
selectors using existing custom properties so dark mode works automatically.

**Phase 8 — ComposeView Dioxus component.**
`inboxly-ui/src/components/compose_view.rs`: header with back button +
title + AccountPickerDropdown, recipient rows (To always visible, Cc/Bcc
collapsible) with chips + Enter/comma parsing, Subject input, body textarea
or Markdown preview (toggleable), attachment chips row, footer. Three
sub-components: `RecipientChip`, `AttachmentChip`, `AccountPickerDropdown`.
Gemini G9 two-phase "Sent — Dismiss" overlay and Failed error banner.

**Phase 9 — FAB wiring.** `SpeedDialFab` onclick dispatches
`Message::OpenCompose`. 3 state-machine tests for the FAB → OpenCompose
flow (round-trip via CloseCompose, previous_view capture from Done,
idempotent second OpenCompose).

**Phase 10 — Auto-save bridge.** New `use_effect` watching
`(compose.dirty, compose.save_generation, compose.draft_id)`. 30-second
`tokio::time::sleep` timer with generation-snapshot stale-result guard
(Issue 1.8). Aborts if `send_state` is `Sending`/`Sent` at wake time
(Gemini G5). Calls `Store::update_draft` gated behind `#[cfg(not(test))]`.
New `compose_state_to_draft_email` helper + `account_id_from_email` helper
using deterministic UUID v5 namespace derivation. 3 helper unit tests.

**Phase 11 — Attachment picker bridge.** New `use_effect` watching a
counter. Opens `rfd::AsyncFileDialog`, reads file metadata BEFORE the bytes
(Gemini G3 metadata-first), enforces 20 MB total cap, copies to
`~/.local/share/inboxly/drafts/<id>/` with UUID-suffixed disk filename
(Gemini G2). New `inboxly-store/src/draft_attachments.rs` with
`ensure_draft_dir`, `make_draft_filename`, `cleanup_draft_dir`. All
picker + file I/O gated behind `#[cfg(not(test))]`. 4 unit tests.

**Phase 12 — Send bridge.** Final `use_effect` watching
`compose.send_state == Sending`. Builds the `DraftEmail`, constructs an
`SmtpSender`, runs the `should_retry` loop (3 attempts with 1s/2s delays).
On success: enqueues `OfflineAction::AppendSent`, deletes the SQLite draft,
runs `cleanup_draft_dir`, dispatches `ComposeSendComplete { success: true }`
which transitions to `Sent { dismiss_pending: true }`. Stale-result guard
for **failures only** (Gemini G9 fix). Password auth reads from
`INBOXLY_SMTP_PASSWORD` env var. New `OfflineAction::AppendSent` variant.

**Phase 13 — Verification.** Final workspace pass: **961 tests passing**
(884 baseline + 77 new across M35a and M35b), clippy clean on library
targets, `cargo build -p inboxly` clean, release-mode `inboxly-ui` tests
all passing (299).

### Known M35 limitations (follow-ups for M36+)

These are documented explicitly so users attempting end-to-end verification
know what to expect. None of these are regressions — they are deliberate
M35b scope cuts that the plan authorized:

- **Password auth requires `INBOXLY_SMTP_PASSWORD` env var.** There's no
  keyring or per-account secret store yet. For manual testing, launch with
  `INBOXLY_SMTP_PASSWORD=<password> cargo run`. M36 will add a real secrets
  backend.
- **OAuth2 SMTP send is not yet wired through the compose bridge.** Gmail
  accounts with `auth_method = "oauth2"` will fail fast with a clear error:
  *"OAuth2 SMTP send not yet wired — M36 will plumb SharedOAuth2 from the
  sync loop."* The `SharedOAuth2` type exists and the `SmtpSender::with_oauth2`
  constructor is wired — only the `Arc<SharedOAuth2>` plumbing from app
  startup down to the send bridge is missing. M36.
- **Sent folder IMAP APPEND is deferred twice.** The send bridge enqueues
  `OfflineAction::AppendSent` on success, but the replay handler logs +
  skips because the `MaildirStore` + active `Session` aren't plumbed
  through `replay_offline_queue`'s signature. Consequence: even on
  successful send, the user's IMAP Sent folder will NOT receive a copy
  of the message until the post-M35b wiring lands. The recipient still
  receives the email; only the user's own Sent folder is missing. M36
  will wire this.
- **Local Maildir Sent folder copy is not written on send.** Related to
  the above — the bridge doesn't write the sent message to the local
  `.Sent/` folder. The SQLite draft row IS deleted on send, so if the
  IMAP Sent APPEND also fails (which it will until M36), the user has no
  local record of the message. For M35b, treat this as a "sent messages
  don't appear in your Sent folder view" limitation.
- **Manual Save Draft button is a no-op.** The `ComposeSaveDraft` handler
  logs a debug line but doesn't trigger an immediate save — the 30-second
  auto-save covers the correctness case. M36 can wire explicit-save via
  a counter pattern if users ask.
- **AppendSent replay handler is a warn-and-skip.** The variant exists in
  `OfflineAction`, the bridge enqueues it, but the replay body is a
  placeholder. M36 will wire the MaildirStore + session integration.

### Test count

- v0.34.1: 884 tests
- v0.35.0: **961 tests** (+77)
  - M35a: 0 new tests (behaviour-preserving refactor)
  - Phase 1: 15 markdown tests
  - Phase 2: 6 drafts integration tests
  - Phase 3: 23 SMTP tests (message builder + retry + redact)
  - Phase 4: 1 has_message_id test
  - Phase 4b: 3 sync dedup tests
  - Phase 5: 4 offline replay tests with MockDraftSender
  - Phase 6: 14 compose state-machine tests
  - Phase 9: 3 FAB wiring tests
  - Phase 10: 3 helper function tests
  - Phase 11: 4 make_draft_filename tests
  - Phase 12: 1 AppendSent serialize round-trip test

## [0.34.1] - 2026-04-07

### Fixed (post-M34 polish)

Three bug fixes caught during the post-merge demo session:

- **`open::that` test side-effect (`#[cfg(not(test))]` gate)** — the M34 Phase 9 `OpenExternalUrl` unit tests dispatched real URLs (`https://example.com`, `mailto:friend@example.com`) through the production handler, which validated the scheme allowlist and then called `open::that()` — actually launching Chrome and kmail every time `cargo test` ran. Approximately 10 background test runs during M34 implementation spawned 10 Chrome windows and 10 kmail compose windows overnight before the bug was caught. Gated `open::that` behind `#[cfg(not(test))]` so the validation logic still runs in tests but the system launch is skipped.
- **`validate_external_url` pure-function refactor** — followed up the `cfg` gate with the cleaner architectural fix: extract the URL parse + scheme allowlist as a pure function `validate_external_url(&str) -> Result<(), String>`. The handler is now a thin wrapper that calls validation then `open::that` on success. The 5 original handler tests are replaced with 6 pure-function tests (`validate_external_url_*`) that exercise the validation logic with zero state setup and zero side effects. The `cfg(not(test))` gate is preserved as defence in depth.
- **`NavigateToSettings` re-entry guard** — pre-existing M29-era bug surfaced during M34 demo testing: clicking the gear icon while already in Settings overwrote `previous_view = Settings`, which trapped the user — `NavigateBack` would no-op back to Settings instead of returning to the inbox. Added `if self.active_view != ActiveView::Settings` guard around the `previous_view` and `drawer_was_open` updates inside `NavigateToSettings`. Settings load + active_view assignment still run unconditionally so the handler stays a valid "reload settings" trigger; only the navigation bookkeeping is gated. Regression test `navigate_to_settings_while_already_in_settings_preserves_previous_view` pins the fix.

884 workspace tests passing (was 879 at v0.34.0 — net +5: removed 5 handler tests, added 6 pure-function tests, added 1 Settings re-entry test, added 1 account-switcher dismissal test was already in v0.34.0). Clippy clean. Build clean.

## [0.34.0] - 2026-04-07

### Added (M34 — Thread Detail View + HTML email rendering)

Restores the second-most-used surface in an email client: clicking a row to read a thread. Ten phases across nineteen commits, all under a `/plan-eng-review` audit (17 issues found, 16 resolved during the review pass).

- **`ThreadDetailView`** Dioxus component — sticky header bar (back button + subject), error banner, scrollable list of `ThreadMessage` children. Reads from a separate `Signal<Option<Arc<LoadedThread>>>` context provided at App level (eng review Issue 1.4) so per-write `Clone` of `Inboxly` doesn't drag thread body bytes around.
- **`ThreadMessage`** child component — single-message renderer: avatar tile (reusing `theme::avatar_colors`), sender name + address, formatted date with `(unknown time)` for `Option<DateTime<Utc>>::None` (Issue 2.7), sanitised HTML body via `dangerous_inner_html` OR plain-text body in `<pre>`, optional attachment list. Takes `Arc<LoadedMessage>` so per-render clones in the parent's `for` loop are refcount bumps, not deep clones of body bytes (Issue 2.8).
- **HTML sanitisation via `ammonia`** (`inboxly-ui::sanitize`) with the project's whitelist on top of ammonia's defaults:
  - Strips `src` and `srcset` from `<img>` tags to block tracking-pixel phone-home on email open (Issue 1.1). Image tags themselves are preserved as broken-image placeholders so layout isn't destroyed; alt text is kept for screen readers.
  - Rewrites every `<a href>` URL to a sentinel-prefixed form `#inboxly-ext:<original>` (Issue 1.2). Because the href now starts with `#`, WebKitGTK treats clicks as same-page anchors instead of navigations, preventing the entire app from being replaced by the linked page.
  - In-page anchors (`href="#section"`) pass through unchanged.
- **Document-level link-click interceptor** in the App component — JS bridge installed via `document::eval` catches clicks on sentinel-prefixed `<a>` elements and forwards the real URL to Rust via `dioxus.send`. The Rust handler dispatches `Message::OpenExternalUrl(url)` which validates the scheme via `url::Url::parse` against an allowlist (`http`/`https`/`mailto` only — Issue 2.2 defence in depth) and hands the URL to `open::that()` for the system browser. Sentinel constant interpolated via `format!` so there's a single source of truth (Issue 2.4).
- **Document-level Escape key handler** (Issue 2.3) — JS bridge installed via `document::eval` catches keydown events outside text inputs (`INPUT`, `TEXTAREA`, `isContentEditable`) and dispatches `Message::CloseThread`. Naive `tabindex: 0` + `onkeydown` on `.app-shell` rejected because it would steal focus from text inputs and pollute the tab order. Both JS bridges have `use_drop` cleanup hooks that remove their listeners on App unmount via the `window.__inboxly_*` global property pattern (Issue 4.2).
- **`ThreadReader` facade in `inboxly-store`** (Issue 1.5) — wraps `Arc<Store>` (SQLite metadata) and `Arc<MaildirStore>` (filesystem bodies) so consumers hold one handle instead of two. Single `load_thread(thread_id) -> Result<Vec<LoadedEmail>, StoreError>` method that hydrates each row's body via `read_email_slim` with `.ok()`-but-logged for non-fatal failures. Hold via `Arc<ThreadReader>` for cheap sharing across components. Future M36 (reply) and M37 (attachments) consumers will inherit this single handle.
- **`SlimEmailContent` view type in `inboxly-core`** (Issue 2.6) — body text/HTML and attachment metadata only, NO headers HashMap, NO attachment byte content. Used by the thread detail view loader to avoid carrying 5–20 KB of headers and potentially MB of attachment bytes through the loader just to drop them at the rendering step. `MaildirStore::read_email_slim` and `parse_email_slim` are the slim accessors mirroring `read_email_content` and `parse_email_content`.
- **Two-signal split for thread state** (Issue 1.4) — `Inboxly::open_thread_id: Option<String>` records the user's intent (lightweight, goes through the message-handler state machine). The actual loaded body data lives in a SEPARATE `Signal<Option<Arc<LoadedThread>>>` context provided at App level. ThreadDetailView reads from the body signal directly, NOT from `Inboxly`. A `use_effect` in App watches `open_thread_id` (via `use_memo`) and bridges intent to body data via a Dioxus `spawn`'d task.
- **Cooperative async loader bridge** (Issue 4.1) — the App-level `use_effect` synchronously sets a `loading_thread()` sentinel, then spawns a Dioxus task that does the actual SQL query + per-row file reads. The click handler returns immediately and the UI shows `(loading…)` until the task completes. The spawn'd task body is still synchronous syscalls on the local single-threaded executor — true async I/O is deferred. A stale-result guard captures the requested id at spawn time and re-checks `open_thread_id` before writing the result, so rapid navigation doesn't let an in-flight load clobber the current view.
- **Five new `LoadedThread` constructors** in `inboxly-ui::loaded_thread`:
  - `empty_thread(thread_id)` — release-build placeholder with `(no content available)` subject and empty messages. Always compiled in.
  - `error_thread(thread_id, message)` — failure path with banner state (Issue 2.1). The ThreadDetailView renders the `error_message` in a red banner above the message list so the user sees what went wrong instead of being silently shown demo/empty content.
  - `loading_thread(thread_id)` — sentinel during async load (Issue 4.1). `(loading…)` subject.
  - `demo_thread(thread_id)` — fixture with two fake messages, gated `#[cfg(debug_assertions)]` per Issue 1.3. Release binaries don't ship the fixture data. The first message includes an HTML body with an `<a href="https://example.com">` link so the link-click interceptor can be exercised by hand.
  - `fallback_thread(thread_id)` — selector that picks `demo_thread` in debug builds and `empty_thread` in release. Single call site for the cfg switch so callers don't have to repeat the gate.
- **`build_loaded_thread` converter** in `inboxly-ui::loaded_thread` — converts `Vec<LoadedEmail>` (raw storage data) into the UI's `LoadedThread` shape. Picks display names (`from_name` OR `from_address` as fallback), converts UNIX timestamps to `Option<DateTime<Utc>>` (returning `None` for out-of-range timestamps per Issue 2.7 instead of falling back to `Utc::now()`), and passes through attachment metadata.
- **`Store::get_emails_by_thread` regression test** (Issue 2.5) — pins the `ORDER BY date ASC` invariant so future schema changes can't silently break chronological message ordering. Inserts three emails with out-of-order dates and asserts the read-back is chronological.
- **6-test `ThreadReader` integration suite** in `inboxly-store/tests/thread_reader.rs` (Issue 3.1) — covers all five production branches against a real on-disk fixture (TempDir for the Maildir, in-memory SQLite for the metadata store): empty thread → Err, body downloaded with valid file → `Some(content)`, body not downloaded → `None`, body downloaded but file missing → `None` (not Err), multi-message chronological ordering, mixed downloaded state per message.
- **State-machine tests** for the new Message variants:
  - `OpenThread` sets `open_thread_id`, dismisses any open menu via `close_menus()`, and clears `account_switcher_open`
  - `CloseThread` clears `open_thread_id`
  - `OpenThread` does NOT load body data into `Inboxly` (Issue 1.4 contract)
  - `OpenExternalUrl` accepts `https`/`http`/`mailto`, rejects `javascript:`, `file://`, garbage input, and any other scheme (Issue 2.2)
  - `SwitchAccount` clears `open_thread_id` (cross-account contamination fix)
- **CSS foundation** for the thread detail view — 25 selectors using existing custom properties so dark/light mode is automatic. Includes `position: sticky` header, `box-shadow` per message, attachment row styling, and a destructive-coloured error banner for the Issue 2.1 failure path.
- **Workspace dependencies added**: `ammonia = "4"` (HTML sanitiser), `open = "5"` (system browser handoff for `OpenExternalUrl`), `url = "2"` (URL parsing for the scheme allowlist).
- **`Inboxly::store` migrated** from `Option<Store>` to `Option<Arc<Store>>` so it can be shared with the `ThreadReader` facade. All ~20 existing call sites compile unchanged via deref coercion.
- 47 new tests across the surface: 9 LoadedThread constructors + 14 sanitiser + 1 chronological + 1 parse_email_slim + 6 ThreadReader integration + 4 build_loaded_thread + 4 OpenThread state-machine + 5 OpenExternalUrl allowlist + 1 account switcher dismissal + 1 SwitchAccount + 1 sentinel-embedded attack pin = **882 total workspace tests passing** (up from 870 at v0.33.1).

### Architecture

M34 is the first Inboxly milestone to introduce HTML sanitisation, a `dangerous_inner_html` escape hatch (correctly fenced behind sanitisation), a data loader bridging two storage layers (`Store` SQLite + `MaildirStore` filesystem), and a webview JS bridge (`document::eval`). The eng review enforced a defence-in-depth security model for HTML rendering and a clean separation between intent state (Inboxly) and body state (App-level signal context).

## [0.33.1] - 2026-04-06

### Fixed (post-M33 polish)

Three small rough edges caught on the first real run of the Dioxus build:

- **Window title** — M32 left the window titled "Dioxus App" (the launcher default). Now set to "Inboxly" via `dioxus::LaunchBuilder::desktop().with_cfg(Config::new().with_window(WindowBuilder::new().with_title("Inboxly")))`.
- **Default WebKitGTK menu bar hidden** — the "Window / Edit / Help" menu that shipped with M32 is now suppressed via `Config::with_menu(None)`.
- **Dark mode background cascade** — the `body { background: var(--bg-color); }` rule was outside the `[data-theme="dark"]` scope (the attribute lives on `.app-shell`, not on `body` or `html`), so the content area rendered with the light-mode `--bg-color` even when dark mode was active. Added `background: var(--bg-color); color: var(--text-primary);` to `.app-shell` so the custom-property lookup resolves inside the dark-mode scope.

## [0.33.0] - 2026-04-06

### Added (M33 — Inbox feed + widgets on Dioxus)

Restores the full inbox feed on the Dioxus 0.7 shell introduced by M32. Ten phases across seventeen commits bring the interactive email-client surface back after the framework migration.

- **InboxFeed** component — date-grouped sections render `EmailRow` and `BundleRow` entries from the existing `feed_sections` model
- **EmailRow** with avatar tile (letter + per-sender colour), sender name, subject + snippet, timestamp, attachment icon, message count badge, overflow button, and right-click context menu
- **BundleRow** with category colour dot, sender previews, unread badge, expand chevron (bundle expand/collapse state lives in `Inboxly::expanded_bundles: HashSet<String>`)
- **SectionHeader** component for date group labels (Today, This Week, Earlier, etc.)
- **Hover actions** on EmailRow: Done (✓), Pin (📌), Snooze (⏰) — CSS `:hover` reveal, each with `aria_label` and `stop_propagation` on click, right-click on actions prevented from opening the row context menu
- **ContextMenu** + **OverflowMenu** — 20-row menus covering Reply/Reply All/Forward, Mark Read/Unread, Move to Inbox/Trash/Spam, Mute, Add to Bundle (eight categories), Create Rule from Sender, Block Sender, Report Spam. Destructive actions styled with `.menu-item.destructive`. Both components share a `menu_actions::render_menu_body()` helper to avoid 229 lines of duplication
- **Menu state foundation**: `OpenOverflowMenu` and `OpenContextMenu` restructured as struct variants carrying `thread_id`, `sender_address`, and `position`; new `overflow_menu_position` and `menu_thread_sender` fields; `Inboxly::close_menus()` helper centralizes the three-field menu-close invariant across eleven action handlers
- **UndoSnackbar** — bottom-centre fixed snackbar with action description and Undo button, auto-expire timer via Dioxus `use_effect` + `tokio::time::sleep(UNDO_TIMEOUT)`. Guarded against stale timers firing against replacement actions via a `generation: u64` counter on `UndoState` — the timer captures the generation at spawn, peeks the current value at fire time, and no-ops if they differ. Reactive scope tightened with `use_memo` so the effect re-runs only on undo state transitions, not on every app mutation
- **SpeedDialFab** — bottom-right Compose button with `aria_label: "Compose new email"`. The Compose action itself is out of M33 scope; onclick is a no-op placeholder for now
- **SnoozePicker** — 2×2 preset grid (Later Today, Tomorrow, This Weekend, Next Week) computing `chrono::DateTime<Utc>` targets from `config.snooze` presets. Nine pure-function unit tests cover the date math, including the same-day edge cases (Later Today falls through to tomorrow morning if the evening hour has passed; This Weekend pushes forward seven days if today is already the weekend day and morning_hour has passed)
- **SnoozeThread handler fix** — previously left the snooze picker open after a preset was chosen; now closes the picker as expected
- **InboxZero** component — celebration view with 🎉 icon shown when the inbox is empty
- **EmptyState** component — generic placeholder used for the Snoozed (⏰) and Done (✅) views with customizable icon and text props
- **Exhaustive `ActiveView` match in `ContentArea`** — explicit arms for Inbox, Snoozed, Done, and Settings prevent silent fallthrough when new views are added
- 242 inboxly-ui tests (up from 225 at the M32 merge); seventeen new tests cover menu state invariants, undo generation counter, snooze date math, and empty-state defaults

### Framework note

M33 builds on top of M32's Iced 0.14 → Dioxus 0.7 desktop conversion. M32 replaced the rendering framework but deleted the Iced views and widgets; M33 restores those widgets as Dioxus components. The state machine, message handlers, feed data model, and theme tokens were preserved throughout M32 and remained the contract M33 builds against.

## [0.30.0] - 2026-03-14

### Added (M30)

- **Bundles settings tab** (M30): Reorderable bundle list with up/down arrows, coloured throttle badge (Immediate/Daily/Weekly parsed from BundleThrottle JSON), visibility toggle (checkbox). All mutations auto-saved to SQLite.
- **Notifications settings tab** (M30): Desktop notifications toggle, sound toggle (disabled when notifications off), "Notify for" section with All/Primary/per-bundle checkboxes. Persisted as JSON to settings store.
- **Keyboard Shortcuts settings tab** (M30): Two-column Action/Shortcut table for all 18 actions, click-to-capture UI ("Press key..."), Reset button for customised bindings. Only non-default overrides stored as JSON.
- **ShortcutMap** replaces compile-time `Shortcuts` struct: Runtime-configurable `HashMap<ShortcutAction, String>` with `defaults()`, `to_overrides_json()`/`from_overrides_json()` for delta persistence, `action_for_key()` reverse lookup
- `ShortcutAction` enum (18 variants) with serde snake_case serialisation, labels, and display ordering
- `key_event_to_shortcut_string()` helper for converting Iced keyboard events to human-readable strings (e.g. "Ctrl+Z", "Shift+R")
- All 6 settings tabs now fully implemented — no more "Coming in M30" placeholders
- 67 new tests (841 total)

## [0.29.0] - 2026-03-14

### Added (M29)

- **Settings view framework** (M29): 240px sidebar with 6 tab buttons (active state: blue text + left border + light blue bg), scrollable content area (640px max-width), replaces main content when `ActiveView::Settings`
- **General tab**: Theme preference (System/Light/Dark chips with auto-save), Default View (Inbox/Snoozed/Done chips), Snooze Presets (4 labeled inputs for morning/afternoon/evening hour + weekend day), Undo Timeout (3s/5s/7s/10s/15s chips)
- **Accounts tab**: Account cards (48px avatar, email, provider/auth info, Edit/Remove buttons), inline add/edit form (8 fields + Cancel/Save), removal confirmation bar, active account deletion prevention (Remove button disabled)
- **Data & Storage tab**: Clear Cache / Rebuild Search Index / Export Data action buttons, storage size display (SQLite, Tantivy, Maildir), last sync timestamp
- `SettingsTab` enum (6 variants), `StoreSettingsAdapter` bridging Store to SettingsReader/Writer
- 24 new Message variants for settings controls with auto-save persistence
- Dark theme support for all settings views
- `format_size()` and `dir_size()` utility helpers
- 34 new tests (808 total)

## [0.28.0] - 2026-03-14

### Added (M28)

- **Account switcher** (M28): Inline expansion at top of nav drawer — collapsed header shows 44px avatar, display name, email, chevron; expanded list shows all accounts with active highlighted (#e8f0fe + checkmark), "Add account" row navigates to Settings
- `ToggleAccountSwitcher` and `SwitchAccount(usize)` messages with proper bounds checking
- Replaced mock `account_email`/`account_count` with `Vec<AccountConfig>` + `active_account_index`
- `active_email()`, `active_display_name()`, `active_account()` convenience methods
- Click-away dismiss via `mouse_area` wrapper on content area
- Accounts loaded from `AppConfig` on startup via `OnceLock` pattern
- `ACCOUNT_SWITCHER_AVATAR` (44dp) and `ACCOUNT_ROW_HEIGHT` (56dp) dimension constants
- 16 new tests (790 total)

## [0.27.0] - 2026-03-14

### Added (M27)

- **Toolbar gear icon** (M27): Navigates to `ActiveView::Settings` with neutral grey `#455a64` toolbar, back-arrow replaces hamburger in Settings mode
- **Overflow menu** (M27): Three-dot (⋮) button appended to hover action row; opens `PopupMenu` (BelowRight) with 4 groups — thread actions (Move to.../Mark read/Mute), reply actions, organisation (Add to bundle/Create rule), safety (Block sender/Report spam)
- **Right-click context menu** (M27): Custom `RightClickArea` widget intercepts right-click events; opens `PopupMenu` (AtCursor) with Done/Pin/Snooze quick actions + full overflow menu items
- **16 new Message variants** for thread actions: MoveTo, MarkReadState, MuteThread, Reply, ReplyAll, Forward, AddToBundle, CreateRuleFromSender, BlockSender, ReportSpam, plus menu open/close and settings navigation
- `MoveDestination` enum (Inbox, Trash, Spam) for typed folder moves
- `toolbar_settings` colour on ThemeColors, `sender_address` on FeedItem
- Themed toolbar colours (`toolbar_color_themed`) for dark mode support
- 18 new tests (774 total)

## [0.26.0] - 2026-03-14

### Added (M26)

- **PopupMenu widget** (M26): Reusable dropdown/context menu overlay using Iced 0.14's `advanced` Widget + Overlay traits — first custom advanced widget in the project
- `MenuItem<Message>` enum with Action (label/icon/message/style), Separator, and Submenu variants; ergonomic constructors (`action`, `destructive`, `action_with_icon`, etc.)
- `MenuItemStyle` (Normal, Destructive) and `PopupAnchor` (BelowRight, BelowLeft, AtCursor) types
- `PopupMenu` wraps any trigger element, delegates layout/draw/events, conditionally renders `MenuOverlay` as overlay
- `MenuOverlay` handles viewport-clamped positioning, shadow + card + item rendering, hover highlighting, click/Escape dismiss
- 5 new colour tokens on `ThemeColors`: `menu_hover`, `menu_destructive_hover`, `menu_destructive_text`, `menu_separator`, `menu_shadow` (both light and dark)
- 9 popup menu dimension constants in `dimensions.rs`
- 41 new tests (756 total)

## [0.25.0] - 2026-03-14

### Added (M21-M25)

- **Snooze picker** (M21): `compute_presets()` returns 5 time presets (Later Today, Tomorrow, This Weekend, Next Week, Someday) with computed UTC times; `SnoozeThread` message with store integration
- **Reminder row widget** (M22): clipboard icon, title, due date (red if overdue), done checkmark button
- **Speed Dial FAB** (M22): 56dp main FAB with expand/collapse, two 40dp mini-FABs (Compose + Reminder)
- **Compose view** (M23): `ComposeState` with To/Cc/Subject/Body, `ComposeMode` (New/Reply/ReplyAll/Forward), `ComposeMessage` events, max-width 920dp layout with Send/Discard
- **Search view** (M24): `SearchResult` type, `search_view()` with empty/no-results states, `ParsedQuery` with from/to/subject/has:attachment/is:unread operators, `parse_query()` parser
- **Inbox Zero Sun** (M25): celebratory view with sun emoji, "You're all done!" heading
- **Keyboard shortcuts** (M25): standard keybindings matching Google Inbox (e=Done, ==Pin, c=Compose, /=Search, j/k=navigate, b=Snooze, r=Refresh)
- 18 new tests across M21-M25 (715 total)

## [0.20.0] - 2026-03-14

### Added

- **Swipe state**: `SwipeState` per-row tracking with drag offset, direction detection (Right=Done, Left=Snooze), 50% arm threshold
- **Swipe state collection**: `SwipeStates` HashMap-keyed collection with get_mut/reset/clear for managing all visible rows
- **Hover action buttons**: `hover_action_buttons()` rendering Done/Pin/Snooze circular 32dp buttons with Unicode icons for desktop interaction
- **Action button**: Reusable `action_button()` helper with icon, accent colour, circular styling
- Full custom Widget-level swipe rendering deferred to M25 polish pass
- 9 new tests (697 total): swipe defaults (1), direction detection (2), threshold arming (2), reset (1), collection ops (2), hover construction (1)

## [0.19.0] - 2026-03-14

### Added

- **Mark Done**: `MarkDone(thread_id)` message marks a thread as done in SQLite, pushes undo action, reloads feed
- **Toggle Pin**: `TogglePin(thread_id)` reads current pin state, toggles it, pushes undo with previous state
- **Sweep**: `Sweep` message marks all unpinned non-done threads as done in a single batch, pushes bulk undo
- **Undo system**: `UndoAction` enum (MarkDone, TogglePin, Sweep) with human-readable descriptions; `UndoState` with 7-second timed window, push/take/clear, expiry tracking
- **Undo handler**: Reverses the pending action -- unmarks done, restores pin state, or unmarks all swept threads
- **Undo snackbar widget**: Bottom-of-content notification showing action description and "Undo" button styled with accent colour
- **UndoExpired handler**: Clears undo state when timer expires (action committed)
- 10 new tests (688 total): undo state lifecycle (5), action descriptions (4), push/take/clear/expiry (1)

## [0.18.0] - 2026-03-14

### Added

- **Bundle row widget**: Collapsed summary row with category icon (40dp tinted circle, Unicode symbol), category name in category colour, unread count badge (pastel pill), up to 3 sender previews (bold if unread), and newest timestamp
- **Bundle icon widget**: `category_icon_circle()` rendering 40dp circles with category-coloured background and Unicode symbols for all 10 categories
- **Mixed feed rendering**: `FeedEntry` enum (Thread | Bundle) dispatching to email rows or bundle rows, sorted by date within sections
- **Bundle summary query**: `query_bundle_summaries()` aggregating active threads by bundle_id with sender preview subquery (top 3 distinct senders)
- **Bundle thread query**: `query_bundle_threads()` for expanded bundle view, reusing inbox thread summary format
- **Unbundled thread filtering**: `query_inbox_threads()` now filters `bundle_id IS NULL` to avoid showing bundled threads individually
- **String-based category colour lookup**: `for_category_str()` maps lowercase category keys to BigTop colour pairs
- **ToggleBundle interaction**: `InboxViewMessage::ToggleBundle(bundle_id)` wired through app message dispatch (expand/collapse state deferred to polish)
- 4 new tests (678 total): bundle summary aggregation (2), thread exclusion from bundle (1), ordering (1)

## [0.17.0] - 2026-03-14

### Added

- **Inbox feed**: Scrollable feed in the main content area with date-grouped section headers (Pinned/Today/Yesterday/This Week/This Month/Earlier)
- **Email row widget**: Avatar circle (40dp, BigTop A-Z palette), sender name (bold if unread, 16sp), subject + snippet (14sp), timestamp (12sp), attachment indicator
- **Date grouping**: `DateGroup` enum with `from_date()` classification relative to local time
- **Relative timestamps**: `format_timestamp()` -- time for today, "Yesterday", weekday name, "Mar 12", "Mar 12, 2025"
- **Feed data model**: `FeedItem` and `FeedSection` view-model types, `build_feed()` query + grouping function
- **Store inbox query**: `query_inbox_threads()` on `Store` joining threads, thread_state, emails, and contacts with `InboxThreadSummary` return type
- **Avatar widget**: Container-based 40dp circle with letter and palette colour from theme module
- **Section header widget**: 48dp tall, 14sp bold grey text, left-aligned
- **Empty inbox placeholder**: Centered "You're all done!" message (full Inbox Zero sun deferred to M25)
- **Inbox view**: Scrollable column composing section headers + email rows, theme-aware colours
- **App integration**: `with_store()` constructor, `ReloadFeed` message, feed sections in app state, inbox view replaces M15 content placeholder
- 19 new tests (674 total): store query (5), date grouping (6), timestamp formatting (4), feed building (1), app integration (3)

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
