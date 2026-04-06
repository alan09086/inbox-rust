# M33: Inbox Feed + Widgets (Dioxus)

## Context

M32 converted the UI shell from Iced to Dioxus Desktop. The app launches with a toolbar, nav drawer, and placeholder content area — but no email content. M33 rebuilds the inbox feed and all interactive widgets as Dioxus components, making the app look and function like an email client again.

**What exists**: Dioxus shell (4 components), state machine (210 tests, 60+ message handlers), feed data model (FeedItem/FeedEntry/FeedSection/build_feed), undo system, CSS with 56 custom properties.

**What's missing**: Email rows, bundle rows, date headers, hover actions, menus, undo snackbar, FAB, snooze picker, empty states.

---

## State Changes

Add to `Inboxly` struct in `app.rs`:
```rust
pub expanded_bundles: HashSet<String>,      // which bundles are expanded
pub snooze_picker_thread: Option<String>,   // thread with open snooze picker
pub snooze_picker_position: Point,          // popup anchor position
```

Add `Message` variants:
```rust
ToggleBundleExpand(String),
OpenSnoozePicker { thread_id: String, position: Point },
CloseSnoozePicker,
```

---

## New Components (12 files)

| File | Purpose | Old Iced LOC |
|------|---------|-------------|
| `inbox_feed.rs` | Main feed list, iterates feed_sections | 373 |
| `email_row.rs` | Thread row: avatar + sender + subject + timestamp + badges | 114 + 41 |
| `bundle_row.rs` | Collapsed/expanded bundle summary | 147 + 54 |
| `section_header.rs` | Date group label ("Today", "This Week") | 31 |
| `hover_actions.rs` | Done/Pin/Snooze buttons on row hover (CSS :hover) | 91 |
| `context_menu.rs` | Right-click popup (position: fixed + backdrop) | 1,095 + 192 |
| `overflow_menu.rs` | Three-dot action menu | 266 |
| `undo_snackbar.rs` | Bottom bar with undo button + timer | 48 |
| `speed_dial_fab.rs` | Compose FAB (position: fixed, bottom-right) | 101 |
| `snooze_picker.rs` | Preset time selection popup | 177 |
| `inbox_zero.rs` | Empty inbox celebration | 45 |
| `empty_state.rs` | Generic empty view | 27 |

**Estimated**: ~900 LOC Dioxus + ~350 LOC CSS (replacing ~2,800 LOC Iced)

---

## Design Decisions

- **Hover actions**: CSS-only via `:hover` — no state tracking needed
- **Context menu**: `position: fixed` using `oncontextmenu` event coords, backdrop div for dismissal
- **Snooze picker**: Inline popup anchored below button, same backdrop pattern
- **Bundle expand**: New `expanded_bundles: HashSet<String>` state field
- **Bundle thread fetch**: Async via `use_resource` + `spawn_blocking` in `BundleRow` component (establishes first async pattern for M34)
- **Undo timer**: `use_future` + `tokio::time::sleep(UNDO_TIMEOUT)` → dispatches `UndoExpired`
- **FAB**: CSS `position: fixed; bottom: 13px; right: 13px`
- **Overlays**: Rendered inside `InboxFeed` using `position: fixed` (escapes parent overflow, keeps components self-contained)
- **Stale state**: Prune `expanded_bundles` in `reload_feed()` — retain only IDs still in feed_sections
- **Tests**: State machine tests (6-8 new) + Dioxus SSR component snapshot tests (new infrastructure)

---

## Implementation Order

### Phase 1: State additions + tests
- Add 3 new fields to `Inboxly`, init in `Default`
- Add 3 new `Message` variants + handlers in `update()`
- Write ~4 unit tests for bundle expand toggle, snooze picker open/close
- **Checkpoint**: `cargo test -p inboxly-ui` passes

### Phase 2: CSS foundation
- Add feed-specific CSS: `.email-row`, `.bundle-row`, `.section-header`, `.avatar`, `.hover-actions`, `.context-menu`, `.overflow-menu`, `.undo-snackbar`, `.fab`, `.snooze-picker`, `.inbox-zero`
- Light/dark theme coverage for all new classes
- Override `.content-area` to be scrollable flex column

### Phase 3: InboxFeed + SectionHeader
- Create `InboxFeed` component — iterates `feed_sections`, renders section headers + entries
- Create `SectionHeader` component — date group label
- Update `ContentArea` to render `InboxFeed` when `active_view == Inbox`
- Register new components in `components/mod.rs`
- **Checkpoint**: Feed structure renders (empty, since no store wired)

