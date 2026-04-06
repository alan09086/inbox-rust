# Inboxly v2: Full Email Client — Design Spec

**Date**: 2026-04-06
**Status**: Draft
**Scope**: M31–M40 — Transform Inboxly from a read-only email viewer into a fully functional daily-driver email client with Gmail and Outlook support.

## Context

Inboxly v0.30.0 is a Rust + Iced email client recreating Google's Inbox. It has 62K LoC, 841 tests, and a polished UI with themes, keyboard shortcuts, multi-account support, and a settings framework across 8 crates.

**What works today:**
- IMAP sync pipeline (OAuth2 for Gmail/Outlook, headers, bodies, incremental, IDLE push)
- Threading algorithm + SQLite persistence (13 tables)
- Tantivy full-text search (backend only)
- Bundler engine (50+ heuristics, rules, sender learning, throttling — disconnected from Store)
- Full UI scaffold (sidebar, toolbar, email list, settings, custom widgets)

**What doesn't work:**
- UI actions (archive, pin, read) mark the DB but never call IMAP
- No thread detail view — can't read full email bodies
- No SMTP — can't send, reply, or forward
- Bundler traits not implemented on Store — engine is orphaned
- Snooze crate is empty (1-line lib.rs)
- Extract/highlights crate is empty (1-line lib.rs)
- Search UI not wired to Tantivy backend
- No attachment download/upload/preview

## Design Decisions

### Framework: Iced + Embedded Webview (Hybrid)

Iced is excellent for the application shell (sidebar, toolbar, list views, compose form, settings) but has no HTML rendering capability. HTML emails are mini web pages requiring a real rendering engine.

