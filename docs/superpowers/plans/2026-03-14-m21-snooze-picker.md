# M21: Snooze + Picker — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Implement the complete snooze system — time presets, location-based snooze via GeoClue2, background scheduler, snooze picker widget, and snoozed view.

**Architecture:** Two crates receive work: `inboxly-snooze` (scheduler, preset resolution, GeoClue2 integration) and `inboxly-ui` (SnoozePicker widget, Snoozed view, toolbar colour, swipe-left integration). `inboxly-snooze` depends on `inboxly-core` and `inboxly-store`. No Iced types leak into `inboxly-snooze`.

**Tech Stack:** Rust, tokio (scheduler + timers), zbus (D-Bus GeoClue2), chrono (time preset resolution), rusqlite (via Store trait)

**Prerequisites:**
- M1 complete — `SnoozeInfo`, `SnoozeUntil`, `ThreadState` types exist in `inboxly-core`
- M3 complete — `thread_state` table in SQLite with `snoozed_until` and `snoozed_location_json` columns; `Store` trait has `get_thread_state()` and `update_thread_state()`
- M19 complete — done/pin actions work, `ThreadState.done` and `ThreadState.pinned` fields are read/written correctly
- M15 complete — Iced shell with nav drawer exists (Snoozed nav item)
- M16 complete — theme system with `toolbar_snoozed` colour (`#ef6c00` light / `#8f4100` dark)
- M20 complete — swipe gesture infrastructure (SwipeContainer) exists; left-swipe slot is wired

---

## Task Overview

| # | Task | Crate | Est. |
|---|------|-------|------|
| 1 | Set up `inboxly-snooze` crate with dependencies | `snooze` | 5 min |
| 2 | Implement `SnoozePreset` enum and time resolution | `snooze` | 15 min |
| 3 | Implement `SnoozeService` with snooze/un-snooze operations | `snooze` | 15 min |
| 4 | Implement background snooze scheduler (time-based) | `snooze` | 20 min |
| 5 | Add `list_snoozed_threads()` to Store trait and implement | `core`, `store` | 15 min |
| 6 | Implement GeoClue2 D-Bus client | `snooze` | 25 min |
| 7 | Implement location-based snooze poller | `snooze` | 20 min |
| 8 | GeoClue2 graceful degradation and availability detection | `snooze` | 15 min |
| 9 | Build `SnoozePicker` widget — preset buttons | `ui` | 25 min |
| 10 | Build `SnoozePicker` widget — custom date/time picker | `ui` | 20 min |
| 11 | Build `SnoozePicker` widget — location picker | `ui` | 20 min |
| 12 | Build Snoozed view with orange toolbar | `ui` | 20 min |
| 13 | Integrate snooze action with swipe-left and hover button | `ui` | 15 min |
| 14 | Wire snooze scheduler into application bootstrap | binary | 10 min |
| 15 | Integration tests — snooze lifecycle end-to-end | `snooze` | 20 min |

---

## Task 1: Set up `inboxly-snooze` crate with dependencies

**Files:**
- Modify: `inboxly-snooze/Cargo.toml`
- Modify: `inboxly-snooze/src/lib.rs`

M1 created the empty crate scaffold. This task adds the actual dependencies and module structure.

- [ ] **Step 1: Update `inboxly-snooze/Cargo.toml`**

```toml
[package]
name = "inboxly-snooze"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "Snooze scheduler and reminder system for Inboxly"

[dependencies]
inboxly-core.workspace = true
inboxly-store.workspace = true

tokio = { version = "1", features = ["rt", "time", "sync", "macros"] }
chrono = { version = "0.4", features = ["serde"] }
zbus = { version = "5", default-features = false, features = ["tokio"] }
tracing = "0.1"
thiserror = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "test-util"] }
```

- [ ] **Step 2: Set up module structure in `inboxly-snooze/src/lib.rs`**

```rust
//! Snooze scheduler and reminder system for Inboxly.
//!
//! Provides time-based and location-based snooze with background scheduling.
//! Location snooze uses GeoClue2 via D-Bus with graceful degradation.

pub mod preset;
pub mod service;
pub mod scheduler;
pub mod geoclue;

pub use preset::SnoozePreset;
pub use service::SnoozeService;
pub use scheduler::SnoozeScheduler;
pub use geoclue::GeoClueClient;
```

- [ ] **Step 3: Create stub files**

Create empty stubs for each module so `cargo check` passes:

- `inboxly-snooze/src/preset.rs` — `pub enum SnoozePreset {}`
- `inboxly-snooze/src/service.rs` — empty
- `inboxly-snooze/src/scheduler.rs` — empty
- `inboxly-snooze/src/geoclue.rs` — empty

- [ ] **Step 4: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-snooze
```

**Commit:** `feat(snooze): set up inboxly-snooze crate with dependencies`

---

## Task 2: Implement `SnoozePreset` enum and time resolution

**Files:**
- Modify: `inboxly-snooze/src/preset.rs`

This implements the six time presets from the spec, with the "Later Today" edge case (4 hours or 6 PM if past 2 PM).

- [ ] **Step 1: Define `SnoozePreset` enum**

```rust
use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};

/// Time-based snooze presets matching Google Inbox's options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnoozePreset {
    /// 4 hours from now, or 6 PM if past 2 PM.
    LaterToday,
    /// 8 AM tomorrow.
    Tomorrow,
    /// 8 AM Saturday.
    ThisWeekend,
    /// 8 AM Monday.
    NextWeek,
    /// 8 AM, 3 months from now.
    Someday,
}
```

- [ ] **Step 2: Implement `resolve()` method**

```rust
impl SnoozePreset {
    /// Resolve a preset to a concrete UTC datetime.
    ///
    /// Resolution uses the local timezone for "8 AM" calculations,
    /// then converts to UTC for storage.
    pub fn resolve(&self) -> DateTime<Utc> {
        self.resolve_from(Local::now())
    }

    /// Resolve relative to a given local time (testable).
    pub fn resolve_from(&self, now: DateTime<Local>) -> DateTime<Utc> {
        let morning = NaiveTime::from_hms_opt(8, 0, 0).unwrap();
        let evening = NaiveTime::from_hms_opt(18, 0, 0).unwrap();
        let cutoff = NaiveTime::from_hms_opt(14, 0, 0).unwrap();

        let result_local = match self {
            SnoozePreset::LaterToday => {
                if now.time() >= cutoff {
                    // Past 2 PM: snooze until 6 PM today
                    now.date_naive()
                        .and_time(evening)
                        .and_local_timezone(now.timezone())
                        .single()
                        .unwrap_or_else(|| now + Duration::hours(4))
                } else {
                    // Before 2 PM: snooze 4 hours
                    now + Duration::hours(4)
                }
            }
            SnoozePreset::Tomorrow => {
                let tomorrow = now.date_naive() + Duration::days(1);
                tomorrow
                    .and_time(morning)
                    .and_local_timezone(now.timezone())
                    .single()
                    .unwrap_or_else(|| now + Duration::hours(24))
            }
            SnoozePreset::ThisWeekend => {
                let days_until_saturday = (Weekday::Sat.num_days_from_monday() as i64
                    - now.weekday().num_days_from_monday() as i64 + 7)
                    % 7;
                // If today is Saturday or Sunday, go to next Saturday
                let days_until_saturday = if days_until_saturday == 0 {
                    7
                } else {
                    days_until_saturday
                };
                let saturday = now.date_naive() + Duration::days(days_until_saturday);
                saturday
                    .and_time(morning)
                    .and_local_timezone(now.timezone())
                    .single()
                    .unwrap_or_else(|| now + Duration::days(days_until_saturday))
            }
            SnoozePreset::NextWeek => {
                let days_until_monday = (Weekday::Mon.num_days_from_monday() as i64
                    - now.weekday().num_days_from_monday() as i64 + 7)
                    % 7;
                // If today is Monday, go to next Monday
                let days_until_monday = if days_until_monday == 0 {
                    7
                } else {
                    days_until_monday
                };
                let monday = now.date_naive() + Duration::days(days_until_monday);
                monday
                    .and_time(morning)
                    .and_local_timezone(now.timezone())
                    .single()
                    .unwrap_or_else(|| now + Duration::days(days_until_monday))
            }
            SnoozePreset::Someday => {
                // 3 months from now at 8 AM
                let future = now.date_naive() + chrono::Months::new(3);
                future
                    .and_time(morning)
                    .and_local_timezone(now.timezone())
                    .single()
                    .unwrap_or_else(|| now + Duration::days(90))
            }
        };

        result_local.with_timezone(&Utc)
    }

