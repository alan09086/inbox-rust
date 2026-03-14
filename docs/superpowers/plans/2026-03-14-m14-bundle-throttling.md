# M14: Bundle Throttling — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement bundle throttling so that non-urgent email bundles (Promos, Social, etc.) are suppressed from the inbox feed until their configured delivery window, then surfaced as a batch. Throttled emails are synced and stored normally — throttling is a presentation-layer filter on the inbox query.

**Prerequisites:** M12 (bundler header heuristics) + M13 (user rules + sender learning). The `inboxly-bundler` crate exists with `BundleCategory`, `BundleRule`, `SenderAffinity`, and the evaluation pipeline. The `inboxly-core` crate has `Bundle`, `BundleThrottle`, `BundleId`, `InboxItem`. The `inboxly-store` crate has the SQLite schema with `bundles` table (including `throttle` column) and the inbox feed query.

**Crate:** `inboxly-bundler` (primary), with supporting changes in `inboxly-core` and `inboxly-store`.

**Architecture:**

The throttle system has three components:
1. **Throttle configuration** — `BundleThrottle` enum and `ThrottleWindow` struct define when a bundle's emails surface (stored in SQLite `bundles.throttle` column).
2. **Throttle filter** — The inbox feed query filters out threads belonging to throttled bundles whose window has not yet opened. This is a SQLite-level filter, not an in-memory post-filter.
3. **Throttle scheduler** — A background `tokio` task that checks every 60 seconds whether any throttle window has opened, and emits an event to refresh the inbox feed when it has.
4. **Body re-evaluation** — When Phase 2 sync delivers a message body, the bundler re-evaluates body-based rules and may reclassify the thread into a different (possibly throttled) bundle.

**Tech Stack:** Rust edition 2024, chrono, tokio, rusqlite, serde

**Key Design Decisions:**
- Throttle state lives entirely in the `bundles` table — no separate throttle table. The `throttle` column stores a JSON-encoded `ThrottleConfig`.
- "Window open" is computed at query time, not pre-computed. This avoids stale state and means changing a throttle setting takes effect immediately.
- The scheduler does NOT modify the database. It only emits an `ThrottleWindowOpened` event that tells the UI to re-query the inbox feed.
- Body re-evaluation reuses the existing bundler pipeline (heuristics → rules → learning) — no new classification logic, just a re-run trigger.

---

## Task 1: Define ThrottleConfig and ThrottleWindow Types in inboxly-core

**Files:**
- Create: `inboxly-core/src/throttle.rs`
- Modify: `inboxly-core/src/lib.rs`

**Step 1: Create the throttle types module**

Create `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/throttle.rs`:

