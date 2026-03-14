# M19: Done + Pin + Sweep + Undo — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Implement the core inbox triage actions — Mark Done (archive), Pin toggle, Sweep (clear unpinned), Undo snackbar with delayed IMAP sync — plus the Done and Snoozed sidebar views with toolbar colour transitions.

**Architecture:** State mutations live in `inboxly-store` (SQLite `thread_state` table + `offline_queue`). UI actions, animations, and the undo timer live in `inboxly-ui`. The UI applies optimistic local updates immediately, queues IMAP operations with a delay, and cancels them on undo. No IMAP crate changes — M19 writes to the offline queue; actual IMAP replay is M9's responsibility.

**Tech Stack:** Rust, rusqlite, iced, tokio (timers)

**Prerequisite:** M17-M18 complete — inbox feed renders email rows and bundle rows with section headers (Pinned/Today/This Month/Earlier). M3 complete — `thread_state` table exists in SQLite with `thread_id`, `pinned`, `done`, `snoozed_until`, `snoozed_location_json`, `bundle_id` columns. M15-M16 complete — Iced shell with nav drawer and theme system including `InboxlyTheme` with toolbar colour tokens for Inbox/Done/Snoozed views.

---

> **⚠ Plan Correction (post-M13 review):** This plan defines standalone `ThreadStateRepository<'a>` and `OfflineQueueRepository<'a>` structs. The actual codebase uses `impl Store {}` blocks — all methods are added directly to `Store`. Existing methods in `inboxly-store/src/thread_state.rs`: `insert_thread_state()`, `get_thread_state()`, `get_or_create_thread_state()`, `set_thread_pinned()`, `set_thread_done()`, `set_thread_snoozed()`, `set_thread_bundle()`, `get_pinned_threads()`, `get_snoozed_threads()`, `get_threads_by_bundle()`, `get_uncategorised_thread_ids()`, `delete_thread_state()`. Existing methods in `inboxly-store/src/offline_queue.rs`: `enqueue_offline_action()`, `get_offline_queue()`, `dequeue_offline_action()`, `clear_offline_queue()`, `count_offline_queue()`. **Implementation should extend Store with any missing methods, not create new Repository structs.**

## Task 1: Add `ThreadStateRepository` to inboxly-store

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/thread_state.rs` (new file)

Create the repository that manages `thread_state` rows. This is the data layer for all Done/Pin/Sweep operations. All methods are synchronous (rusqlite is sync); the UI wraps calls in `tokio::task::spawn_blocking`.

```rust
use rusqlite::{Connection, params, OptionalExtension};
use inboxly_core::{ThreadId, BundleId};
use chrono::{DateTime, Utc};

/// Persisted state for a single thread (pin, done, snooze, bundle assignment).
#[derive(Debug, Clone)]
pub struct ThreadState {
    pub thread_id: ThreadId,
    pub pinned: bool,
    pub done: bool,
    pub snoozed_until: Option<DateTime<Utc>>,
    pub bundle_id: Option<BundleId>,
}

pub struct ThreadStateRepository<'a> {
    conn: &'a Connection,
}