    /// Human-readable label for UI display.
    pub fn label(&self) -> &'static str {
        match self {
            SnoozePreset::LaterToday => "Later Today",
            SnoozePreset::Tomorrow => "Tomorrow",
            SnoozePreset::ThisWeekend => "This Weekend",
            SnoozePreset::NextWeek => "Next Week",
            SnoozePreset::Someday => "Someday",
        }
    }

    /// Description of when the snooze will fire (for UI subtitle).
    pub fn description(&self) -> String {
        let resolved = self.resolve();
        let local: DateTime<Local> = resolved.into();
        local.format("%a, %b %-d, %-I:%M %p").to_string()
    }

    /// All presets in display order.
    pub fn all() -> &'static [SnoozePreset] {
        &[
            SnoozePreset::LaterToday,
            SnoozePreset::Tomorrow,
            SnoozePreset::ThisWeekend,
            SnoozePreset::NextWeek,
            SnoozePreset::Someday,
        ]
    }
}
```

- [ ] **Step 3: Add `TimeOfDay` enum for custom snooze**

The spec says custom snooze uses "Morning/Afternoon/Evening/Night" time-of-day selectors.

```rust
/// Time-of-day options for custom snooze date picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeOfDay {
    /// 8:00 AM
    Morning,
    /// 1:00 PM
    Afternoon,
    /// 6:00 PM
    Evening,
    /// 9:00 PM
    Night,
}

impl TimeOfDay {
    pub fn to_naive_time(&self) -> NaiveTime {
        match self {
            TimeOfDay::Morning => NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
            TimeOfDay::Afternoon => NaiveTime::from_hms_opt(13, 0, 0).unwrap(),
            TimeOfDay::Evening => NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
            TimeOfDay::Night => NaiveTime::from_hms_opt(21, 0, 0).unwrap(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TimeOfDay::Morning => "Morning",
            TimeOfDay::Afternoon => "Afternoon",
            TimeOfDay::Evening => "Evening",
            TimeOfDay::Night => "Night",
        }
    }

    pub fn all() -> &'static [TimeOfDay] {
        &[TimeOfDay::Morning, TimeOfDay::Afternoon, TimeOfDay::Evening, TimeOfDay::Night]
    }
}

/// Resolve a custom date + time-of-day to a UTC datetime.
pub fn resolve_custom(
    date: chrono::NaiveDate,
    time_of_day: TimeOfDay,
) -> DateTime<Utc> {
    let local_time = date
        .and_time(time_of_day.to_naive_time())
        .and_local_timezone(Local)
        .single()
        .unwrap_or_else(|| {
            // Fallback for ambiguous times (DST)
            Local::now() + Duration::days(1)
        });
    local_time.with_timezone(&Utc)
}
```

- [ ] **Step 4: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn later_today_before_2pm_adds_4_hours() {
        // 10 AM on a Wednesday
        let now = Local.with_ymd_and_hms(2026, 3, 18, 10, 0, 0).unwrap();
        let result = SnoozePreset::LaterToday.resolve_from(now);
        let expected = now + Duration::hours(4);
        assert_eq!(result, expected.with_timezone(&Utc));
    }

    #[test]
    fn later_today_after_2pm_snaps_to_6pm() {
        // 3 PM on a Wednesday
        let now = Local.with_ymd_and_hms(2026, 3, 18, 15, 0, 0).unwrap();
        let result = SnoozePreset::LaterToday.resolve_from(now);
        let expected = Local.with_ymd_and_hms(2026, 3, 18, 18, 0, 0).unwrap();
        assert_eq!(result, expected.with_timezone(&Utc));
    }

    #[test]
    fn tomorrow_resolves_to_8am() {
        let now = Local.with_ymd_and_hms(2026, 3, 18, 22, 0, 0).unwrap();
        let result = SnoozePreset::Tomorrow.resolve_from(now);
        let expected = Local.with_ymd_and_hms(2026, 3, 19, 8, 0, 0).unwrap();
        assert_eq!(result, expected.with_timezone(&Utc));
    }

    #[test]
    fn this_weekend_from_wednesday_goes_to_saturday() {
        // Wednesday March 18, 2026
        let now = Local.with_ymd_and_hms(2026, 3, 18, 10, 0, 0).unwrap();
        let result = SnoozePreset::ThisWeekend.resolve_from(now);
        let expected = Local.with_ymd_and_hms(2026, 3, 21, 8, 0, 0).unwrap();
        assert_eq!(result, expected.with_timezone(&Utc));
    }

    #[test]
    fn this_weekend_from_saturday_goes_to_next_saturday() {
        // Saturday March 21, 2026
        let now = Local.with_ymd_and_hms(2026, 3, 21, 10, 0, 0).unwrap();
        let result = SnoozePreset::ThisWeekend.resolve_from(now);
        let expected = Local.with_ymd_and_hms(2026, 3, 28, 8, 0, 0).unwrap();
        assert_eq!(result, expected.with_timezone(&Utc));
    }

    #[test]
    fn next_week_from_wednesday_goes_to_monday() {
        // Wednesday March 18, 2026
        let now = Local.with_ymd_and_hms(2026, 3, 18, 10, 0, 0).unwrap();
        let result = SnoozePreset::NextWeek.resolve_from(now);
        let expected = Local.with_ymd_and_hms(2026, 3, 23, 8, 0, 0).unwrap();
        assert_eq!(result, expected.with_timezone(&Utc));
    }

    #[test]
    fn next_week_from_monday_goes_to_next_monday() {
        // Monday March 16, 2026
        let now = Local.with_ymd_and_hms(2026, 3, 16, 10, 0, 0).unwrap();
        let result = SnoozePreset::NextWeek.resolve_from(now);
        let expected = Local.with_ymd_and_hms(2026, 3, 23, 8, 0, 0).unwrap();
        assert_eq!(result, expected.with_timezone(&Utc));
    }

    #[test]
    fn someday_resolves_to_3_months() {
        let now = Local.with_ymd_and_hms(2026, 3, 18, 10, 0, 0).unwrap();
        let result = SnoozePreset::Someday.resolve_from(now);
        let expected = Local.with_ymd_and_hms(2026, 6, 18, 8, 0, 0).unwrap();
        assert_eq!(result, expected.with_timezone(&Utc));
    }

    #[test]
    fn time_of_day_values() {
        assert_eq!(TimeOfDay::Morning.to_naive_time().hour(), 8);
        assert_eq!(TimeOfDay::Afternoon.to_naive_time().hour(), 13);
        assert_eq!(TimeOfDay::Evening.to_naive_time().hour(), 18);
        assert_eq!(TimeOfDay::Night.to_naive_time().hour(), 21);
    }

    #[test]
    fn custom_resolve_date_and_time() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let result = resolve_custom(date, TimeOfDay::Morning);
        // Should be April 15, 2026 8:00 AM local, converted to UTC
        assert!(result > Utc::now());
    }

    #[test]
    fn all_presets_returns_five() {
        assert_eq!(SnoozePreset::all().len(), 5);
    }

    #[test]
    fn labels_are_nonempty() {
        for preset in SnoozePreset::all() {
            assert!(!preset.label().is_empty());
        }
    }
}
```

- [ ] **Step 5: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-snooze -- preset
```

**Commit:** `feat(snooze): implement SnoozePreset enum with time resolution`

---

## Task 3: Implement `SnoozeService` with snooze/un-snooze operations

**Files:**
- Modify: `inboxly-snooze/src/service.rs`

The `SnoozeService` is the public API for snoozing/un-snoozing threads. It reads and writes `ThreadState` via the `Store` trait, and emits events via a channel for the UI to react.

- [ ] **Step 1: Define the `SnoozeEvent` enum**

```rust
use chrono::{DateTime, Utc};
use inboxly_core::{ThreadId, SnoozeInfo, SnoozeUntil};

/// Events emitted by the snooze system for the UI.
#[derive(Debug, Clone)]
pub enum SnoozeEvent {
    /// A thread was snoozed. UI should remove it from inbox.
    Snoozed {
        thread_id: ThreadId,
        info: SnoozeInfo,
    },
    /// A thread's snooze has expired. UI should return it to inbox.
    UnSnoozed {
        thread_id: ThreadId,
        original_date: DateTime<Utc>,
    },
    /// GeoClue2 availability changed.
    LocationAvailabilityChanged {
        available: bool,
    },
}
```

- [ ] **Step 2: Define `SnoozeService` struct**

```rust
use std::sync::Arc;
use tokio::sync::mpsc;
use inboxly_core::Store;

/// Service for snoozing and un-snoozing threads.
///
/// Wraps Store operations with event emission.
/// Does NOT own the scheduler — that's a separate background task.
pub struct SnoozeService<S: Store> {
    store: Arc<S>,
    event_tx: mpsc::UnboundedSender<SnoozeEvent>,
}

impl<S: Store> SnoozeService<S> {
    pub fn new(store: Arc<S>, event_tx: mpsc::UnboundedSender<SnoozeEvent>) -> Self {
        Self { store, event_tx }
    }

    /// Snooze a thread until a specific time.
    pub async fn snooze_until_time(
        &self,
        thread_id: &ThreadId,
        until: DateTime<Utc>,
    ) -> inboxly_core::Result<()> {
        let mut state = self.store.get_thread_state(thread_id).await?;
        let info = SnoozeInfo {
            until: SnoozeUntil::Time(until),
            original_date: Utc::now(),
        };
        state.snoozed = Some(info.clone());
        state.done = false; // Un-done if it was archived
        self.store.update_thread_state(&state).await?;
        let _ = self.event_tx.send(SnoozeEvent::Snoozed {
            thread_id: thread_id.clone(),
            info,
        });
        Ok(())
    }

    /// Snooze a thread using a preset.
    pub async fn snooze_preset(
        &self,
        thread_id: &ThreadId,
        preset: crate::SnoozePreset,
    ) -> inboxly_core::Result<()> {
        let until = preset.resolve();
        self.snooze_until_time(thread_id, until).await
    }