```rust
//! Bundle throttle configuration and delivery window computation.
//!
//! Throttling controls when a bundle's emails surface in the inbox feed.
//! Emails are always synced and stored — throttling is presentation-only.

use chrono::{DateTime, Datelike, NaiveTime, Utc, Weekday};
use serde::{Deserialize, Serialize};

/// How a bundle delivers its emails to the inbox feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum BundleThrottle {
    /// Emails appear as they arrive (default).
    Immediate,

    /// Bundle surfaces once per day at the configured time.
    Daily {
        /// Time of day to deliver (e.g., 17:00 for 5 PM). Local time.
        delivery_time: NaiveTime,
    },

    /// Bundle surfaces once per week at the configured day and time.
    Weekly {
        /// Day of week to deliver (e.g., Monday).
        delivery_day: WeekdayWrapper,
        /// Time of day to deliver (e.g., 08:00 for 8 AM). Local time.
        delivery_time: NaiveTime,
    },
}

impl Default for BundleThrottle {
    fn default() -> Self {
        BundleThrottle::Immediate
    }
}

/// Wrapper around `chrono::Weekday` for serde support.
///
/// chrono's Weekday doesn't implement Serialize/Deserialize, so we wrap it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeekdayWrapper(pub Weekday);

impl Serialize for WeekdayWrapper {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(weekday_to_str(self.0))
    }
}

impl<'de> Deserialize<'de> for WeekdayWrapper {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        weekday_from_str(&s)
            .map(WeekdayWrapper)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid weekday: {s}")))
    }
}

fn weekday_to_str(w: Weekday) -> &'static str {
    match w {
        Weekday::Mon => "monday",
        Weekday::Tue => "tuesday",
        Weekday::Wed => "wednesday",
        Weekday::Thu => "thursday",
        Weekday::Fri => "friday",
        Weekday::Sat => "saturday",
        Weekday::Sun => "sunday",
    }
}

fn weekday_from_str(s: &str) -> Option<Weekday> {
    match s.to_lowercase().as_str() {
        "monday" | "mon" => Some(Weekday::Mon),
        "tuesday" | "tue" => Some(Weekday::Tue),
        "wednesday" | "wed" => Some(Weekday::Wed),
        "thursday" | "thu" => Some(Weekday::Thu),
        "friday" | "fri" => Some(Weekday::Fri),
        "saturday" | "sat" => Some(Weekday::Sat),
        "sunday" | "sun" => Some(Weekday::Sun),
        _ => None,
    }
}

impl BundleThrottle {
    /// Returns `true` if this throttle allows emails to surface right now.
    ///
    /// For `Immediate`, always returns `true`.
    /// For `Daily`, returns `true` if `now` is within the delivery window
    /// (from delivery_time today until delivery_time tomorrow).
    /// For `Weekly`, returns `true` if `now` is within the delivery window
    /// (from delivery_time on delivery_day until delivery_time on the next delivery_day).
    ///
    /// `local_now` should be the current time in the user's local timezone.
    pub fn is_window_open(&self, local_now: &DateTime<chrono::Local>) -> bool {
        match self {
            BundleThrottle::Immediate => true,
            BundleThrottle::Daily { delivery_time } => {
                let now_time = local_now.time();
                // Window is open from delivery_time until end of day
                // (emails delivered at 5 PM remain visible until next 5 PM cycle)
                now_time >= *delivery_time
            }
            BundleThrottle::Weekly { delivery_day, delivery_time } => {
                let now_weekday = local_now.weekday();
                let now_time = local_now.time();
                if now_weekday == delivery_day.0 {
                    now_time >= *delivery_time
                } else {
                    // Check if we're in the days after delivery_day but before the next one
                    days_since(delivery_day.0, now_weekday) > 0
                        && days_since(delivery_day.0, now_weekday) < 7
                }
            }
        }
    }

    /// Returns the next time this throttle's delivery window opens.
    ///
    /// For `Immediate`, returns `None` (always open).
    /// For `Daily` and `Weekly`, returns the next delivery time as UTC.
    pub fn next_window(&self, local_now: &DateTime<chrono::Local>) -> Option<DateTime<Utc>> {
        match self {
            BundleThrottle::Immediate => None,
            BundleThrottle::Daily { delivery_time } => {
                let today = local_now.date_naive();
                let candidate = today.and_time(*delivery_time);
                let next = if local_now.naive_local() >= candidate {
                    // Already past today's window, next is tomorrow
                    candidate + chrono::Duration::days(1)
                } else {
                    candidate
                };
                Some(next.and_utc())
            }
            BundleThrottle::Weekly { delivery_day, delivery_time } => {
                let today = local_now.date_naive();
                let today_weekday = today.weekday();
                let days_ahead = days_until(today_weekday, delivery_day.0);
                let candidate_date = today + chrono::Duration::days(days_ahead as i64);
                let candidate = candidate_date.and_time(*delivery_time);
                let next = if days_ahead == 0 && local_now.naive_local() >= candidate {
                    // Same day but past the time, next week
                    candidate + chrono::Duration::days(7)
                } else {
                    candidate
                };
                Some(next.and_utc())
            }
        }
    }

    /// Returns `true` if this throttle suppresses emails (is not Immediate).
    pub fn is_throttled(&self) -> bool {
        !matches!(self, BundleThrottle::Immediate)
    }
}

/// Number of days from `from` to `to` going forward (0 if same day).
fn days_since(from: Weekday, to: Weekday) -> u32 {
    let from_num = from.num_days_from_monday();
    let to_num = to.num_days_from_monday();
    (to_num + 7 - from_num) % 7
}

/// Number of days until `target` from `current` (7 if same day, for "next week" calculation).
fn days_until(current: Weekday, target: Weekday) -> u32 {
    let current_num = current.num_days_from_monday();
    let target_num = target.num_days_from_monday();
    if current_num == target_num {
        0 // Same day — caller checks time to decide if 0 or 7
    } else {
        (target_num + 7 - current_num) % 7
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime, TimeZone};

    /// Helper: create a local datetime from components.
    fn local(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<chrono::Local> {
        let naive = NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, min, 0)
            .unwrap();
        chrono::Local.from_local_datetime(&naive).unwrap()
    }

    #[test]
    fn immediate_always_open() {
        let throttle = BundleThrottle::Immediate;
        let now = chrono::Local::now();
        assert!(throttle.is_window_open(&now));
        assert!(throttle.next_window(&now).is_none());
        assert!(!throttle.is_throttled());
    }

    #[test]
    fn daily_before_window() {
        let throttle = BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
        };
        // 2 PM — before 5 PM window
        let now = local(2026, 3, 14, 14, 0);
        assert!(!throttle.is_window_open(&now));
        assert!(throttle.is_throttled());
    }

    #[test]
    fn daily_after_window() {
        let throttle = BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
        };
        // 6 PM — after 5 PM window
        let now = local(2026, 3, 14, 18, 0);
        assert!(throttle.is_window_open(&now));
    }

    #[test]
    fn daily_exactly_at_window() {
        let throttle = BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
        };
        let now = local(2026, 3, 14, 17, 0);
        assert!(throttle.is_window_open(&now));
    }

    #[test]
    fn weekly_on_delivery_day_before_time() {
        let throttle = BundleThrottle::Weekly {
            delivery_day: WeekdayWrapper(Weekday::Mon),
            delivery_time: NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
        };
        // Monday 7 AM — before 8 AM
        let now = local(2026, 3, 16, 7, 0); // March 16, 2026 is a Monday
        assert!(!throttle.is_window_open(&now));
    }

    #[test]
    fn weekly_on_delivery_day_after_time() {
        let throttle = BundleThrottle::Weekly {
            delivery_day: WeekdayWrapper(Weekday::Mon),
            delivery_time: NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
        };
        // Monday 9 AM — after 8 AM
        let now = local(2026, 3, 16, 9, 0);
        assert!(throttle.is_window_open(&now));
    }

    #[test]
    fn weekly_on_different_day_after_delivery() {
        let throttle = BundleThrottle::Weekly {
            delivery_day: WeekdayWrapper(Weekday::Mon),
            delivery_time: NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
        };
        // Wednesday — two days after Monday delivery
        let now = local(2026, 3, 18, 12, 0);
        assert!(throttle.is_window_open(&now));
    }

    #[test]
    fn weekly_on_day_before_delivery() {
        let throttle = BundleThrottle::Weekly {
            delivery_day: WeekdayWrapper(Weekday::Mon),
            delivery_time: NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
        };
        // Sunday — day before Monday delivery (should show last week's batch)
        let now = local(2026, 3, 15, 12, 0);
        assert!(throttle.is_window_open(&now));
    }

    #[test]
    fn serde_roundtrip_immediate() {
        let throttle = BundleThrottle::Immediate;
        let json = serde_json::to_string(&throttle).unwrap();
        let decoded: BundleThrottle = serde_json::from_str(&json).unwrap();
        assert_eq!(throttle, decoded);
    }

    #[test]
    fn serde_roundtrip_daily() {
        let throttle = BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
        };
        let json = serde_json::to_string(&throttle).unwrap();
        let decoded: BundleThrottle = serde_json::from_str(&json).unwrap();
        assert_eq!(throttle, decoded);
    }

    #[test]
    fn serde_roundtrip_weekly() {
        let throttle = BundleThrottle::Weekly {
            delivery_day: WeekdayWrapper(Weekday::Mon),
            delivery_time: NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
        };
        let json = serde_json::to_string(&throttle).unwrap();
        assert!(json.contains("monday"));
        let decoded: BundleThrottle = serde_json::from_str(&json).unwrap();
        assert_eq!(throttle, decoded);
    }

    #[test]
    fn next_window_daily_before_time() {
        let throttle = BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
        };
        let now = local(2026, 3, 14, 14, 0);
        let next = throttle.next_window(&now).unwrap();
        // Should be today at 5 PM
        assert_eq!(next.date_naive(), NaiveDate::from_ymd_opt(2026, 3, 14).unwrap());
    }

    #[test]
    fn next_window_daily_after_time() {
        let throttle = BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
        };
        let now = local(2026, 3, 14, 18, 0);
        let next = throttle.next_window(&now).unwrap();
        // Should be tomorrow at 5 PM
        assert_eq!(next.date_naive(), NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());
    }

    #[test]
    fn days_since_same_day() {
        assert_eq!(days_since(Weekday::Mon, Weekday::Mon), 0);
    }

    #[test]
    fn days_since_next_day() {
        assert_eq!(days_since(Weekday::Mon, Weekday::Tue), 1);
    }

    #[test]
    fn days_since_wrap_around() {
        assert_eq!(days_since(Weekday::Sat, Weekday::Mon), 2);
    }
}
```