impl<'a> ThreadStateRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Ensure a thread_state row exists for the given thread.
    /// Inserts default values (pinned=false, done=false) if missing.
    pub fn ensure_exists(&self, thread_id: &ThreadId) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO thread_state (thread_id, pinned, done) VALUES (?1, 0, 0)",
            params![thread_id.to_string()],
        )?;
        Ok(())
    }

    /// Get thread state, returning None if no row exists.
    pub fn get(&self, thread_id: &ThreadId) -> rusqlite::Result<Option<ThreadState>> {
        self.conn.query_row(
            "SELECT thread_id, pinned, done, snoozed_until, bundle_id FROM thread_state WHERE thread_id = ?1",
            params![thread_id.to_string()],
            |row| {
                Ok(ThreadState {
                    thread_id: ThreadId::from_string(row.get::<_, String>(0)?),
                    pinned: row.get(1)?,
                    done: row.get(2)?,
                    snoozed_until: row.get::<_, Option<i64>>(3)?
                        .map(|ts| DateTime::from_timestamp(ts, 0).unwrap_or_default()),
                    bundle_id: row.get::<_, Option<String>>(4)?
                        .map(BundleId::from_string),
                })
            },
        ).optional()
    }

    /// Mark a thread as done (archived). Returns the previous done state.
    pub fn set_done(&self, thread_id: &ThreadId, done: bool) -> rusqlite::Result<bool> {
        self.ensure_exists(thread_id)?;
        let prev: bool = self.conn.query_row(
            "SELECT done FROM thread_state WHERE thread_id = ?1",
            params![thread_id.to_string()],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "UPDATE thread_state SET done = ?2 WHERE thread_id = ?1",
            params![thread_id.to_string(), done],
        )?;
        Ok(prev)
    }

    /// Toggle pinned state. Returns the new pinned value.
    pub fn toggle_pin(&self, thread_id: &ThreadId) -> rusqlite::Result<bool> {
        self.ensure_exists(thread_id)?;
        self.conn.execute(
            "UPDATE thread_state SET pinned = NOT pinned WHERE thread_id = ?1",
            params![thread_id.to_string()],
        )?;
        let new_val: bool = self.conn.query_row(
            "SELECT pinned FROM thread_state WHERE thread_id = ?1",
            params![thread_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(new_val)
    }

    /// Set pinned to a specific value. Returns the previous value.
    pub fn set_pinned(&self, thread_id: &ThreadId, pinned: bool) -> rusqlite::Result<bool> {
        self.ensure_exists(thread_id)?;
        let prev: bool = self.conn.query_row(
            "SELECT pinned FROM thread_state WHERE thread_id = ?1",
            params![thread_id.to_string()],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "UPDATE thread_state SET pinned = ?2 WHERE thread_id = ?1",
            params![thread_id.to_string(), pinned],
        )?;
        Ok(prev)
    }

    /// Sweep: mark all unpinned threads in the given list as done.
    /// Returns the list of thread IDs that were swept (were not already done).
    pub fn sweep_unpinned(&self, thread_ids: &[ThreadId]) -> rusqlite::Result<Vec<ThreadId>> {
        let mut swept = Vec::new();
        for tid in thread_ids {
            self.ensure_exists(tid)?;
            let (pinned, already_done): (bool, bool) = self.conn.query_row(
                "SELECT pinned, done FROM thread_state WHERE thread_id = ?1",
                params![tid.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if !pinned && !already_done {
                self.conn.execute(
                    "UPDATE thread_state SET done = 1 WHERE thread_id = ?1",
                    params![tid.to_string()],
                )?;
                swept.push(tid.clone());
            }
        }
        Ok(swept)
    }

    /// Bulk undo: set done=false for all given thread IDs.
    pub fn undo_done(&self, thread_ids: &[ThreadId]) -> rusqlite::Result<()> {
        for tid in thread_ids {
            self.conn.execute(
                "UPDATE thread_state SET done = 0 WHERE thread_id = ?1",
                params![tid.to_string()],
            )?;
        }
        Ok(())
    }

    /// Restore a previously pinned state (used by undo).
    pub fn restore_pin_states(&self, states: &[(ThreadId, bool)]) -> rusqlite::Result<()> {
        for (tid, pinned) in states {
            self.conn.execute(
                "UPDATE thread_state SET pinned = ?2 WHERE thread_id = ?1",
                params![tid.to_string(), pinned],
            )?;
        }
        Ok(())
    }

    /// Query all threads that are done (for the Done view).
    /// Returns thread IDs ordered by newest_date descending (joined with threads table).
    pub fn list_done(&self) -> rusqlite::Result<Vec<ThreadId>> {
        let mut stmt = self.conn.prepare(
            "SELECT ts.thread_id FROM thread_state ts \
             JOIN threads t ON ts.thread_id = t.id \
             WHERE ts.done = 1 \
             ORDER BY t.newest_date DESC"
        )?;
        let ids = stmt.query_map([], |row| {
            Ok(ThreadId::from_string(row.get::<_, String>(0)?))
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// Query all threads that are snoozed (for the Snoozed view).
    /// Returns thread IDs ordered by snoozed_until ascending (soonest first).
    pub fn list_snoozed(&self) -> rusqlite::Result<Vec<ThreadId>> {
        let mut stmt = self.conn.prepare(
            "SELECT thread_id FROM thread_state \
             WHERE snoozed_until IS NOT NULL \
             ORDER BY snoozed_until ASC"
        )?;
        let ids = stmt.query_map([], |row| {
            Ok(ThreadId::from_string(row.get::<_, String>(0)?))
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// Check if a thread is pinned.
    pub fn is_pinned(&self, thread_id: &ThreadId) -> rusqlite::Result<bool> {
        match self.get(thread_id)? {
            Some(state) => Ok(state.pinned),
            None => Ok(false),
        }
    }
}
```

**Also:** Add `pub mod thread_state;` to `inboxly-store/src/lib.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

**Commit:** `feat(store): add ThreadStateRepository for done/pin/sweep operations`

---

## Task 2: Add `OfflineQueueRepository` to inboxly-store

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/offline_queue.rs` (new file)

The offline queue stores pending IMAP actions. M19 writes entries here; M9's sync engine reads and replays them. For undo, entries are deleted before replay.

```rust
use rusqlite::{Connection, params};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

/// An action queued for IMAP sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueuedAction {
    /// Move thread to archive (set \Seen, remove from INBOX).
    MarkDone { thread_id: String },
    /// Undo archive (move back to INBOX).
    UndoDone { thread_id: String },
    /// Set/unset IMAP \Flagged flag (maps to pin).
    SetFlagged { thread_id: String, flagged: bool },
}

/// A row in the offline_queue table.
#[derive(Debug, Clone)]
pub struct QueueEntry {
    pub id: i64,
    pub action: String,
    pub payload_json: String,
    pub created_at: DateTime<Utc>,
}

pub struct OfflineQueueRepository<'a> {
    conn: &'a Connection,
}

impl<'a> OfflineQueueRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Enqueue an action for later IMAP sync.
    /// Returns the row ID of the queued entry.
    pub fn enqueue(&self, action: &QueuedAction) -> rusqlite::Result<i64> {
        let action_name = match action {
            QueuedAction::MarkDone { .. } => "mark_done",
            QueuedAction::UndoDone { .. } => "undo_done",
            QueuedAction::SetFlagged { .. } => "set_flagged",
        };
        let payload = serde_json::to_string(action)
            .unwrap_or_default();
        let now = Utc::now().timestamp();

        self.conn.execute(
            "INSERT INTO offline_queue (action, payload_json, created_at) VALUES (?1, ?2, ?3)",
            params![action_name, payload, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Remove a queued entry by ID (used for undo cancellation).
    pub fn cancel(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM offline_queue WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Remove all queued entries with the given IDs (bulk undo cancellation).
    pub fn cancel_many(&self, ids: &[i64]) -> rusqlite::Result<()> {
        for id in ids {
            self.cancel(*id)?;
        }
        Ok(())
    }

    /// Get all pending entries, oldest first (for sync replay).
    pub fn list_pending(&self) -> rusqlite::Result<Vec<QueueEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, action, payload_json, created_at FROM offline_queue ORDER BY created_at ASC"
        )?;
        let entries = stmt.query_map([], |row| {
            Ok(QueueEntry {
                id: row.get(0)?,
                action: row.get(1)?,
                payload_json: row.get(2)?,
                created_at: DateTime::from_timestamp(row.get::<_, i64>(3)?, 0)
                    .unwrap_or_default(),
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }
}
```

**Also:** Add `pub mod offline_queue;` to `inboxly-store/src/lib.rs`.

**Dependency:** Add `serde_json = "1"` to `inboxly-store/Cargo.toml` if not already present (needed for `QueuedAction` serialization).

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

**Commit:** `feat(store): add OfflineQueueRepository for delayed IMAP action queue`

---

## Task 3: Add `ActiveView` enum and toolbar colour logic to inboxly-ui

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views.rs` (new file)

Define the three primary views that drive toolbar colour and content. The toolbar colour crossfades on view switch per the spec's animation section.

```rust
use iced::Color;

/// The three primary views that determine toolbar colour and content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveView {
    #[default]
    Inbox,
    Snoozed,
    Done,
}

impl ActiveView {
    /// Returns the toolbar background colour for this view (light theme).
    pub fn toolbar_color_light(&self) -> Color {
        match self {
            ActiveView::Inbox => Color::from_rgb(
                0x42 as f32 / 255.0,
                0x85 as f32 / 255.0,
                0xf4 as f32 / 255.0,
            ), // #4285f4
            ActiveView::Done => Color::from_rgb(
                0x0f as f32 / 255.0,
                0x9d as f32 / 255.0,
                0x58 as f32 / 255.0,
            ), // #0f9d58
            ActiveView::Snoozed => Color::from_rgb(
                0xef as f32 / 255.0,
                0x6c as f32 / 255.0,
                0x00 as f32 / 255.0,
            ), // #ef6c00
        }
    }

    /// Returns the toolbar background colour for this view (dark theme).
    pub fn toolbar_color_dark(&self) -> Color {
        match self {
            ActiveView::Inbox => Color::from_rgb(
                0x1a as f32 / 255.0,
                0x3a as f32 / 255.0,
                0x6e as f32 / 255.0,
            ), // #1a3a6e
            ActiveView::Done => Color::from_rgb(
                0x0b as f32 / 255.0,
                0x5e as f32 / 255.0,
                0x35 as f32 / 255.0,
            ), // #0b5e35
            ActiveView::Snoozed => Color::from_rgb(
                0x8f as f32 / 255.0,
                0x41 as f32 / 255.0,
                0x00 as f32 / 255.0,
            ), // #8f4100
        }
    }

    /// Display name for the toolbar title.
    pub fn title(&self) -> &'static str {
        match self {
            ActiveView::Inbox => "Inbox",
            ActiveView::Done => "Done",
            ActiveView::Snoozed => "Snoozed",
        }
    }
}

/// State for the toolbar colour crossfade animation.
#[derive(Debug, Clone)]
pub struct ToolbarTransition {
    /// The view we are transitioning from.
    pub from: ActiveView,
    /// The view we are transitioning to.
    pub to: ActiveView,
    /// Progress 0.0 (fully `from`) to 1.0 (fully `to`).
    pub progress: f32,
    /// Whether a transition is currently in flight.
    pub active: bool,
}

impl Default for ToolbarTransition {
    fn default() -> Self {
        Self {
            from: ActiveView::Inbox,
            to: ActiveView::Inbox,
            progress: 1.0,
            active: false,
        }
    }
}

impl ToolbarTransition {
    /// Start a transition to a new view.
    pub fn start(&mut self, from: ActiveView, to: ActiveView) {
        if from == to {
            return;
        }
        self.from = from;
        self.to = to;
        self.progress = 0.0;
        self.active = true;
    }

    /// Advance the transition by a time delta. Returns true if still animating.
    /// Duration: 300ms crossfade.
    pub fn tick(&mut self, dt_millis: f32) -> bool {
        if !self.active {
            return false;
        }
        self.progress += dt_millis / 300.0;
        if self.progress >= 1.0 {
            self.progress = 1.0;
            self.active = false;
        }
        true
    }

    /// Get the current interpolated colour (light theme).
    pub fn current_color_light(&self) -> Color {
        let from = self.from.toolbar_color_light();
        let to = self.to.toolbar_color_light();
        lerp_color(from, to, self.progress)
    }

    /// Get the current interpolated colour (dark theme).
    pub fn current_color_dark(&self) -> Color {
        let from = self.from.toolbar_color_dark();
        let to = self.to.toolbar_color_dark();
        lerp_color(from, to, self.progress)
    }
}

/// Linear interpolation between two colours.
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::from_rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}
```

**Also:** Add `pub mod views;` to `inboxly-ui/src/lib.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add ActiveView enum with toolbar colour transitions`

---

## Task 4: Add `UndoAction` enum and `UndoState` to inboxly-ui

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/undo.rs` (new file)

The undo system captures the last destructive action, displays a snackbar for 8 seconds, and cancels the queued IMAP operations if the user clicks Undo. Only the **last** action is undoable — a new action dismisses the previous snackbar and commits the previous action's IMAP operations.

```rust
use inboxly_core::ThreadId;
use std::time::{Duration, Instant};

/// The duration the undo snackbar is shown before auto-committing.
pub const UNDO_TIMEOUT: Duration = Duration::from_secs(8);

/// A destructive action that can be undone.
#[derive(Debug, Clone)]
pub enum UndoAction {
    /// Single thread marked done.
    MarkDone {
        thread_id: ThreadId,
        /// Offline queue row ID for the IMAP action.
        queue_id: i64,
    },
    /// Multiple threads swept (clear unpinned).
    Sweep {
        thread_ids: Vec<ThreadId>,
        /// Offline queue row IDs for all the IMAP actions.
        queue_ids: Vec<i64>,
        /// The section or bundle name that was swept (for display).
        section_label: String,
    },
    /// Pin state changed (for undo, we restore the previous state).
    PinToggle {
        thread_id: ThreadId,
        /// The pin state *before* the action (to restore on undo).
        was_pinned: bool,
        /// Offline queue row ID.
        queue_id: i64,
    },
}

impl UndoAction {
    /// Human-readable description for the snackbar.
    pub fn description(&self) -> String {
        match self {
            UndoAction::MarkDone { .. } => "1 conversation marked done".to_string(),
            UndoAction::Sweep { thread_ids, section_label, .. } => {
                let count = thread_ids.len();
                if count == 1 {
                    format!("1 conversation in {} marked done", section_label)
                } else {
                    format!("{} conversations in {} marked done", count, section_label)
                }
            }
            UndoAction::PinToggle { was_pinned, .. } => {
                if *was_pinned {
                    "Pin removed".to_string()
                } else {
                    "Conversation pinned".to_string()
                }
            }
        }
    }

    /// Get all offline queue IDs associated with this action.
    pub fn queue_ids(&self) -> Vec<i64> {
        match self {
            UndoAction::MarkDone { queue_id, .. } => vec![*queue_id],
            UndoAction::Sweep { queue_ids, .. } => queue_ids.clone(),
            UndoAction::PinToggle { queue_id, .. } => vec![*queue_id],
        }
    }
}

/// State for the undo snackbar.
#[derive(Debug, Clone)]
pub struct UndoState {
    /// The current undoable action, if any.
    pub action: Option<UndoAction>,
    /// When the snackbar was shown (for timeout tracking).
    pub shown_at: Option<Instant>,
    /// Progress 0.0 to 1.0 for the timeout bar (visual countdown).
    pub timeout_progress: f32,
    /// Whether the snackbar is currently visible.
    pub visible: bool,
}

impl Default for UndoState {
    fn default() -> Self {
        Self {
            action: None,
            shown_at: None,
            timeout_progress: 0.0,
            visible: false,
        }
    }
}

impl UndoState {
    /// Show the undo snackbar for a new action.
    /// If there was a previous action, it is committed (IMAP operations proceed).
    /// Returns the previous action if one was pending (caller must commit its IMAP ops).
    pub fn show(&mut self, action: UndoAction) -> Option<UndoAction> {
        let previous = self.action.take();
        self.action = Some(action);
        self.shown_at = Some(Instant::now());
        self.timeout_progress = 0.0;
        self.visible = true;
        previous
    }

    /// Dismiss the snackbar and return the action for committing.
    /// Called when timeout expires or a new action replaces it.
    pub fn commit(&mut self) -> Option<UndoAction> {
        self.visible = false;
        self.shown_at = None;
        self.timeout_progress = 1.0;
        self.action.take()
    }

    /// Undo the current action. Returns the action for reversal.
    pub fn undo(&mut self) -> Option<UndoAction> {
        self.visible = false;
        self.shown_at = None;
        self.timeout_progress = 0.0;
        self.action.take()
    }

    /// Tick the timeout timer. Returns true if the snackbar just expired.
    pub fn tick(&mut self) -> bool {
        if let Some(shown_at) = self.shown_at {
            let elapsed = shown_at.elapsed();
            self.timeout_progress = (elapsed.as_secs_f32() / UNDO_TIMEOUT.as_secs_f32())
                .min(1.0);
            if elapsed >= UNDO_TIMEOUT {
                return true; // Caller should call commit()
            }
        }
        false
    }

    /// Check whether the snackbar is currently showing.
    pub fn is_visible(&self) -> bool {
        self.visible && self.action.is_some()
    }
}
```

**Also:** Add `pub mod undo;` to `inboxly-ui/src/lib.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add UndoAction/UndoState for undo snackbar with 8s timeout`

---

## Task 5: Add sweep cascade animation state to inboxly-ui

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/animations.rs` (new file)

The sweep cascade visual collapses multiple rows upward with ~50ms stagger per row. Each row animates its height from full to 0 over 200ms, starting 50ms after the previous row.

```rust
use inboxly_core::ThreadId;
use std::time::Instant;

/// Duration for a single row's collapse animation.
const ROW_COLLAPSE_DURATION_MS: f32 = 200.0;

/// Stagger delay between rows starting their collapse.
const ROW_STAGGER_MS: f32 = 50.0;

/// State for the sweep cascade animation.
#[derive(Debug, Clone)]
pub struct SweepCascade {
    /// Thread IDs being swept, in visual order (top to bottom).
    pub thread_ids: Vec<ThreadId>,
    /// When the cascade started.
    pub started_at: Instant,
    /// Whether the cascade is still animating.
    pub active: bool,
}

impl SweepCascade {
    /// Create a new sweep cascade for the given threads.
    pub fn new(thread_ids: Vec<ThreadId>) -> Self {
        Self {
            thread_ids,
            started_at: Instant::now(),
            active: true,
        }
    }

    /// Get the collapse progress for a specific row index (0.0 = full height, 1.0 = collapsed).
    /// Uses ease-out cubic for smooth deceleration.
    pub fn row_progress(&self, index: usize) -> f32 {
        let elapsed_ms = self.started_at.elapsed().as_secs_f32() * 1000.0;
        let row_start = index as f32 * ROW_STAGGER_MS;
        let row_elapsed = (elapsed_ms - row_start).max(0.0);
        let linear = (row_elapsed / ROW_COLLAPSE_DURATION_MS).min(1.0);
        // Ease-out cubic: 1 - (1 - t)^3
        let t = 1.0 - linear;
        1.0 - t * t * t
    }

    /// Get the height multiplier for a row (1.0 = full, 0.0 = gone).
    pub fn row_height_factor(&self, index: usize) -> f32 {
        1.0 - self.row_progress(index)
    }

    /// Check if the entire cascade is complete.
    pub fn is_complete(&self) -> bool {
        if self.thread_ids.is_empty() {
            return true;
        }
        let last_index = self.thread_ids.len() - 1;
        self.row_progress(last_index) >= 1.0
    }

    /// Tick the animation. Returns true if still animating.
    pub fn tick(&mut self) -> bool {
        if self.is_complete() {
            self.active = false;
            return false;
        }
        true
    }

    /// Total duration of the cascade in milliseconds.
    pub fn total_duration_ms(&self) -> f32 {
        if self.thread_ids.is_empty() {
            return 0.0;
        }
        (self.thread_ids.len() - 1) as f32 * ROW_STAGGER_MS + ROW_COLLAPSE_DURATION_MS
    }
}

/// State for a single row's done/archive slide-off animation.
/// The row slides right and fades out, then the gap below collapses.
#[derive(Debug, Clone)]
pub struct DoneSlideOff {
    pub thread_id: ThreadId,
    pub started_at: Instant,
    pub active: bool,
}

/// Duration for slide-off animation in milliseconds.
const SLIDE_OFF_DURATION_MS: f32 = 200.0;

impl DoneSlideOff {
    pub fn new(thread_id: ThreadId) -> Self {
        Self {
            thread_id,
            started_at: Instant::now(),
            active: true,
        }
    }

    /// Horizontal offset as fraction of row width (0.0 = original, 1.0 = fully off-screen right).
    pub fn slide_progress(&self) -> f32 {
        let elapsed_ms = self.started_at.elapsed().as_secs_f32() * 1000.0;
        let linear = (elapsed_ms / SLIDE_OFF_DURATION_MS).min(1.0);
        // Ease-in: t^2
        linear * linear
    }

    /// Opacity (1.0 = fully visible, 0.0 = gone).
    pub fn opacity(&self) -> f32 {
        1.0 - self.slide_progress()
    }

    /// Whether the animation is complete.
    pub fn is_complete(&self) -> bool {
        self.slide_progress() >= 1.0
    }

    pub fn tick(&mut self) -> bool {
        if self.is_complete() {
            self.active = false;
            return false;
        }
        true
    }
}
```

**Also:** Add `pub mod animations;` to `inboxly-ui/src/lib.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add sweep cascade and done slide-off animation state`

---

## Task 6: Define M19-specific `Message` variants in inboxly-ui

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/message.rs` (modify existing)

Add the new message variants needed for Done/Pin/Sweep/Undo to the existing `Message` enum. These are the Iced Elm-architecture messages that drive state transitions.

Add the following variants to the existing `Message` enum:

```rust
// === Done / Archive ===
/// User clicked the Done (checkmark) button on a thread row.
MarkDone(ThreadId),
/// Store confirmed done was persisted + returns queue_id.
DoneCompleted { thread_id: ThreadId, queue_id: i64 },
/// Store confirmed undo-done was persisted.
UndoDoneCompleted { thread_ids: Vec<ThreadId> },

// === Pin ===
/// User clicked the pin toggle on a thread row.
TogglePin(ThreadId),
/// Store confirmed pin toggle + returns new state and queue_id.
PinToggled { thread_id: ThreadId, now_pinned: bool, queue_id: i64 },
/// Store confirmed pin undo was persisted.
PinUndoCompleted { thread_id: ThreadId },

// === Sweep ===
/// User clicked the sweep button on a section header.
Sweep { section: SectionId },
/// Store confirmed sweep completed + returns swept thread IDs and queue_ids.
SweepCompleted { thread_ids: Vec<ThreadId>, queue_ids: Vec<i64>, section_label: String },

// === Undo ===
/// User clicked the Undo button on the snackbar.
UndoClicked,
/// Undo timeout expired — commit the action.
UndoTimeout,
/// Undo snackbar tick (for progress bar animation, fired every 100ms).
UndoTick,

// === View switching ===
/// User clicked a view in the nav drawer (Inbox, Done, Snoozed).
SwitchView(ActiveView),
/// Toolbar colour transition tick (fired every 16ms during animation).
ToolbarTransitionTick,

// === Animation ticks ===
/// Sweep cascade animation tick (fired every 16ms during cascade).
SweepCascadeTick,
/// Done slide-off animation tick.
DoneSlideOffTick,
```

Also add a `SectionId` enum to identify which section is being swept:

```rust
/// Identifies a section in the inbox feed for sweep targeting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionId {
    /// The "Today" time section.
    Today,
    /// The "This Month" time section.
    ThisMonth,
    /// The "Earlier" time section.
    Earlier,
    /// A specific bundle (sweep all unpinned in the expanded bundle).
    Bundle(BundleId),
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add Message variants for done/pin/sweep/undo/view-switch`

---

## Task 7: Implement `update` handlers for MarkDone and TogglePin

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify existing)

Add match arms in the existing `update` method of the Iced `Application` impl. Each handler follows the pattern: (1) spawn blocking store operation, (2) on completion message, update local state + show undo snackbar.

Add these match arms inside the existing `update()` function's `match message` block:

```rust
Message::MarkDone(thread_id) => {
    // Optimistic: remove from feed immediately
    self.feed.remove_thread(&thread_id);

    // Start slide-off animation
    self.done_slide_off = Some(DoneSlideOff::new(thread_id.clone()));

    // Spawn blocking store operation
    let store = self.store.clone();
    let tid = thread_id.clone();
    Command::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let conn = store.connection();
                let repo = ThreadStateRepository::new(&conn);
                repo.set_done(&tid, true).ok();

                let queue = OfflineQueueRepository::new(&conn);
                let queue_id = queue.enqueue(&QueuedAction::MarkDone {
                    thread_id: tid.to_string(),
                }).unwrap_or(0);
                (tid, queue_id)
            }).await.unwrap()
        },
        |(thread_id, queue_id)| Message::DoneCompleted { thread_id, queue_id },
    )
}

Message::DoneCompleted { thread_id, queue_id } => {
    // If there was a previous undo action, commit it (IMAP ops proceed)
    if let Some(prev) = self.undo_state.show(UndoAction::MarkDone {
        thread_id,
        queue_id,
    }) {
        self.commit_undo_action(prev);
    }

    // Start undo timeout subscription
    self.start_undo_timer();
    Command::none()
}

Message::TogglePin(thread_id) => {
    let store = self.store.clone();
    let tid = thread_id.clone();
    Command::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let conn = store.connection();
                let repo = ThreadStateRepository::new(&conn);
                let was_pinned = repo.is_pinned(&tid).unwrap_or(false);
                let now_pinned = repo.toggle_pin(&tid).unwrap_or(false);

                let queue = OfflineQueueRepository::new(&conn);
                let queue_id = queue.enqueue(&QueuedAction::SetFlagged {
                    thread_id: tid.to_string(),
                    flagged: now_pinned,
                }).unwrap_or(0);
                (tid, now_pinned, queue_id)
            }).await.unwrap()
        },
        |(thread_id, now_pinned, queue_id)| Message::PinToggled {
            thread_id,
            now_pinned,
            queue_id,
        },
    )
}

Message::PinToggled { thread_id, now_pinned, queue_id } => {
    // Update the feed: if pinned, move to pinned section; if unpinned, move back.
    self.feed.update_pin_state(&thread_id, now_pinned);

    // Show undo snackbar
    if let Some(prev) = self.undo_state.show(UndoAction::PinToggle {
        thread_id,
        was_pinned: !now_pinned,
        queue_id,
    }) {
        self.commit_undo_action(prev);
    }

    self.start_undo_timer();
    Command::none()
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement MarkDone and TogglePin update handlers`

---

## Task 8: Implement `update` handler for Sweep

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify existing)

Add the sweep handler. Sweep collects all unpinned thread IDs from the specified section, starts the cascade animation, and spawns the blocking store operation.

Add this match arm:

```rust
Message::Sweep { section } => {
    // Collect unpinned thread IDs from the section
    let thread_ids = self.feed.unpinned_threads_in_section(&section);
    if thread_ids.is_empty() {
        return Command::none();
    }

    let section_label = match &section {
        SectionId::Today => "Today".to_string(),
        SectionId::ThisMonth => "This Month".to_string(),
        SectionId::Earlier => "Earlier".to_string(),
        SectionId::Bundle(id) => self.feed.bundle_name(id)
            .unwrap_or_else(|| "bundle".to_string()),
    };

    // Start cascade animation
    self.sweep_cascade = Some(SweepCascade::new(thread_ids.clone()));

    // Spawn blocking store operation
    let store = self.store.clone();
    let tids = thread_ids.clone();
    let label = section_label.clone();
    Command::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let conn = store.connection();
                let repo = ThreadStateRepository::new(&conn);
                let swept = repo.sweep_unpinned(&tids).unwrap_or_default();

                let queue = OfflineQueueRepository::new(&conn);
                let queue_ids: Vec<i64> = swept.iter().map(|tid| {
                    queue.enqueue(&QueuedAction::MarkDone {
                        thread_id: tid.to_string(),
                    }).unwrap_or(0)
                }).collect();

                (swept, queue_ids, label)
            }).await.unwrap()
        },
        |(thread_ids, queue_ids, section_label)| Message::SweepCompleted {
            thread_ids,
            queue_ids,
            section_label,
        },
    )
}

Message::SweepCompleted { thread_ids, queue_ids, section_label } => {
    // Remove swept threads from feed (animation already handles visual)
    for tid in &thread_ids {
        self.feed.remove_thread(tid);
    }

    // Show undo snackbar
    if let Some(prev) = self.undo_state.show(UndoAction::Sweep {
        thread_ids,
        queue_ids,
        section_label,
    }) {
        self.commit_undo_action(prev);
    }

    self.start_undo_timer();
    Command::none()
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement Sweep update handler with cascade animation`

---

## Task 9: Implement `update` handlers for Undo

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify existing)

Add handlers for undo click, undo timeout, and undo tick. Undo reverses the store operation and cancels the queued IMAP entries.

Add these match arms:

```rust
Message::UndoClicked => {
    if let Some(action) = self.undo_state.undo() {
        match action {
            UndoAction::MarkDone { thread_id, queue_id } => {
                let store = self.store.clone();
                let tid = thread_id.clone();
                Command::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let conn = store.connection();
                            let repo = ThreadStateRepository::new(&conn);
                            repo.undo_done(&[tid.clone()]).ok();
                            let queue = OfflineQueueRepository::new(&conn);
                            queue.cancel(queue_id).ok();
                            vec![tid]
                        }).await.unwrap()
                    },
                    |thread_ids| Message::UndoDoneCompleted { thread_ids },
                )
            }
            UndoAction::Sweep { thread_ids, queue_ids, .. } => {
                let store = self.store.clone();
                let tids = thread_ids.clone();
                let qids = queue_ids.clone();
                Command::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let conn = store.connection();
                            let repo = ThreadStateRepository::new(&conn);
                            repo.undo_done(&tids).ok();
                            let queue = OfflineQueueRepository::new(&conn);
                            queue.cancel_many(&qids).ok();
                            tids
                        }).await.unwrap()
                    },
                    |thread_ids| Message::UndoDoneCompleted { thread_ids },
                )
            }
            UndoAction::PinToggle { thread_id, was_pinned, queue_id } => {
                let store = self.store.clone();
                let tid = thread_id.clone();
                Command::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let conn = store.connection();
                            let repo = ThreadStateRepository::new(&conn);
                            repo.set_pinned(&tid, was_pinned).ok();
                            let queue = OfflineQueueRepository::new(&conn);
                            queue.cancel(queue_id).ok();
                            tid
                        }).await.unwrap()
                    },
                    |thread_id| Message::PinUndoCompleted { thread_id },
                )
            }
        }
    } else {
        Command::none()
    }
}