    /// Snooze a thread until arriving at a location.
    pub async fn snooze_until_location(
        &self,
        thread_id: &ThreadId,
        lat: f64,
        lng: f64,
        radius_m: f64,
        label: String,
    ) -> inboxly_core::Result<()> {
        let mut state = self.store.get_thread_state(thread_id).await?;
        let info = SnoozeInfo {
            until: SnoozeUntil::Location {
                lat,
                lng,
                radius_m,
                label,
            },
            original_date: Utc::now(),
        };
        state.snoozed = Some(info.clone());
        state.done = false;
        self.store.update_thread_state(&state).await?;
        let _ = self.event_tx.send(SnoozeEvent::Snoozed {
            thread_id: thread_id.clone(),
            info,
        });
        Ok(())
    }

    /// Un-snooze a thread — move it back to the inbox.
    pub async fn unsnooze(
        &self,
        thread_id: &ThreadId,
    ) -> inboxly_core::Result<()> {
        let mut state = self.store.get_thread_state(thread_id).await?;
        let original_date = state
            .snoozed
            .as_ref()
            .map(|s| s.original_date)
            .unwrap_or_else(Utc::now);
        state.snoozed = None;
        state.done = false;
        self.store.update_thread_state(&state).await?;
        let _ = self.event_tx.send(SnoozeEvent::UnSnoozed {
            thread_id: thread_id.clone(),
            original_date,
        });
        Ok(())
    }

    /// Convert a location-snoozed thread to a time-based snooze.
    /// Used when GeoClue2 becomes unavailable.
    pub async fn convert_location_to_time(
        &self,
        thread_id: &ThreadId,
        new_until: DateTime<Utc>,
    ) -> inboxly_core::Result<()> {
        let mut state = self.store.get_thread_state(thread_id).await?;
        if let Some(ref mut info) = state.snoozed {
            info.until = SnoozeUntil::Time(new_until);
            self.store.update_thread_state(&state).await?;
        }
        Ok(())
    }

    /// Check if a thread is currently snoozed.
    pub async fn is_snoozed(
        &self,
        thread_id: &ThreadId,
    ) -> inboxly_core::Result<bool> {
        let state = self.store.get_thread_state(thread_id).await?;
        Ok(state.snoozed.is_some())
    }
}
```

- [ ] **Step 3: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // Tests use a mock Store implementation
    // (defined in a test_support module or using mockall)

    #[tokio::test]
    async fn snooze_preset_sets_thread_state() {
        // Setup mock store, call snooze_preset, verify state
    }

    #[tokio::test]
    async fn unsnooze_clears_snooze_and_emits_event() {
        // Setup snoozed thread, call unsnooze, verify cleared + event sent
    }

    #[tokio::test]
    async fn snooze_location_sets_geofence() {
        // Call snooze_until_location, verify state has Location variant
    }

    #[tokio::test]
    async fn convert_location_to_time_preserves_original_date() {
        // Setup location snooze, convert, verify original_date unchanged
    }
}
```

Note: Tests require a mock `Store`. The recommended approach is to create a `test_support` module in `inboxly-snooze` with an `InMemoryStore` that implements the `Store` trait using `HashMap`s, or use the `mockall` crate. The implementer should choose the simplest approach — if `Store` is an `async_trait` with `dyn` dispatch, `mockall` works; if it uses RPITIT (return position impl trait), an in-memory implementation is needed.

- [ ] **Step 4: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-snooze -- service
```

**Commit:** `feat(snooze): implement SnoozeService with snooze/un-snooze operations`

---

## Task 4: Implement background snooze scheduler (time-based)

**Files:**
- Modify: `inboxly-snooze/src/scheduler.rs`

Background tokio task that checks snoozed items every 60 seconds and un-snoozes expired ones.

- [ ] **Step 1: Define `SnoozeScheduler`**

```rust
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn};

use inboxly_core::{SnoozeUntil, Store};
use crate::service::{SnoozeEvent, SnoozeService};

/// Background scheduler that checks snoozed items and un-snoozes expired ones.
pub struct SnoozeScheduler<S: Store> {
    service: Arc<SnoozeService<S>>,
    store: Arc<S>,
    /// How often to check (default: 60 seconds).
    check_interval: Duration,
}

impl<S: Store + 'static> SnoozeScheduler<S> {
    pub fn new(service: Arc<SnoozeService<S>>, store: Arc<S>) -> Self {
        Self {
            service,
            store,
            check_interval: Duration::from_secs(60),
        }
    }

    /// Override the check interval (for testing).
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.check_interval = interval;
        self
    }

    /// Start the scheduler loop. Returns a JoinHandle.
    ///
    /// Runs until the provided cancellation token is triggered.
    pub fn spawn(
        self,
        cancel: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run(cancel).await;
        })
    }

    async fn run(
        self,
        mut cancel: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut ticker = interval(self.check_interval);
        info!(
            interval_secs = self.check_interval.as_secs(),
            "snooze scheduler started"
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = self.check_expired().await {
                        warn!(error = %e, "snooze scheduler check failed");
                    }
                }
                _ = cancel.changed() => {
                    if *cancel.borrow() {
                        info!("snooze scheduler shutting down");
                        break;
                    }
                }
            }
        }
    }

    /// Check all time-snoozed threads and un-snooze any that have expired.
    async fn check_expired(&self) -> inboxly_core::Result<()> {
        let snoozed = self.store.list_snoozed_threads().await?;
        let now = chrono::Utc::now();
        let mut unsnoozed_count = 0u32;

        for state in &snoozed {
            if let Some(ref info) = state.snoozed {
                match &info.until {
                    SnoozeUntil::Time(until) => {
                        if *until <= now {
                            debug!(
                                thread_id = %state.thread_id,
                                "un-snoozing expired thread"
                            );
                            self.service.unsnooze(&state.thread_id).await?;
                            unsnoozed_count += 1;
                        }
                    }
                    SnoozeUntil::Location { .. } => {
                        // Location-based snoozes handled by the location poller
                    }
                }
            }
        }

        if unsnoozed_count > 0 {
            info!(count = unsnoozed_count, "un-snoozed expired threads");
        }

        Ok(())
    }
}
```

- [ ] **Step 2: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn scheduler_unsnoozes_expired_thread() {
        // 1. Create in-memory store with a thread snoozed until 30 seconds ago
        // 2. Start scheduler with 1-second interval
        // 3. Advance time 2 seconds (tokio::time::advance)
        // 4. Verify the thread's snooze was cleared
        // 5. Verify an UnSnoozed event was received on the channel
    }

    #[tokio::test(start_paused = true)]
    async fn scheduler_ignores_future_snooze() {
        // 1. Create thread snoozed until 1 hour from now
        // 2. Run one check cycle
        // 3. Verify thread is still snoozed
    }

    #[tokio::test(start_paused = true)]
    async fn scheduler_ignores_location_snooze() {
        // 1. Create thread with location snooze
        // 2. Run one check cycle
        // 3. Verify thread is still snoozed (location handled by poller)
    }

    #[tokio::test]
    async fn scheduler_stops_on_cancel() {
        // 1. Start scheduler
        // 2. Send cancel signal
        // 3. Verify task completes
    }
}
```