**Step 2: Register the module in lib.rs**

In `/mnt/TempNVME/projects/inbox-rust/inboxly-core/src/lib.rs`, add:

```rust
pub mod throttle;
pub use throttle::{BundleThrottle, WeekdayWrapper};
```

**Step 3: Update the Bundle struct to use ThrottleConfig**

Replace the placeholder `BundleThrottle` enum in the core types (if it exists as a simple enum from the spec) with a re-export from the new module. The `Bundle` struct's `throttle` field type becomes `BundleThrottle` from `throttle.rs`.

**Build and test:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-core -- throttle
```

**Commit:**

```
feat(core): add BundleThrottle type with delivery window computation (M14)

Defines Immediate/Daily/Weekly throttle variants with serde support,
window-open checks, and next-window computation. WeekdayWrapper provides
serializable chrono::Weekday.
```

---

## Task 2: Add Throttle CRUD to the Store Layer

**Files:**
- Modify: `inboxly-store/src/db.rs` (or equivalent SQLite module)
- Create: `inboxly-store/src/throttle.rs`
- Modify: `inboxly-store/src/lib.rs`

**Step 1: Ensure the `bundles.throttle` column stores JSON**

The SQLite schema from M12 already has a `throttle` column in the `bundles` table. This step ensures it stores JSON-encoded `BundleThrottle`. If the column type is TEXT, it already works. If it was an INTEGER enum, add a migration:

```sql
-- Migration: update throttle column to JSON (only if needed)
-- The column was TEXT DEFAULT 'immediate' — now stores full JSON.
-- Default value for existing rows:
UPDATE bundles SET throttle = '{"mode":"Immediate"}' WHERE throttle = 'immediate' OR throttle IS NULL;
```

**Step 2: Create the throttle store module**

Create `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/throttle.rs`:

```rust
//! Throttle-related database operations.
//!
//! CRUD for per-bundle throttle settings, plus the throttle-aware inbox query.

use chrono::{DateTime, Local};
use inboxly_core::throttle::BundleThrottle;
use inboxly_core::BundleId;
use rusqlite::{params, Connection};

use crate::error::Result;

/// Get the throttle configuration for a bundle.
pub fn get_bundle_throttle(conn: &Connection, bundle_id: &BundleId) -> Result<BundleThrottle> {
    let json: String = conn.query_row(
        "SELECT throttle FROM bundles WHERE id = ?1",
        params![bundle_id.to_string()],
        |row| row.get(0),
    )?;
    let throttle: BundleThrottle = serde_json::from_str(&json)?;
    Ok(throttle)
}

/// Set the throttle configuration for a bundle.
pub fn set_bundle_throttle(
    conn: &Connection,
    bundle_id: &BundleId,
    throttle: &BundleThrottle,
) -> Result<()> {
    let json = serde_json::to_string(throttle)?;
    conn.execute(
        "UPDATE bundles SET throttle = ?1 WHERE id = ?2",
        params![json, bundle_id.to_string()],
    )?;
    Ok(())
}

/// Get all bundles that have non-Immediate throttle settings.
///
/// Returns `(BundleId, BundleThrottle)` pairs for bundles with active throttling.
pub fn get_throttled_bundles(conn: &Connection) -> Result<Vec<(BundleId, BundleThrottle)>> {
    let mut stmt = conn.prepare(
        "SELECT id, throttle FROM bundles WHERE throttle != '{\"mode\":\"Immediate\"}'"
    )?;
    let rows = stmt.query_map([], |row| {
        let id_str: String = row.get(0)?;
        let json: String = row.get(1)?;
        Ok((id_str, json))
    })?;

    let mut result = Vec::new();
    for row in rows {
        let (id_str, json) = row?;
        let bundle_id: BundleId = id_str.parse()?;
        let throttle: BundleThrottle = serde_json::from_str(&json)?;
        if throttle.is_throttled() {
            result.push((bundle_id, throttle));
        }
    }
    Ok(result)
}

/// Returns the set of bundle IDs that are currently throttled (window not open).
///
/// This is used by the inbox feed query to filter out throttled bundles.
pub fn get_currently_suppressed_bundle_ids(
    conn: &Connection,
    local_now: &DateTime<Local>,
) -> Result<Vec<BundleId>> {
    let throttled = get_throttled_bundles(conn)?;
    let suppressed = throttled
        .into_iter()
        .filter(|(_, throttle)| !throttle.is_window_open(local_now))
        .map(|(id, _)| id)
        .collect();
    Ok(suppressed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;
    use inboxly_core::throttle::WeekdayWrapper;
    use rusqlite::Connection;
    use uuid::Uuid;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE bundles (
                id TEXT PRIMARY KEY,
                category TEXT NOT NULL,
                name TEXT NOT NULL,
                color TEXT,
                badge_color TEXT,
                visibility TEXT DEFAULT 'Bundled',
                throttle TEXT DEFAULT '{\"mode\":\"Immediate\"}',
                sort_order INTEGER DEFAULT 0
            );"
        ).unwrap();
        conn
    }

    fn insert_bundle(conn: &Connection, id: &str, throttle: &BundleThrottle) {
        let json = serde_json::to_string(throttle).unwrap();
        conn.execute(
            "INSERT INTO bundles (id, category, name, throttle) VALUES (?1, 'Promos', 'Test', ?2)",
            params![id, json],
        ).unwrap();
    }

    #[test]
    fn get_set_throttle_roundtrip() {
        let conn = setup_db();
        let id = BundleId::from(Uuid::new_v4());
        let throttle = BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
        };
        insert_bundle(&conn, &id.to_string(), &BundleThrottle::Immediate);

        set_bundle_throttle(&conn, &id, &throttle).unwrap();
        let loaded = get_bundle_throttle(&conn, &id).unwrap();
        assert_eq!(throttle, loaded);
    }

    #[test]
    fn get_throttled_bundles_excludes_immediate() {
        let conn = setup_db();
        let id1 = Uuid::new_v4().to_string();
        let id2 = Uuid::new_v4().to_string();
        insert_bundle(&conn, &id1, &BundleThrottle::Immediate);
        insert_bundle(&conn, &id2, &BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
        });

        let throttled = get_throttled_bundles(&conn).unwrap();
        assert_eq!(throttled.len(), 1);
    }
}
```

**Step 3: Register the module**

In `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/lib.rs`, add:

```rust
pub mod throttle;
```

**Build and test:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store -- throttle
```

**Commit:**