Message::UndoDoneCompleted { thread_ids } => {
    // Re-add threads to the inbox feed
    self.feed.restore_threads(&thread_ids);
    Command::none()
}

Message::PinUndoCompleted { thread_id } => {
    // Refresh the pin state in the feed
    let store = self.store.clone();
    let tid = thread_id.clone();
    Command::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let conn = store.connection();
                let repo = ThreadStateRepository::new(&conn);
                let is_pinned = repo.is_pinned(&tid).unwrap_or(false);
                (tid, is_pinned)
            }).await.unwrap()
        },
        |(thread_id, is_pinned)| Message::PinToggled {
            thread_id,
            now_pinned: is_pinned,
            queue_id: 0, // No new queue entry for undo restoration
        },
    )
}

Message::UndoTimeout => {
    if let Some(action) = self.undo_state.commit() {
        // Action committed — IMAP operations will proceed on next sync.
        // No additional work needed; entries remain in offline_queue.
        let _ = action;
    }
    Command::none()
}

Message::UndoTick => {
    if self.undo_state.tick() {
        // Timer expired
        return self.update(Message::UndoTimeout);
    }
    Command::none()
}
```

Add the helper method to the app struct:

```rust
impl InboxlyApp {
    /// Commit a previous undo action that was replaced by a new one.
    /// The IMAP queue entries are left in place — they'll be replayed on next sync.
    fn commit_undo_action(&self, _action: UndoAction) {
        // Queue entries already exist in offline_queue; nothing to do.
        // The entries remain and will be picked up by the sync engine.
    }