- [ ] **Step 3: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-snooze -- scheduler
```

**Commit:** `feat(snooze): implement background snooze scheduler with 60-second check`

---

## Task 5: Add `list_snoozed_threads()` to Store trait and implement

**Files:**
- Modify: `inboxly-core/src/traits.rs`
- Modify: `inboxly-store/src/...` (wherever Store is implemented — likely `inboxly-store/src/sqlite.rs` or similar)

The scheduler needs to query all currently-snoozed threads efficiently.

- [ ] **Step 1: Add `list_snoozed_threads()` to the `Store` trait**

In `inboxly-core/src/traits.rs`, add to the `Store` trait:

```rust
    /// List all threads that are currently snoozed (have a non-NULL snoozed state).
    fn list_snoozed_threads(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<ThreadState>>> + Send;

    /// List threads snoozed until a location (for the location poller).
    fn list_location_snoozed_threads(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<ThreadState>>> + Send;
```

- [ ] **Step 2: Implement in `inboxly-store`**

The SQLite query for `list_snoozed_threads`:

```sql
SELECT thread_id, pinned, done, snoozed_until, snoozed_location_json, bundle_id
FROM thread_state
WHERE snoozed_until IS NOT NULL OR snoozed_location_json IS NOT NULL
```

The query for `list_location_snoozed_threads`:

```sql
SELECT thread_id, pinned, done, snoozed_until, snoozed_location_json, bundle_id
FROM thread_state
WHERE snoozed_location_json IS NOT NULL
```

The implementer should map `snoozed_until` (integer epoch) to `SnoozeUntil::Time` and `snoozed_location_json` (JSON text) to `SnoozeUntil::Location`, constructing the `SnoozeInfo` with the `original_date` stored alongside.

**Note:** The M3 schema has `snoozed_until` (INTEGER, nullable) and `snoozed_location_json` (TEXT, nullable) in `thread_state`. The service needs an additional column for `original_date` — if M3 doesn't include this, add a migration:

```sql
ALTER TABLE thread_state ADD COLUMN snoozed_original_date INTEGER;
```

- [ ] **Step 3: Write store-level tests**

```rust
#[tokio::test]
async fn list_snoozed_threads_returns_snoozed_only() {
    // Insert 3 threads: one snoozed-time, one snoozed-location, one not snoozed
    // Verify list_snoozed_threads returns exactly 2
}

#[tokio::test]
async fn list_location_snoozed_threads_filters_correctly() {
    // Insert time-snoozed and location-snoozed threads
    // Verify only location-snoozed returned
}
```

- [ ] **Step 4: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store -- snoozed
```

**Commit:** `feat(store): add list_snoozed_threads and list_location_snoozed_threads to Store trait`

---

## Task 6: Implement GeoClue2 D-Bus client

**Files:**
- Modify: `inboxly-snooze/src/geoclue.rs`

GeoClue2 provides location via D-Bus. The client talks to `org.freedesktop.GeoClue2` to get lat/lng/accuracy.

- [ ] **Step 1: Define the GeoClue2 D-Bus proxy interfaces**

Use `zbus` proxy macros to define the D-Bus interfaces.

```rust
use zbus::{Connection, Result as ZbusResult};
use zbus::proxy;

/// GeoClue2 Manager — creates client sessions.
#[proxy(
    interface = "org.freedesktop.GeoClue2.Manager",
    default_service = "org.freedesktop.GeoClue2",
    default_path = "/org/freedesktop/GeoClue2/Manager"
)]
trait GeoClue2Manager {
    /// Create a new client session. Returns the object path of the client.
    fn get_client(&self) -> ZbusResult<zbus::zvariant::OwnedObjectPath>;
}

/// GeoClue2 Client — configures and starts location updates.
#[proxy(
    interface = "org.freedesktop.GeoClue2.Client",
    default_service = "org.freedesktop.GeoClue2"
)]
trait GeoClue2Client {
    /// Start receiving location updates.
    fn start(&self) -> ZbusResult<()>;
    /// Stop receiving location updates.
    fn stop(&self) -> ZbusResult<()>;

    /// Set the desktop ID (required for GeoClue2 agent authorization).
    #[zbus(property)]
    fn set_desktop_id(&self, id: &str) -> ZbusResult<()>;

    /// Set requested accuracy level.
    /// 0 = None, 1 = Country, 4 = City, 5 = Neighborhood, 6 = Street, 8 = Exact
    #[zbus(property)]
    fn set_requested_accuracy_level(&self, level: u32) -> ZbusResult<()>;

    /// Set the minimum distance threshold for updates (meters).
    #[zbus(property)]
    fn set_distance_threshold(&self, distance: u32) -> ZbusResult<()>;

    /// Get the current location object path.
    #[zbus(property)]
    fn location(&self) -> ZbusResult<zbus::zvariant::OwnedObjectPath>;

    /// Signal emitted when location changes.
    #[zbus(signal)]
    fn location_updated(
        &self,
        old: zbus::zvariant::ObjectPath<'_>,
        new: zbus::zvariant::ObjectPath<'_>,
    ) -> ZbusResult<()>;
}

/// GeoClue2 Location — read lat/lng/accuracy from a location object.
#[proxy(
    interface = "org.freedesktop.GeoClue2.Location",
    default_service = "org.freedesktop.GeoClue2"
)]
trait GeoClue2Location {
    #[zbus(property)]
    fn latitude(&self) -> ZbusResult<f64>;
    #[zbus(property)]
    fn longitude(&self) -> ZbusResult<f64>;
    #[zbus(property)]
    fn accuracy(&self) -> ZbusResult<f64>;
    #[zbus(property)]
    fn description(&self) -> ZbusResult<String>;
}
```

- [ ] **Step 2: Define `GeoClueClient` wrapper**

```rust
use tracing::{debug, info, warn};

/// Current device location.
#[derive(Debug, Clone, Copy)]
pub struct DeviceLocation {
    pub lat: f64,
    pub lng: f64,
    /// Accuracy in meters. Lower = better.
    pub accuracy_m: f64,
}

/// GeoClue2 availability status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoClueStatus {
    /// GeoClue2 is available and location can be queried.
    Available,
    /// GeoClue2 D-Bus service is not installed or not running.
    Unavailable,
    /// Location permission was denied by the user or GeoClue2 agent.
    PermissionDenied,
}

/// High-level GeoClue2 client wrapping D-Bus interaction.
pub struct GeoClueClient {
    status: GeoClueStatus,
    connection: Option<Connection>,
    client_path: Option<zbus::zvariant::OwnedObjectPath>,
}

impl GeoClueClient {
    /// Attempt to connect to GeoClue2. Returns status.
    pub async fn connect() -> Self {
        match Self::try_connect().await {
            Ok(client) => client,
            Err(e) => {
                warn!(error = %e, "GeoClue2 unavailable");
                Self {
                    status: GeoClueStatus::Unavailable,
                    connection: None,
                    client_path: None,
                }
            }
        }
    }

    async fn try_connect() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let connection = Connection::system().await?;

        // Check if GeoClue2 service exists
        let manager = GeoClue2ManagerProxy::new(&connection).await?;
        let client_path = manager.get_client().await?;

        let client = GeoClue2ClientProxy::builder(&connection)
            .path(&client_path)?
            .build()
            .await?;

        // Configure: desktop ID, accuracy level (Street = 6), distance 100m
        client.set_desktop_id("inboxly").await?;
        client.set_requested_accuracy_level(6).await?;
        client.set_distance_threshold(100).await?;

        // Attempt to start. Permission denial shows up here.
        match client.start().await {
            Ok(()) => {
                info!("GeoClue2 client started successfully");
                Ok(Self {
                    status: GeoClueStatus::Available,
                    connection: Some(connection),
                    client_path: Some(client_path),
                })
            }
            Err(e) => {
                warn!(error = %e, "GeoClue2 permission denied or start failed");
                Ok(Self {
                    status: GeoClueStatus::PermissionDenied,
                    connection: Some(connection),
                    client_path: Some(client_path),
                })
            }
        }
    }

    pub fn status(&self) -> GeoClueStatus {
        self.status
    }

    pub fn is_available(&self) -> bool {
        self.status == GeoClueStatus::Available
    }

    /// Get the current device location. Returns None if unavailable.
    pub async fn get_location(&self) -> Option<DeviceLocation> {
        let connection = self.connection.as_ref()?;
        let client_path = self.client_path.as_ref()?;

        let client = GeoClue2ClientProxy::builder(connection)
            .path(client_path.as_ref())
            .ok()?
            .build()
            .await
            .ok()?;

        let location_path = client.location().await.ok()?;

        // "/org/freedesktop/GeoClue2/Location/0" indicates no location yet
        if location_path.as_str().ends_with("/0") {
            debug!("no location available yet");
            return None;
        }

        let location = GeoClue2LocationProxy::builder(connection)
            .path(location_path.as_ref())
            .ok()?
            .build()
            .await
            .ok()?;

        let lat = location.latitude().await.ok()?;
        let lng = location.longitude().await.ok()?;
        let accuracy = location.accuracy().await.ok()?;

        Some(DeviceLocation {
            lat,
            lng,
            accuracy_m: accuracy,
        })
    }

    /// Stop the GeoClue2 client (release resources).
    pub async fn stop(&self) {
        if let (Some(conn), Some(path)) = (&self.connection, &self.client_path) {
            if let Ok(client) = GeoClue2ClientProxy::builder(conn)
                .path(path.as_ref())
                .and_then(|b| Ok(b))
            {
                if let Ok(proxy) = client.build().await {
                    let _ = proxy.stop().await;
                }
            }
        }
    }
}
```

- [ ] **Step 3: Add haversine distance utility**

```rust
/// Calculate distance between two lat/lng points in meters (Haversine formula).
pub fn haversine_distance_m(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let dlat = (lat2 - lat1).to_radians();
    let dlng = (lng2 - lng1).to_radians();

    let a = (dlat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (dlng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_M * c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_same_point_is_zero() {
        let d = haversine_distance_m(43.6532, -79.3832, 43.6532, -79.3832);
        assert!(d < 0.01);
    }

    #[test]
    fn haversine_known_distance() {
        // Toronto (43.6532, -79.3832) to Hamilton (43.2557, -79.8711)
        // Approx 59 km
        let d = haversine_distance_m(43.6532, -79.3832, 43.2557, -79.8711);
        assert!(d > 55_000.0 && d < 65_000.0);
    }

    #[test]
    fn haversine_short_distance() {
        // Two points ~100m apart
        let d = haversine_distance_m(43.6532, -79.3832, 43.6541, -79.3832);
        assert!(d > 50.0 && d < 200.0);
    }
}
```

- [ ] **Step 4: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-snooze -- geoclue
```

**Commit:** `feat(snooze): implement GeoClue2 D-Bus client with haversine distance`

---

## Task 7: Implement location-based snooze poller

**Files:**
- Create: `inboxly-snooze/src/location_poller.rs`
- Modify: `inboxly-snooze/src/lib.rs`

Background task that polls GeoClue2 every 5 minutes when location-snoozed items exist.

- [ ] **Step 1: Define `LocationPoller`**

```rust
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn};

use inboxly_core::{SnoozeUntil, Store};
use crate::geoclue::{DeviceLocation, GeoClueClient, GeoClueStatus, haversine_distance_m};
use crate::service::SnoozeService;

/// Minimum accuracy threshold (meters). If accuracy > this, widen geofence.
const POOR_ACCURACY_THRESHOLD_M: f64 = 500.0;

/// Polls GeoClue2 and checks location-snoozed geofences.
pub struct LocationPoller<S: Store> {
    service: Arc<SnoozeService<S>>,
    store: Arc<S>,
    geoclue: GeoClueClient,
    /// Poll interval (default: 5 minutes).
    poll_interval: Duration,
}

impl<S: Store + 'static> LocationPoller<S> {
    pub fn new(
        service: Arc<SnoozeService<S>>,
        store: Arc<S>,
        geoclue: GeoClueClient,
    ) -> Self {
        Self {
            service,
            store,
            geoclue,
            poll_interval: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Override poll interval (for testing).
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Start the poller. Only polls when location-snoozed items exist.
    pub fn spawn(
        self,
        cancel: watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run(cancel).await;
        })
    }

    async fn run(self, mut cancel: watch::Receiver<bool>) {
        if !self.geoclue.is_available() {
            info!("GeoClue2 unavailable — location poller will not run");
            // Wait for cancel signal then exit
            let _ = cancel.changed().await;
            return;
        }

        let mut ticker = interval(self.poll_interval);
        info!(
            interval_secs = self.poll_interval.as_secs(),
            "location poller started"
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = self.check_geofences().await {
                        warn!(error = %e, "location poller check failed");
                    }
                }
                _ = cancel.changed() => {
                    if *cancel.borrow() {
                        info!("location poller shutting down");
                        self.geoclue.stop().await;
                        break;
                    }
                }
            }
        }
    }

    async fn check_geofences(&self) -> inboxly_core::Result<()> {
        // Only query location if there are location-snoozed items
        let location_snoozed = self.store.list_location_snoozed_threads().await?;
        if location_snoozed.is_empty() {
            debug!("no location-snoozed threads, skipping poll");
            return Ok(());
        }

        let device_loc = match self.geoclue.get_location().await {
            Some(loc) => loc,
            None => {
                debug!("no location fix available");
                return Ok(());
            }
        };

        if device_loc.accuracy_m > POOR_ACCURACY_THRESHOLD_M {
            warn!(
                accuracy_m = device_loc.accuracy_m,
                "poor location accuracy, widening geofence check radius"
            );
        }

        for state in &location_snoozed {
            if let Some(ref info) = state.snoozed {
                if let SnoozeUntil::Location {
                    lat,
                    lng,
                    radius_m,
                    ref label,
                } = info.until
                {
                    let distance = haversine_distance_m(
                        device_loc.lat,
                        device_loc.lng,
                        lat,
                        lng,
                    );

                    // If accuracy is poor, widen the effective radius
                    let effective_radius = if device_loc.accuracy_m > POOR_ACCURACY_THRESHOLD_M {
                        radius_m + device_loc.accuracy_m
                    } else {
                        radius_m
                    };

                    if distance <= effective_radius {
                        info!(
                            thread_id = %state.thread_id,
                            label = label,
                            distance_m = distance,
                            radius_m = effective_radius,
                            "entered geofence — un-snoozing"
                        );
                        self.service.unsnooze(&state.thread_id).await?;
                    }
                }
            }
        }

        Ok(())
    }
}
```

- [ ] **Step 2: Register module in `lib.rs`**

Add to `inboxly-snooze/src/lib.rs`:

```rust
pub mod location_poller;
pub use location_poller::LocationPoller;
```

- [ ] **Step 3: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geofence_inside_radius() {
        // Device at (43.6535, -79.3835), fence at (43.6532, -79.3832), radius 500m
        let distance = haversine_distance_m(43.6535, -79.3835, 43.6532, -79.3832);
        assert!(distance < 500.0);
    }

    #[test]
    fn geofence_outside_radius() {
        // Device far away
        let distance = haversine_distance_m(44.0, -80.0, 43.6532, -79.3832);
        assert!(distance > 500.0);
    }

    #[test]
    fn poor_accuracy_widens_radius() {
        let device = DeviceLocation {
            lat: 43.660,
            lng: -79.390,
            accuracy_m: 800.0, // poor
        };
        let fence_lat = 43.653;
        let fence_lng = -79.383;
        let fence_radius = 200.0;

        let distance = haversine_distance_m(device.lat, device.lng, fence_lat, fence_lng);
        let effective_radius = fence_radius + device.accuracy_m;

        // Without widening, distance > 200m (outside). With widening, distance < 1000m (inside).
        assert!(distance > fence_radius);
        assert!(distance < effective_radius);
    }
}
```