```
feat(store): add throttle CRUD and suppressed-bundle query (M14)

Per-bundle throttle get/set, list of throttled bundles, and function to
compute which bundles are currently suppressed based on delivery window.
```

---

## Task 3: Integrate Throttle Filter into Inbox Feed Query

**Files:**
- Modify: `inboxly-store/src/query.rs` (or wherever the inbox feed query lives)

**Step 1: Add throttle filtering to the inbox feed query**

The existing inbox feed query returns `Vec<InboxItem>` (threads and bundles). Modify it to accept an optional set of suppressed bundle IDs and exclude matching threads/bundles.

In the inbox feed query function (e.g., `get_inbox_feed`), add a parameter:

```rust
/// Fetch the inbox feed, optionally filtering out throttled bundles.
///
/// `suppressed_bundles`: bundle IDs whose delivery window has not opened.
/// Threads assigned to these bundles are excluded from the feed.
/// The bundles themselves are also excluded from the feed.
pub fn get_inbox_feed(
    conn: &Connection,
    suppressed_bundles: &[BundleId],
    // ... existing parameters ...
) -> Result<Vec<InboxItem>> {
    // Build the exclusion clause for SQL
    let exclusion_clause = if suppressed_bundles.is_empty() {
        String::new()
    } else {
        let placeholders: Vec<String> = suppressed_bundles
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 10)) // offset to avoid collision with other params
            .collect();
        format!(
            " AND (ts.bundle_id IS NULL OR ts.bundle_id NOT IN ({}))",
            placeholders.join(", ")
        )
    };

    // ... incorporate exclusion_clause into the WHERE clause of the inbox query ...
}
```

**Step 2: Add a convenience function that computes suppression and queries in one call**

```rust
/// Fetch the inbox feed with automatic throttle filtering.
///
/// Computes which bundles are currently suppressed based on the local time,
/// then runs the inbox feed query with those bundles excluded.
pub fn get_inbox_feed_throttled(
    conn: &Connection,
    // ... existing parameters ...
) -> Result<Vec<InboxItem>> {
    let now = chrono::Local::now();
    let suppressed = crate::throttle::get_currently_suppressed_bundle_ids(conn, &now)?;
    get_inbox_feed(conn, &suppressed, /* ... */)
}
```

**Step 3: Add tests for throttle filtering**

```rust
#[cfg(test)]
mod throttle_filter_tests {
    use super::*;

    #[test]
    fn feed_excludes_throttled_bundle_threads() {
        // Setup: create a bundle with Daily throttle, assign a thread to it.
        // Query before window opens: thread should be absent.
        // Query after window opens: thread should be present.
    }

    #[test]
    fn feed_includes_immediate_bundle_threads() {
        // Setup: create a bundle with Immediate throttle, assign a thread.
        // Thread should always be present in the feed.
    }

    #[test]
    fn feed_excludes_throttled_bundle_from_bundle_list() {
        // The collapsed bundle row itself should not appear in the feed
        // when throttled.
    }

    #[test]
    fn feed_includes_unbundled_threads_regardless_of_throttle() {
        // Threads not assigned to any bundle are never throttled.
    }
}
```

**Build and test:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store -- throttle_filter
```

**Commit:**

```
feat(store): integrate throttle filter into inbox feed query (M14)

The inbox feed query now accepts a list of suppressed bundle IDs and
excludes their threads and bundle rows. Convenience function computes
suppression from current time automatically.
```

---

## Task 4: Implement the Throttle Scheduler

**Files:**
- Create: `inboxly-bundler/src/scheduler.rs`
- Modify: `inboxly-bundler/src/lib.rs`

**Step 1: Define the throttle event type**

In `/mnt/TempNVME/projects/inbox-rust/inboxly-bundler/src/scheduler.rs`:

```rust
//! Background scheduler for throttle window checking.
//!
//! Runs a tokio task that periodically checks whether any bundle's throttle
//! window has opened. When it has, emits a `ThrottleWindowOpened` event so
//! the UI can refresh the inbox feed.

use std::sync::Arc;

use chrono::Local;
use rusqlite::Connection;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use inboxly_core::BundleId;

/// Events emitted by the throttle scheduler.
#[derive(Debug, Clone)]
pub enum ThrottleEvent {
    /// One or more bundle windows have opened. The UI should refresh the inbox feed.
    /// Contains the bundle IDs whose windows just opened.
    WindowOpened(Vec<BundleId>),
}

/// Configuration for the throttle scheduler.
#[derive(Debug, Clone)]
pub struct ThrottleSchedulerConfig {
    /// How often to check throttle windows, in seconds. Default: 60.
    pub check_interval_secs: u64,
}

impl Default for ThrottleSchedulerConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 60,
        }
    }
}

/// Tracks which bundles were suppressed on the last check, so we can detect transitions.
struct ThrottleState {
    /// Bundle IDs that were suppressed (window closed) on the last check.
    previously_suppressed: Vec<BundleId>,
}

/// Spawn the throttle scheduler as a background tokio task.
///
/// Returns a `JoinHandle` for the task and an event receiver.
///
/// The scheduler:
/// 1. Every `check_interval_secs`, queries the store for currently suppressed bundles.
/// 2. Compares with the previous check's suppressed set.
/// 3. If any bundle has transitioned from suppressed → not suppressed, emits `WindowOpened`.
///
/// The `db` parameter is a function that provides a database connection.
/// This avoids holding a connection across await points.
pub fn spawn_throttle_scheduler<F>(
    config: ThrottleSchedulerConfig,
    db: F,
    event_tx: mpsc::UnboundedSender<ThrottleEvent>,
) -> tokio::task::JoinHandle<()>
where
    F: Fn() -> Arc<Connection> + Send + 'static,
{
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(config.check_interval_secs));
        let mut state = ThrottleState {
            previously_suppressed: Vec::new(),
        };

        // Initial population
        if let Ok(conn) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| db())) {
            let now = Local::now();
            if let Ok(suppressed) =
                inboxly_store::throttle::get_currently_suppressed_bundle_ids(&conn, &now)
            {
                state.previously_suppressed = suppressed;
            }
        }

        loop {
            tick.tick().await;

            let conn = db();
            let now = Local::now();

            let currently_suppressed = match inboxly_store::throttle::get_currently_suppressed_bundle_ids(&conn, &now) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("throttle scheduler: failed to query suppressed bundles: {e}");
                    continue;
                }
            };

            // Find bundles that were suppressed before but are no longer suppressed
            let newly_opened: Vec<BundleId> = state
                .previously_suppressed
                .iter()
                .filter(|id| !currently_suppressed.contains(id))
                .cloned()
                .collect();

            if !newly_opened.is_empty() {
                tracing::info!(
                    "throttle scheduler: {} bundle window(s) opened: {:?}",
                    newly_opened.len(),
                    newly_opened
                );
                if event_tx.send(ThrottleEvent::WindowOpened(newly_opened)).is_err() {
                    tracing::debug!("throttle scheduler: event channel closed, shutting down");
                    break;
                }
            }

            state.previously_suppressed = currently_suppressed;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn scheduler_detects_window_opening() {
        // This test uses a mock database connection and a short interval.
        // The test sets up a daily throttle that opens "now" and verifies
        // the scheduler emits a WindowOpened event.
        //
        // Full integration test requires the store layer — see Task 7.
    }

    #[test]
    fn default_config_is_60_seconds() {
        let config = ThrottleSchedulerConfig::default();
        assert_eq!(config.check_interval_secs, 60);
    }
}
```

**Step 2: Register the module**

In `/mnt/TempNVME/projects/inbox-rust/inboxly-bundler/src/lib.rs`, add:

```rust
pub mod scheduler;
pub use scheduler::{ThrottleEvent, ThrottleSchedulerConfig, spawn_throttle_scheduler};
```

**Build and test:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-bundler -- scheduler
```