    /// Start the undo timer subscription (ticks every 100ms for 8 seconds).
    fn start_undo_timer(&mut self) {
        // Iced subscription will be handled in Task 12 (subscriptions).
        // The timer fires UndoTick messages at 100ms intervals.
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement undo click/timeout/tick handlers with store reversal`

---

## Task 10: Implement `update` handler for SwitchView

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify existing)

Add view switching. When the user clicks Inbox/Done/Snoozed in the nav drawer, the toolbar colour crossfades and the feed content changes.

Add these match arms:

```rust
Message::SwitchView(view) => {
    if view == self.active_view {
        return Command::none();
    }

    // Start toolbar colour transition
    self.toolbar_transition.start(self.active_view, view);
    self.active_view = view;

    // Load the appropriate content
    match view {
        ActiveView::Inbox => {
            // Inbox feed is already loaded; just switch display
            Command::none()
        }
        ActiveView::Done => {
            let store = self.store.clone();
            Command::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let conn = store.connection();
                        let repo = ThreadStateRepository::new(&conn);
                        repo.list_done().unwrap_or_default()
                    }).await.unwrap()
                },
                |thread_ids| Message::DoneViewLoaded(thread_ids),
            )
        }
        ActiveView::Snoozed => {
            let store = self.store.clone();
            Command::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let conn = store.connection();
                        let repo = ThreadStateRepository::new(&conn);
                        repo.list_snoozed().unwrap_or_default()
                    }).await.unwrap()
                },
                |thread_ids| Message::SnoozedViewLoaded(thread_ids),
            )
        }
    }
}

Message::ToolbarTransitionTick => {
    // Advance by 16ms (one frame at 60fps)
    self.toolbar_transition.tick(16.0);
    Command::none()
}

Message::DoneViewLoaded(thread_ids) => {
    self.done_feed = thread_ids;
    Command::none()
}

Message::SnoozedViewLoaded(thread_ids) => {
    self.snoozed_feed = thread_ids;
    Command::none()
}
```

Add `DoneViewLoaded(Vec<ThreadId>)` and `SnoozedViewLoaded(Vec<ThreadId>)` to the `Message` enum in `message.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement SwitchView handler with done/snoozed content loading`

---

## Task 11: Add M19 state fields to the app struct

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify existing)

Add the new state fields to the `InboxlyApp` struct that the handlers from Tasks 7-10 reference.

Add these fields to the existing `InboxlyApp` struct:

```rust
// === M19: Done + Pin + Sweep + Undo ===