**Decision**: Use `wry` (Tauri's webview engine, backed by WebKitGTK on Linux) as an embedded webview widget for the email body panel only. The webview is sandboxed: no JavaScript execution, no external network access. Link clicks open the system browser.

**Alternatives considered:**
- Full framework rewrite (Tauri/GTK4) — rejected, 62K LoC investment in Iced
- HTML-to-Iced rich text conversion — rejected, too many emails would render incorrectly
- Render HTML to image — rejected, loses interactivity (link clicking)

### Provider Support: Gmail + Outlook from Day One

Both providers use OAuth2 XOAUTH2 SASL, which the IMAP layer already implements. SMTP will reuse the same token infrastructure.

- **Gmail**: `smtp.gmail.com:587` with STARTTLS, XOAUTH2
- **Outlook**: `smtp.office365.com:587` with STARTTLS, XOAUTH2
- **Other IMAP**: password auth for generic providers

OAuth2 client registration:
- Google: Desktop app flow with localhost redirect
- Microsoft: Device code flow

### Milestone Strategy: Vertical Slices

Each milestone delivers one end-to-end user story rather than building layers bottom-up. Every milestone produces something the user can actually do that they couldn't before.

## Architecture

### Crate Changes

**Modified crates:**
- `inboxly-core` — new SMTP config types, highlight model expansion
- `inboxly-imap` — IMAP action executor (STORE, COPY, DELETE commands)
- `inboxly-store` — implement `RuleStore`, `AffinityStore`, `BundleStore` traits; snooze table; highlight writes
- `inboxly-bundler` — no changes (engine is complete, just needs real Store)
- `inboxly-snooze` — full implementation (scheduler, persistence, reappear logic)
- `inboxly-extract` — full implementation (pattern extractors, highlight cards)
- `inboxly-ui` — thread detail view, compose wiring, search wiring, webview widget, first-run wizard

**New crate (potential):**
- `inboxly-smtp` — SMTP client, RFC 5322 message builder, MIME encoding. Alternatively, this could live as a module within `inboxly-imap` to reuse OAuth2/TLS infrastructure. Decision deferred to implementation planning.

**New dependencies:**
- `wry` — webview embedding for email body rendering
- `rfd` — native file dialogs for attachment save/open
- `notify-rust` — desktop notifications
- `chrono-tz` — timezone-aware snooze scheduling
- `lettre` — SMTP client (mature, async-capable, handles STARTTLS/OAuth2; prefer over custom implementation)

### Data Flow: Action Execution

```
UI Action (e.g., Archive)
  → Message::MarkDone(thread_id)
  → app.update() writes OfflineAction to SQLite queue
  → ActionExecutor picks up queued action
  → Maps to IMAP command (STORE +FLAGS \Deleted, EXPUNGE or MOVE)
  → On success: remove from queue
  → On failure: retry with backoff, surface error in UI
  → On offline: queue persists, replayed on reconnect
```

### Data Flow: Compose + Send

```
Compose View
  → User fills To/Cc/Subject/Body, attaches files
  → Draft auto-save (periodic → SQLite + IMAP Drafts folder via APPEND)
  → Message::Send
  → RFC 5322 builder constructs MIME message
  → SMTP client authenticates (OAuth2 XOAUTH2)
  → SMTP MAIL FROM / RCPT TO / DATA
  → On success: IMAP APPEND to Sent folder, remove draft
  → On failure: keep draft, surface error with retry option
```

### Data Flow: Snooze

```
Snooze Picker → (Later Today / Tomorrow / Next Week / Custom)
  → Write snooze record to SQLite (thread_id, reappear_at_utc, original_bundle)
  → Remove thread from inbox view
  → Thread appears in Snoozed view with countdown
  → Tokio timer fires at reappear_at
  → Move thread back to inbox (top, marked unread)
  → Restore original bundle assignment
  → Fire desktop notification
  → On app restart: reload active snoozes, re-arm timers
```

### Data Flow: Highlights

```
Phase 2 Body Arrival (or new email via incremental sync)
  → Extraction pipeline scans HTML + plain text body
  → Pattern matchers run in order: flights → packages → events → bills
  → Matched highlights written to SQLite highlights table (type, fields as JSON)
  → Trip grouper checks date proximity + destination for related highlights
  → Thread detail view renders highlight card above email body in webview
  → Highlight summary view aggregates across all accounts
```

## Milestones

### M31: Store Trait Integration + IMAP Action Execution

**User story**: When I archive/pin/mark-read an email in the UI, it actually syncs to the server.

**Deliverables:**
1. Implement `RuleStore`, `AffinityStore`, `BundleStore` traits on the `Store` struct. The SQL methods already exist in `bundle_rules.rs`, `sender_affinity.rs`, `bundles.rs` — they need to satisfy the bundler's trait interfaces.
2. `ActionExecutor` struct that maps `OfflineAction` variants to IMAP commands:
   - `MarkRead` → `STORE +FLAGS \Seen`
   - `MarkUnread` → `STORE -FLAGS \Seen`
   - `MarkDone` (archive) → Gmail: `MOVE` to `[Gmail]/All Mail`; Outlook: `MOVE` to `Archive`; generic IMAP: `STORE +FLAGS \Deleted` + `EXPUNGE`
   - `Pin` → `STORE +FLAGS \Flagged`
   - `Unpin` → `STORE -FLAGS \Flagged`
   - `MoveToFolder` → `COPY` + `STORE +FLAGS \Deleted` + `EXPUNGE` (or `MOVE` if supported)
   - `Trash` → move to Trash folder
   - `MarkSpam` → move to Spam folder
3. Replay loop: on sync, drain offline queue oldest-first. Retry with exponential backoff on transient failure. Skip and log permanently failed actions.
4. Wire UI `Message` handlers to call `ActionExecutor` instead of just logging.

**Tests**: Store trait integration tests (real SQLite), action executor unit tests with mock IMAP.

---

### M32: Thread Detail View + Webview Integration

**User story**: I can click an email and read the full content with proper HTML rendering.

**Deliverables:**
1. `wry` webview widget embedded in Iced — renders HTML email body in a sandboxed context (no JS execution, no external network). Content loaded from Maildir-stored body via data URI or local file protocol.
2. Thread detail view layout: conversation thread with individual messages stacked vertically. Each message shows sender avatar/name, date, and the rendered body. Quoted replies are collapsed by default with a "show trimmed content" expander.
3. Attachment list below each message: filename, size, MIME type, download icon. (Download action wired in M35.)
4. Link handling: intercept navigation events in the webview, open URLs in the system browser via `open::that()`.
5. Navigation: click email in list → detail view slides in (or replaces list view). Back button / Escape returns to list. Keyboard: Enter to open, Escape to close.

**Tests**: Webview widget rendering tests (headless where possible), HTML sanitisation tests, thread layout snapshot tests.

---

### M33: SMTP Engine + Compose

**User story**: I can compose a new email and send it via Gmail or Outlook.

**Deliverables:**
1. SMTP client — TLS connection to provider SMTP servers. OAuth2 XOAUTH2 authentication reusing existing token infrastructure from `inboxly-imap`. Support STARTTLS on port 587.
2. RFC 5322 message builder — construct well-formed messages with headers: From, To, Cc, Subject, Date, Message-ID, MIME-Version, Content-Type. Plain text body with `text/plain` content type (HTML compose deferred).
3. Wire `compose_view.rs` — the compose form (To/Cc/Subject/Body) already exists. Connect `Message::Send` to the SMTP engine. Show sending progress and success/failure feedback.
4. Sent folder sync — after successful send, IMAP APPEND the message to the Sent folder so it appears in the user's sent mail.
5. Draft auto-save — periodic timer (30s) saves draft to SQLite. On explicit save or app close, also APPEND to IMAP Drafts folder. Resume drafts from the drafts list.

**Tests**: RFC 5322 message construction tests, SMTP command sequence tests (mock server), draft persistence tests.

---

### M34: Reply + Reply All + Forward

**User story**: I can reply to or forward any email from the thread detail view.

**Deliverables:**
1. Reply composer — pre-fills To (original sender), Subject (`Re:` prefix, RFC 5322 compliant — don't double-prefix). Sets `In-Reply-To` and `References` headers for threading continuity. Quoted body with attribution line (`On <date>, <sender> wrote:`).
2. Reply All — pre-fills To (original sender) + Cc (all other recipients), excluding the user's own address.
3. Forward — pre-fills Subject (`Fwd:` prefix). Includes original body as quoted content. Original attachments forwarded as MIME parts.
4. Inline reply UI — compose panel appears below the thread detail view, keeping conversation context visible. Toggle between inline and full-screen compose.
5. All send paths use M33's SMTP engine.

**Tests**: Header pre-fill logic tests (reply, reply-all, forward), Re:/Fwd: prefix normalisation tests, References header chain tests.

---

### M35: Full Attachment Support

**User story**: I can download, preview, and send attachments.

**Deliverables:**
1. Download — click attachment in thread detail view, native save dialog via `rfd`. MIME decode from Maildir body (base64, quoted-printable). Save to user-chosen location.
2. Inline preview — images render inside the webview (natural with HTML rendering). PDF/document attachments show a preview card with "Open" button that launches system viewer. Other types show icon + filename.
3. Send attachments — file picker in compose view (via `rfd`). MIME multipart/mixed encoding: base64 for binary, 7bit for text. Proper Content-Disposition headers.
4. Size guardrails — warn when total attachment size exceeds 25 MB. Show individual file sizes in compose view.
5. Drag and drop — handle Iced file drop events on the compose view to attach files.

**Tests**: MIME encoding/decoding round-trip tests, multipart message construction tests, size validation tests.

---

### M36: Advanced Search

**User story**: I can search my email with Gmail-style operators.

**Deliverables:**
1. Query parser — tokenise search input and parse operators into Tantivy field queries:
   - `from:<text>` — match sender name or address
   - `to:<text>` — match recipient
   - `subject:<text>` — match subject line
   - `has:attachment` — filter to emails with attachments
   - `has:star` / `is:pinned` — flag filters
   - `is:unread` / `is:read` — read state filters
   - `in:<bundle>` — bundle membership filter
   - `before:<date>` / `after:<date>` — date range (YYYY-MM-DD format)
   - `folder:<name>` / `label:<name>` — folder filter
   - Bare text — full-text search across subject + body (existing BM25 + recency boost)
   - Operators combinable: `from:alice has:attachment after:2026-01-01`
2. Wire search UI to backend — connect the existing search bar → parser → Tantivy query → results list.
3. Search results view — display results with highlighted matching snippets, grouped by thread. Click result opens thread detail view (M32).
4. Search history — persist recent queries in SQLite, show as autocomplete suggestions when search bar is focused.

**Tests**: Query parser unit tests (valid operators, edge cases, malformed input), search integration tests against indexed test data.

---

### M37: End-to-End Bundling

**User story**: My inbox automatically groups emails into bundles and I can manage rules.

**Deliverables:**
1. Activate bundler on sync — after Phase 1 header sync and Phase 2 body arrival, run `BundlerEngine::categorise()` against new emails using the real Store (trait impls from M31). Recategorise on body arrival for body-based rules.
2. Bundle inbox view — wire existing `BundleRow` UI to query real bundle assignments from Store. Show email count badges, expand/collapse groups.
3. Throttle delivery — Immediate/Daily/Weekly bundles use the existing `ThrottleScheduler`. Surface "N new" badges. Expand when throttle window opens.
4. Move-to-bundle — user moves email to different bundle → fire sender learning via `AffinityStore` so the bundler learns the preference.
5. Rule creation UI — dialog triggered from overflow menu "Create rule from sender": pick field (From/Subject/Body), pick operator (contains/equals/startswith/regex), pick target bundle. Persists via `RuleStore`.
6. Block sender — wire existing button to create a rule routing all email from that sender to Trash bundle.

**Tests**: End-to-end bundling integration tests (sync → categorise → UI query), rule creation round-trip tests, sender learning tests with real Store.

---

### M38: Snooze System

**User story**: I can snooze an email and it reappears in my inbox at the chosen time.

**Deliverables:**
1. Snooze scheduler in `inboxly-snooze` — tokio timer watching snoozed threads. Fires `SnoozeEvent::Reappear` at the scheduled time. Persists to SQLite: `snoozes` table with `thread_id`, `reappear_at_utc`, `original_bundle`, `created_at`.
2. Snooze picker wiring — existing `SnoozePicker` widget has presets (Later Today, Tomorrow, Next Week, custom datetime). Wire `Message::SnoozeThread` to write snooze record, remove thread from inbox view.
3. Reappear logic — on timer fire: move thread back to inbox (top of list), mark unread, restore original bundle, fire desktop notification via `notify-rust`.
4. Snoozed view — existing nav item shows all snoozed threads sorted by reappear time. Show countdown. Allow un-snooze (cancel and return to inbox immediately).
5. Time zone — store as UTC, display in local time using `chrono-tz` with system timezone detection.
6. Restart persistence — on app launch, load active snoozes from SQLite, re-arm tokio timers for any that haven't fired. Handle past-due snoozes (reappear immediately).

**Tests**: Scheduler unit tests (timer fire, past-due handling), persistence round-trip tests, reappear logic tests.

---

### M39: Smart Highlights & Extraction

**User story**: The app detects flights, packages, events, and bills and surfaces them as rich cards.

**Deliverables:**
1. Extraction engine in `inboxly-extract` — pattern-based extractors scanning HTML + plain text bodies:
   - **Flights**: airline confirmation numbers, departure/arrival airports and times, gate/terminal info. Regex patterns for major airline email templates.
   - **Packages**: tracking number patterns (UPS 1Z*, FedEx 12/15/20/22-digit, USPS 20/22-digit, Canada Post 16-digit, DHL 10-digit), carrier detection, estimated delivery date extraction.
   - **Events**: iCalendar (.ics) attachment parsing via `ical` crate, date/time/location extraction from invitation emails.
   - **Bills/Finance**: amount due (currency + number patterns), due date, payee name from common billing templates.
2. Highlight storage — populate existing `highlights` table: `type` (flight/package/event/bill), `email_id`, `thread_id`, `data` (structured JSON with type-specific fields), `detected_at`.
3. Highlight cards — render rich cards in the webview above the email body: flight itinerary card, package tracking card with carrier logo, event summary card, bill due-date card.
4. Trip bundles — group related highlights (flight + hotel + car rental for same date range / destination) into trip bundles using date proximity. Populate existing `TripBundle` types in core.
5. Highlight summary view — nav drawer entry showing upcoming flights, pending deliveries, due bills across all accounts. Sorted by date.

**Tests**: Extractor unit tests per type (real email samples), highlight storage round-trip tests, trip grouping logic tests.

---

### M40: Integration Polish & First Run

**User story**: A new user can set up their account and start using Inboxly immediately.

**Deliverables:**
1. First-run wizard — on launch with no accounts: choose provider (Gmail / Outlook / Other IMAP) → OAuth2 consent flow (opens system browser, localhost redirect for Google, device code for Microsoft) → initial sync with progress bar and email count → brief bundle introduction screen.
2. OAuth2 client registration — bundle client IDs for Google (desktop app flow) and Microsoft (device code flow). Store tokens securely in the existing auth infrastructure.
3. Desktop notifications via `notify-rust` — new email arrival, snooze reappearance, throttled bundle window opening. Respect per-bundle notification settings from M30's notification tab.
4. Undo system completion — the snackbar undo UI exists. Wire reversible actions: undo archive (un-mark done + IMAP unflag), undo pin, undo snooze (cancel timer), undo move-to-bundle. Replay inverse IMAP action within the undo timeout window.
5. Error handling & offline mode — graceful degradation when network is unavailable. Sync status indicator in toolbar (green dot / yellow spinner / red X). All actions queue locally, replay on reconnect via M31's ActionExecutor.
6. Performance — lazy-load thread detail view (don't render webview until opened), virtualised email list for large inboxes (10K+ threads), throttle background indexing to avoid UI jank.

**Tests**: First-run flow integration test, undo round-trip tests, offline queue replay tests, notification permission tests.

## Dependency Graph

```
M31 (Store + Actions) ──────┬──────────────────────────────────────┐
                             │                                      │
M32 (Thread View + Webview) ─┤                                      │
                             │                                      │
M33 (SMTP + Compose) ───────┤  depends on M31 (actions), M32 (view) │
                             │                                      │
M34 (Reply/Forward) ─────── M33                                     │
                                                                    │
M35 (Attachments) ────────── M33 + M32                              │
                                                                    │
M36 (Search) ────────────── M32 (results open thread view)          │
                                                                    │
M37 (Bundling) ──────────── M31 (Store traits)                      │
                                                                    │
M38 (Snooze) ────────────── M31 (actions) + M37 (bundle restore)   │
                                                                    │
M39 (Highlights) ─────────── M32 (webview cards) + M37 (trip bundles)
                                                                    │
M40 (Polish) ────────────── All above                               │
```

## Success Criteria

The project is complete when a user can:
1. Launch Inboxly for the first time and connect a Gmail or Outlook account via OAuth2
2. See their inbox with emails grouped into bundles (Social, Promos, Updates, etc.)
3. Click an email and read the full HTML body with proper rendering
4. Compose a new email with attachments and send it
5. Reply to, reply-all, and forward emails with quoted context
6. Archive, pin, mark read/unread, and have those actions sync to the server
7. Snooze an email and have it reappear at the scheduled time
8. Search with operators like `from:alice has:attachment before:2026-01-01`
9. See highlight cards for flights, packages, events, and bills
10. Work offline with actions queued and replayed on reconnect

## Non-Goals (Explicitly Excluded)

- **Calendar integration** — highlight cards link to system calendar but Inboxly doesn't manage calendar events
- **Contact management** — contacts are extracted from email headers, not editable
- **Email encryption** — no PGP/S/MIME support in this phase
- **Mobile/web clients** — desktop only (Linux first, macOS/Windows via cross-compilation later)
- **Multiple simultaneous compose windows** — one compose at a time
- **Rich text / HTML compose** — compose in plain text, render received HTML. HTML compose is a future enhancement
- **Custom SMTP server configuration** — Gmail/Outlook/generic IMAP only for now