**Commit:**

```
feat(bundler): add background throttle scheduler (M14)

Spawns a tokio task that checks throttle windows every 60 seconds and
emits ThrottleWindowOpened events when a bundle transitions from
suppressed to visible. Drives inbox feed refresh in the UI.
```

---

## Task 5: Implement Body Re-evaluation on Phase 2 Catch-up

**Files:**
- Modify: `inboxly-bundler/src/engine.rs` (or `categorise.rs` — wherever the bundler pipeline lives)
- Modify: `inboxly-bundler/src/lib.rs`

**Step 1: Add a re-evaluation entry point**

The bundler already has a `categorise(email_meta, headers)` function from M12/M13. Add a parallel function that accepts the full body content for re-evaluation:

```rust
/// Re-evaluate an email's bundle assignment using full body content.
///
/// Called when Phase 2 sync delivers a message body for an email that was
/// previously categorised using headers only. Runs the full bundler pipeline
/// (heuristics → user rules → sender learning) with body content available.
///
/// Returns `Some(new_bundle_id)` if the re-evaluation changed the assignment,
/// or `None` if the assignment remains the same.
pub fn re_evaluate_with_body(
    &self,
    email_id: &EmailId,
    email_meta: &EmailMeta,
    headers: &HashMap<String, String>,
    body_text: Option<&str>,
    body_html: Option<&str>,
    current_bundle_id: Option<&BundleId>,
) -> Result<Option<BundleId>> {
    // Run the full pipeline with body available
    let new_bundle_id = self.categorise_full(email_meta, headers, body_text, body_html)?;

    // Compare with current assignment
    if new_bundle_id.as_ref() != current_bundle_id {
        Ok(new_bundle_id)
    } else {
        Ok(None)
    }
}
```

**Step 2: Add body-aware rule evaluation**

Extend the rule evaluation to handle `RuleField::Body`:

```rust
/// Evaluate a single rule against email content.
///
/// Body-based rules (`RuleField::Body`) only match when `body_text` or
/// `body_html` is provided. During Phase 1 (headers-only), these rules
/// are skipped — the email will be re-evaluated when the body arrives.
fn evaluate_rule(
    &self,
    rule: &BundleRule,
    email_meta: &EmailMeta,
    headers: &HashMap<String, String>,
    body_text: Option<&str>,
    body_html: Option<&str>,
) -> bool {
    match &rule.field {
        RuleField::Body => {
            let body = body_text.or(body_html);
            match body {
                Some(text) => self.match_operator(&rule.operator, &rule.value, text),
                None => false, // Body not available yet — skip this rule
            }
        }
        // ... existing From/To/Subject/Header cases unchanged ...
    }
}
```

**Step 3: Wire into the sync engine's Phase 2 callback**

The IMAP sync engine (from M8) should call `re_evaluate_with_body` when it downloads a message body. Add a hook point:

```rust
/// Called by the sync engine when a message body is downloaded during Phase 2.
///
/// Re-runs the bundler on this email with full body content. If the bundle
/// assignment changes, updates the store and emits a `BundleChanged` event.
pub async fn on_body_downloaded(
    &self,
    email_id: &EmailId,
    body_text: Option<&str>,
    body_html: Option<&str>,
) -> Result<()> {
    let email_meta = self.store.get_email_meta(email_id)?;
    let headers = self.store.get_email_headers(email_id)?;
    let current_bundle = self.store.get_thread_bundle(&email_meta.thread_id)?;

    if let Some(new_bundle_id) = self.engine.re_evaluate_with_body(
        email_id,
        &email_meta,
        &headers,
        body_text,
        body_html,
        current_bundle.as_ref(),
    )? {
        // Update the thread's bundle assignment
        self.store.set_thread_bundle(&email_meta.thread_id, Some(&new_bundle_id))?;

        // Emit event so UI can update
        self.event_tx.send(BundlerEvent::BundleChanged {
            thread_id: email_meta.thread_id.clone(),
            old_bundle: current_bundle,
            new_bundle: Some(new_bundle_id),
        })?;
    }

    Ok(())
}
```

**Step 4: Add tests**

```rust
#[cfg(test)]
mod body_reevaluation_tests {
    use super::*;

    #[test]
    fn body_rule_skipped_when_no_body() {
        // Create a rule matching "unsubscribe" in body.
        // Categorise with headers only — rule should not fire.
        // Email stays uncategorised or gets header-based assignment.
    }

    #[test]
    fn body_rule_fires_on_reevaluation() {
        // Same rule as above.
        // Re-evaluate with body containing "unsubscribe" — rule fires.
        // Email gets reassigned to the rule's bundle.
    }

    #[test]
    fn reevaluation_no_change_returns_none() {
        // Email already correctly categorised by headers.
        // Re-evaluate with body — no body rules match.
        // Returns None (no change).
    }

    #[test]
    fn body_rule_overrides_header_heuristic() {
        // Header heuristic assigns to Social.
        // User rule on body assigns to Custom bundle (higher priority).
        // Re-evaluation with body should change assignment.
    }
}
```

**Build and test:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-bundler -- body_reevaluation
```

**Commit:**

```
feat(bundler): implement body re-evaluation for Phase 2 catch-up (M14)