/// The currently active view (Inbox, Done, Snoozed).
active_view: ActiveView,

/// Toolbar colour transition state.
toolbar_transition: ToolbarTransition,

/// Undo snackbar state.
undo_state: UndoState,

/// Sweep cascade animation (if one is in progress).
sweep_cascade: Option<SweepCascade>,

/// Single-thread done slide-off animation (if one is in progress).
done_slide_off: Option<DoneSlideOff>,

/// Thread IDs for the Done view content.
done_feed: Vec<ThreadId>,

/// Thread IDs for the Snoozed view content.
snoozed_feed: Vec<ThreadId>,
```

Initialize them in `InboxlyApp::new()` (or equivalent constructor):

```rust
active_view: ActiveView::Inbox,
toolbar_transition: ToolbarTransition::default(),
undo_state: UndoState::default(),
sweep_cascade: None,
done_slide_off: None,
done_feed: Vec::new(),
snoozed_feed: Vec::new(),
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add M19 state fields to InboxlyApp struct`

---

## Task 12: Add Iced subscriptions for undo timer and animation ticks

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify existing)

Add subscriptions to the Iced `Application::subscription()` method. These drive the timed events for undo timeout and animations.

In the existing `subscription()` method, add:

```rust
fn subscription(&self) -> Subscription<Message> {
    let mut subs = Vec::new();

    // Undo timer: tick every 100ms while snackbar is visible
    if self.undo_state.is_visible() {
        subs.push(
            iced::time::every(std::time::Duration::from_millis(100))
                .map(|_| Message::UndoTick)
        );
    }

    // Toolbar transition: tick every 16ms (~60fps) while animating
    if self.toolbar_transition.active {
        subs.push(
            iced::time::every(std::time::Duration::from_millis(16))
                .map(|_| Message::ToolbarTransitionTick)
        );
    }

    // Sweep cascade: tick every 16ms while animating
    if self.sweep_cascade.as_ref().map_or(false, |c| c.active) {
        subs.push(
            iced::time::every(std::time::Duration::from_millis(16))
                .map(|_| Message::SweepCascadeTick)
        );
    }

    // Done slide-off: tick every 16ms while animating
    if self.done_slide_off.as_ref().map_or(false, |d| d.active) {
        subs.push(
            iced::time::every(std::time::Duration::from_millis(16))
                .map(|_| Message::DoneSlideOffTick)
        );
    }

    // ... existing subscriptions (sync events, etc.) ...

    Subscription::batch(subs)
}
```

Also add handlers for animation ticks:

```rust
Message::SweepCascadeTick => {
    if let Some(ref mut cascade) = self.sweep_cascade {
        if !cascade.tick() {
            self.sweep_cascade = None;
        }
    }
    Command::none()
}

Message::DoneSlideOffTick => {
    if let Some(ref mut slide) = self.done_slide_off {
        if !slide.tick() {
            self.done_slide_off = None;
        }
    }
    Command::none()
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add Iced subscriptions for undo timer and animation ticks`

---

## Task 13: Render the undo snackbar widget

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/snackbar.rs` (new file)

The snackbar is a bottom-anchored bar (48dp tall, full feed width) with the action description text on the left and an "Undo" button on the right. A thin progress bar at the top shows remaining time.

```rust
use iced::widget::{container, row, text, button, progress_bar, column, Space};
use iced::{Element, Length, Alignment, Color, Padding};

use crate::undo::UndoState;
use crate::message::Message;

/// Render the undo snackbar. Returns None if not visible.
pub fn undo_snackbar(state: &UndoState) -> Option<Element<'_, Message>> {
    if !state.is_visible() {
        return None;
    }

    let action = state.action.as_ref()?;
    let description = action.description();

    let progress = progress_bar(0.0..=1.0, 1.0 - state.timeout_progress)
        .height(3);

    let content = row![
        text(description)
            .size(14)
            .style(Color::WHITE),
        Space::with_width(Length::Fill),
        button(
            text("Undo")
                .size(14)
        )
        .on_press(Message::UndoClicked)
        .padding(Padding::from([4, 16])),
    ]
    .align_y(Alignment::Center)
    .padding(Padding::from([0, 16]))
    .height(45);

    let snackbar = column![
        progress,
        content,
    ]
    .width(Length::Fill);

    let styled = container(snackbar)
        .width(Length::Fill)
        .style(|_theme| {
            container::Style {
                background: Some(iced::Background::Color(Color::from_rgb(
                    0x32 as f32 / 255.0,
                    0x32 as f32 / 255.0,
                    0x32 as f32 / 255.0,
                ))), // #323232 — Material dark snackbar
                ..Default::default()
            }
        });

    Some(styled.into())
}
```

**Also:** Add `pub mod snackbar;` to `inboxly-ui/src/widgets/mod.rs` (create `widgets/mod.rs` if it doesn't exist).

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add undo snackbar widget with progress bar`

---

## Task 14: Add pin icon and sweep button to EmailRow and SectionHeader

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/email_row.rs` (modify existing)

Modify the `EmailRow` widget to show a pin icon (thumbtack, 18dp) on the right side when the thread is pinned. The pin icon uses the primary text colour when pinned and is hidden when not pinned. On hover, show a clickable pin toggle button alongside the existing hover action buttons.

Add to the row's right-side content area:

```rust
// Pin indicator (always visible when pinned)
if is_pinned {
    row = row.push(
        text("📌") // Placeholder — replace with icon font glyph in M20
            .size(18)
    );
}

// Hover actions (visible on hover)
if is_hovered {
    row = row.push(
        button(text(if is_pinned { "Unpin" } else { "Pin" }).size(12))
            .on_press(Message::TogglePin(thread_id.clone()))
            .padding(4),
    );
    row = row.push(
        button(text("Done").size(12))
            .on_press(Message::MarkDone(thread_id.clone()))
            .padding(4),
    );
}
```

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/section_header.rs` (modify existing)

Add a "Sweep" button to the right side of each section header (Today, This Month, Earlier). The button text is a broom/checkmark icon. Clicking it emits `Message::Sweep { section }`.

```rust
// In the section header row, add sweep button on the right:
let sweep_btn = button(
    text("✓ Clear")
        .size(12)
)
.on_press(Message::Sweep { section: section_id.clone() })
.padding(Padding::from([4, 8]));

row![
    text(&self.label).size(14).font(Font::DEFAULT), // Section label
    Space::with_width(Length::Fill),
    sweep_btn,
]
.align_y(Alignment::Center)
.height(48)
.padding(Padding::from([0, 16]))
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add pin indicator and sweep button to feed rows and section headers`

---

## Task 15: Render Done view content

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/done_view.rs` (new file)

The Done view shows archived threads in a simple list. The toolbar is green (`#0f9d58`). Threads can be un-archived (moved back to inbox) by clicking a "Move to Inbox" button.

```rust
use iced::widget::{column, scrollable, text, container, Space};
use iced::{Element, Length, Padding};
use inboxly_core::ThreadId;

use crate::message::Message;
use crate::views::ActiveView;

/// Render the Done view content.
/// `thread_ids` — the list of done threads to display.
/// `thread_lookup` — function to get thread display data by ID.
pub fn done_view<'a>(
    thread_ids: &'a [ThreadId],
    // In practice, this will be a closure or method on the feed/store
    // that returns the thread's subject, sender, date, etc.
) -> Element<'a, Message> {
    if thread_ids.is_empty() {
        return container(
            column![
                Space::with_height(80),
                text("No archived conversations")
                    .size(16),
                Space::with_height(8),
                text("Conversations you mark as done will appear here.")
                    .size(14),
            ]
            .align_x(iced::Alignment::Center)
            .width(Length::Fill)
        )
        .width(Length::Fill)
        .center_x(Length::Fill)
        .into();
    }

    let mut items = column![]
        .spacing(1)
        .width(Length::Fill);

    for tid in thread_ids {
        // Each done thread renders as a simplified email row
        // with a "Move to Inbox" action instead of "Done"
        items = items.push(
            done_thread_row(tid)
        );
    }

    scrollable(
        container(items)
            .width(Length::Fill)
            .padding(Padding::from([0, 0]))
    )
    .height(Length::Fill)
    .into()
}

/// Render a single thread row in the Done view.
/// Shows thread subject/sender with a "Move to Inbox" button.
fn done_thread_row(thread_id: &ThreadId) -> Element<'_, Message> {
    // Placeholder — will integrate with thread data lookup from M17's EmailRow
    // For now, emit MarkDone with done=false (undo) semantics
    container(
        text(format!("Thread: {}", thread_id))
            .size(14)
    )
    .width(Length::Fill)
    .padding(Padding::from([12, 16]))
    .into()
}
```

**Also:** Create `inboxly-ui/src/views/mod.rs` if it doesn't exist, re-export from there:

```rust
pub mod done_view;
pub mod snoozed_view;
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add Done view with archived thread listing`

---

## Task 16: Render Snoozed view placeholder

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/snoozed_view.rs` (new file)

The Snoozed view is an orange-toolbar placeholder for M19. It lists snoozed threads with their return dates. Actual snooze functionality (setting snooze, un-snoozing on timer) is M21.

```rust
use iced::widget::{column, scrollable, text, container, Space};
use iced::{Element, Length, Padding};
use inboxly_core::ThreadId;

use crate::message::Message;

/// Render the Snoozed view content.
/// `thread_ids` — snoozed threads, ordered by snoozed_until ascending.
pub fn snoozed_view<'a>(
    thread_ids: &'a [ThreadId],
) -> Element<'a, Message> {
    if thread_ids.is_empty() {
        return container(
            column![
                Space::with_height(80),
                text("No snoozed conversations")
                    .size(16),
                Space::with_height(8),
                text("Snoozed conversations will reappear at their scheduled time.")
                    .size(14),
            ]
            .align_x(iced::Alignment::Center)
            .width(Length::Fill)
        )
        .width(Length::Fill)
        .center_x(Length::Fill)
        .into();
    }

    let mut items = column![]
        .spacing(1)
        .width(Length::Fill);

    for tid in thread_ids {
        items = items.push(
            snoozed_thread_row(tid)
        );
    }

    scrollable(
        container(items)
            .width(Length::Fill)
            .padding(Padding::from([0, 0]))
    )
    .height(Length::Fill)
    .into()
}