### Phase 4: EmailRow + avatar
- Create `EmailRow` component — avatar circle (letter + colour), sender name (bold if unread), subject+snippet, timestamp, attachment icon, email count badge
- Wire `oncontextmenu` for right-click
- **Checkpoint**: Thread rows render with real data from `feed_sections`

### Phase 5: BundleRow
- Create `BundleRow` — category colour dot, name, sender previews, unread badge, expand chevron
- Click dispatches `ToggleBundleExpand` — expanded state shows child email rows
- Expanded rows fetched via `store.query_bundle_threads(bundle_id)`

### Phase 6: Hover actions
- Add `.hover-actions` div inside `EmailRow` — hidden by default, visible on `:hover`
- Three buttons: Done (✓), Pin (📌), Snooze (⏰)
- Dispatch `MarkDone`, `TogglePin`, `OpenSnoozePicker`

### Phase 7: Context menu + overflow menu
- Create `ContextMenu` — full-screen backdrop + positioned card with action groups
- Actions: Reply, Reply All, Forward, Mark Read/Unread, Move To (Inbox/Trash/Spam), Mute, Add To Bundle, Create Rule, Block Sender, Report Spam
- Create `OverflowMenu` — same actions, anchored to three-dot button
- Rendered inside InboxFeed using position: fixed
- **Checkpoint**: Right-click and three-dot menus work

### Phase 8: Undo snackbar
- Create `UndoSnackbar` — fixed bottom centre, shows `undo_state.description()`
- "Undo" button dispatches `Message::Undo`
- `use_future` spawns timer → dispatches `UndoExpired` after 7s
- Render in App component when `undo_state.is_active()`

### Phase 9: FAB + snooze picker
- Create `SpeedDialFab` — fixed bottom-right compose button
- Create `SnoozePicker` — preset grid (Later Today, Tomorrow, Next Week, custom)
- Compute target `DateTime<Utc>` from `config.snooze` presets
- Dispatch `SnoozeThread { thread_id, until }`

### Phase 10: Empty states + polish
- Create `InboxZero` — celebration when feed is empty
- Create `EmptyState` — generic placeholder for Snoozed/Done views
- Integration pass: verify all interactions work together
- **Final checkpoint**: `cargo test --workspace && cargo clippy --workspace -- -D warnings`

---

## Critical Files

- `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` — state + message additions
- `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/components/content_area.rs` — wire InboxFeed
- `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/components/app.rs` — render overlays at root
- `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/feed.rs` — FeedItem/FeedEntry types (read-only)
- `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/assets/main.css` — all new component styles
- `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/components/mod.rs` — register new modules

## Reusable Existing Code

- `feed::build_feed(store)` — already builds the complete feed model
- `feed::format_timestamp()` — already formats dates for display
- `theme::avatar_colors::for_letter()` → `.to_css()` for avatar backgrounds
- `theme::category_color()` → `.title.to_css()` for bundle dots
- `theme::dimensions::*` — all layout constants
- `app::Message::*` — all action handlers already wired in update()
- `undo::UndoState` — is_active(), description(), time_remaining()

---

## Verification

1. `cargo test --workspace` — 790+ tests pass (new state + component tests)
2. `cargo clippy --workspace -- -D warnings` — clean
3. `cargo run -p inboxly` — window opens, shows:
   - Feed with date-grouped sections (or Inbox Zero if no data)
   - Email rows with avatars, sender, subject, snippet, timestamp
   - Bundle rows with category colours, expand to show child threads
   - Hover actions appear on email row hover
   - Three-dot menu opens with action list
   - Right-click context menu works
   - Undo snackbar appears after marking done
   - FAB visible at bottom-right
   - Snooze picker opens from hover action
   - Dark/light theme applies to all new components
4. Bundle expansion error: if store query fails, expanded bundle shows error state (not empty)

## Eng Review Decisions

- **Overlays**: Render inside InboxFeed (position: fixed), not at App root
- **Bundle thread fetch**: Async via use_resource + spawn_blocking (with error state fallback)
- **Stale expanded_bundles**: Prune in reload_feed()
- **Tests**: State machine tests + Dioxus SSR component snapshot tests (new infrastructure)
- **Scope**: All 12 files kept in M33 (no deferrals)
- **Performance**: No action needed; single Signal is fine for M33 feed scale

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | — |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR (PLAN) | 3 issues, 0 critical gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | — |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |

**VERDICT:** ENG CLEARED — 3 issues resolved (overlay location, async bundle fetch, stale pruning). Test plan includes state + component tests.