- [ ] **Step 4: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-snooze -- location
```

**Commit:** `feat(snooze): implement location-based snooze poller with geofence checking`

---

## Task 8: GeoClue2 graceful degradation and availability detection

**Files:**
- Create: `inboxly-snooze/src/availability.rs`
- Modify: `inboxly-snooze/src/lib.rs`

Per the spec, four graceful degradation scenarios must be handled.

- [ ] **Step 1: Define `LocationCapability` — checked at startup**

```rust
use crate::geoclue::{GeoClueClient, GeoClueStatus};
use crate::service::SnoozeEvent;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Describes the system's location capability for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationCapability {
    /// GeoClue2 available, permission granted. Show location option in picker.
    FullyAvailable,
    /// GeoClue2 available, but permission not yet requested. Show option with prompt.
    NeedsPermission,
    /// GeoClue2 not installed or D-Bus service not running. Hide location option.
    Unavailable,
}

/// Check location capability at startup.
pub async fn check_location_capability() -> LocationCapability {
    let client = GeoClueClient::connect().await;
    match client.status() {
        GeoClueStatus::Available => {
            info!("location capability: fully available");
            client.stop().await;
            LocationCapability::FullyAvailable
        }
        GeoClueStatus::PermissionDenied => {
            info!("location capability: needs permission");
            LocationCapability::NeedsPermission
        }
        GeoClueStatus::Unavailable => {
            info!("location capability: unavailable (GeoClue2 not found)");
            LocationCapability::Unavailable
        }
    }
}
```

- [ ] **Step 2: Handle existing location snoozes when GeoClue2 is unavailable**

```rust
use std::sync::Arc;
use inboxly_core::Store;

/// Handle existing location-snoozed items when GeoClue2 is unavailable.
///
/// Per spec: items remain snoozed, shown in Snoozed view with
/// "(location unavailable)" badge. User can manually un-snooze
/// or convert to time-based.
pub async fn mark_location_snoozes_degraded<S: Store>(
    store: &Arc<S>,
    event_tx: &mpsc::UnboundedSender<SnoozeEvent>,
) {
    match store.list_location_snoozed_threads().await {
        Ok(threads) => {
            if !threads.is_empty() {
                warn!(
                    count = threads.len(),
                    "GeoClue2 unavailable with {} location-snoozed threads — \
                     they will show '(location unavailable)' badge",
                    threads.len()
                );
                let _ = event_tx.send(SnoozeEvent::LocationAvailabilityChanged {
                    available: false,
                });
            }
        }
        Err(e) => {
            warn!(error = %e, "failed to check location-snoozed threads");
        }
    }
}
```

- [ ] **Step 3: Register module in `lib.rs`**

```rust
pub mod availability;
pub use availability::{LocationCapability, check_location_capability};
```

- [ ] **Step 4: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_variants() {
        // Smoke test: all variants are distinct
        assert_ne!(LocationCapability::FullyAvailable, LocationCapability::Unavailable);
        assert_ne!(LocationCapability::NeedsPermission, LocationCapability::Unavailable);
        assert_ne!(LocationCapability::FullyAvailable, LocationCapability::NeedsPermission);
    }
}
```

- [ ] **Step 5: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-snooze -- availability
```

**Commit:** `feat(snooze): implement GeoClue2 graceful degradation and availability detection`

---

## Task 9: Build `SnoozePicker` widget — preset buttons

**Files:**
- Create: `inboxly-ui/src/widgets/snooze_picker.rs`
- Modify: `inboxly-ui/src/widgets/mod.rs`

The SnoozePicker is a 2-column grid dialog, 288dp wide. Each preset is a cell (142dp x 122dp) with an icon + label + subtitle.

- [ ] **Step 1: Define `SnoozePicker` state and messages**

```rust
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length, Padding};

use inboxly_snooze::{SnoozePreset, LocationCapability};

/// Messages emitted by the SnoozePicker widget.
#[derive(Debug, Clone)]
pub enum SnoozePickerMessage {
    /// User selected a time preset.
    PresetSelected(SnoozePreset),
    /// User clicked "Pick date & time" for custom snooze.
    CustomTimeClicked,
    /// User clicked "Pick place" for location snooze.
    PickLocationClicked,
    /// User cancelled / closed the picker.
    Dismissed,
}

/// Visibility and configuration for the SnoozePicker.
pub struct SnoozePickerState {
    /// Whether the picker is currently visible.
    pub visible: bool,
    /// Whether to show the location option.
    pub location_capability: LocationCapability,
    /// Whether custom time sub-picker is open.
    pub custom_time_open: bool,
    /// Whether location sub-picker is open.
    pub location_picker_open: bool,
}