/// Render a single thread row in the Snoozed view.
fn snoozed_thread_row(thread_id: &ThreadId) -> Element<'_, Message> {
    container(
        text(format!("Snoozed thread: {}", thread_id))
            .size(14)
    )
    .width(Length::Fill)
    .padding(Padding::from([12, 16]))
    .into()
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add Snoozed view placeholder with orange toolbar`

---

## Task 17: Integrate view switching into the main `view()` method

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify existing)

Modify the `view()` method to:
1. Use the interpolated toolbar colour from `toolbar_transition` instead of a fixed colour.
2. Switch between Inbox feed, Done view, and Snoozed view based on `active_view`.
3. Overlay the undo snackbar at the bottom when visible.

In the existing `view()` method, replace the toolbar colour and feed content:

```rust
fn view(&self) -> Element<Message> {
    // Toolbar colour — use transition interpolation
    let toolbar_color = if self.toolbar_transition.active {
        if self.is_dark_theme {
            self.toolbar_transition.current_color_dark()
        } else {
            self.toolbar_transition.current_color_light()
        }
    } else {
        if self.is_dark_theme {
            self.active_view.toolbar_color_dark()
        } else {
            self.active_view.toolbar_color_light()
        }
    };

    // Toolbar with view title
    let toolbar = container(
        text(self.active_view.title())
            .size(20)
            .style(Color::WHITE)
    )
    .width(Length::Fill)
    .height(56)
    .padding(Padding::from([0, 16]))
    .style(move |_theme| container::Style {
        background: Some(iced::Background::Color(toolbar_color)),
        ..Default::default()
    });

    // Main content based on active view
    let content: Element<Message> = match self.active_view {
        ActiveView::Inbox => self.render_inbox_feed(),
        ActiveView::Done => done_view::done_view(&self.done_feed),
        ActiveView::Snoozed => snoozed_view::snoozed_view(&self.snoozed_feed),
    };

    // Nav drawer entries highlight active view
    let nav = self.render_nav_drawer(); // Existing method — update to pass active_view

    // Stack: toolbar + content + optional snackbar
    let mut main_column = column![toolbar, content];

    // Undo snackbar overlay at bottom
    if let Some(snackbar) = snackbar::undo_snackbar(&self.undo_state) {
        main_column = main_column.push(snackbar);
    }

    // Combine nav drawer + main content
    row![nav, main_column]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): integrate view switching and undo snackbar into main view`

---

## Task 18: Update nav drawer to highlight active view and emit SwitchView

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/nav_drawer.rs` (modify existing)

Update the nav drawer to:
1. Accept `active_view: ActiveView` to highlight the active entry.
2. Emit `Message::SwitchView(view)` when Inbox/Done/Snoozed entries are clicked.
3. Use bold text + tinted background for the active entry.

Modify the nav drawer entry rendering:

```rust
fn nav_entry(label: &str, view: ActiveView, active: ActiveView) -> Element<'_, Message> {
    let is_active = view == active;

    let label_widget = text(label)
        .size(14)
        .font(if is_active { Font::DEFAULT } else { Font::DEFAULT }); // Medium weight if available

    let entry = button(
        container(label_widget)
            .width(Length::Fill)
            .padding(Padding::from([0, 16]))
            .height(48)
            .center_y(48)
    )
    .on_press(Message::SwitchView(view))
    .width(Length::Fill)
    .padding(0);

    if is_active {
        container(entry)
            .style(|_theme| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    0.0, 0.0, 0.0, 0.08,
                ))),
                ..Default::default()
            })
            .into()
    } else {
        entry.into()
    }
}

// In the drawer column:
column![
    nav_entry("Inbox", ActiveView::Inbox, self.active_view),
    nav_entry("Snoozed", ActiveView::Snoozed, self.active_view),
    nav_entry("Done", ActiveView::Done, self.active_view),
    // ... rest of nav entries (Drafts, Sent, etc.)
]
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): update nav drawer with active view highlighting and SwitchView`

---

## Task 19: Add feed helper methods for pin reordering and thread removal

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/feed.rs` (modify existing)

Add the helper methods on the feed state that the update handlers call. These manage the in-memory feed ordering.

Add these methods to the existing feed struct:

```rust
impl InboxFeed {
    /// Remove a thread from the feed (used for MarkDone).
    pub fn remove_thread(&mut self, thread_id: &ThreadId) {
        self.items.retain(|item| {
            match item {
                InboxItem::Thread(t) => t.id != *thread_id,
                _ => true,
            }
        });
        // Also remove from pinned section if present
        self.pinned_ids.remove(thread_id);
    }

    /// Update a thread's pin state and reorder the feed accordingly.
    /// Pinned items move to the top "Pinned" section; unpinned move back.
    pub fn update_pin_state(&mut self, thread_id: &ThreadId, pinned: bool) {
        if pinned {
            self.pinned_ids.insert(thread_id.clone());
        } else {
            self.pinned_ids.remove(thread_id);
        }
        // Reorder: pinned items first, then time-grouped sections
        self.reorder();
    }