When Phase 2 sync delivers a message body, the bundler re-runs the full
pipeline with body content available. Body-based user rules that were
skipped during headers-only Phase 1 now fire, and the thread's bundle
assignment is updated if it changes. Addresses the Known Limitation
documented in the design spec.
```

---

## Task 6: Per-Bundle Throttle Settings CRUD in the Bundler Engine

**Files:**
- Modify: `inboxly-bundler/src/engine.rs` (or `bundler.rs` — the main bundler engine)
- Modify: `inboxly-bundler/src/lib.rs`

**Step 1: Add throttle management methods to the bundler engine**

```rust
impl BundlerEngine {
    /// Get the throttle setting for a bundle.
    pub fn get_throttle(&self, bundle_id: &BundleId) -> Result<BundleThrottle> {
        inboxly_store::throttle::get_bundle_throttle(&self.conn(), bundle_id)
    }

    /// Set the throttle for a bundle. Takes effect immediately — the next
    /// inbox feed query will reflect the new throttle setting.
    pub fn set_throttle(
        &self,
        bundle_id: &BundleId,
        throttle: BundleThrottle,
    ) -> Result<()> {
        inboxly_store::throttle::set_bundle_throttle(&self.conn(), bundle_id, &throttle)?;

        // Emit event so UI refreshes
        self.event_tx.send(BundlerEvent::ThrottleChanged {
            bundle_id: bundle_id.clone(),
            throttle: throttle.clone(),
        })?;

        Ok(())
    }

    /// Get all bundles with their throttle settings.
    ///
    /// Used by the settings UI to display throttle configuration.
    pub fn get_all_bundle_throttles(&self) -> Result<Vec<(BundleId, String, BundleThrottle)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, name, throttle FROM bundles ORDER BY sort_order"
        )?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let json: String = row.get(2)?;
            Ok((id, name, json))
        })?;

        let mut result = Vec::new();
        for row in rows {
            let (id_str, name, json) = row?;
            let bundle_id: BundleId = id_str.parse()?;
            let throttle: BundleThrottle = serde_json::from_str(&json)?;
            result.push((bundle_id, name, throttle));
        }
        Ok(result)
    }

    /// Set default throttle for a bundle category.
    ///
    /// New bundles of this category will inherit this throttle setting.
    /// Does not affect existing bundles.
    pub fn set_category_default_throttle(
        &self,
        category: &BundleCategory,
        throttle: BundleThrottle,
    ) -> Result<()> {
        let json = serde_json::to_string(&throttle)?;
        let key = format!("throttle_default_{}", category.as_str());
        self.conn().execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, json],
        )?;
        Ok(())
    }

    /// Get the default throttle for a bundle category.
    ///
    /// Falls back to `Immediate` if no default is set.
    pub fn get_category_default_throttle(
        &self,
        category: &BundleCategory,
    ) -> Result<BundleThrottle> {
        let key = format!("throttle_default_{}", category.as_str());
        match self.conn().query_row(
            "SELECT value FROM settings WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get::<_, String>(0),
        ) {
            Ok(json) => Ok(serde_json::from_str(&json)?),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(BundleThrottle::default()),
            Err(e) => Err(e.into()),
        }
    }
}
```

**Step 2: Add `ThrottleChanged` variant to `BundlerEvent`**

In the bundler event enum:

```rust
pub enum BundlerEvent {
    // ... existing variants ...

    /// A bundle's throttle setting was changed.
    ThrottleChanged {
        bundle_id: BundleId,
        throttle: BundleThrottle,
    },

    /// A thread's bundle assignment changed (e.g., due to body re-evaluation).
    BundleChanged {
        thread_id: ThreadId,
        old_bundle: Option<BundleId>,
        new_bundle: Option<BundleId>,
    },
}
```

**Step 3: Add `as_str()` to BundleCategory**

If not already present, add a method to convert `BundleCategory` to a string key:

```rust
impl BundleCategory {
    pub fn as_str(&self) -> &str {
        match self {
            BundleCategory::Social => "social",
            BundleCategory::Promos => "promos",
            BundleCategory::Updates => "updates",
            BundleCategory::Finance => "finance",
            BundleCategory::Purchases => "purchases",
            BundleCategory::Travel => "travel",
            BundleCategory::Forums => "forums",
            BundleCategory::LowPriority => "low_priority",
            BundleCategory::Saved => "saved",
            BundleCategory::Custom(name) => name.as_str(),
        }
    }
}
```

**Step 4: Add tests**

```rust
#[cfg(test)]
mod throttle_crud_tests {
    use super::*;

    #[test]
    fn set_and_get_throttle() {
        // Create a bundle, set its throttle to Daily at 5 PM, read it back.
    }

    #[test]
    fn set_throttle_emits_event() {
        // Set a throttle, verify the event channel received ThrottleChanged.
    }

    #[test]
    fn get_all_bundle_throttles_returns_all() {
        // Create 3 bundles with different throttles, verify all are returned.
    }

    #[test]
    fn category_default_throttle_fallback() {
        // No default set — should return Immediate.
        // Set a default — should return it.
    }

    #[test]
    fn new_bundle_inherits_category_default() {
        // Set default throttle for Promos to Daily.
        // Create a new Promos bundle — verify it gets Daily throttle.
    }
}
```

**Build and test:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-bundler -- throttle_crud
```

**Commit:**

```
feat(bundler): add per-bundle throttle CRUD with category defaults (M14)

BundlerEngine gains get/set_throttle for individual bundles, bulk listing
for the settings UI, and per-category default throttle (new Promos bundles
default to Daily, for example). Emits ThrottleChanged events for UI refresh.
```

---

## Task 7: Integration Tests — End-to-End Throttle Behaviour

**Files:**
- Create: `inboxly-bundler/tests/throttle_integration.rs`

**Step 1: Create integration test file**

Create `/mnt/TempNVME/projects/inbox-rust/inboxly-bundler/tests/throttle_integration.rs`:

