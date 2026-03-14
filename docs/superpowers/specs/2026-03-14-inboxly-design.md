# Inboxly — Design Specification

A desktop email client that recreates Google Inbox's interaction paradigm, built in Rust with Iced. Universal IMAP support, offline-first, with hybrid rule-based + learning email categorisation.

## Table of Contents

1. [Goals & Principles](#goals--principles)
2. [Architecture](#architecture)
3. [Data Model](#data-model)
4. [IMAP Sync Engine](#imap-sync-engine)
5. [Bundler / Categorisation Engine](#bundler--categorisation-engine)
6. [Snooze & Reminder System](#snooze--reminder-system)
7. [Search & Highlights](#search--highlights)
8. [UI Architecture](#ui-architecture)
9. [Theme System](#theme-system)
10. [v1 Feature Scope](#v1-feature-scope)
11. [Deferred & Extension Points](#deferred--extension-points)
12. [Key Dependencies](#key-dependencies)

---

## Goals & Principles

1. **Offline-first** — Full IMAP sync to local Maildir. The app works without connectivity; actions taken offline are queued and replayed on reconnect.
2. **Server wins** — Remote IMAP state is authoritative for conflicts. Local-only state (pins, bundles, snooze) lives in SQLite.
3. **Your data, your disk** — Emails stored as plain Maildir files. Survives database loss. Rebuildable indexes.
4. **Faithful but modernised** — Google Inbox's BigTop design tokens as the baseline, with tasteful modern updates.
5. **Engine/UI separation** — Clean crate APIs with no Iced types in core crates. Enables future TUI, mobile, and web frontends.
6. **Universal IMAP** — Works with any IMAP provider (Gmail, Fastmail, self-hosted, etc.), not locked to Google's ecosystem.

## Architecture

### Workspace Structure

```
inboxly/
├── Cargo.toml                  ← workspace root, shared version
├── inboxly-core/               ← shared types, config, error types
├── inboxly-imap/               ← IMAP sync engine + OAuth2
├── inboxly-store/              ← Maildir + SQLite + tantivy
├── inboxly-bundler/            ← categorisation engine (rules + learning)
├── inboxly-snooze/             ← snooze scheduler + reminders
├── inboxly-extract/            ← highlights, trip detection, smart extraction
├── inboxly-ui/                 ← Iced app, views, widgets, theme, animations
└── inboxly/                    ← thin binary, wires crates together
```

### Dependency Flow (A → B means "A depends on B")

```
                    ┌──────┐
                    │ core │
                    └──┬───┘
          ┌───────────┼───────────┐
          ▼           ▼           ▼
      ┌──────┐   ┌───────┐   ┌─────────┐
      │ imap │   │ store │   │ extract │
      └──┬───┘   └───┬───┘   └────┬────┘
         │           │            │
         ▼           ▼            ▼
      ┌─────────────────────────────┐
      │          bundler            │
      │  (depends on: core, store)  │
      └─────────────┬───────────────┘
                    ▼
              ┌──────────┐
              │  snooze  │
              │ (core,   │
              │  store)  │
              └────┬─────┘
                   ▼
              ┌─────────┐
              │   ui    │
              │ (all)   │
              └────┬────┘
                   ▼
              ┌─────────┐
              │ binary  │
              └─────────┘
```

- `inboxly-core`: no internal dependencies (foundation types)
- `inboxly-imap`: depends on `core` (uses Email/Account types, writes raw data)
- `inboxly-store`: depends on `core` (persists core types to Maildir/SQLite/tantivy)
- `inboxly-extract`: depends on `core` (reads Email, produces Highlights)
- `inboxly-bundler`: depends on `core`, `store` (reads emails from store, writes bundle assignments)
- `inboxly-snooze`: depends on `core`, `store` (reads/writes snooze state)
- `inboxly-ui`: depends on all crates (renders everything)
- `inboxly` (binary): depends on `ui` (bootstrap only)

### Crate Responsibilities

| Crate | Owns |
|-------|------|
| `inboxly-core` | Config, error types, `Email`/`Thread`/`Bundle`/`Account` models, trait definitions |
| `inboxly-imap` | IMAP connection, OAuth2 flow, full + incremental sync, IDLE push, SMTP send |
| `inboxly-store` | Maildir write/read, SQLite schema (metadata, bundle assignments, pin/snooze state), tantivy index |
| `inboxly-bundler` | Rule engine, header-based heuristics, sender learning, custom bundle CRUD |
| `inboxly-snooze` | Snooze timer management, reminder storage, location geofence triggers, scheduler loop |
| `inboxly-extract` | Email body parsing, regex/schema.org extraction for tracking numbers, flights, hotels, events, trip grouping |
| `inboxly-ui` | Iced application, all views, custom widgets (bundle card, swipe row, snooze picker, FAB speed dial), BigTop theme, animations |
| `inboxly` | `main.rs` — CLI arg parsing, runtime setup, launches UI |

### Storage Architecture

Three-layer storage with clear responsibilities:

- **Maildir++** — Canonical email store. Raw `.eml` files on disk. Survives database loss. Uses Maildir++ layout with `new/`, `cur/`, `tmp/` subdirectories per folder (e.g., `.Sent/`, `.Drafts/`, `.Trash/`). IMAP flags map to Maildir filename suffixes: `:2,S` (seen), `:2,F` (flagged/starred), `:2,R` (replied), `:2,D` (draft). Compatible with other Maildir-aware tools (mutt, notmuch, offlineimap).
- **SQLite** — Metadata, state, bundle rules/assignments, reminders, settings, sync state. All structured data that doesn't belong in Maildir.
- **Tantivy** — Full-text search index. Rebuildable from Maildir + SQLite at any time.

## Data Model

### Core Types

```rust
// === Identity ===
AccountId(Uuid)
EmailId(String)          // Message-ID header
ThreadId(Uuid)           // locally generated, groups by References/In-Reply-To (see Threading below)
BundleId(Uuid)

// === Email ===
// EmailMeta is what lives in SQLite and memory (lightweight).
// Body content is loaded lazily from Maildir on demand.
EmailMeta {
    id: EmailId,
    account_id: AccountId,
    thread_id: ThreadId,
    from: Contact,           // name + address
    to: Vec<Contact>,
    cc: Vec<Contact>,
    subject: String,
    snippet: String,         // first ~200 chars, plaintext
    date: DateTime<Utc>,
    maildir_path: PathBuf,   // canonical location on disk
    attachments: Vec<AttachmentMeta>,  // name, mime, size (not content)
    flags: EmailFlags,       // read, starred, answered, draft
    size_bytes: u64,
    imap_uid: u32,           // for sync tracking, scoped to (account_id, folder)
    imap_folder: String,     // IMAP folder this UID belongs to (e.g., "INBOX", "Sent")
}

// Full email content — loaded on demand when user opens a message.
EmailContent {
    id: EmailId,
    body_text: Option<String>,
    body_html: Option<String>,
    headers: HashMap<String, String>,
    attachments: Vec<Attachment>,  // includes content bytes
}

// === Thread (conversation) ===
Thread {
    id: ThreadId,
    account_id: AccountId,
    subject: String,
    participants: Vec<Contact>,
    emails: Vec<EmailId>,    // ordered by date
    newest_date: DateTime<Utc>,
    oldest_date: DateTime<Utc>,
    unread_count: u32,
    has_attachments: bool,
    snippet: String,         // from newest email
}

// === Bundle ===
Bundle {
    id: BundleId,
    category: BundleCategory,
    name: String,
    color: Color,            // title colour from BigTop palette
    badge_color: Color,      // pastel badge background
    icon: BundleIcon,
    threads: Vec<ThreadId>,
    unread_count: u32,
    newest_date: DateTime<Utc>,
    visibility: BundleVisibility,  // Bundled | Unbundled | SkipInbox
    throttle: BundleThrottle,      // Immediate | Daily | Weekly
}

// === Bundle Categories ===
BundleCategory enum {
    Social, Promos, Updates, Finance,
    Purchases, Travel, Forums, LowPriority,
    Saved, Custom(String),
}

// === Inbox Item (the unified feed) ===
InboxItem enum {
    Thread(Thread),
    Bundle(Bundle),
    Reminder { id, title, due, done },
    TripBundle(TripBundle),
}

// === Thread State ===
ThreadState {
    thread_id: ThreadId,
    pinned: bool,
    done: bool,              // archived
    snoozed: Option<SnoozeInfo>,
    bundle_id: Option<BundleId>,
    highlights: Vec<Highlight>,
}

// === Snooze ===
SnoozeInfo {
    until: SnoozeUntil,
    original_date: DateTime<Utc>,
}

SnoozeUntil enum {
    Time(DateTime<Utc>),
    Location { lat: f64, lng: f64, radius_m: f64, label: String },
}

// === Highlights ===
Highlight enum {
    TrackingNumber { carrier, number, url },
    Flight { airline, number, depart, arrive, gate },
    Hotel { name, checkin, checkout, confirmation },
    Event { title, datetime, location },
    Payment { amount, currency, from_or_to },
}
```

### Threading Algorithm

Simplified References-based threading (inspired by JWZ but without the full container/subject-merge complexity):

1. On ingest, extract `Message-ID`, `In-Reply-To`, and `References` headers.
2. If `References` is present, the thread ID is determined by the **first** Message-ID in the References list (the original message that started the thread).
3. If only `In-Reply-To` is present, look up the referenced message and join its thread.
4. If neither header exists, the email starts a new thread with a fresh `ThreadId`.
5. **No subject-based grouping** — this avoids false positives (e.g., multiple "Re: Hello" threads).
6. When a reply arrives before its parent, a placeholder thread is created. When the parent arrives, the placeholder is resolved and the thread is unified. The `emails.thread_id` column is mutable — thread unification issues an `UPDATE` to reassign orphaned emails to the resolved thread.
7. **Manual thread merging is deferred** — not in v1 scope. Listed in Deferred section.

### SQLite Schema

**`emails`** — per-email metadata (most-queried table):
- id (TEXT PK), account_id, thread_id, from_name, from_address, to_json, cc_json
- subject, snippet, date (INTEGER, unix epoch), maildir_path
- flags (INTEGER, bitmask: read/starred/answered/draft/deleted)
- size_bytes, imap_uid, imap_folder, has_attachments (BOOLEAN)
- message_id_header, in_reply_to, references_json
- UNIQUE constraint on (account_id, imap_folder, imap_uid) — UIDs are scoped per folder

**`threads`** — aggregated thread metadata:
- id (TEXT PK), account_id, subject, newest_date, oldest_date
- email_count, unread_count, has_attachments, snippet

**`accounts`** — id, email, display_name, provider, auth_method, imap_host, imap_port, smtp_host, smtp_port

**`sync_state`** — account_id, folder_name, uid_validity, uid_next, highest_modseq, last_sync

**`thread_state`** — thread_id, pinned, done, snoozed_until, snoozed_location_json, bundle_id

**`contacts`** — address (TEXT PK), display_name, avatar_letter, avatar_color_index, last_seen
- Populated from From headers on ingest, avoids re-parsing

**`bundles`** — id, category, name, color, badge_color, visibility, throttle, sort_order

**`bundle_rules`** — id, bundle_id, field, operator, value, priority

**`sender_affinity`** — sender_domain, sender_address, bundle_category, confidence, learned_at

**`reminders`** — id, title, due_at, location_lat, location_lng, location_label, recurring, done

**`highlights`** — thread_id, highlight_type, data_json

**`settings`** — key (TEXT PK), value (TEXT)

**`offline_queue`** — id, action, payload_json, created_at

## IMAP Sync Engine

### Three Sync Modes

1. **Initial sync** — First run per account. Two-phase approach:
   - **Phase 1 (fast)**: Fetch headers + envelope data for all messages via `FETCH (ENVELOPE FLAGS RFC822.SIZE)`, newest-first, in batches of 500. This populates `emails` table, builds threads, runs bundler. The inbox is usable once phase 1 completes for the most recent batch.
   - **Phase 2 (background)**: Download full message bodies (RFC822) to Maildir, also in batches of 500, newest-first. Bodies are written to Maildir and tantivy index is updated. Highlights extraction runs during this phase.
   - Progress reported to UI via channel (e.g., "Syncing headers: 12,000 / 45,000", "Downloading bodies: 3,500 / 45,000").
   - For very large mailboxes (100k+), phase 2 may take hours. The app is functional after phase 1 completes; opening an email whose body hasn't been fetched yet triggers an on-demand fetch.

2. **Incremental sync** — Subsequent launches. Uses IMAP `UIDVALIDITY` + `UIDNEXT` to fetch only new messages since last sync. Flag change detection:
   - If server supports `CONDSTORE`: use `FETCH (FLAGS) (CHANGEDSINCE <highestmodseq>)` to get only changed flags. Efficient even for 100k+ mailboxes.
   - If no `CONDSTORE`: fetch flags for the last 30 days of UIDs only (`UID FETCH <recent_uid>:* (FLAGS)`), not the entire mailbox. Older flag changes are accepted as missed — an acceptable trade-off vs. scanning the full mailbox on every sync.

3. **Push sync** — Maintains IMAP `IDLE` connection for real-time new mail notification. When IDLE breaks (server drops after ~29 min), reconnects and does quick incremental catch-up.

### Authentication

| Provider | Method |
|----------|--------|
| Gmail | OAuth2 (XOAUTH2 SASL) — requires client ID registration |
| Fastmail | App-specific password via IMAP LOGIN |
| Generic IMAP | Username + password, STARTTLS or implicit TLS |
| OAuth2 providers | Authorization code flow with PKCE, token refresh |

### Synced Folders

v1 syncs a fixed set of well-known IMAP folders per account:

| Folder | IMAP Name | Gmail Mapping | Purpose |
|--------|-----------|---------------|---------|
| Inbox | `INBOX` | `INBOX` | Primary mail, bundling target |
| Sent | `Sent` | `[Gmail]/Sent Mail` | Sent mail archive |
| Drafts | `Drafts` | `[Gmail]/Drafts` | Draft messages |
| Trash | `Trash` | `[Gmail]/Trash` | Deleted messages |
| Spam | `Spam` / `Junk` | `[Gmail]/Spam` | Spam/junk |

Folder names are resolved via IMAP `LIST` command with `SPECIAL-USE` attributes (RFC 6154) where supported, falling back to common name matching. Custom/nested user folders are not synced in v1 — only the 5 standard folders above.

### Design Decisions

- Sync runs on a background tokio runtime, completely decoupled from the UI thread
- UI receives updates via `tokio::sync::mpsc` channel — new emails, state changes, sync progress
- Outbound mail sent via SMTP (`lettre`), saved to Maildir + Sent folder via IMAP APPEND
- Conflict resolution: server wins. If a flag changes locally AND remotely, remote state takes precedence
- Per-account sync tasks run independently — one slow/broken account doesn't block others

### Offline Behaviour

- App is fully functional offline using cached Maildir + SQLite
- Actions taken offline (done, pin, snooze) queued in `offline_queue` table and replayed on reconnect
- Compose offline → saved as draft in Maildir, sent on reconnect

## Bundler / Categorisation Engine

Three-layer categorisation with clear precedence:

### Layer 1 — Header Heuristics (instant, zero config)

v1 ships with the patterns listed below. The rule set is iterative — additional patterns are added over time based on user feedback and common sender discovery. The architecture supports loading rules from a TOML config file for easy expansion without code changes.

| Signal | Category |
|--------|----------|
| `List-Id` / `List-Unsubscribe` present | Forums |
| `X-Mailer` contains "campaign" / "mailchimp" | Promos |
| `List-Unsubscribe` + no `List-Id` | Promos |
| From: `*@facebookmail.com`, `*noreply@github*` | Social |
| From: `*@amazon.*`, subject matches receipt patterns | Purchases |
| From: `*@bank*`, `*@paypal*`, `*statement*` | Finance |
| From: `*@booking.com`, `*@airline*` | Travel |
| `Precedence: bulk` | Low Priority |

### Layer 2 — User-Defined Rules (explicit control)

```rust
BundleRule {
    bundle_id: BundleId,
    field: RuleField,     // From, To, Subject, Header(name), Body
    operator: RuleOp,     // Contains, Equals, Matches(regex), Domain
    value: String,
    priority: i32,        // higher priority wins conflicts
}
```

Users create custom bundles and rules through the settings UI. Rules override heuristics.

### Layer 3 — Sender Learning (hybrid magic)

When a user manually moves an email to a different bundle:

```rust
SenderAffinity {
    sender_domain: String,
    sender_address: String,
    bundle_category: BundleCategory,
    confidence: f32,          // increases with repeated actions
    learned_at: DateTime<Utc>,
}
```

Future emails from that sender auto-bundle. Confidence decays over time if overridden.

### Evaluation Order

User rules (highest priority) → Sender learning (if confidence > threshold) → Header heuristics → Uncategorised (stays in primary inbox)

### Bundle Throttling

- **Immediate** — emails appear as they arrive (default for custom bundles)
- **Daily** — bundle collapses and shows once per day at configured time
- **Weekly** — same, once per week

Throttled emails are synced and stored but don't surface in the inbox feed until their delivery window.

### Known Limitation: Body-Based Rules and Initial Sync

During initial sync Phase 1, only headers/envelopes are fetched. Rules using `RuleField::Body` and header heuristics that depend on body content will not fire until Phase 2 downloads the full message bodies. Emails are re-evaluated through the bundler as bodies arrive during Phase 2, so categorisation catches up automatically — but there may be a temporary window where some emails appear uncategorised in the inbox before their bodies are fetched.

## Snooze & Reminder System

### Snooze Scheduler

- Background tokio task checks snoozed items every 60 seconds
- When `snooze_until <= now`, moves thread back to inbox and emits UI event
- State lives in SQLite (`thread_state.snoozed_until`), email stays in Maildir

### Time-Based Presets

| Option | Resolves To |
|--------|------------|
| Later Today | 4 hours from now (or 6 PM if past 2 PM) |
| Tomorrow | 8 AM tomorrow |
| This Weekend | 8 AM Saturday |
| Next Week | 8 AM Monday |
| Someday | 8 AM in 3 months |
| Custom | User picks date + time of day (Morning/Afternoon/Evening/Night) |

### Location-Based Snooze

- Stores lat/lng/radius/label in SQLite
- Background task polls system location via D-Bus `GeoClue2` interface every 5 minutes (only when location-snoozed items exist, otherwise idle)
- When device enters geofence radius → un-snooze and notify
- Primarily useful for laptop users who move between locations; much more powerful on mobile

**Graceful degradation:**
- If GeoClue2 is unavailable (not installed or D-Bus fails): location snooze option is hidden from the snooze picker UI. Time-based snooze remains fully functional.
- If GeoClue2 is available but location permission is denied: show a one-time prompt explaining why location access is needed. If denied, hide the option.
- If location accuracy is poor (>500m): widen geofence radius automatically and log a warning.
- Existing location-snoozed items when GeoClue2 becomes unavailable: remain snoozed, shown in Snoozed view with a "(location unavailable)" badge. User can manually un-snooze or convert to time-based.

### Reminders

- Stored in SQLite `reminders` table
- Appear in inbox feed as `InboxItem::Reminder`
- Created via speed dial FAB → "Remember to..." input + date/time or location picker
- Marking done archives the reminder (same UX as email done)
- Snoozable with same snooze picker as emails
- No external integration for v1 (Google Keep/Calendar sync is future scope)

## Search & Highlights

### Tantivy Search

- Index built during initial sync, updated incrementally with new mail
- Indexed fields: `from`, `to`, `subject`, `body_text` (full-text), `date` (datetime), `account_id` (facet), `bundle_category` (facet), `has_attachment` (bool)
- Results ranked by BM25 relevance with recency boost
- Search UI: toolbar search bar expands into full results view with snippet highlighting

**Query syntax** — application-layer parser translates user queries to tantivy queries:

| User Syntax | Tantivy Mapping |
|------------|-----------------|
| `from:sarah` | Term query on `from` field |
| `to:bob@example.com` | Term query on `to` field |
| `subject:lunch` | Term query on `subject` field |
| `has:attachment` | Bool query on `has_attachment` field |
| `in:purchases` | Facet query on `bundle_category` field |
| `after:2026-01-01` | Range query on `date` field (>= epoch) |
| `before:2026-03-01` | Range query on `date` field (<= epoch) |
| `is:pinned` | Joined with SQLite `thread_state.pinned` (not in tantivy) |
| `is:unread` | Joined with SQLite `emails.flags` (not in tantivy) |
| bare words | Full-text query across `subject` + `body_text` |

Queries mixing tantivy fields and SQLite-only fields (like `is:pinned`) are executed as tantivy search first, then filtered via SQLite join. This keeps the common case (text search) fast while supporting stateful filters.

### Highlights / Smart Extraction

Regex + pattern matching against email body and headers:

| Highlight Type | Detection Strategy |
|---------------|-------------------|
| Tracking numbers | Carrier-specific regex: UPS (`1Z...`), FedEx (12-34 digit), USPS (20-22 digit), Canada Post (`[0-9]{16}`), DHL. Also `X-Tracking-Number` headers. |
| Flights | Flight number regex (`AC 123`, `WS 456`). Parse departure/arrival/gate from airline confirmation templates. |
| Hotels | Confirmation number patterns, check-in/checkout date extraction from booking.com, Airbnb, hotel chain templates. |
| Events | Parse `.ics` attachments (iCalendar). Regex for "Date:", "When:", "Where:" in invitation emails. |
| Payments | Currency amount regex (`$123.45`, `CAD 50.00`), detect "receipt"/"invoice"/"payment" in subject/body. |

### Trip Bundle Assembly

- When multiple travel highlights cluster within a date range (flight + hotel + car rental in same week), auto-group into `TripBundle`
- Trip bundle shows as card with destination label, date range, and reservation timeline
- Detection: group travel highlights by overlapping date ranges, label by destination from flight arrival city or hotel location
- `inboxly-extract` runs at index time, stores `Vec<Highlight>` in SQLite per thread
- Trip assembly runs as periodic sweep over recent highlights

## UI Architecture

### Framework

Iced (wgpu-rendered, elm architecture). Custom widgets for Inbox-specific interactions.

### Main Layout

```
┌──────────────────────────────────────────────────────┐
│ Toolbar (56dp, colour changes by view)    [Search]   │
├────────────┬─────────────────────────────────────────┤
│ Nav Drawer │  Inbox Feed                             │
│ (264dp)    │                                         │
│            │  [Pinned section]                       │
│ Account    │  [Today] ──────────────── [Sweep]       │
│ ──────     │    Email row                            │
│ Inbox  ●   │    Reminder (blue border)               │
│ Snoozed    │    Bundle: Social (collapsed)    [4new] │
│ Done       │    Bundle: Purchases (collapsed) [2new] │
│ ──────     │    Email with highlight card             │
│ Drafts     │  [This Month] ────────── [Sweep]        │
│ Sent       │    Bundle: Promos (collapsed)    [12]   │
│ Reminders  │                                         │
│ Trash      │                              [FAB]      │
│ Spam       │                         🔔  ✏️          │
│ ──────     │                                         │
│ Bundles    │                                         │
│  ● Social  │                                         │
│  ● Promos  │                                         │
│  ● Updates │                                         │
└────────────┴─────────────────────────────────────────┘
```

### Custom Widgets

| Widget | Purpose |
|--------|---------|
| `EmailRow` | Single email item with avatar, sender, subject, snippet, timestamp. Hover reveals Done/Snooze/Pin actions. |
| `BundleRow` | Collapsed bundle with category icon/colour, name, unread badge, sender preview. Click expands in-place. |
| `ReminderRow` | Reminder item with blue left border and bell icon. |
| `SectionHeader` | Date group header (Pinned/Today/This Month/Earlier) with sweep button. |
| `SwipeContainer` | Wraps any row. Supports two input modes: **Mouse** (click-and-drag horizontally on the row) and **touchpad** (horizontal two-finger swipe). Right drag = green + checkmark (Done). Left drag = orange + clock (Snooze). Two thresholds: arm at 25% of row width, commit at 50%. On hover (without drag), action buttons (Done/Snooze/Pin) appear on the right side of the row as icon buttons — this is the primary desktop interaction path, with swipe as an accelerator. |
| `SpeedDialFab` | Main FAB (red, 56dp) expands to Compose + Reminder with scrim overlay. |
| `SnoozePicker` | 2-column grid dialog (288dp): time presets + custom time + pick location. |
| `ComposeView` | Full email composition: To/Cc/Bcc fields, subject, plaintext body with Markdown preview toggle (v1 — full WYSIWYG is deferred as Iced lacks a built-in rich text editor). Attachments via file picker. Draft auto-save to Maildir every 30 seconds. Outbound HTML generated from Markdown on send. |
| `ConversationView` | Thread view with stacked messages, expand/collapse per message, reply inline. |
| `SearchBar` | Toolbar search with query syntax, expands to results view. |
| `InboxZeroSun` | Celebratory sun illustration when inbox is clear. |

### View States

Three primary views drive toolbar colour and content:

| View | Toolbar | Content |
|------|---------|---------|
| Inbox | `#4285f4` (blue) | Active inbox feed with bundles, pinned items, reminders |
| Snoozed | `#ef6c00` (orange) | Snoozed items with return dates |
| Done | `#0f9d58` (green) | Archived items |

### Animations

- **Bundle expand/collapse**: In-place expansion, items above slide up, items below slide down. Container transform (250-300ms, ease-out).
- **Swipe gesture**: Row tracks finger with coloured background reveal. Elastic snapback on cancel. Commit slides row off-screen, gap collapses (200ms).
- **Sweep cascade**: Multiple rows collapse upward with ~50ms stagger per row.
- **FAB speed dial**: Main button rotates, options fly in from below with staggered fade+slide.
- **View transitions**: Toolbar colour crossfade on view switch.

### Undo Mechanism

Destructive actions (Done, Sweep, Delete) show a snackbar at the bottom of the inbox view:
- Snackbar displays for 8 seconds with the action description (e.g., "3 conversations marked done") and an **Undo** button
- Undo reverses the action immediately (moves threads back to inbox, restores pin state)
- Only the **last** action is undoable — performing a new action dismisses the previous snackbar
- Implementation: the action is applied optimistically in the UI and SQLite, but the IMAP sync (e.g., setting `\Deleted` flag, moving to archive) is delayed until the snackbar expires. If undone, the IMAP operation is cancelled. This avoids network round-trips for undo.

### Communication

- UI thread communicates with sync engine via `tokio::sync::mpsc` channels
- Sync events: `NewEmail`, `EmailFlagsChanged`, `EmailDeleted`, `SyncProgress`, `SyncError`
- UI actions: `MarkDone`, `Pin`, `Unpin`, `Snooze`, `MoveToBundle`, `Compose`, `Search`, `Undo`

## Theme System

### Light Theme (BigTop baseline)

| Token | Value |
|-------|-------|
| Background | `#ececec` |
| Surface (cards) | `#ffffff` |
| Surface selected | `#ebf2ff` |
| Primary text | `#212121` |
| Secondary text | `#757575` |
| Divider/stroke | `#e0e0e0` |
| Toolbar Inbox | `#4285f4` |
| Toolbar Done | `#0f9d58` |
| Toolbar Snoozed | `#ef6c00` |

### Dark Theme

| Token | Value |
|-------|-------|
| Background | `#121212` |
| Surface (cards) | `#1e1e1e` |
| Surface selected | `#1a2744` |
| Primary text | `#e0e0e0` |
| Secondary text | `#9e9e9e` |
| Divider/stroke | `#2c2c2c` |
| Toolbar Inbox | `#1a3a6e` |
| Toolbar Done | `#0b5e35` |
| Toolbar Snoozed | `#8f4100` |

### Constants Across Themes

- Bundle category colours (Social red `#d23f31`, Promos cyan `#00acc1`, etc.)
- Avatar letter tile colours (26-colour A-Z palette)
- Inbox zero sun illustration

### Implementation

- `InboxlyTheme` struct with all colour tokens as fields
- `InboxlyTheme::light()`, `InboxlyTheme::dark()`, `InboxlyTheme::from_system()`
- System theme detection via `org.freedesktop.portal.Settings` D-Bus
- Manual toggle in settings overrides system detection
- Preference persisted in SQLite settings table

### Bundle Category Colours

| Category | Title | Badge Background |
|----------|-------|-----------------|
| Social | `#d23f31` | `#faebea` |
| Promos | `#00acc1` | `#e5f6f9` |
| Updates | `#f4511e` | `#feede8` |
| Finance | `#558b2f` | `#eef3ea` |
| Purchases | `#6d4c41` | `#f0edec` |
| Travel | `#8e24aa` | `#f3e9f6` |
| Forums | `#3949ab` | `#ebecf6` |
| Low Priority | `#212121` | `#e5e5e5` |

### Avatar Letter Tile Colours (A-Z)

```
A=#e06055  B=#ed6192  C=#ba68c8  D=#9575cd  E=#7986cb  F=#5e97f6  G=#4fc3f7
H=#58d0e1  I=#4fb6ac  J=#57bb8a  K=#9ccc65  L=#d4e157  M=#fdd835  N=#f6bf32
O=#f5a631  P=#f18864  Q=#c2c2c2  R=#90a4ae  S=#a1887f  T=#a3a3a3  U=#afb6e0
V=#b39ddb  W=#c2c2c2  X=#80deea  Y=#bcaaa4  Z=#aed581  default=#efefef
```

### Dimensions (from BigTop APK)

All dimensions use logical pixels (1dp = 1 logical pixel at 1x DPI scaling). On HiDPI displays, Iced's built-in scaling applies automatically. Typography uses `sp` from the APK which maps 1:1 to logical pixels on desktop (sp's accessibility scaling is an Android concept; on desktop, system font scaling is handled by the window manager).

| Token | Value |
|-------|-------|
| Toolbar height | 56dp |
| Toolbar elevation | 2dp |
| Nav drawer width | 264dp |
| Nav drawer item height | 48dp |
| Default margin/padding | 16dp |
| Avatar diameter | 40dp |
| Avatar column width | 72dp |
| List item card elevation | 2dp |
| List item corner radius | 0dp (flat cards) |
| Section header height | 48dp |
| FAB diameter | 56dp |
| Mini FAB diameter | 40dp |
| FAB margin from edges | 13dp |
| Snooze grid width | 288dp |
| Snooze option cell | 142dp × 122dp |
| Compose max width | 920dp |
| Divider thickness | 1px |

### Typography

| Element | Size | Weight |
|---------|------|--------|
| Toolbar title | 20sp | Normal |
| Email title/sender | 16sp | Normal (bold if unread) |
| Author name | 14sp | Normal |
| Snippet/preview | 14sp | Normal |
| Timestamp | 12sp | Normal |
| Section header | 14sp | Bold |
| Unread count badge | 16sp | Bold |
| Nav drawer items | 14sp | Medium |
| Compose subject | 18sp | Bold |
| Compose body | 16sp | Normal |

## v1 Feature Scope

### Must Have (13 features)

1. **Automatic Bundling** — Rule-based + sender learning categorisation into Social, Promos, Updates, Finance, Purchases, Travel, Forums, Low Priority. Expand/collapse in-place. Custom bundles.
2. **Done + Sweep** — "Mark as Done" (archive), swipe-to-done, "Clear unpinned" sweep per section/bundle. Done view in sidebar. Undo via snackbar.
3. **Pinning** — Pin important emails to keep at top. Pinned items survive sweep.
4. **Snooze (Time-based)** — Later Today, Tomorrow, This Weekend, Next Week, Someday, Custom. Snoozed view in sidebar.
5. **Snooze (Location-based)** — Geofence triggers via GeoClue2 D-Bus.
6. **Reminders** — Non-email tasks in the feed. Speed dial FAB creation. Time/date triggers.
7. **Compose + Reply** — Full email authoring: reply, reply-all, forward, Markdown body with preview (HTML on send), attachments, draft auto-save.
8. **Full-text Search** — Tantivy-powered with query syntax and snippet highlighting.
9. **Highlights / Smart Extraction** — Tracking numbers, flights, hotels, events, payments shown inline.
10. **Multi-account** — Multiple IMAP accounts. Per-account sync. Account switcher in nav drawer.
11. **Trip Bundles** — Auto-grouped travel itineraries with destination label and reservation timeline.
12. **Dark Mode** — Full dark theme. System detection via freedesktop portal. Manual toggle.
13. **Inbox Zero Sun** — Iconic celebration illustration when inbox is clear.

### Deferred

- **Smart Reply** — Needs LLM infrastructure (local or API). Future version.
- **Keyboard Shortcuts** — Low effort, may sneak in during development.
- **Desktop Notifications** — Depends on sync engine stability. Post-v1.

## Deferred & Extension Points

### Mobile (Android) — Planned Extension

All non-UI crates (`inboxly-core`, `inboxly-imap`, `inboxly-store`, `inboxly-bundler`, `inboxly-snooze`, `inboxly-extract`) are designed to compile for Android via NDK.

Planned architecture:

```
┌─────────────────────────────┐
│  Kotlin / Jetpack Compose   │  ← Android-native UI
├─────────────────────────────┤
│      UniFFI / JNI bridge    │  ← auto-generated bindings
├─────────────────────────────┤
│  inboxly-core + imap +      │
│  store + bundler + snooze   │  ← shared Rust library (.so)
│  + extract                  │
└─────────────────────────────┘
```

**Requirement**: No Iced types or platform-specific types in any non-UI crate API. All public APIs must use types from `inboxly-core` or std.

### Other Future Frontends

The same engine/UI separation enables:
- TUI client (ratatui)
- CLI client
- Web frontend (WASM)

### Manual Thread Merging

Users may want to manually merge or split threads when the References-based algorithm doesn't group correctly. Deferred to post-v1.

### Custom IMAP Folder Sync

v1 syncs only the 5 standard folders (Inbox, Sent, Drafts, Trash, Spam). Syncing user-created folders/labels is deferred.

### WYSIWYG Compose Editor

v1 uses Markdown with preview. A full WYSIWYG rich text editor widget for Iced is deferred pending ecosystem maturity.

### Smart Reply

Future integration with local LLM (e.g., llama.cpp) or Claude API for contextual quick-reply suggestions.

### Calendar/Keep Integration

Future sync with Google Calendar and Google Keep for reminders interop.

## Key Dependencies

| Crate | Dependency | Purpose |
|-------|-----------|---------|
| `inboxly-core` | `serde`, `thiserror`, `chrono`, `uuid` | Serialisation, errors, datetime, IDs |
| `inboxly-imap` | `async-imap`, `tokio`, `tokio-rustls`, `oauth2`, `lettre` | IMAP client, async runtime, TLS, OAuth, SMTP |
| `inboxly-store` | `rusqlite`, `tantivy`, `maildir` | SQLite, full-text search, Maildir format |
| `inboxly-bundler` | `regex` | Pattern matching for rules |
| `inboxly-snooze` | `tokio`, `zbus` | Async scheduler, D-Bus (GeoClue2) |
| `inboxly-extract` | `regex`, `scraper`, `ical` | HTML parsing, iCal parsing |
| `inboxly-ui` | `iced` | GUI framework |
| `inboxly` | `clap` | CLI argument parsing |

## Licence

GPL-3.0

## Research Reference

Full design research (APK decompilation, open-source analysis, UX studies) available at:
- `docs/research/inbox-by-google-complete-reference.md`
- `inbox-decompiled/` — decompiled BigTop APK resources and source
- `reference/` — cloned open-source Inbox recreations (pinbox, inbox-reborn, inboxy, material-inbox)