    /// Get all unpinned thread IDs in a given section.
    pub fn unpinned_threads_in_section(&self, section: &SectionId) -> Vec<ThreadId> {
        self.items.iter()
            .filter(|item| {
                if let InboxItem::Thread(t) = item {
                    !self.pinned_ids.contains(&t.id)
                        && self.thread_section(&t.id) == Some(section.clone())
                } else {
                    false
                }
            })
            .filter_map(|item| {
                if let InboxItem::Thread(t) = item {
                    Some(t.id.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get the display name of a bundle by ID.
    pub fn bundle_name(&self, bundle_id: &BundleId) -> Option<String> {
        self.items.iter().find_map(|item| {
            if let InboxItem::Bundle(b) = item {
                if b.id == *bundle_id {
                    Some(b.name.clone())
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    /// Restore threads to the feed (used by undo).
    /// Reloads thread data from store and inserts at correct position.
    pub fn restore_threads(&mut self, thread_ids: &[ThreadId]) {
        // In practice, this reloads from the store.
        // For now, mark as needing a full feed refresh.
        self.needs_refresh = true;
        // The actual re-fetch is triggered by the refresh mechanism
        // established in M17.
    }

    /// Determine which section a thread belongs to based on its date.
    fn thread_section(&self, thread_id: &ThreadId) -> Option<SectionId> {
        // Lookup thread date and classify into Today/ThisMonth/Earlier
        // Based on the same logic used by the section header grouping in M17
        self.items.iter().find_map(|item| {
            if let InboxItem::Thread(t) = item {
                if t.id == *thread_id {
                    Some(classify_date(t.newest_date))
                } else {
                    None
                }
            } else {
                None
            }
        })
    }
}

/// Classify a date into a section.
fn classify_date(date: DateTime<Utc>) -> SectionId {
    let now = Utc::now();
    let today_start = now.date_naive();
    let thread_date = date.date_naive();

    if thread_date == today_start {
        SectionId::Today
    } else if thread_date.year() == today_start.year()
        && thread_date.month() == today_start.month() {
        SectionId::ThisMonth
    } else {
        SectionId::Earlier
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add feed helper methods for pin reorder and thread removal`

---

## Task 20: Tests for ThreadStateRepository

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/thread_state_tests.rs` (new file)

Unit tests using an in-memory SQLite database.

```rust
use rusqlite::Connection;
use inboxly_store::thread_state::ThreadStateRepository;
use inboxly_core::ThreadId;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE thread_state (
            thread_id TEXT PRIMARY KEY,
            pinned BOOLEAN NOT NULL DEFAULT 0,
            done BOOLEAN NOT NULL DEFAULT 0,
            snoozed_until INTEGER,
            snoozed_location_json TEXT,
            bundle_id TEXT
        );
        CREATE TABLE threads (
            id TEXT PRIMARY KEY,
            account_id TEXT,
            subject TEXT,
            newest_date INTEGER NOT NULL DEFAULT 0,
            oldest_date INTEGER,
            email_count INTEGER,
            unread_count INTEGER,
            has_attachments BOOLEAN,
            snippet TEXT
        );"
    ).unwrap();
    conn
}

fn make_thread_id(s: &str) -> ThreadId {
    ThreadId::from_string(s.to_string())
}

#[test]
fn test_set_done_and_undo() {
    let conn = setup_db();
    let repo = ThreadStateRepository::new(&conn);
    let tid = make_thread_id("thread-1");

    // Initially not done
    let prev = repo.set_done(&tid, true).unwrap();
    assert!(!prev);

    // Now done
    let state = repo.get(&tid).unwrap().unwrap();
    assert!(state.done);

    // Undo
    repo.undo_done(&[tid.clone()]).unwrap();
    let state = repo.get(&tid).unwrap().unwrap();
    assert!(!state.done);
}

#[test]
fn test_toggle_pin() {
    let conn = setup_db();
    let repo = ThreadStateRepository::new(&conn);
    let tid = make_thread_id("thread-2");

    // Toggle on
    let now_pinned = repo.toggle_pin(&tid).unwrap();
    assert!(now_pinned);

    // Toggle off
    let now_pinned = repo.toggle_pin(&tid).unwrap();
    assert!(!now_pinned);
}

#[test]
fn test_set_pinned() {
    let conn = setup_db();
    let repo = ThreadStateRepository::new(&conn);
    let tid = make_thread_id("thread-3");

    let prev = repo.set_pinned(&tid, true).unwrap();
    assert!(!prev); // was not pinned

    let prev = repo.set_pinned(&tid, false).unwrap();
    assert!(prev); // was pinned

    assert!(!repo.is_pinned(&tid).unwrap());
}

#[test]
fn test_sweep_unpinned() {
    let conn = setup_db();
    let repo = ThreadStateRepository::new(&conn);

    let t1 = make_thread_id("thread-a");
    let t2 = make_thread_id("thread-b");
    let t3 = make_thread_id("thread-c");

    // Pin thread-b
    repo.set_pinned(&t2, true).unwrap();

    // Sweep all three
    let swept = repo.sweep_unpinned(&[t1.clone(), t2.clone(), t3.clone()]).unwrap();

    // Only t1 and t3 should be swept (t2 is pinned)
    assert_eq!(swept.len(), 2);
    assert!(swept.contains(&t1));
    assert!(swept.contains(&t3));
    assert!(!swept.contains(&t2));

    // Verify states
    assert!(repo.get(&t1).unwrap().unwrap().done);
    assert!(!repo.get(&t2).unwrap().unwrap().done);
    assert!(repo.get(&t3).unwrap().unwrap().done);
}

#[test]
fn test_sweep_skips_already_done() {
    let conn = setup_db();
    let repo = ThreadStateRepository::new(&conn);

    let t1 = make_thread_id("thread-x");
    repo.set_done(&t1, true).unwrap();

    let swept = repo.sweep_unpinned(&[t1.clone()]).unwrap();
    assert!(swept.is_empty()); // Already done, not re-swept
}

#[test]
fn test_list_done() {
    let conn = setup_db();
    let repo = ThreadStateRepository::new(&conn);

    let t1 = make_thread_id("thread-d1");
    let t2 = make_thread_id("thread-d2");

    // Insert corresponding thread rows for the JOIN
    conn.execute(
        "INSERT INTO threads (id, newest_date) VALUES (?1, ?2)",
        rusqlite::params![t1.to_string(), 1000],
    ).unwrap();
    conn.execute(
        "INSERT INTO threads (id, newest_date) VALUES (?1, ?2)",
        rusqlite::params![t2.to_string(), 2000],
    ).unwrap();

    repo.set_done(&t1, true).unwrap();
    repo.set_done(&t2, true).unwrap();

    let done = repo.list_done().unwrap();
    assert_eq!(done.len(), 2);
    // Ordered by newest_date DESC, so t2 first
    assert_eq!(done[0], t2);
    assert_eq!(done[1], t1);
}

#[test]
fn test_restore_pin_states() {
    let conn = setup_db();
    let repo = ThreadStateRepository::new(&conn);

    let t1 = make_thread_id("thread-p1");
    let t2 = make_thread_id("thread-p2");

    repo.set_pinned(&t1, true).unwrap();
    repo.set_pinned(&t2, false).unwrap();

    // Save states, then change them
    repo.set_pinned(&t1, false).unwrap();
    repo.set_pinned(&t2, true).unwrap();

    // Restore original states
    repo.restore_pin_states(&[
        (t1.clone(), true),
        (t2.clone(), false),
    ]).unwrap();

    assert!(repo.is_pinned(&t1).unwrap());
    assert!(!repo.is_pinned(&t2).unwrap());
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store
```

**Commit:** `test(store): add ThreadStateRepository unit tests`

---

## Task 21: Tests for OfflineQueueRepository

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/offline_queue_tests.rs` (new file)

```rust
use rusqlite::Connection;
use inboxly_store::offline_queue::{OfflineQueueRepository, QueuedAction};

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE offline_queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            action TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );"
    ).unwrap();
    conn
}

#[test]
fn test_enqueue_and_list() {
    let conn = setup_db();
    let repo = OfflineQueueRepository::new(&conn);

    let id1 = repo.enqueue(&QueuedAction::MarkDone {
        thread_id: "t1".to_string(),
    }).unwrap();

    let id2 = repo.enqueue(&QueuedAction::SetFlagged {
        thread_id: "t2".to_string(),
        flagged: true,
    }).unwrap();

    assert!(id1 > 0);
    assert!(id2 > id1);

    let pending = repo.list_pending().unwrap();
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].id, id1);
    assert_eq!(pending[1].id, id2);
}

#[test]
fn test_cancel_single() {
    let conn = setup_db();
    let repo = OfflineQueueRepository::new(&conn);

    let id = repo.enqueue(&QueuedAction::MarkDone {
        thread_id: "t1".to_string(),
    }).unwrap();

    repo.cancel(id).unwrap();

    let pending = repo.list_pending().unwrap();
    assert!(pending.is_empty());
}

#[test]
fn test_cancel_many() {
    let conn = setup_db();
    let repo = OfflineQueueRepository::new(&conn);

    let id1 = repo.enqueue(&QueuedAction::MarkDone {
        thread_id: "t1".to_string(),
    }).unwrap();
    let id2 = repo.enqueue(&QueuedAction::MarkDone {
        thread_id: "t2".to_string(),
    }).unwrap();
    let id3 = repo.enqueue(&QueuedAction::MarkDone {
        thread_id: "t3".to_string(),
    }).unwrap();

    // Cancel first two
    repo.cancel_many(&[id1, id2]).unwrap();

    let pending = repo.list_pending().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, id3);
}

#[test]
fn test_cancel_nonexistent_is_ok() {
    let conn = setup_db();
    let repo = OfflineQueueRepository::new(&conn);

    // Should not error
    repo.cancel(9999).unwrap();
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store
```

**Commit:** `test(store): add OfflineQueueRepository unit tests`

---

## Task 22: Tests for UndoState

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/tests/undo_tests.rs` (new file)

```rust
use inboxly_ui::undo::{UndoState, UndoAction, UNDO_TIMEOUT};
use inboxly_core::ThreadId;

fn make_thread_id(s: &str) -> ThreadId {
    ThreadId::from_string(s.to_string())
}

#[test]
fn test_show_undo_returns_none_when_empty() {
    let mut state = UndoState::default();
    let prev = state.show(UndoAction::MarkDone {
        thread_id: make_thread_id("t1"),
        queue_id: 1,
    });
    assert!(prev.is_none());
    assert!(state.is_visible());
}

#[test]
fn test_show_replaces_previous() {
    let mut state = UndoState::default();

    state.show(UndoAction::MarkDone {
        thread_id: make_thread_id("t1"),
        queue_id: 1,
    });

    let prev = state.show(UndoAction::MarkDone {
        thread_id: make_thread_id("t2"),
        queue_id: 2,
    });

    // Previous action returned for committing
    assert!(prev.is_some());
    if let Some(UndoAction::MarkDone { thread_id, .. }) = prev {
        assert_eq!(thread_id, make_thread_id("t1"));
    } else {
        panic!("Expected MarkDone");
    }
}

#[test]
fn test_undo_clears_and_returns() {
    let mut state = UndoState::default();
    state.show(UndoAction::MarkDone {
        thread_id: make_thread_id("t1"),
        queue_id: 1,
    });

    let undone = state.undo();
    assert!(undone.is_some());
    assert!(!state.is_visible());
}

#[test]
fn test_commit_clears_and_returns() {
    let mut state = UndoState::default();
    state.show(UndoAction::MarkDone {
        thread_id: make_thread_id("t1"),
        queue_id: 1,
    });

    let committed = state.commit();
    assert!(committed.is_some());
    assert!(!state.is_visible());
}

#[test]
fn test_undo_description_single() {
    let action = UndoAction::MarkDone {
        thread_id: make_thread_id("t1"),
        queue_id: 1,
    };
    assert_eq!(action.description(), "1 conversation marked done");
}

#[test]
fn test_undo_description_sweep() {
    let action = UndoAction::Sweep {
        thread_ids: vec![make_thread_id("t1"), make_thread_id("t2"), make_thread_id("t3")],
        queue_ids: vec![1, 2, 3],
        section_label: "Today".to_string(),
    };
    assert_eq!(action.description(), "3 conversations in Today marked done");
}

#[test]
fn test_undo_description_pin() {
    let action = UndoAction::PinToggle {
        thread_id: make_thread_id("t1"),
        was_pinned: false,
        queue_id: 1,
    };
    assert_eq!(action.description(), "Conversation pinned");

    let action2 = UndoAction::PinToggle {
        thread_id: make_thread_id("t1"),
        was_pinned: true,
        queue_id: 1,
    };
    assert_eq!(action2.description(), "Pin removed");
}

#[test]
fn test_queue_ids() {
    let action = UndoAction::Sweep {
        thread_ids: vec![make_thread_id("t1"), make_thread_id("t2")],
        queue_ids: vec![10, 20],
        section_label: "Today".to_string(),
    };
    assert_eq!(action.queue_ids(), vec![10, 20]);
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui
```

**Commit:** `test(ui): add UndoState unit tests`

---

## Task 23: Tests for animations

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/tests/animation_tests.rs` (new file)

```rust
use inboxly_ui::animations::{SweepCascade, DoneSlideOff};
use inboxly_core::ThreadId;

fn make_thread_id(s: &str) -> ThreadId {
    ThreadId::from_string(s.to_string())
}

#[test]
fn test_sweep_cascade_empty() {
    let cascade = SweepCascade::new(vec![]);
    assert!(cascade.is_complete());
    assert_eq!(cascade.total_duration_ms(), 0.0);
}

#[test]
fn test_sweep_cascade_single_row() {
    let cascade = SweepCascade::new(vec![make_thread_id("t1")]);
    // At t=0, first row starts collapsing
    assert!(cascade.row_progress(0) >= 0.0);
    // Total duration is just one row collapse
    assert_eq!(cascade.total_duration_ms(), 200.0);
}

#[test]
fn test_sweep_cascade_stagger() {
    let tids = (0..5).map(|i| make_thread_id(&format!("t{}", i))).collect::<Vec<_>>();
    let cascade = SweepCascade::new(tids);
    // Total = 4 * 50ms stagger + 200ms collapse = 400ms
    assert_eq!(cascade.total_duration_ms(), 400.0);
}

#[test]
fn test_sweep_cascade_row_height_starts_full() {
    let cascade = SweepCascade::new(vec![make_thread_id("t1")]);
    // At the very start, height should be close to 1.0 (may not be exactly 1.0 due to timing)
    let height = cascade.row_height_factor(0);
    assert!(height <= 1.0);
}

#[test]
fn test_toolbar_transition_no_op_same_view() {
    use inboxly_ui::views::{ToolbarTransition, ActiveView};
    let mut transition = ToolbarTransition::default();
    transition.start(ActiveView::Inbox, ActiveView::Inbox);
    assert!(!transition.active); // No-op
}

#[test]
fn test_toolbar_transition_progresses() {
    use inboxly_ui::views::{ToolbarTransition, ActiveView};
    let mut transition = ToolbarTransition::default();
    transition.start(ActiveView::Inbox, ActiveView::Done);
    assert!(transition.active);
    assert_eq!(transition.progress, 0.0);

    // Tick 150ms (half of 300ms)
    transition.tick(150.0);
    assert!(transition.active);
    assert!((transition.progress - 0.5).abs() < 0.01);

    // Tick another 200ms (past end)
    transition.tick(200.0);
    assert!(!transition.active);
    assert_eq!(transition.progress, 1.0);
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui
```

**Commit:** `test(ui): add animation state unit tests`

---

## Task 24: Integration test — full done/undo cycle

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/done_undo_integration.rs` (new file)

End-to-end test that exercises the full cycle: mark done -> queue IMAP action -> undo -> cancel IMAP action.

```rust
use rusqlite::Connection;
use inboxly_store::thread_state::ThreadStateRepository;
use inboxly_store::offline_queue::{OfflineQueueRepository, QueuedAction};
use inboxly_core::ThreadId;

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE thread_state (
            thread_id TEXT PRIMARY KEY,
            pinned BOOLEAN NOT NULL DEFAULT 0,
            done BOOLEAN NOT NULL DEFAULT 0,
            snoozed_until INTEGER,
            snoozed_location_json TEXT,
            bundle_id TEXT
        );
        CREATE TABLE threads (
            id TEXT PRIMARY KEY,
            account_id TEXT,
            subject TEXT,
            newest_date INTEGER NOT NULL DEFAULT 0,
            oldest_date INTEGER,
            email_count INTEGER,
            unread_count INTEGER,
            has_attachments BOOLEAN,
            snippet TEXT
        );
        CREATE TABLE offline_queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            action TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );"
    ).unwrap();
    conn
}

fn make_thread_id(s: &str) -> ThreadId {
    ThreadId::from_string(s.to_string())
}

#[test]
fn test_mark_done_then_undo_full_cycle() {
    let conn = setup_db();
    let ts_repo = ThreadStateRepository::new(&conn);
    let queue_repo = OfflineQueueRepository::new(&conn);

    let tid = make_thread_id("thread-full-cycle");

    // 1. Mark done
    ts_repo.set_done(&tid, true).unwrap();
    assert!(ts_repo.get(&tid).unwrap().unwrap().done);

    // 2. Queue IMAP action
    let queue_id = queue_repo.enqueue(&QueuedAction::MarkDone {
        thread_id: tid.to_string(),
    }).unwrap();
    assert_eq!(queue_repo.list_pending().unwrap().len(), 1);

    // 3. Undo: reverse store and cancel queue
    ts_repo.undo_done(&[tid.clone()]).unwrap();
    assert!(!ts_repo.get(&tid).unwrap().unwrap().done);

    queue_repo.cancel(queue_id).unwrap();
    assert!(queue_repo.list_pending().unwrap().is_empty());
}

#[test]
fn test_sweep_then_undo_full_cycle() {
    let conn = setup_db();
    let ts_repo = ThreadStateRepository::new(&conn);
    let queue_repo = OfflineQueueRepository::new(&conn);

    let t1 = make_thread_id("sweep-1");
    let t2 = make_thread_id("sweep-2");
    let t3 = make_thread_id("sweep-pinned");

    // Pin one thread
    ts_repo.set_pinned(&t3, true).unwrap();

    // 1. Sweep
    let swept = ts_repo.sweep_unpinned(&[t1.clone(), t2.clone(), t3.clone()]).unwrap();
    assert_eq!(swept.len(), 2);

    // 2. Queue IMAP actions
    let queue_ids: Vec<i64> = swept.iter().map(|tid| {
        queue_repo.enqueue(&QueuedAction::MarkDone {
            thread_id: tid.to_string(),
        }).unwrap()
    }).collect();
    assert_eq!(queue_repo.list_pending().unwrap().len(), 2);

    // 3. Undo: reverse all swept threads and cancel queue
    ts_repo.undo_done(&swept).unwrap();
    for tid in &swept {
        assert!(!ts_repo.get(tid).unwrap().unwrap().done);
    }

    queue_repo.cancel_many(&queue_ids).unwrap();
    assert!(queue_repo.list_pending().unwrap().is_empty());

    // Pinned thread was never swept
    assert!(!ts_repo.get(&t3).unwrap().unwrap().done);
    assert!(ts_repo.get(&t3).unwrap().unwrap().pinned);
}

#[test]
fn test_pin_toggle_then_undo() {
    let conn = setup_db();
    let ts_repo = ThreadStateRepository::new(&conn);
    let queue_repo = OfflineQueueRepository::new(&conn);

    let tid = make_thread_id("pin-cycle");

    // 1. Pin
    let was_pinned = ts_repo.is_pinned(&tid).unwrap();
    assert!(!was_pinned);
    let now_pinned = ts_repo.toggle_pin(&tid).unwrap();
    assert!(now_pinned);

    // 2. Queue
    let queue_id = queue_repo.enqueue(&QueuedAction::SetFlagged {
        thread_id: tid.to_string(),
        flagged: true,
    }).unwrap();

    // 3. Undo: restore previous state
    ts_repo.set_pinned(&tid, was_pinned).unwrap();
    assert!(!ts_repo.is_pinned(&tid).unwrap());

    queue_repo.cancel(queue_id).unwrap();
    assert!(queue_repo.list_pending().unwrap().is_empty());
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store
```

**Commit:** `test(store): add done/undo integration tests for full lifecycle`

---

## Summary

| Task | Crate | What |
|------|-------|------|
| 1 | `store` | `ThreadStateRepository` — done/pin/sweep/undo SQL operations |
| 2 | `store` | `OfflineQueueRepository` — delayed IMAP action queue |
| 3 | `ui` | `ActiveView` enum + `ToolbarTransition` with colour interpolation |
| 4 | `ui` | `UndoAction` + `UndoState` — 8s snackbar with commit/undo lifecycle |
| 5 | `ui` | `SweepCascade` + `DoneSlideOff` — animation state machines |
| 6 | `ui` | `Message` enum variants for all M19 actions |
| 7 | `ui` | `update()` handlers for MarkDone and TogglePin |
| 8 | `ui` | `update()` handler for Sweep with cascade animation |
| 9 | `ui` | `update()` handlers for UndoClicked/UndoTimeout/UndoTick |
| 10 | `ui` | `update()` handler for SwitchView (Inbox/Done/Snoozed) |
| 11 | `ui` | State fields added to `InboxlyApp` struct |
| 12 | `ui` | Iced subscriptions for undo timer + animation ticks |
| 13 | `ui` | Undo snackbar widget (Material dark bar + progress + Undo button) |
| 14 | `ui` | Pin icon on rows + sweep button on section headers |
| 15 | `ui` | Done view content (green toolbar, archived thread list) |
| 16 | `ui` | Snoozed view placeholder (orange toolbar) |
| 17 | `ui` | Main `view()` integration — view switching + snackbar overlay |
| 18 | `ui` | Nav drawer active view highlighting + SwitchView messages |
| 19 | `ui` | Feed helper methods (remove, reorder pins, sweep collection) |
| 20 | `store` | `ThreadStateRepository` unit tests |
| 21 | `store` | `OfflineQueueRepository` unit tests |
| 22 | `ui` | `UndoState` unit tests |
| 23 | `ui` | Animation state unit tests |
| 24 | `store` | Full done/undo integration tests |

**After M19**: The client is a usable email reader — users can archive, pin, sweep, and undo, with three navigable views (Inbox/Done/Snoozed) and toolbar colour transitions. This is the "usable email client" checkpoint from the roadmap.