```rust
//! Integration tests for bundle throttling end-to-end behaviour.
//!
//! These tests verify the full throttle lifecycle:
//! 1. Email arrives → assigned to throttled bundle → suppressed from feed
//! 2. Delivery window opens → scheduler emits event → feed refresh shows email
//! 3. Body arrives in Phase 2 → re-evaluation may change bundle → throttle may change
//! 4. User changes throttle setting → takes effect immediately

use std::sync::Arc;

use chrono::{Local, NaiveTime, Weekday};
use inboxly_bundler::{
    BundlerEngine, ThrottleEvent, ThrottleSchedulerConfig, spawn_throttle_scheduler,
};
use inboxly_core::throttle::{BundleThrottle, WeekdayWrapper};
use inboxly_core::{BundleCategory, BundleId, EmailId, ThreadId};
use rusqlite::Connection;
use tokio::sync::mpsc;

/// Helper: create an in-memory database with full schema.
fn setup_test_db() -> Arc<Connection> {
    let conn = Connection::open_in_memory().unwrap();
    // Apply full schema from inboxly-store
    inboxly_store::schema::apply_migrations(&conn).unwrap();
    Arc::new(conn)
}

#[tokio::test]
async fn throttled_bundle_emails_hidden_then_visible() {
    // Setup:
    // 1. Create a Promos bundle with Daily throttle at 5 PM
    // 2. Insert an email assigned to this bundle
    // 3. Query feed at 2 PM — email should be absent
    // 4. Query feed at 6 PM — email should be present
}

#[tokio::test]
async fn immediate_bundle_always_visible() {
    // Setup:
    // 1. Create a Social bundle with Immediate throttle
    // 2. Insert an email assigned to this bundle
    // 3. Query feed at any time — email should always be present
}

#[tokio::test]
async fn scheduler_emits_event_on_window_opening() {
    // Setup:
    // 1. Create a bundle with a throttle window about to open
    // 2. Start scheduler with 1-second interval
    // 3. Wait for the window to open
    // 4. Verify WindowOpened event is received with correct bundle ID
}

#[tokio::test]
async fn changing_throttle_takes_effect_immediately() {
    // Setup:
    // 1. Create a bundle with Daily throttle
    // 2. Insert email, verify it's hidden before window
    // 3. Change throttle to Immediate
    // 4. Re-query — email should now be visible
}

#[tokio::test]
async fn body_reevaluation_changes_throttled_assignment() {
    // Setup:
    // 1. Email arrives, header heuristics assign to Social (Immediate)
    // 2. Email is visible in feed
    // 3. Body arrives with "unsubscribe" link
    // 4. Body re-evaluation reassigns to Promos (Daily throttle)
    // 5. If window is closed, email disappears from feed until window opens
}

#[tokio::test]
async fn weekly_throttle_window_behaviour() {
    // Setup:
    // 1. Create a bundle with Weekly throttle (Monday 8 AM)
    // 2. Insert emails
    // 3. Query on Sunday 11 PM — emails should be visible (last Monday's window)
    // 4. Query on Monday 7 AM — emails should be suppressed (before this week's window)
    // Actually: weekly window means "from delivery_time on delivery_day onward"
    // so Sunday is within the previous window (Mon→Sun), and Mon 7 AM is before
    // the new window opens at 8 AM.
}

#[tokio::test]
async fn multiple_throttled_bundles_independent() {
    // Setup:
    // 1. Promos: Daily at 5 PM
    // 2. Social: Daily at 9 AM
    // 3. Query at 10 AM — Social visible, Promos hidden
    // 4. Query at 6 PM — both visible
}

#[tokio::test]
async fn unbundled_threads_never_throttled() {
    // Setup:
    // 1. Insert a thread with no bundle assignment
    // 2. Query feed at any time — thread should always be visible
    // 3. Verify throttle filtering doesn't affect unbundled threads
}
```

**Build and test:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-bundler --test throttle_integration
```

**Commit:**

```
test(bundler): add integration tests for throttle lifecycle (M14)

End-to-end tests covering suppression, window opening, scheduler events,
immediate setting changes, body re-evaluation interactions, and weekly
throttle behaviour. Verifies unbundled threads are never affected.
```

---

## Task 8: Wire Throttle into the Application Event Loop

**Files:**
- Modify: `inboxly/src/main.rs` (or the application bootstrap)
- Modify: `inboxly-bundler/src/lib.rs` (public API surface)

**Step 1: Start the throttle scheduler in the application bootstrap**

In the binary's startup sequence, after the bundler engine is initialised:

```rust
// Start the throttle scheduler
let (throttle_tx, mut throttle_rx) = tokio::sync::mpsc::unbounded_channel();
let db_for_scheduler = db_pool.clone(); // clone the DB pool/factory
let scheduler_handle = inboxly_bundler::spawn_throttle_scheduler(
    inboxly_bundler::ThrottleSchedulerConfig::default(),
    move || db_for_scheduler.get_connection(),
    throttle_tx,
);

// Forward throttle events to the UI event channel
tokio::spawn(async move {
    while let Some(event) = throttle_rx.recv().await {
        match event {
            ThrottleEvent::WindowOpened(bundle_ids) => {
                // Tell the UI to refresh the inbox feed
                ui_event_tx.send(UiEvent::RefreshInboxFeed {
                    reason: RefreshReason::ThrottleWindowOpened(bundle_ids),
                }).ok();
            }
        }
    }
});
```

**Step 2: Handle throttle events in the UI event loop**

In the Iced application's `update` method (or the UI event dispatcher):

```rust
UiEvent::RefreshInboxFeed { reason: RefreshReason::ThrottleWindowOpened(bundle_ids) } => {
    // Re-query the inbox feed — the throttle filter will now
    // include the newly-opened bundles
    self.reload_inbox_feed();

    // Optionally show a subtle notification
    tracing::info!("Throttle window opened for {} bundle(s)", bundle_ids.len());
}
```

**Step 3: Ensure shutdown cleanliness**

The scheduler task should stop when the application shuts down. This happens naturally when the `throttle_tx` sender is dropped (the channel closes, the scheduler loop breaks). Add a shutdown hook:

```rust
// On application shutdown:
drop(throttle_tx); // signal scheduler to stop
scheduler_handle.abort(); // belt and suspenders
```

**Build and test:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo build
```

**Commit:**

```
feat: wire throttle scheduler into application event loop (M14)

The binary starts the throttle scheduler on launch, forwards
ThrottleWindowOpened events to the UI for feed refresh, and
cleanly shuts down the scheduler on exit.
```

---

## Task 9: Add Default Throttle Presets for Built-in Categories

**Files:**
- Modify: `inboxly-bundler/src/engine.rs`
- Modify: `inboxly-store/src/db.rs` (seed data)

**Step 1: Define sensible default throttles for built-in categories**

When the bundler creates built-in bundles for the first time (during initial setup or first sync), apply these default throttles:

```rust
/// Default throttle presets for built-in bundle categories.
///
/// These match Google Inbox's behaviour: most bundles batch daily,
/// Social is immediate (you want to see social notifications promptly),
/// and Low Priority batches weekly.
pub fn default_throttle_for_category(category: &BundleCategory) -> BundleThrottle {
    match category {
        BundleCategory::Social => BundleThrottle::Immediate,
        BundleCategory::Promos => BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(), // 5 PM
        },
        BundleCategory::Updates => BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(), // 9 AM
        },
        BundleCategory::Finance => BundleThrottle::Immediate,
        BundleCategory::Purchases => BundleThrottle::Immediate,
        BundleCategory::Travel => BundleThrottle::Immediate,
        BundleCategory::Forums => BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(12, 0, 0).unwrap(), // Noon
        },
        BundleCategory::LowPriority => BundleThrottle::Weekly {
            delivery_day: WeekdayWrapper(Weekday::Mon),
            delivery_time: NaiveTime::from_hms_opt(8, 0, 0).unwrap(), // Monday 8 AM
        },
        BundleCategory::Saved => BundleThrottle::Immediate,
        BundleCategory::Custom(_) => BundleThrottle::Immediate,
    }
}
```