impl Default for SnoozePickerState {
    fn default() -> Self {
        Self {
            visible: false,
            location_capability: LocationCapability::Unavailable,
            custom_time_open: false,
            location_picker_open: false,
        }
    }
}
```

- [ ] **Step 2: Implement the preset grid view**

```rust
impl SnoozePickerState {
    /// Build the 2-column preset grid.
    ///
    /// Layout (288dp total, 2 columns of 142dp + 4dp gap):
    /// ┌──────────────┬──────────────┐
    /// │ Later Today  │  Tomorrow    │
    /// │ (time desc)  │  8:00 AM     │
    /// ├──────────────┼──────────────┤
    /// │ This Weekend │  Next Week   │
    /// │ Sat 8:00 AM  │  Mon 8:00 AM │
    /// ├──────────────┼──────────────┤
    /// │ Someday      │  Pick date   │
    /// │ 3 months     │  & time      │
    /// ├──────────────┴──────────────┤
    /// │ Pick place (if available)   │
    /// └─────────────────────────────┘
    pub fn view(&self) -> Element<SnoozePickerMessage> {
        if !self.visible {
            return Space::new(0, 0).into();
        }

        let presets = SnoozePreset::all();

        // Build 2-column rows of preset buttons
        let mut rows: Vec<Element<SnoozePickerMessage>> = Vec::new();

        // Row 1: Later Today | Tomorrow
        rows.push(self.preset_row(presets[0], presets[1]));
        // Row 2: This Weekend | Next Week
        rows.push(self.preset_row(presets[2], presets[3]));
        // Row 3: Someday | Pick date & time
        rows.push(self.preset_and_custom_row(presets[4]));

        // Row 4: Pick place (conditional on location capability)
        if self.location_capability != LocationCapability::Unavailable {
            rows.push(self.location_row());
        }

        let grid = column(rows)
            .spacing(4)
            .padding(Padding::from(8));

        // Wrap in container with fixed width and background
        container(grid)
            .width(Length::Fixed(288.0))
            .padding(Padding::from(8))
            // .style(theme::snooze_picker_container)
            .into()
    }

    fn preset_button(
        &self,
        preset: SnoozePreset,
    ) -> Element<SnoozePickerMessage> {
        let label = text(preset.label()).size(14);
        let description = text(preset.description()).size(12);
        // Icon would be added via theme/icon system

        let content = column![label, description]
            .spacing(4)
            .align_x(Alignment::Center)
            .padding(Padding::from(8));

        button(content)
            .on_press(SnoozePickerMessage::PresetSelected(preset))
            .width(Length::Fixed(142.0))
            .height(Length::Fixed(122.0))
            // .style(theme::snooze_preset_button)
            .into()
    }

    fn preset_row(
        &self,
        left: SnoozePreset,
        right: SnoozePreset,
    ) -> Element<SnoozePickerMessage> {
        row![
            self.preset_button(left),
            self.preset_button(right),
        ]
        .spacing(4)
        .into()
    }

    fn preset_and_custom_row(
        &self,
        left_preset: SnoozePreset,
    ) -> Element<SnoozePickerMessage> {
        let custom_label = text("Pick date").size(14);
        let custom_desc = text("& time").size(12);
        let custom_content = column![custom_label, custom_desc]
            .spacing(4)
            .align_x(Alignment::Center)
            .padding(Padding::from(8));

        let custom_button = button(custom_content)
            .on_press(SnoozePickerMessage::CustomTimeClicked)
            .width(Length::Fixed(142.0))
            .height(Length::Fixed(122.0));

        row![
            self.preset_button(left_preset),
            custom_button,
        ]
        .spacing(4)
        .into()
    }

    fn location_row(&self) -> Element<SnoozePickerMessage> {
        let label = text("Pick place").size(14);
        let content = column![label]
            .align_x(Alignment::Center)
            .padding(Padding::from(12));

        let btn = button(content)
            .on_press(SnoozePickerMessage::PickLocationClicked)
            .width(Length::Fill)
            .height(Length::Fixed(48.0));

        container(btn)
            .width(Length::Fill)
            .into()
    }
}
```

- [ ] **Step 3: Register widget in `inboxly-ui/src/widgets/mod.rs`**

```rust
pub mod snooze_picker;
pub use snooze_picker::{SnoozePickerMessage, SnoozePickerState};
```

- [ ] **Step 4: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): build SnoozePicker widget with preset buttons grid`

---

## Task 10: Build `SnoozePicker` widget — custom date/time picker

**Files:**
- Modify: `inboxly-ui/src/widgets/snooze_picker.rs`

Add the custom date + time-of-day sub-picker that appears when "Pick date & time" is clicked.

- [ ] **Step 1: Extend `SnoozePickerMessage` with custom time messages**

```rust
// Add to SnoozePickerMessage:
/// User selected a custom date.
CustomDateSelected(chrono::NaiveDate),
/// User selected a time-of-day for the custom snooze.
CustomTimeOfDaySelected(inboxly_snooze::preset::TimeOfDay),
/// User confirmed the custom snooze.
CustomSnoozeConfirmed {
    date: chrono::NaiveDate,
    time_of_day: inboxly_snooze::preset::TimeOfDay,
},
/// User went back from custom picker to preset grid.
BackToPresets,
```

- [ ] **Step 2: Add custom picker state to `SnoozePickerState`**

```rust
// Add fields:
pub selected_date: Option<chrono::NaiveDate>,
pub selected_time_of_day: Option<inboxly_snooze::preset::TimeOfDay>,
```

- [ ] **Step 3: Implement custom date/time view**

Build a simple date picker UI:
- Month/year header with prev/next arrows
- 7-column day grid (Mon-Sun)
- 4 time-of-day buttons below (Morning, Afternoon, Evening, Night)
- "Snooze" confirm button at bottom

```rust
impl SnoozePickerState {
    fn custom_time_view(&self) -> Element<SnoozePickerMessage> {
        // Back button
        let back = button(text("← Back").size(14))
            .on_press(SnoozePickerMessage::BackToPresets);

        // Simple date selector using Iced's built-in widgets
        // Show current month calendar grid
        let today = chrono::Local::now().date_naive();
        let calendar = self.build_calendar_grid(today);

        // Time-of-day selector
        let time_buttons = row(
            inboxly_snooze::preset::TimeOfDay::all()
                .iter()
                .map(|tod| {
                    let is_selected = self.selected_time_of_day == Some(*tod);
                    button(text(tod.label()).size(12))
                        .on_press(SnoozePickerMessage::CustomTimeOfDaySelected(*tod))
                        // Style based on is_selected
                        .into()
                })
                .collect(),
        )
        .spacing(4);

        // Confirm button (enabled when both date and time are selected)
        let confirm_enabled = self.selected_date.is_some()
            && self.selected_time_of_day.is_some();
        let mut confirm = button(text("Snooze").size(14));
        if confirm_enabled {
            if let (Some(date), Some(tod)) = (self.selected_date, self.selected_time_of_day) {
                confirm = confirm.on_press(
                    SnoozePickerMessage::CustomSnoozeConfirmed {
                        date,
                        time_of_day: tod,
                    },
                );
            }
        }

        column![back, calendar, time_buttons, confirm]
            .spacing(8)
            .padding(Padding::from(8))
            .into()
    }

    fn build_calendar_grid(
        &self,
        reference_date: chrono::NaiveDate,
    ) -> Element<SnoozePickerMessage> {
        // Build a simple monthly calendar grid
        // Implementation uses chrono to enumerate days of the month
        // Each day is a small button that emits CustomDateSelected
        // Selected date is highlighted
        // Past dates are disabled

        // ... (detailed calendar grid implementation)
        // This is the most complex sub-widget — the implementer
        // should build a reusable calendar_grid widget or function.

        Space::new(0, 0).into() // Placeholder — replace with actual grid
    }
}
```

- [ ] **Step 4: Update `view()` to switch between preset grid and custom picker**

```rust
pub fn view(&self) -> Element<SnoozePickerMessage> {
    if !self.visible {
        return Space::new(0, 0).into();
    }

    if self.custom_time_open {
        return self.custom_time_view();
    }

    if self.location_picker_open {
        return self.location_picker_view();
    }

    // Default: preset grid (existing code from Task 9)
    self.preset_grid_view()
}
```

- [ ] **Step 5: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add custom date/time picker to SnoozePicker`

---

## Task 11: Build `SnoozePicker` widget — location picker

**Files:**
- Modify: `inboxly-ui/src/widgets/snooze_picker.rs`

Add the location picker sub-view. Since this is a desktop app without a map widget, the location picker uses a text input for the place label and numeric inputs for lat/lng/radius, with a "Use current location" button.

- [ ] **Step 1: Extend `SnoozePickerMessage` with location messages**

```rust
// Add to SnoozePickerMessage:
/// Location label text changed.
LocationLabelChanged(String),
/// Latitude input changed.
LocationLatChanged(String),
/// Longitude input changed.
LocationLngChanged(String),
/// Radius input changed (meters).
LocationRadiusChanged(String),
/// User clicked "Use current location" to populate lat/lng.
UseCurrentLocation,
/// Current location was retrieved from GeoClue2.
CurrentLocationReceived { lat: f64, lng: f64 },
/// User confirmed the location snooze.
LocationSnoozeConfirmed {
    lat: f64,
    lng: f64,
    radius_m: f64,
    label: String,
},
```

- [ ] **Step 2: Add location picker state**

```rust
// Add to SnoozePickerState:
pub location_label: String,
pub location_lat: String,
pub location_lng: String,
pub location_radius: String,  // default "200"
```

- [ ] **Step 3: Implement location picker view**

```rust
impl SnoozePickerState {
    fn location_picker_view(&self) -> Element<SnoozePickerMessage> {
        let back = button(text("← Back").size(14))
            .on_press(SnoozePickerMessage::BackToPresets);

        let title = text("Snooze until you arrive at:").size(16);

        // Label input
        let label_input = text_input("Place name (e.g., Office)", &self.location_label)
            .on_input(SnoozePickerMessage::LocationLabelChanged)
            .size(14);

        // Use current location button
        let use_current = button(text("📍 Use current location").size(12))
            .on_press(SnoozePickerMessage::UseCurrentLocation);

        // Lat/lng inputs (pre-filled if "Use current" was pressed)
        let lat_input = text_input("Latitude", &self.location_lat)
            .on_input(SnoozePickerMessage::LocationLatChanged)
            .size(14);

        let lng_input = text_input("Longitude", &self.location_lng)
            .on_input(SnoozePickerMessage::LocationLngChanged)
            .size(14);

        let radius_input = text_input("Radius (m)", &self.location_radius)
            .on_input(SnoozePickerMessage::LocationRadiusChanged)
            .size(14);

        // Confirm button (validate inputs)
        let valid = self.validate_location_inputs();
        let mut confirm = button(text("Snooze at location").size(14))
            .width(Length::Fill);
        if let Some((lat, lng, radius, label)) = valid {
            confirm = confirm.on_press(SnoozePickerMessage::LocationSnoozeConfirmed {
                lat,
                lng,
                radius_m: radius,
                label,
            });
        }

        column![
            back,
            title,
            label_input,
            use_current,
            row![lat_input, lng_input].spacing(4),
            radius_input,
            confirm,
        ]
        .spacing(8)
        .padding(Padding::from(8))
        .into()
    }

    fn validate_location_inputs(&self) -> Option<(f64, f64, f64, String)> {
        let lat: f64 = self.location_lat.parse().ok()?;
        let lng: f64 = self.location_lng.parse().ok()?;
        let radius: f64 = self.location_radius.parse().ok()?;
        if self.location_label.is_empty() {
            return None;
        }
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lng) {
            return None;
        }
        if radius <= 0.0 || radius > 50_000.0 {
            return None;
        }
        Some((lat, lng, radius, self.location_label.clone()))
    }
}
```

- [ ] **Step 4: Show permission prompt for `NeedsPermission` status**

If `location_capability == LocationCapability::NeedsPermission`, the location picker view shows an info banner before the inputs:

```rust
if self.location_capability == LocationCapability::NeedsPermission {
    let prompt = container(
        text("Inboxly needs location access to snooze until you arrive somewhere. \
              Grant permission in your system settings.")
            .size(12)
    )
    .padding(Padding::from(8));
    // .style(theme::info_banner)

    // Add prompt before the inputs
}
```

- [ ] **Step 5: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add location picker to SnoozePicker with validation`

---

## Task 12: Build Snoozed view with orange toolbar

**Files:**
- Create: `inboxly-ui/src/views/snoozed.rs`
- Modify: `inboxly-ui/src/views/mod.rs`
- Modify: `inboxly-ui/src/app.rs` (or main app file — wherever view switching lives)

The Snoozed view shows snoozed items with their return dates and an orange toolbar.

- [ ] **Step 1: Define the Snoozed view**

```rust
use chrono::{DateTime, Local, Utc};
use iced::widget::{column, container, row, scrollable, text, Space};
use iced::{Element, Length, Padding};

use inboxly_core::{SnoozeInfo, SnoozeUntil, ThreadId, ThreadState};

/// Messages from the Snoozed view.
#[derive(Debug, Clone)]
pub enum SnoozedViewMessage {
    /// Un-snooze a thread immediately.
    UnSnooze(ThreadId),
    /// Open a snoozed thread to read it.
    OpenThread(ThreadId),
    /// Convert a location snooze to time-based (when GeoClue2 unavailable).
    ConvertToTime(ThreadId),
}

pub struct SnoozedView {
    /// Snoozed threads, sorted by snooze expiry (soonest first for time-based).
    pub threads: Vec<SnoozedItem>,
    /// Whether GeoClue2 is currently available.
    pub location_available: bool,
}

/// A snoozed thread with display info.
pub struct SnoozedItem {
    pub thread_id: ThreadId,
    pub subject: String,
    pub sender: String,
    pub snippet: String,
    pub snooze_info: SnoozeInfo,
}
```

- [ ] **Step 2: Implement the view rendering**

```rust
impl SnoozedView {
    pub fn view(&self) -> Element<SnoozedViewMessage> {
        if self.threads.is_empty() {
            return container(
                text("No snoozed items").size(16)
            )
            .center(Length::Fill)
            .into();
        }

        let items: Vec<Element<SnoozedViewMessage>> = self
            .threads
            .iter()
            .map(|item| self.render_snoozed_item(item))
            .collect();

        scrollable(column(items).spacing(1))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn render_snoozed_item(
        &self,
        item: &SnoozedItem,
    ) -> Element<SnoozedViewMessage> {
        let sender = text(&item.sender).size(14);
        let subject = text(&item.subject).size(14);
        let snippet = text(&item.snippet).size(12);

        let return_label = self.format_return_label(&item.snooze_info);
        let return_text = text(return_label).size(12);
        // .style(theme::snooze_return_date)  // orange-tinted

        let mut content = column![sender, subject, snippet, return_text]
            .spacing(2)
            .padding(Padding::from(12));

        // Show "(location unavailable)" badge for location snoozes without GeoClue2
        if !self.location_available {
            if matches!(item.snooze_info.until, SnoozeUntil::Location { .. }) {
                let badge = text("(location unavailable)").size(11);
                // .style(theme::warning_badge)
                content = content.push(badge);
            }
        }

        container(content)
            .width(Length::Fill)
            // .style(theme::email_row_container)
            .into()
    }

    fn format_return_label(&self, info: &SnoozeInfo) -> String {
        match &info.until {
            SnoozeUntil::Time(until) => {
                let local: DateTime<Local> = (*until).into();
                format!("Snoozing until {}", local.format("%a, %b %-d at %-I:%M %p"))
            }
            SnoozeUntil::Location { label, .. } => {
                format!("Snoozing until you arrive at {}", label)
            }
        }
    }
}
```

- [ ] **Step 3: Wire toolbar colour change**

In the main app file (wherever the toolbar is rendered), the active view determines toolbar colour:

```rust
// In the app's view() method or toolbar rendering:
let toolbar_color = match self.active_view {
    ActiveView::Inbox => theme.toolbar_inbox,      // #4285f4
    ActiveView::Snoozed => theme.toolbar_snoozed,  // #ef6c00
    ActiveView::Done => theme.toolbar_done,         // #0f9d58
};

// Toolbar title changes:
let toolbar_title = match self.active_view {
    ActiveView::Inbox => "Inbox",
    ActiveView::Snoozed => "Snoozed",
    ActiveView::Done => "Done",
};
```

- [ ] **Step 4: Wire nav drawer "Snoozed" click to switch view**

The nav drawer already has a "Snoozed" item (from M15). Wire its click to switch `active_view` to `ActiveView::Snoozed` and load snoozed threads from the store.

- [ ] **Step 5: Register view in `inboxly-ui/src/views/mod.rs`**

```rust
pub mod snoozed;
pub use snoozed::{SnoozedView, SnoozedViewMessage};
```

- [ ] **Step 6: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): build Snoozed view with orange toolbar and return date labels`

---

## Task 13: Integrate snooze action with swipe-left and hover button

**Files:**
- Modify: `inboxly-ui/src/widgets/swipe_container.rs` (from M20)
- Modify: `inboxly-ui/src/widgets/email_row.rs` (from M17)
- Modify: `inboxly-ui/src/app.rs`

Connect the snooze picker to the swipe-left gesture and the hover clock button.

- [ ] **Step 1: Wire swipe-left commit to open SnoozePicker**

In the SwipeContainer (M20), left-swipe commit currently has a placeholder action. Wire it to:

1. Record which thread triggered the swipe
2. Open the SnoozePicker dialog positioned near the swiped row
3. The picker's preset/custom/location selection flows through `SnoozePickerMessage` to the app's `update()` method

```rust
// In app update() handler for SwipeContainer messages:
SwipeAction::SnoozeTriggered(thread_id) => {
    self.snooze_target = Some(thread_id);
    self.snooze_picker.visible = true;
}
```

- [ ] **Step 2: Wire hover button to open SnoozePicker**

The EmailRow hover actions (M20) show a clock icon button on the right side. Wire its `on_press` to the same flow:

```rust
// In EmailRow hover actions:
let snooze_btn = button(clock_icon)
    .on_press(AppMessage::OpenSnoozePickerFor(thread_id.clone()));