**Step 2: Apply defaults when creating built-in bundles**

In the bundle creation logic (from M12), apply the default throttle:

```rust
/// Create the built-in bundles if they don't exist.
pub fn ensure_builtin_bundles(&self) -> Result<()> {
    let builtin_categories = [
        BundleCategory::Social,
        BundleCategory::Promos,
        BundleCategory::Updates,
        BundleCategory::Finance,
        BundleCategory::Purchases,
        BundleCategory::Travel,
        BundleCategory::Forums,
        BundleCategory::LowPriority,
    ];

    for category in &builtin_categories {
        if !self.bundle_exists_for_category(category)? {
            let throttle = default_throttle_for_category(category);
            self.create_bundle(category.default_name(), category.clone(), throttle)?;
        }
    }

    Ok(())
}
```

**Step 3: Add tests**

```rust
#[cfg(test)]
mod default_throttle_tests {
    use super::*;

    #[test]
    fn promos_defaults_to_daily_5pm() {
        let throttle = default_throttle_for_category(&BundleCategory::Promos);
        match throttle {
            BundleThrottle::Daily { delivery_time } => {
                assert_eq!(delivery_time, NaiveTime::from_hms_opt(17, 0, 0).unwrap());
            }
            _ => panic!("expected Daily"),
        }
    }

    #[test]
    fn low_priority_defaults_to_weekly_monday() {
        let throttle = default_throttle_for_category(&BundleCategory::LowPriority);
        match throttle {
            BundleThrottle::Weekly { delivery_day, delivery_time } => {
                assert_eq!(delivery_day.0, Weekday::Mon);
                assert_eq!(delivery_time, NaiveTime::from_hms_opt(8, 0, 0).unwrap());
            }
            _ => panic!("expected Weekly"),
        }
    }

    #[test]
    fn social_defaults_to_immediate() {
        let throttle = default_throttle_for_category(&BundleCategory::Social);
        assert_eq!(throttle, BundleThrottle::Immediate);
    }

    #[test]
    fn custom_bundles_default_to_immediate() {
        let throttle = default_throttle_for_category(&BundleCategory::Custom("My Bundle".into()));
        assert_eq!(throttle, BundleThrottle::Immediate);
    }

    #[test]
    fn finance_and_travel_immediate_for_urgency() {
        // Finance and Travel are time-sensitive — should always be Immediate
        assert_eq!(
            default_throttle_for_category(&BundleCategory::Finance),
            BundleThrottle::Immediate
        );
        assert_eq!(
            default_throttle_for_category(&BundleCategory::Travel),
            BundleThrottle::Immediate
        );
    }
}
```

**Build and test:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-bundler -- default_throttle
```

**Commit:**

```
feat(bundler): add default throttle presets for built-in categories (M14)

Promos daily at 5 PM, Updates daily at 9 AM, Forums daily at noon,
Low Priority weekly Monday 8 AM. Social/Finance/Travel/Purchases stay
Immediate for urgency. Custom bundles default to Immediate.
```

---

## Task 10: Documentation and Final Verification

**Files:**
- Modify: `inboxly-bundler/src/lib.rs` (module-level docs)
- Modify: `README.md` (if it exists — add throttling to feature list)

**Step 1: Add module-level documentation**

In `inboxly-bundler/src/lib.rs`, update the crate-level doc comment:

```rust
//! # inboxly-bundler
//!
//! Email categorisation engine for Inboxly.
//!
//! ## Layers
//!
//! 1. **Header heuristics** (M12) — zero-config pattern matching on email headers
//! 2. **User rules + sender learning** (M13) — explicit rules and learned sender affinity
//! 3. **Bundle throttling** (M14) — delivery window control for non-urgent bundles
//!
//! ## Throttling
//!
//! Bundles can be set to `Immediate`, `Daily`, or `Weekly` delivery. Throttled
//! bundles suppress their emails from the inbox feed until the delivery window
//! opens. The throttle scheduler runs as a background tokio task and emits
//! `ThrottleWindowOpened` events when windows transition.
//!
//! Body-based rules are re-evaluated when Phase 2 sync delivers message bodies,
//! ensuring categorisation catches up even when initial classification was
//! headers-only.
```

**Step 2: Run full test suite**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test --workspace
cd /mnt/TempNVME/projects/inbox-rust && cargo clippy --workspace -- -D warnings
```

**Step 3: Verify no regressions in M12/M13 functionality**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-bundler
```

**Commit:**

```
docs(bundler): add throttling documentation to crate-level docs (M14)

Describes the three-layer categorisation pipeline and throttle system
architecture. Final milestone commit — M14 complete.
```

---

## Summary

| Task | Description | Files | Tests |
|------|-------------|-------|-------|
| 1 | ThrottleConfig + window computation types | `inboxly-core/src/throttle.rs` | 14 unit tests |
| 2 | Throttle CRUD in store layer | `inboxly-store/src/throttle.rs` | 2 unit tests |
| 3 | Throttle filter in inbox feed query | `inboxly-store/src/query.rs` | 4 unit tests |
| 4 | Background throttle scheduler | `inboxly-bundler/src/scheduler.rs` | 1 unit test |
| 5 | Body re-evaluation (Phase 2 catch-up) | `inboxly-bundler/src/engine.rs` | 4 unit tests |
| 6 | Per-bundle throttle CRUD in engine | `inboxly-bundler/src/engine.rs` | 5 unit tests |
| 7 | Integration tests | `inboxly-bundler/tests/throttle_integration.rs` | 8 integration tests |
| 8 | Wire into application event loop | `inboxly/src/main.rs` | build verification |
| 9 | Default throttle presets | `inboxly-bundler/src/engine.rs` | 5 unit tests |
| 10 | Documentation + final verification | `inboxly-bundler/src/lib.rs` | full suite pass |

**Total: 10 tasks, ~43 tests, 10 commits**

After M14, the "complete backend engine" checkpoint is reached (per roadmap). The sync engine (M6-M9), store (M3-M5), threading (M10-M11), and bundler (M12-M14) form a fully functional headless email backend ready for the UI layer starting at M15.