```

- [ ] **Step 3: Handle SnoozePicker result in app update()**

```rust
// In app update():
SnoozePickerMessage::PresetSelected(preset) => {
    if let Some(thread_id) = &self.snooze_target {
        // Dispatch async: self.snooze_service.snooze_preset(thread_id, preset)
        self.snooze_picker.visible = false;
        self.snooze_target = None;
        // Remove thread from inbox feed immediately (optimistic update)
    }
}
SnoozePickerMessage::CustomSnoozeConfirmed { date, time_of_day } => {
    if let Some(thread_id) = &self.snooze_target {
        let until = inboxly_snooze::preset::resolve_custom(date, time_of_day);
        // Dispatch async: self.snooze_service.snooze_until_time(thread_id, until)
        self.snooze_picker.visible = false;
        self.snooze_target = None;
    }
}
SnoozePickerMessage::LocationSnoozeConfirmed { lat, lng, radius_m, label } => {
    if let Some(thread_id) = &self.snooze_target {
        // Dispatch async: self.snooze_service.snooze_until_location(...)
        self.snooze_picker.visible = false;
        self.snooze_target = None;
    }
}
SnoozePickerMessage::Dismissed => {
    self.snooze_picker.visible = false;
    self.snooze_target = None;
}
```

- [ ] **Step 4: Handle SnoozeEvent::UnSnoozed in the app**

When the scheduler un-snoozes a thread, the app receives an event via the channel:

```rust
// In app subscription or event handler:
SnoozeEvent::UnSnoozed { thread_id, original_date } => {
    // Re-insert thread into inbox feed
    // Optionally show a brief notification: "Thread returned from snooze"
}
```

- [ ] **Step 5: Verify**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): integrate snooze picker with swipe-left gesture and hover button`

---

## Task 14: Wire snooze scheduler into application bootstrap

**Files:**
- Modify: `inboxly/src/main.rs`

Connect all snooze components at application startup.

- [ ] **Step 1: Bootstrap snooze system in `main.rs`**

```rust
// In main() or app initialization, after store is ready:

// 1. Create snooze event channel
let (snooze_tx, snooze_rx) = tokio::sync::mpsc::unbounded_channel();

// 2. Create snooze service
let snooze_service = Arc::new(SnoozeService::new(store.clone(), snooze_tx.clone()));

// 3. Check location capability
let location_capability = check_location_capability().await;

// 4. Start time-based scheduler
let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
let scheduler = SnoozeScheduler::new(snooze_service.clone(), store.clone());
let scheduler_handle = scheduler.spawn(cancel_rx.clone());

// 5. Start location poller (if available)
let location_handle = if location_capability == LocationCapability::FullyAvailable {
    let geoclue = GeoClueClient::connect().await;
    let poller = LocationPoller::new(snooze_service.clone(), store.clone(), geoclue);
    Some(poller.spawn(cancel_rx.clone()))
} else {
    // Mark existing location snoozes as degraded
    mark_location_snoozes_degraded(&store, &snooze_tx).await;
    None
};

// 6. Pass snooze_rx and location_capability to the UI
// (UI subscribes to snooze_rx for UnSnoozed events)

// On app shutdown:
// let _ = cancel_tx.send(true);
// scheduler_handle.await;
// if let Some(h) = location_handle { h.await; }
```

- [ ] **Step 2: Pass snooze dependencies to UI app**

The Iced app needs:
- `snooze_service: Arc<SnoozeService<S>>` — to perform snooze actions
- `snooze_rx: mpsc::UnboundedReceiver<SnoozeEvent>` — to receive un-snooze notifications
- `location_capability: LocationCapability` — to configure the picker

These are passed via the app's `new()` or `flags()` method, depending on Iced's initialization pattern.

- [ ] **Step 3: Verify full compilation**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo build
```

**Commit:** `feat: wire snooze scheduler, location poller, and service into app bootstrap`

---

## Task 15: Integration tests — snooze lifecycle end-to-end

**Files:**
- Create: `inboxly-snooze/tests/integration.rs`

End-to-end tests covering the complete snooze lifecycle.

- [ ] **Step 1: Test helper — in-memory store**

Create or reuse an `InMemoryStore` that implements the `Store` trait using `HashMap`s and `Vec`s. This avoids SQLite dependency in snooze crate tests.

```rust
// tests/test_support.rs or tests/integration.rs
use std::collections::HashMap;
use std::sync::Mutex;
use inboxly_core::*;

pub struct InMemoryStore {
    thread_states: Mutex<HashMap<ThreadId, ThreadState>>,
}

// Implement Store trait methods using the HashMap
```

- [ ] **Step 2: Test — snooze preset → scheduler un-snooze**

```rust
#[tokio::test(start_paused = true)]
async fn snooze_preset_lifecycle() {
    // 1. Create store with a thread
    // 2. Create service + scheduler (1-second interval for test speed)
    // 3. Snooze with LaterToday preset
    // 4. Verify thread is snoozed
    // 5. Advance time past snooze expiry
    // 6. Verify thread is un-snoozed
    // 7. Verify UnSnoozed event was received
}
```

- [ ] **Step 3: Test — custom time snooze**

```rust
#[tokio::test(start_paused = true)]
async fn custom_time_snooze_lifecycle() {
    // 1. Snooze with custom date + Morning
    // 2. Advance time just before expiry → still snoozed
    // 3. Advance past expiry → un-snoozed
}
```

- [ ] **Step 4: Test — location snooze (mocked)**

```rust
#[tokio::test]
async fn location_snooze_geofence_match() {
    // 1. Snooze with location (43.653, -79.383, 500m, "Office")
    // 2. Simulate device at (43.655, -79.385) → within 500m
    // 3. Verify un-snoozed
}

#[tokio::test]
async fn location_snooze_outside_geofence() {
    // 1. Snooze with location (43.653, -79.383, 200m, "Office")
    // 2. Simulate device at (44.0, -80.0) → outside
    // 3. Verify still snoozed
}
```

- [ ] **Step 5: Test — un-snooze restores to inbox**

```rust
#[tokio::test]
async fn unsnooze_clears_done_flag() {
    // 1. Thread that was done + snoozed
    // 2. Un-snooze
    // 3. Verify done=false, snoozed=None
}
```

- [ ] **Step 6: Test — convert location to time**

```rust
#[tokio::test]
async fn convert_location_to_time_preserves_state() {
    // 1. Location-snoozed thread
    // 2. Convert to time (1 hour from now)
    // 3. Verify SnoozeUntil::Time, original_date unchanged
}
```

- [ ] **Step 7: Test — scheduler cancel**

```rust
#[tokio::test]
async fn scheduler_shutdown_on_cancel() {
    // 1. Start scheduler
    // 2. Send cancel
    // 3. Verify JoinHandle completes within 5 seconds
}
```

- [ ] **Step 8: Verify all tests pass**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-snooze
```

**Commit:** `test(snooze): add integration tests for complete snooze lifecycle`

---

## Task Dependency Graph

```
Task 1 (crate setup)
  ↓
Task 2 (presets)
  ↓
Task 3 (service) ←── Task 5 (store trait: list_snoozed)
  ↓
Task 4 (scheduler) ──→ Task 14 (bootstrap wiring)
  ↓
Task 6 (GeoClue2 client)
  ↓
Task 7 (location poller) ──→ Task 14
  ↓
Task 8 (degradation)
  ↓
Task 9 (picker: presets)
  ↓
Task 10 (picker: custom time)
  ↓
Task 11 (picker: location)
  ↓
Task 12 (snoozed view) ──→ Task 13 (integration)
                              ↓
                           Task 14 (bootstrap)
                              ↓
                           Task 15 (integration tests)
```

**Critical ordering notes:**
- Task 5 (Store trait extension) must happen before Task 4 (scheduler calls `list_snoozed_threads`)
- Tasks 9-11 (UI) can proceed in parallel with Tasks 6-8 (backend) if desired
- Task 14 (wiring) depends on all backend + UI tasks
- Task 15 (tests) depends on everything

---

## Commit Strategy

15 commits, one per task:

1. **`feat(snooze): set up inboxly-snooze crate with dependencies`** (Task 1)
2. **`feat(snooze): implement SnoozePreset enum with time resolution`** (Task 2)
3. **`feat(snooze): implement SnoozeService with snooze/un-snooze operations`** (Task 3)
4. **`feat(snooze): implement background snooze scheduler with 60-second check`** (Task 4)
5. **`feat(store): add list_snoozed_threads and list_location_snoozed_threads to Store trait`** (Task 5)
6. **`feat(snooze): implement GeoClue2 D-Bus client with haversine distance`** (Task 6)
7. **`feat(snooze): implement location-based snooze poller with geofence checking`** (Task 7)
8. **`feat(snooze): implement GeoClue2 graceful degradation and availability detection`** (Task 8)
9. **`feat(ui): build SnoozePicker widget with preset buttons grid`** (Task 9)
10. **`feat(ui): add custom date/time picker to SnoozePicker`** (Task 10)
11. **`feat(ui): add location picker to SnoozePicker with validation`** (Task 11)
12. **`feat(ui): build Snoozed view with orange toolbar and return date labels`** (Task 12)
13. **`feat(ui): integrate snooze picker with swipe-left gesture and hover button`** (Task 13)
14. **`feat: wire snooze scheduler, location poller, and service into app bootstrap`** (Task 14)
15. **`test(snooze): add integration tests for complete snooze lifecycle`** (Task 15)
