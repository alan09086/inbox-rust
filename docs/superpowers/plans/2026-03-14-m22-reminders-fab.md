# M22: Reminders + Speed Dial FAB — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Add full reminder CRUD in `inboxly-snooze`, render reminders in the inbox feed, build the SpeedDialFab widget, and wire up the reminder creation dialog.

**Architecture:** Reminder domain logic lives in `inboxly-snooze` (depends on `core`, `store`). Reminder CRUD goes through a `ReminderService` that wraps SQLite operations via the `Store` trait. The UI crate gains three new widgets (`ReminderRow`, `SpeedDialFab`, `ReminderDialog`) and a `RemindersView` for the nav drawer. Reminders surface in the inbox feed as `InboxItem::Reminder` items, sorted by `due_at` among email threads.

**Tech Stack:** Rust, rusqlite, chrono, uuid, iced

**Prerequisites:**
- **M1** — `inboxly-core` exists with `InboxItem::Reminder { id, title, due, done }`, `SnoozeInfo`, `SnoozeUntil`, `Uuid`, `DateTime<Utc>`
- **M3** — SQLite schema exists with `reminders` table (`id, title, due_at, location_lat, location_lng, location_label, recurring, done`)
- **M15** — Iced shell + nav drawer (nav items like "Reminders" in sidebar)
- **M16** — Theme system (`InboxlyTheme` with colour tokens)
- **M17** — Inbox feed rendering with `InboxItem` enum dispatch + section headers
- **M19** — Done action infrastructure (mark done + undo snackbar)
- **M21** — Snooze picker widget + snooze presets (reused for reminder date/time selection)

---

## Task Overview

| # | Task | Crate | Est. |
|---|------|-------|------|
| 1 | Define `Reminder` struct in `inboxly-core` | `core` | 5 min |
| 2 | Extend `InboxItem` with proper `Reminder` variant | `core` | 5 min |
| 3 | Add `ReminderStore` trait to `inboxly-core` | `core` | 5 min |
| 4 | Implement `ReminderStore` for SQLite in `inboxly-store` | `store` | 15 min |
| 5 | Build `ReminderService` in `inboxly-snooze` | `snooze` | 15 min |
| 6 | Add reminder scheduler to snooze background task | `snooze` | 10 min |
| 7 | Implement `ReminderRow` widget | `ui` | 15 min |
| 8 | Mix reminders into inbox feed | `ui` | 10 min |
| 9 | Implement `ReminderDialog` widget | `ui` | 20 min |
| 10 | Implement `SpeedDialFab` widget | `ui` | 25 min |
| 11 | Add FAB scrim overlay | `ui` | 10 min |
| 12 | Wire FAB actions to compose + reminder dialog | `ui` | 10 min |
| 13 | Implement reminder done action | `ui`, `snooze` | 10 min |
| 14 | Implement reminder snooze action | `ui`, `snooze` | 10 min |
| 15 | Add `RemindersView` to nav drawer | `ui` | 15 min |
| 16 | Integration tests | `snooze`, `ui` | 15 min |

---

### Task 1: Define `Reminder` struct in `inboxly-core`

**Files:**
- Create: `inboxly-core/src/reminder.rs`
- Modify: `inboxly-core/src/lib.rs`

The `InboxItem::Reminder` variant from M1 uses inline fields (`id`, `title`, `due`, `done`). We need a proper `Reminder` struct that maps to the SQLite `reminders` table schema and holds all fields including location-based triggers and recurrence.

- [ ] **Step 1: Create `inboxly-core/src/reminder.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A user-created reminder (non-email task) that appears in the inbox feed.
///
/// Maps to the SQLite `reminders` table. Reminders support both time-based
/// and location-based triggers, and can optionally recur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reminder {
    /// Unique identifier.
    pub id: Uuid,
    /// User-entered reminder text (e.g., "Buy groceries").
    pub title: String,
    /// When the reminder should surface in the inbox feed.
    pub due_at: DateTime<Utc>,
    /// Optional location trigger (lat/lng/label).
    pub location: Option<ReminderLocation>,
    /// Whether this reminder recurs (e.g., daily, weekly). Empty string = no recurrence.
    pub recurring: Option<String>,
    /// Whether the reminder has been marked done (archived).
    pub done: bool,
    /// When the reminder was created.
    pub created_at: DateTime<Utc>,
    /// Snooze state — if the reminder has been snoozed, this holds the snooze info.
    pub snoozed: Option<crate::inbox::SnoozeInfo>,
}

/// Location trigger for a reminder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReminderLocation {
    pub lat: f64,
    pub lng: f64,
    pub label: String,
}

impl Reminder {
    /// Create a new time-based reminder with the given title and due date.
    pub fn new(title: impl Into<String>, due_at: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            due_at,
            location: None,
            recurring: None,
            done: false,
            created_at: Utc::now(),
            snoozed: None,
        }
    }

    /// Returns true if this reminder is past due and not yet done.
    pub fn is_overdue(&self) -> bool {
        !self.done && self.due_at <= Utc::now()
    }

    /// Returns true if this reminder is currently snoozed.
    pub fn is_snoozed(&self) -> bool {
        self.snoozed.is_some()
    }

    /// Returns the effective sort date: if snoozed, use the original due date;
    /// otherwise use due_at.
    pub fn sort_date(&self) -> DateTime<Utc> {
        self.due_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_reminder_defaults() {
        let due = Utc::now() + chrono::Duration::hours(2);
        let r = Reminder::new("Buy groceries", due);
        assert_eq!(r.title, "Buy groceries");
        assert_eq!(r.due_at, due);
        assert!(!r.done);
        assert!(r.location.is_none());
        assert!(r.recurring.is_none());
        assert!(r.snoozed.is_none());
    }

    #[test]
    fn is_overdue_when_past_due() {
        let mut r = Reminder::new("Test", Utc::now() - chrono::Duration::hours(1));
        assert!(r.is_overdue());
        r.done = true;
        assert!(!r.is_overdue());
    }

    #[test]
    fn is_not_overdue_when_future() {
        let r = Reminder::new("Test", Utc::now() + chrono::Duration::hours(1));
        assert!(!r.is_overdue());
    }

    #[test]
    fn reminder_with_location() {
        let mut r = Reminder::new("Pick up package", Utc::now());
        r.location = Some(ReminderLocation {
            lat: 43.6532,
            lng: -79.3832,
            label: "Office".into(),
        });
        assert!(r.location.is_some());
        assert_eq!(r.location.as_ref().unwrap().label, "Office");
    }

    #[test]
    fn reminder_serde_roundtrip() {
        let r = Reminder::new("Call dentist", Utc::now() + chrono::Duration::days(3));
        let json = serde_json::to_string(&r).unwrap();
        let back: Reminder = serde_json::from_str(&json).unwrap();
        assert_eq!(r.id, back.id);
        assert_eq!(r.title, back.title);
        assert_eq!(r.done, back.done);
    }
}
```

- [ ] **Step 2: Register the module in `inboxly-core/src/lib.rs`**

Add:

```rust
pub mod reminder;

pub use reminder::{Reminder, ReminderLocation};
```

- [ ] **Step 3: Run `cargo test -p inboxly-core`**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-core
```

**Commit:** `feat(core): add Reminder and ReminderLocation types`

---

### Task 2: Extend `InboxItem` with proper `Reminder` variant

**Files:**
- Modify: `inboxly-core/src/inbox.rs`

The existing `InboxItem::Reminder` uses inline fields `{ id, title, due, done }`. Replace it with `InboxItem::Reminder(Reminder)` to use the full struct from Task 1. This is a breaking change to the enum — all match arms in the codebase must be updated.

- [ ] **Step 1: Update `InboxItem::Reminder` variant in `inboxly-core/src/inbox.rs`**

Change:

```rust
    /// A user-created reminder (non-email task).
    Reminder {
        id: Uuid,
        title: String,
        due: DateTime<Utc>,
        done: bool,
    },
```

To:

```rust
    /// A user-created reminder (non-email task).
    Reminder(crate::reminder::Reminder),
```

- [ ] **Step 2: Remove the now-unused `Uuid` import from `inbox.rs` if it was only used by the old Reminder variant**

Check whether `Uuid` is used elsewhere in the file. If not, remove `use uuid::Uuid;`.

- [ ] **Step 3: Update the `inbox_item_reminder` test in `inbox.rs`**

Replace the old test:

```rust
    #[test]
    fn inbox_item_reminder() {
        let item = InboxItem::Reminder {
            id: Uuid::new_v4(),
            title: "Buy groceries".into(),
            due: Utc::now() + chrono::Duration::hours(2),
            done: false,
        };
        match item {
            InboxItem::Reminder { title, done, .. } => {
                assert_eq!(title, "Buy groceries");
                assert!(!done);
            }
            _ => panic!("expected Reminder variant"),
        }
    }
```

With:

```rust
    #[test]
    fn inbox_item_reminder() {
        use crate::reminder::Reminder;
        let due = Utc::now() + chrono::Duration::hours(2);
        let reminder = Reminder::new("Buy groceries", due);
        let item = InboxItem::Reminder(reminder);
        match item {
            InboxItem::Reminder(r) => {
                assert_eq!(r.title, "Buy groceries");
                assert!(!r.done);
            }
            _ => panic!("expected Reminder variant"),
        }
    }
```

- [ ] **Step 4: Search for all other `InboxItem::Reminder` match arms across the workspace and update them**

```bash
cd /mnt/TempNVME/projects/inbox-rust && grep -rn "InboxItem::Reminder" --include="*.rs"
```

Update every match site to use `InboxItem::Reminder(r)` destructuring instead of the old inline fields.

- [ ] **Step 5: Run `cargo check --workspace && cargo test -p inboxly-core`**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check --workspace && cargo test -p inboxly-core
```

**Commit:** `refactor(core): replace InboxItem::Reminder inline fields with Reminder struct`

---

### Task 3: Add `ReminderStore` trait to `inboxly-core`

**Files:**
- Create: `inboxly-core/src/reminder_store.rs`
- Modify: `inboxly-core/src/lib.rs`

Define the storage trait for reminder CRUD. This lives in `core` so both `store` (implementation) and `snooze` (consumer) can depend on it without circular dependencies.

- [ ] **Step 1: Create `inboxly-core/src/reminder_store.rs`**

```rust
use crate::error::Result;
use crate::reminder::Reminder;
use uuid::Uuid;

/// Storage interface for reminder CRUD operations.
///
/// Implemented by `inboxly-store` (SQLite backend).
/// Consumed by `inboxly-snooze::ReminderService`.
pub trait ReminderStore: Send + Sync {
    /// Insert a new reminder.
    fn create_reminder(
        &self,
        reminder: &Reminder,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Retrieve a reminder by ID.
    fn get_reminder(
        &self,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<Option<Reminder>>> + Send;

    /// List all reminders that are not done, ordered by due_at ascending.
    fn list_active_reminders(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<Reminder>>> + Send;

    /// List all reminders (including done), ordered by due_at descending.
    fn list_all_reminders(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<Reminder>>> + Send;

    /// List reminders that are due (due_at <= now) and not done.
    fn list_due_reminders(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<Reminder>>> + Send;

    /// Update a reminder (title, due_at, location, recurring, done, snoozed).
    fn update_reminder(
        &self,
        reminder: &Reminder,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Mark a reminder as done.
    fn mark_reminder_done(
        &self,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Mark a reminder as not done (undo).
    fn mark_reminder_undone(
        &self,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Permanently delete a reminder.
    fn delete_reminder(
        &self,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}
```

- [ ] **Step 2: Register the module in `inboxly-core/src/lib.rs`**

Add:

```rust
pub mod reminder_store;

pub use reminder_store::ReminderStore;
```

- [ ] **Step 3: Run `cargo check -p inboxly-core`**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-core
```

**Commit:** `feat(core): add ReminderStore trait for reminder CRUD`

---

### Task 4: Implement `ReminderStore` for SQLite in `inboxly-store`

**Files:**
- Create: `inboxly-store/src/reminder_store.rs`
- Modify: `inboxly-store/src/lib.rs`

Implement the `ReminderStore` trait against the existing `reminders` SQLite table from M3.

**SQLite `reminders` table schema (from M3):**
```sql
CREATE TABLE reminders (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    due_at INTEGER NOT NULL,
    location_lat REAL,
    location_lng REAL,
    location_label TEXT,
    recurring TEXT,
    done INTEGER NOT NULL DEFAULT 0
);
```

Note: The `reminders` table from M3 does not have `created_at` or `snoozed` columns. We need to add them via migration.

- [ ] **Step 1: Add migration to extend `reminders` table**

In the store's migration system (established in M3), add a migration that adds the missing columns:

```sql
ALTER TABLE reminders ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE reminders ADD COLUMN snoozed_json TEXT;
```

Add this to the store's migration list in the appropriate migration file or function. The exact location depends on the M3 migration infrastructure (check `inboxly-store/src/migrations.rs` or equivalent).

- [ ] **Step 2: Create `inboxly-store/src/reminder_store.rs`**

```rust
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use inboxly_core::error::{InboxlyError, Result};
use inboxly_core::inbox::SnoozeInfo;
use inboxly_core::reminder::{Reminder, ReminderLocation};
use inboxly_core::ReminderStore;

use crate::SqliteStore;

impl SqliteStore {
    /// Map a SQLite row to a `Reminder`.
    fn row_to_reminder(row: &Row<'_>) -> rusqlite::Result<Reminder> {
        let id_str: String = row.get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })?;

        let title: String = row.get("title")?;
        let due_at_epoch: i64 = row.get("due_at")?;
        let due_at = DateTime::from_timestamp(due_at_epoch, 0)
            .unwrap_or_else(|| Utc::now());

        let location_lat: Option<f64> = row.get("location_lat")?;
        let location_lng: Option<f64> = row.get("location_lng")?;
        let location_label: Option<String> = row.get("location_label")?;

        let location = match (location_lat, location_lng, location_label) {
            (Some(lat), Some(lng), Some(label)) => Some(ReminderLocation { lat, lng, label }),
            _ => None,
        };

        let recurring: Option<String> = row.get("recurring")?;
        let done_int: i32 = row.get("done")?;
        let done = done_int != 0;

        let created_at_epoch: i64 = row.get("created_at")?;
        let created_at = DateTime::from_timestamp(created_at_epoch, 0)
            .unwrap_or_else(|| Utc::now());

        let snoozed_json: Option<String> = row.get("snoozed_json")?;
        let snoozed: Option<SnoozeInfo> = snoozed_json
            .and_then(|s| serde_json::from_str(&s).ok());

        Ok(Reminder {
            id,
            title,
            due_at,
            location,
            recurring,
            done,
            created_at,
            snoozed,
        })
    }
}

impl ReminderStore for SqliteStore {
    async fn create_reminder(&self, reminder: &Reminder) -> Result<()> {
        let reminder = reminder.clone();
        self.conn(move |conn| {
            let (loc_lat, loc_lng, loc_label) = match &reminder.location {
                Some(loc) => (Some(loc.lat), Some(loc.lng), Some(loc.label.clone())),
                None => (None, None, None),
            };
            let snoozed_json = reminder.snoozed.as_ref()
                .map(|s| serde_json::to_string(s).unwrap_or_default());

            conn.execute(
                "INSERT INTO reminders (id, title, due_at, location_lat, location_lng, \
                 location_label, recurring, done, created_at, snoozed_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    reminder.id.to_string(),
                    reminder.title,
                    reminder.due_at.timestamp(),
                    loc_lat,
                    loc_lng,
                    loc_label,
                    reminder.recurring,
                    reminder.done as i32,
                    reminder.created_at.timestamp(),
                    snoozed_json,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| InboxlyError::Database(e.to_string()))
    }

    async fn get_reminder(&self, id: Uuid) -> Result<Option<Reminder>> {
        self.conn(move |conn| {
            conn.query_row(
                "SELECT * FROM reminders WHERE id = ?1",
                params![id.to_string()],
                Self::row_to_reminder,
            )
            .optional()
        })
        .await
        .map_err(|e| InboxlyError::Database(e.to_string()))
    }

    async fn list_active_reminders(&self) -> Result<Vec<Reminder>> {
        self.conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM reminders WHERE done = 0 ORDER BY due_at ASC",
            )?;
            let reminders = stmt
                .query_map([], Self::row_to_reminder)?
                .filter_map(|r| r.ok())
                .collect();
            Ok(reminders)
        })
        .await
        .map_err(|e| InboxlyError::Database(e.to_string()))
    }

    async fn list_all_reminders(&self) -> Result<Vec<Reminder>> {
        self.conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM reminders ORDER BY due_at DESC",
            )?;
            let reminders = stmt
                .query_map([], Self::row_to_reminder)?
                .filter_map(|r| r.ok())
                .collect();
            Ok(reminders)
        })
        .await
        .map_err(|e| InboxlyError::Database(e.to_string()))
    }

    async fn list_due_reminders(&self) -> Result<Vec<Reminder>> {
        let now = Utc::now().timestamp();
        self.conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM reminders WHERE done = 0 AND due_at <= ?1 ORDER BY due_at ASC",
            )?;
            let reminders = stmt
                .query_map(params![now], Self::row_to_reminder)?
                .filter_map(|r| r.ok())
                .collect();
            Ok(reminders)
        })
        .await
        .map_err(|e| InboxlyError::Database(e.to_string()))
    }

    async fn update_reminder(&self, reminder: &Reminder) -> Result<()> {
        let reminder = reminder.clone();
        self.conn(move |conn| {
            let (loc_lat, loc_lng, loc_label) = match &reminder.location {
                Some(loc) => (Some(loc.lat), Some(loc.lng), Some(loc.label.clone())),
                None => (None, None, None),
            };
            let snoozed_json = reminder.snoozed.as_ref()
                .map(|s| serde_json::to_string(s).unwrap_or_default());

            conn.execute(
                "UPDATE reminders SET title = ?1, due_at = ?2, location_lat = ?3, \
                 location_lng = ?4, location_label = ?5, recurring = ?6, done = ?7, \
                 snoozed_json = ?8 WHERE id = ?9",
                params![
                    reminder.title,
                    reminder.due_at.timestamp(),
                    loc_lat,
                    loc_lng,
                    loc_label,
                    reminder.recurring,
                    reminder.done as i32,
                    snoozed_json,
                    reminder.id.to_string(),
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| InboxlyError::Database(e.to_string()))
    }

    async fn mark_reminder_done(&self, id: Uuid) -> Result<()> {
        self.conn(move |conn| {
            conn.execute(
                "UPDATE reminders SET done = 1 WHERE id = ?1",
                params![id.to_string()],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| InboxlyError::Database(e.to_string()))
    }

    async fn mark_reminder_undone(&self, id: Uuid) -> Result<()> {
        self.conn(move |conn| {
            conn.execute(
                "UPDATE reminders SET done = 0 WHERE id = ?1",
                params![id.to_string()],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| InboxlyError::Database(e.to_string()))
    }

    async fn delete_reminder(&self, id: Uuid) -> Result<()> {
        self.conn(move |conn| {
            conn.execute(
                "DELETE FROM reminders WHERE id = ?1",
                params![id.to_string()],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| InboxlyError::Database(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inboxly_core::reminder::Reminder;

    // These tests require the test helper from M3 that creates an in-memory SQLite store.
    // Adjust the helper name to match your M3 implementation.

    #[tokio::test]
    async fn create_and_get_reminder() {
        let store = SqliteStore::in_memory().await.unwrap();
        let r = Reminder::new("Buy milk", Utc::now() + chrono::Duration::hours(2));
        store.create_reminder(&r).await.unwrap();

        let fetched = store.get_reminder(r.id).await.unwrap().unwrap();
        assert_eq!(fetched.title, "Buy milk");
        assert!(!fetched.done);
    }

    #[tokio::test]
    async fn list_active_excludes_done() {
        let store = SqliteStore::in_memory().await.unwrap();
        let r1 = Reminder::new("Active one", Utc::now() + chrono::Duration::hours(1));
        let mut r2 = Reminder::new("Done one", Utc::now() + chrono::Duration::hours(2));
        r2.done = true;

        store.create_reminder(&r1).await.unwrap();
        store.create_reminder(&r2).await.unwrap();

        let active = store.list_active_reminders().await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].title, "Active one");
    }

    #[tokio::test]
    async fn mark_done_and_undone() {
        let store = SqliteStore::in_memory().await.unwrap();
        let r = Reminder::new("Test", Utc::now());
        store.create_reminder(&r).await.unwrap();

        store.mark_reminder_done(r.id).await.unwrap();
        let fetched = store.get_reminder(r.id).await.unwrap().unwrap();
        assert!(fetched.done);

        store.mark_reminder_undone(r.id).await.unwrap();
        let fetched = store.get_reminder(r.id).await.unwrap().unwrap();
        assert!(!fetched.done);
    }

    #[tokio::test]
    async fn list_due_reminders() {
        let store = SqliteStore::in_memory().await.unwrap();
        let past = Reminder::new("Overdue", Utc::now() - chrono::Duration::hours(1));
        let future = Reminder::new("Future", Utc::now() + chrono::Duration::hours(2));

        store.create_reminder(&past).await.unwrap();
        store.create_reminder(&future).await.unwrap();

        let due = store.list_due_reminders().await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].title, "Overdue");
    }

    #[tokio::test]
    async fn update_reminder_fields() {
        let store = SqliteStore::in_memory().await.unwrap();
        let mut r = Reminder::new("Original", Utc::now());
        store.create_reminder(&r).await.unwrap();

        r.title = "Updated".into();
        r.location = Some(ReminderLocation {
            lat: 43.6,
            lng: -79.4,
            label: "Home".into(),
        });
        store.update_reminder(&r).await.unwrap();

        let fetched = store.get_reminder(r.id).await.unwrap().unwrap();
        assert_eq!(fetched.title, "Updated");
        assert_eq!(fetched.location.unwrap().label, "Home");
    }

    #[tokio::test]
    async fn delete_reminder() {
        let store = SqliteStore::in_memory().await.unwrap();
        let r = Reminder::new("Delete me", Utc::now());
        store.create_reminder(&r).await.unwrap();

        store.delete_reminder(r.id).await.unwrap();
        let fetched = store.get_reminder(r.id).await.unwrap();
        assert!(fetched.is_none());
    }
}
```

- [ ] **Step 3: Register the module in `inboxly-store/src/lib.rs`**

Add:

```rust
mod reminder_store;
```

No re-export needed — the trait impl is the public interface.

- [ ] **Step 4: Run tests**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store -- reminder
```

**Commit:** `feat(store): implement ReminderStore for SQLite`

---

### Task 5: Build `ReminderService` in `inboxly-snooze`

**Files:**
- Create: `inboxly-snooze/src/reminder_service.rs`
- Modify: `inboxly-snooze/src/lib.rs`

The `ReminderService` is the public API for reminder operations. It wraps `ReminderStore` and adds business logic (snooze integration, validation, event emission).

- [ ] **Step 1: Create `inboxly-snooze/src/reminder_service.rs`**

```rust
use chrono::{DateTime, Utc};
use tokio::sync::mpsc;
use uuid::Uuid;

use inboxly_core::error::{InboxlyError, Result};
use inboxly_core::inbox::{InboxItem, SnoozeInfo, SnoozeUntil};
use inboxly_core::reminder::Reminder;
use inboxly_core::ReminderStore;

/// Events emitted by the ReminderService to the UI.
#[derive(Debug, Clone)]
pub enum ReminderEvent {
    /// A reminder is now due and should appear in the inbox feed.
    ReminderDue(Reminder),
    /// A reminder was created.
    ReminderCreated(Reminder),
    /// A reminder was marked done.
    ReminderDone(Uuid),
    /// A reminder was marked undone (undo).
    ReminderUndone(Uuid),
    /// A reminder was snoozed.
    ReminderSnoozed { id: Uuid, until: SnoozeUntil },
    /// A snoozed reminder has returned (snooze expired).
    ReminderUnsnoozed(Reminder),
}

/// Service layer for reminder CRUD and snooze integration.
///
/// Wraps a `ReminderStore` implementation and emits events to the UI.
pub struct ReminderService<S: ReminderStore> {
    store: S,
    event_tx: mpsc::UnboundedSender<ReminderEvent>,
}

impl<S: ReminderStore> ReminderService<S> {
    /// Create a new `ReminderService`.
    pub fn new(store: S, event_tx: mpsc::UnboundedSender<ReminderEvent>) -> Self {
        Self { store, event_tx }
    }

    /// Create a new reminder with the given title and due date.
    pub async fn create(&self, title: String, due_at: DateTime<Utc>) -> Result<Reminder> {
        if title.trim().is_empty() {
            return Err(InboxlyError::Other("Reminder title cannot be empty".into()));
        }

        let reminder = Reminder::new(title, due_at);
        self.store.create_reminder(&reminder).await?;
        let _ = self.event_tx.send(ReminderEvent::ReminderCreated(reminder.clone()));
        Ok(reminder)
    }

    /// Mark a reminder as done (archive it).
    pub async fn mark_done(&self, id: Uuid) -> Result<()> {
        self.store.mark_reminder_done(id).await?;
        let _ = self.event_tx.send(ReminderEvent::ReminderDone(id));
        Ok(())
    }

    /// Undo marking a reminder as done.
    pub async fn mark_undone(&self, id: Uuid) -> Result<()> {
        self.store.mark_reminder_undone(id).await?;
        let _ = self.event_tx.send(ReminderEvent::ReminderUndone(id));
        Ok(())
    }

    /// Snooze a reminder until a given time.
    pub async fn snooze(&self, id: Uuid, until: SnoozeUntil) -> Result<()> {
        let mut reminder = self
            .store
            .get_reminder(id)
            .await?
            .ok_or_else(|| InboxlyError::Other(format!("Reminder {id} not found")))?;

        reminder.snoozed = Some(SnoozeInfo {
            until: until.clone(),
            original_date: reminder.due_at,
        });
        self.store.update_reminder(&reminder).await?;
        let _ = self.event_tx.send(ReminderEvent::ReminderSnoozed { id, until });
        Ok(())
    }

    /// Un-snooze a reminder (called by the scheduler when snooze time arrives).
    pub async fn unsnooze(&self, id: Uuid) -> Result<()> {
        let mut reminder = self
            .store
            .get_reminder(id)
            .await?
            .ok_or_else(|| InboxlyError::Other(format!("Reminder {id} not found")))?;

        reminder.snoozed = None;
        self.store.update_reminder(&reminder).await?;
        let _ = self.event_tx.send(ReminderEvent::ReminderUnsnoozed(reminder));
        Ok(())
    }

    /// Update the title and/or due date of a reminder.
    pub async fn update(
        &self,
        id: Uuid,
        title: Option<String>,
        due_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let mut reminder = self
            .store
            .get_reminder(id)
            .await?
            .ok_or_else(|| InboxlyError::Other(format!("Reminder {id} not found")))?;

        if let Some(t) = title {
            if t.trim().is_empty() {
                return Err(InboxlyError::Other("Reminder title cannot be empty".into()));
            }
            reminder.title = t;
        }
        if let Some(d) = due_at {
            reminder.due_at = d;
        }
        self.store.update_reminder(&reminder).await?;
        Ok(())
    }

    /// Delete a reminder permanently.
    pub async fn delete(&self, id: Uuid) -> Result<()> {
        self.store.delete_reminder(id).await
    }

    /// Get all active (not done, not snoozed) reminders for the inbox feed.
    /// Returns them sorted by due_at ascending.
    pub async fn list_for_inbox(&self) -> Result<Vec<Reminder>> {
        let all = self.store.list_active_reminders().await?;
        // Filter out snoozed reminders — they appear in the Snoozed view instead
        let visible: Vec<Reminder> = all
            .into_iter()
            .filter(|r| r.snoozed.is_none())
            .collect();
        Ok(visible)
    }

    /// Get all reminders (for the Reminders nav drawer view).
    pub async fn list_all(&self) -> Result<Vec<Reminder>> {
        self.store.list_all_reminders().await
    }

    /// Get reminders that are currently due. Called by the scheduler.
    pub async fn check_due(&self) -> Result<Vec<Reminder>> {
        self.store.list_due_reminders().await
    }

    /// Convert a reminder to an `InboxItem` for feed rendering.
    pub fn to_inbox_item(reminder: &Reminder) -> InboxItem {
        InboxItem::Reminder(reminder.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock store for testing ReminderService without SQLite.
    // In practice, use the in-memory SqliteStore from inboxly-store tests.
    // These tests verify the service logic, not the store implementation.

    #[tokio::test]
    async fn create_rejects_empty_title() {
        let (tx, _rx) = mpsc::unbounded_channel();
        // This test requires a mock or in-memory store.
        // Implementation note: create a simple in-memory mock that implements ReminderStore.
        // For now, document the expected behavior:
        // - create("", due) should return Err(InboxlyError::Other("...empty..."))
        // - create("  ", due) should also fail (whitespace-only)
    }
}
```

- [ ] **Step 2: Register the module in `inboxly-snooze/src/lib.rs`**

Add:

```rust
pub mod reminder_service;

pub use reminder_service::{ReminderEvent, ReminderService};
```

- [ ] **Step 3: Add `inboxly-store` to `inboxly-snooze/Cargo.toml` dev-dependencies for integration tests**

```toml
[dev-dependencies]
inboxly-store.workspace = true
```

- [ ] **Step 4: Run `cargo check -p inboxly-snooze`**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-snooze
```

**Commit:** `feat(snooze): add ReminderService with CRUD, snooze, and event emission`

---

### Task 6: Add reminder scheduler to snooze background task

**Files:**
- Modify: `inboxly-snooze/src/scheduler.rs` (or the file containing the snooze scheduler from M21)

M21 established a background tokio task that checks snoozed threads every 60 seconds. Extend it to also check due reminders and snoozed reminders.

- [ ] **Step 1: Extend the existing scheduler loop**

In the snooze scheduler's tick handler (the 60-second interval loop from M21), add two new checks:

```rust
// --- Existing: check snoozed threads ---
// (already implemented in M21)

// --- New: check due reminders ---
// Query list_due_reminders(). For each due reminder that is not snoozed,
// emit ReminderEvent::ReminderDue(reminder) to the UI.
let due_reminders = reminder_store.list_due_reminders().await;
if let Ok(reminders) = due_reminders {
    for reminder in reminders {
        if reminder.snoozed.is_none() {
            let _ = event_tx.send(ReminderEvent::ReminderDue(reminder));
        }
    }
}

// --- New: check snoozed reminders ---
// Query list_active_reminders(). For each reminder with snoozed = Some(SnoozeInfo),
// check if SnoozeUntil::Time(t) <= now. If so, clear the snooze and emit
// ReminderEvent::ReminderUnsnoozed.
let active = reminder_store.list_active_reminders().await;
if let Ok(reminders) = active {
    let now = Utc::now();
    for reminder in reminders {
        if let Some(ref snooze) = reminder.snoozed {
            match &snooze.until {
                SnoozeUntil::Time(t) if *t <= now => {
                    // Un-snooze this reminder
                    let mut unsnoozed = reminder.clone();
                    unsnoozed.snoozed = None;
                    let _ = reminder_store.update_reminder(&unsnoozed).await;
                    let _ = event_tx.send(ReminderEvent::ReminderUnsnoozed(unsnoozed));
                }
                _ => {} // Location snooze or future time — skip
            }
        }
    }
}
```

- [ ] **Step 2: Update the scheduler constructor to accept a `ReminderStore` reference**

The scheduler from M21 needs access to the store for reminder queries. Add the store as a parameter to the scheduler's `spawn` or `new` function. The exact signature depends on M21's implementation — it likely already has a store handle for thread snooze checks.

- [ ] **Step 3: Run `cargo check -p inboxly-snooze`**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-snooze
```

**Commit:** `feat(snooze): extend scheduler to check due and snoozed reminders`

---

### Task 7: Implement `ReminderRow` widget

**Files:**
- Create: `inboxly-ui/src/widgets/reminder_row.rs`
- Modify: `inboxly-ui/src/widgets/mod.rs`

The `ReminderRow` renders a single reminder in the inbox feed. Per the spec:
- Blue left border (4dp wide, `#4285f4`)
- Bell icon avatar (40dp diameter circle, blue background)
- "Reminder" label in blue (`#4285f4`, 14sp, bold)
- Title text (16sp, primary text colour)
- Due date (12sp, secondary text colour, relative format: "Today", "Tomorrow", "Mar 18")
- Same height and padding as `EmailRow` for visual alignment
- Hover reveals Done and Snooze action buttons (same as email hover actions from M20)

- [ ] **Step 1: Create `inboxly-ui/src/widgets/reminder_row.rs`**

```rust
use iced::widget::{button, container, row, text, Column, Row, Space};
use iced::{Alignment, Color, Element, Length, Padding, Theme};
use uuid::Uuid;

use inboxly_core::reminder::Reminder;

use crate::theme::InboxlyTheme;

/// Messages produced by a ReminderRow.
#[derive(Debug, Clone)]
pub enum ReminderRowMessage {
    /// User clicked the reminder row (open detail/edit).
    Clicked(Uuid),
    /// User clicked the Done action button.
    Done(Uuid),
    /// User clicked the Snooze action button.
    Snooze(Uuid),
}

/// Widget that renders a single reminder in the inbox feed.
///
/// Layout:
/// ```text
/// ┌─┬──────────────────────────────────────────┬────────┐
/// │▎│ 🔔  Reminder          [Done] [Snooze]    │ Today  │
/// │▎│     Buy groceries                        │        │
/// └─┴──────────────────────────────────────────┴────────┘
///  ↑ 4dp blue left border
/// ```
pub fn reminder_row<'a>(
    reminder: &Reminder,
    theme: &InboxlyTheme,
    hovered: bool,
) -> Element<'a, ReminderRowMessage> {
    let reminder_blue = Color::from_rgb(
        0x42 as f32 / 255.0,
        0x85 as f32 / 255.0,
        0xf4 as f32 / 255.0,
    );

    // Blue left border (4dp)
    let left_border = container(Space::new(4, Length::Fill))
        .style(move |_: &Theme| container::Style {
            background: Some(reminder_blue.into()),
            ..Default::default()
        });

    // Bell icon avatar circle (40dp)
    let avatar = container(
        text("🔔").size(20),
    )
    .width(40)
    .height(40)
    .center_x(40)
    .center_y(40)
    .style(move |_: &Theme| container::Style {
        background: Some(Color::from_rgba(
            0x42 as f32 / 255.0,
            0x85 as f32 / 255.0,
            0xf4 as f32 / 255.0,
            0.15,
        ).into()),
        border: iced::Border {
            radius: 20.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });

    // "Reminder" label in blue
    let label = text("Reminder")
        .size(14)
        .color(reminder_blue);

    // Title
    let title = text(&reminder.title)
        .size(16)
        .color(theme.primary_text);

    // Due date (formatted)
    let due_text = format_due_date(reminder.due_at);
    let due_label = text(due_text)
        .size(12)
        .color(theme.secondary_text);

    // Content column: label + title
    let content = Column::new()
        .push(label)
        .push(title)
        .spacing(2);

    // Action buttons (visible on hover)
    let actions = if hovered {
        let id = reminder.id;
        Row::new()
            .push(
                button(text("✓").size(16))
                    .on_press(ReminderRowMessage::Done(id))
                    .padding(Padding::from([4, 8])),
            )
            .push(
                button(text("🕐").size(16))
                    .on_press(ReminderRowMessage::Snooze(id))
                    .padding(Padding::from([4, 8])),
            )
            .spacing(4)
    } else {
        Row::new()
    };

    // Main row: left_border | avatar | content | spacer | actions | due_date
    let main_row = Row::new()
        .push(left_border)
        .push(Space::new(12, 0))
        .push(avatar)
        .push(Space::new(12, 0))
        .push(content)
        .push(Space::new(Length::Fill, 0))
        .push(actions)
        .push(Space::new(8, 0))
        .push(due_label)
        .align_y(Alignment::Center)
        .padding(Padding::from([12, 16]));

    // Wrap in clickable button
    let id = reminder.id;
    button(main_row)
        .on_press(ReminderRowMessage::Clicked(id))
        .width(Length::Fill)
        .padding(0)
        .into()
}

/// Format a due date for display. Uses relative labels for nearby dates.
fn format_due_date(due: chrono::DateTime<chrono::Utc>) -> String {
    use chrono::{Local, NaiveDate};

    let local_due = due.with_timezone(&Local);
    let today = Local::now().date_naive();
    let due_date = local_due.date_naive();

    if due_date == today {
        format!("{}", local_due.format("%l:%M %p").to_string().trim())
    } else if due_date == today + chrono::Duration::days(1) {
        "Tomorrow".to_string()
    } else if due_date == today - chrono::Duration::days(1) {
        "Yesterday".to_string()
    } else if due_date < today {
        // Overdue — show date with visual emphasis
        format!("{}", local_due.format("%b %-d"))
    } else if (due_date - today).num_days() < 7 {
        // Within a week — show day name
        format!("{}", local_due.format("%A"))
    } else {
        // Further out — show month + day
        format!("{}", local_due.format("%b %-d"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Local, Utc};

    #[test]
    fn format_due_date_tomorrow() {
        let tomorrow = Utc::now() + Duration::days(1);
        // Adjust to same time of day in local timezone to ensure date comparison works
        let result = format_due_date(tomorrow);
        assert_eq!(result, "Tomorrow");
    }

    #[test]
    fn format_due_date_far_future() {
        let far = Utc::now() + Duration::days(30);
        let result = format_due_date(far);
        // Should be a "Mon DD" format
        assert!(result.len() > 2);
    }
}
```

- [ ] **Step 2: Register the widget in `inboxly-ui/src/widgets/mod.rs`**

Add:

```rust
pub mod reminder_row;

pub use reminder_row::{reminder_row, ReminderRowMessage};
```

- [ ] **Step 3: Run `cargo check -p inboxly-ui`**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add ReminderRow widget with blue border and bell icon`

---

### Task 8: Mix reminders into inbox feed

**Files:**
- Modify: `inboxly-ui/src/views/inbox_feed.rs` (or wherever the inbox feed view is implemented from M17)

The inbox feed from M17 renders a `Vec<InboxItem>`. Reminders need to be fetched from `ReminderService::list_for_inbox()` and merged into the feed, sorted by date among email threads.

- [ ] **Step 1: Add reminder fetching to the inbox feed data loading**

In the feed loading/refresh logic:

```rust
// Existing: load threads and bundles for the inbox feed
let mut items: Vec<InboxItem> = /* existing thread/bundle loading */;

// New: load active reminders and merge them into the feed
let reminders = reminder_service.list_for_inbox().await?;
for reminder in reminders {
    items.push(InboxItem::Reminder(reminder));
}
```

- [ ] **Step 2: Sort the unified feed by date**

The feed is sorted by date (newest first within each section). Add a `sort_date` helper for `InboxItem`:

```rust
impl InboxItem {
    /// Returns the date used for sorting in the inbox feed.
    pub fn sort_date(&self) -> DateTime<Utc> {
        match self {
            InboxItem::Thread(t) => t.newest_date,
            InboxItem::Bundle(b) => b.newest_date,
            InboxItem::Reminder(r) => r.due_at,
            InboxItem::TripBundle(tb) => {
                // Use start_date converted to DateTime
                tb.start_date.and_hms_opt(0, 0, 0)
                    .map(|ndt| DateTime::from_naive_utc_and_offset(ndt, Utc))
                    .unwrap_or_else(Utc::now)
            }
        }
    }
}

// Sort items by date (newest first)
items.sort_by(|a, b| b.sort_date().cmp(&a.sort_date()));
```

**Note:** If `sort_date()` is better placed in `inboxly-core/src/inbox.rs` (as a method on `InboxItem`), add it there instead and import it. This keeps the sorting logic reusable across views.

- [ ] **Step 3: Update the feed rendering match arm**

In the feed's `view()` method that renders each `InboxItem`, add the `Reminder` arm:

```rust
match item {
    InboxItem::Thread(thread) => {
        // existing: render EmailRow
    }
    InboxItem::Bundle(bundle) => {
        // existing: render BundleRow
    }
    InboxItem::Reminder(reminder) => {
        let is_hovered = self.hovered_reminder == Some(reminder.id);
        reminder_row(&reminder, &self.theme, is_hovered)
            .map(|msg| match msg {
                ReminderRowMessage::Done(id) => AppMessage::ReminderDone(id),
                ReminderRowMessage::Snooze(id) => AppMessage::ShowSnoozePicker(
                    SnoozeTarget::Reminder(id),
                ),
                ReminderRowMessage::Clicked(id) => AppMessage::OpenReminder(id),
            })
    }
    InboxItem::TripBundle(trip) => {
        // existing or placeholder
    }
}
```

- [ ] **Step 4: Add `ReminderDone`, `OpenReminder`, and `SnoozeTarget::Reminder` to `AppMessage`**

In the application message enum (from M15/M17), add:

```rust
pub enum AppMessage {
    // ... existing variants ...
    ReminderDone(Uuid),
    ReminderUndone(Uuid),
    OpenReminder(Uuid),
    CreateReminder,
    // ... existing SnoozeTarget may need a Reminder variant ...
}

/// Target for the snooze picker (thread or reminder).
pub enum SnoozeTarget {
    Thread(ThreadId),
    Reminder(Uuid),
}
```

- [ ] **Step 5: Add hovered reminder tracking state**

In the app state struct, add:

```rust
/// Which reminder ID is currently hovered (for showing action buttons).
hovered_reminder: Option<Uuid>,
```

- [ ] **Step 6: Run `cargo check -p inboxly-ui`**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): mix reminders into inbox feed sorted by due date`

---

### Task 9: Implement `ReminderDialog` widget

**Files:**
- Create: `inboxly-ui/src/widgets/reminder_dialog.rs`
- Modify: `inboxly-ui/src/widgets/mod.rs`

The reminder creation dialog shows a text input ("Remember to...") and a date/time picker that reuses the snooze presets from M21.

- [ ] **Step 1: Create `inboxly-ui/src/widgets/reminder_dialog.rs`**

```rust
use chrono::{DateTime, Utc};
use iced::widget::{button, column, container, row, text, text_input, Column, Row, Space};
use iced::{Alignment, Element, Length, Padding};

use crate::theme::InboxlyTheme;
use crate::widgets::snooze_picker::{SnoozePreset, resolve_snooze_preset};

/// Messages produced by the ReminderDialog.
#[derive(Debug, Clone)]
pub enum ReminderDialogMessage {
    /// User typed in the title field.
    TitleChanged(String),
    /// User selected a time preset.
    PresetSelected(SnoozePreset),
    /// User confirmed creation.
    Confirm,
    /// User cancelled.
    Cancel,
}

/// State for the reminder creation dialog.
#[derive(Debug, Clone)]
pub struct ReminderDialogState {
    /// The text the user has typed for the reminder title.
    pub title: String,
    /// The selected due date/time. None until user picks a preset.
    pub due_at: Option<DateTime<Utc>>,
    /// Which preset was selected (for visual highlighting).
    pub selected_preset: Option<SnoozePreset>,
}

impl Default for ReminderDialogState {
    fn default() -> Self {
        Self {
            title: String::new(),
            due_at: None,
            selected_preset: None,
        }
    }
}

impl ReminderDialogState {
    /// Returns true if the dialog has enough data to create a reminder.
    pub fn is_valid(&self) -> bool {
        !self.title.trim().is_empty() && self.due_at.is_some()
    }

    /// Apply a message and return whether the dialog should close.
    pub fn update(&mut self, message: ReminderDialogMessage) -> bool {
        match message {
            ReminderDialogMessage::TitleChanged(t) => {
                self.title = t;
                false
            }
            ReminderDialogMessage::PresetSelected(preset) => {
                self.due_at = Some(resolve_snooze_preset(&preset));
                self.selected_preset = Some(preset);
                false
            }
            ReminderDialogMessage::Confirm => true,
            ReminderDialogMessage::Cancel => true,
        }
    }
}

/// Render the reminder creation dialog.
///
/// Layout:
/// ```text
/// ┌─────────────────────────────────────┐
/// │  Remember to...                     │
/// │  ┌─────────────────────────────┐    │
/// │  │ [title input field]         │    │
/// │  └─────────────────────────────┘    │
/// │                                     │
/// │  When?                              │
/// │  ┌──────────────┬──────────────┐    │
/// │  │ Later Today  │  Tomorrow    │    │
/// │  ├──────────────┼──────────────┤    │
/// │  │ This Weekend │  Next Week   │    │
/// │  ├──────────────┼──────────────┤    │
/// │  │ Someday      │  Custom...   │    │
/// │  └──────────────┴──────────────┘    │
/// │                                     │
/// │            [Cancel]  [Save]         │
/// └─────────────────────────────────────┘
/// ```
pub fn reminder_dialog<'a>(
    state: &ReminderDialogState,
    theme: &InboxlyTheme,
) -> Element<'a, ReminderDialogMessage> {
    let title_label = text("Remember to...")
        .size(18)
        .color(theme.primary_text);

    let title_input = text_input("Enter your reminder...", &state.title)
        .on_input(ReminderDialogMessage::TitleChanged)
        .padding(Padding::from([8, 12]))
        .size(16);

    let when_label = text("When?")
        .size(14)
        .color(theme.secondary_text);

    // Reuse snooze presets in a 2-column grid (same as SnoozePicker from M21)
    let presets = vec![
        ("Later Today", SnoozePreset::LaterToday),
        ("Tomorrow", SnoozePreset::Tomorrow),
        ("This Weekend", SnoozePreset::ThisWeekend),
        ("Next Week", SnoozePreset::NextWeek),
        ("Someday", SnoozePreset::Someday),
    ];

    let mut grid = Column::new().spacing(4);
    for row_presets in presets.chunks(2) {
        let mut r = Row::new().spacing(4);
        for (label, preset) in row_presets {
            let is_selected = state.selected_preset.as_ref() == Some(preset);
            let btn = button(
                container(text(*label).size(14))
                    .center_x(142)
                    .center_y(48)
            )
            .width(142)
            .on_press(ReminderDialogMessage::PresetSelected(preset.clone()));
            r = r.push(btn);
        }
        grid = grid.push(r);
    }

    // Action buttons
    let cancel_btn = button(text("Cancel").size(14))
        .on_press(ReminderDialogMessage::Cancel)
        .padding(Padding::from([8, 16]));

    let save_btn = if state.is_valid() {
        button(text("Save").size(14))
            .on_press(ReminderDialogMessage::Confirm)
            .padding(Padding::from([8, 16]))
    } else {
        button(text("Save").size(14))
            .padding(Padding::from([8, 16]))
        // No on_press — disabled
    };

    let actions_row = Row::new()
        .push(Space::new(Length::Fill, 0))
        .push(cancel_btn)
        .push(Space::new(8, 0))
        .push(save_btn)
        .align_y(Alignment::Center);

    // Full dialog layout
    let content = Column::new()
        .push(title_label)
        .push(Space::new(0, 8))
        .push(title_input)
        .push(Space::new(0, 16))
        .push(when_label)
        .push(Space::new(0, 8))
        .push(grid)
        .push(Space::new(0, 16))
        .push(actions_row)
        .padding(Padding::from(24))
        .max_width(340);

    container(content)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(theme.surface.into()),
            border: iced::Border {
                radius: 8.0.into(),
                width: 1.0,
                color: theme.divider,
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 12.0,
            },
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_default_is_invalid() {
        let state = ReminderDialogState::default();
        assert!(!state.is_valid());
    }

    #[test]
    fn state_valid_when_title_and_preset_set() {
        let mut state = ReminderDialogState::default();
        state.update(ReminderDialogMessage::TitleChanged("Buy groceries".into()));
        assert!(!state.is_valid()); // no due date yet

        state.update(ReminderDialogMessage::PresetSelected(SnoozePreset::Tomorrow));
        assert!(state.is_valid());
    }

    #[test]
    fn whitespace_only_title_is_invalid() {
        let mut state = ReminderDialogState::default();
        state.update(ReminderDialogMessage::TitleChanged("   ".into()));
        state.update(ReminderDialogMessage::PresetSelected(SnoozePreset::Tomorrow));
        assert!(!state.is_valid());
    }

    #[test]
    fn confirm_returns_should_close() {
        let mut state = ReminderDialogState::default();
        let should_close = state.update(ReminderDialogMessage::Confirm);
        assert!(should_close);
    }

    #[test]
    fn cancel_returns_should_close() {
        let mut state = ReminderDialogState::default();
        let should_close = state.update(ReminderDialogMessage::Cancel);
        assert!(should_close);
    }
}
```

- [ ] **Step 2: Register the widget in `inboxly-ui/src/widgets/mod.rs`**

Add:

```rust
pub mod reminder_dialog;

pub use reminder_dialog::{reminder_dialog, ReminderDialogMessage, ReminderDialogState};
```

- [ ] **Step 3: Run `cargo check -p inboxly-ui`**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add ReminderDialog with title input and snooze preset grid`

---

### Task 10: Implement `SpeedDialFab` widget

**Files:**
- Create: `inboxly-ui/src/widgets/speed_dial_fab.rs`
- Modify: `inboxly-ui/src/widgets/mod.rs`

Per the spec: main red FAB (56dp) at bottom-right. Click toggles between compose icon and expanded state showing two mini-FABs (Compose + Reminder). FAB options fly in from below with staggered fade+slide animation. Main button rotates on expand.

- [ ] **Step 1: Create `inboxly-ui/src/widgets/speed_dial_fab.rs`**

```rust
use iced::widget::{button, column, container, row, text, Column, Row, Space};
use iced::{Alignment, Color, Element, Length, Padding};

use crate::theme::InboxlyTheme;

/// Messages produced by the SpeedDialFab.
#[derive(Debug, Clone)]
pub enum FabMessage {
    /// User clicked the main FAB button (toggle expand/collapse).
    Toggle,
    /// User selected the Compose option.
    Compose,
    /// User selected the Reminder option.
    Reminder,
    /// User clicked away from the FAB (dismiss).
    Dismiss,
}

/// State for the speed dial FAB.
#[derive(Debug, Clone, Default)]
pub struct FabState {
    /// Whether the FAB is expanded showing options.
    pub expanded: bool,
    /// Animation progress (0.0 = collapsed, 1.0 = expanded).
    /// Used for smooth transitions.
    pub animation_progress: f32,
}

impl FabState {
    /// Toggle expanded state.
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
        self.animation_progress = if self.expanded { 1.0 } else { 0.0 };
    }

    /// Collapse the FAB.
    pub fn collapse(&mut self) {
        self.expanded = false;
        self.animation_progress = 0.0;
    }
}

/// Render the speed dial FAB.
///
/// When collapsed:
/// ```text
///                               ┌─────┐
///                               │  ✏️  │  56dp red circle
///                               └─────┘
/// ```
///
/// When expanded:
/// ```text
///                          ┌──────────────┐
///                          │ 🔔 Reminder  │  mini-fab (40dp) + label
///                          ├──────────────┤
///                          │  ✏️ Compose   │  mini-fab (40dp) + label
///                          ├──────────────┤
///                          │      ✕       │  56dp main fab (rotated to X)
///                          └──────────────┘
/// ```
pub fn speed_dial_fab<'a>(
    state: &FabState,
    theme: &InboxlyTheme,
) -> Element<'a, FabMessage> {
    let fab_red = Color::from_rgb(
        0xdb as f32 / 255.0,
        0x44 as f32 / 255.0,
        0x37 as f32 / 255.0,
    );

    if state.expanded {
        // Expanded: show options + main button
        let reminder_option = mini_fab_option(
            "🔔",
            "Reminder",
            FabMessage::Reminder,
            theme,
        );

        let compose_option = mini_fab_option(
            "✏️",
            "Compose",
            FabMessage::Compose,
            theme,
        );

        // Main FAB shows X (close) icon when expanded
        let main_fab = container(
            button(
                container(text("✕").size(24).color(Color::WHITE))
                    .center_x(56)
                    .center_y(56),
            )
            .on_press(FabMessage::Toggle)
            .width(56)
            .height(56)
            .padding(0)
            .style(move |_, _| button::Style {
                background: Some(fab_red.into()),
                border: iced::Border {
                    radius: 28.0.into(),
                    ..Default::default()
                },
                text_color: Color::WHITE,
                shadow: iced::Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
                    offset: iced::Vector::new(0.0, 4.0),
                    blur_radius: 8.0,
                },
                ..Default::default()
            }),
        );

        // Stack options above main FAB, aligned right
        let options_column = Column::new()
            .push(reminder_option)
            .push(Space::new(0, 12))
            .push(compose_option)
            .push(Space::new(0, 16))
            .push(main_fab)
            .align_x(Alignment::End)
            .padding(Padding::from([0, 13, 13, 0]));

        container(options_column)
            .width(Length::Shrink)
            .align_right(Length::Fill)
            .align_bottom(Length::Fill)
            .into()
    } else {
        // Collapsed: just the main FAB with compose icon
        let main_fab = button(
            container(text("✏️").size(24).color(Color::WHITE))
                .center_x(56)
                .center_y(56),
        )
        .on_press(FabMessage::Toggle)
        .width(56)
        .height(56)
        .padding(0)
        .style(move |_, _| button::Style {
            background: Some(fab_red.into()),
            border: iced::Border {
                radius: 28.0.into(),
                ..Default::default()
            },
            text_color: Color::WHITE,
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 8.0,
            },
            ..Default::default()
        });

        container(main_fab)
            .width(Length::Shrink)
            .padding(Padding::from([0, 13, 13, 0]))
            .align_right(Length::Fill)
            .align_bottom(Length::Fill)
            .into()
    }
}

/// Render a mini-FAB option (40dp circle + text label).
fn mini_fab_option<'a>(
    icon: &'static str,
    label: &'static str,
    message: FabMessage,
    theme: &InboxlyTheme,
) -> Element<'a, FabMessage> {
    let label_text = container(
        text(label)
            .size(14)
            .color(theme.primary_text),
    )
    .style(move |_: &iced::Theme| container::Style {
        background: Some(theme.surface.into()),
        border: iced::Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        shadow: iced::Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.2),
            offset: iced::Vector::new(0.0, 2.0),
            blur_radius: 4.0,
        },
        ..Default::default()
    })
    .padding(Padding::from([4, 8]));

    let icon_btn = button(
        container(text(icon).size(18).color(Color::WHITE))
            .center_x(40)
            .center_y(40),
    )
    .on_press(message)
    .width(40)
    .height(40)
    .padding(0)
    .style(move |_, _| button::Style {
        background: Some(theme.surface.into()),
        border: iced::Border {
            radius: 20.0.into(),
            ..Default::default()
        },
        shadow: iced::Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
            offset: iced::Vector::new(0.0, 2.0),
            blur_radius: 4.0,
        },
        ..Default::default()
    });

    Row::new()
        .push(label_text)
        .push(Space::new(12, 0))
        .push(icon_btn)
        .align_y(Alignment::Center)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fab_state_default_collapsed() {
        let state = FabState::default();
        assert!(!state.expanded);
        assert_eq!(state.animation_progress, 0.0);
    }

    #[test]
    fn fab_toggle() {
        let mut state = FabState::default();
        state.toggle();
        assert!(state.expanded);
        assert_eq!(state.animation_progress, 1.0);

        state.toggle();
        assert!(!state.expanded);
        assert_eq!(state.animation_progress, 0.0);
    }

    #[test]
    fn fab_collapse() {
        let mut state = FabState::default();
        state.toggle(); // expand
        state.collapse();
        assert!(!state.expanded);
    }
}
```

- [ ] **Step 2: Register the widget in `inboxly-ui/src/widgets/mod.rs`**

Add:

```rust
pub mod speed_dial_fab;

pub use speed_dial_fab::{speed_dial_fab, FabMessage, FabState};
```

- [ ] **Step 3: Run `cargo check -p inboxly-ui`**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add SpeedDialFab widget with expand/collapse and mini-FAB options`

---

### Task 11: Add FAB scrim overlay

**Files:**
- Modify: `inboxly-ui/src/views/inbox_feed.rs` (or the main app view)

When the FAB is expanded, a semi-transparent scrim covers the content area. Clicking the scrim collapses the FAB.

- [ ] **Step 1: Add scrim overlay to the main content area**

In the view function that composes the inbox feed, wrap the content in a stack that includes the scrim when the FAB is expanded:

```rust
use iced::widget::{mouse_area, stack, opaque};

fn view_content<'a>(
    feed: Element<'a, AppMessage>,
    fab_state: &FabState,
    theme: &InboxlyTheme,
) -> Element<'a, AppMessage> {
    let fab = speed_dial_fab(fab_state, theme)
        .map(|msg| match msg {
            FabMessage::Toggle => AppMessage::FabToggle,
            FabMessage::Compose => AppMessage::FabCompose,
            FabMessage::Reminder => AppMessage::FabReminder,
            FabMessage::Dismiss => AppMessage::FabDismiss,
        });

    if fab_state.expanded {
        // Scrim: semi-transparent dark overlay
        let scrim = mouse_area(
            container(Space::new(Length::Fill, Length::Fill))
                .style(|_: &iced::Theme| container::Style {
                    background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.4).into()),
                    ..Default::default()
                })
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_press(AppMessage::FabDismiss);

        // Stack: feed → scrim → FAB
        stack![feed, opaque(scrim), fab]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        // No scrim — just float the FAB over the feed
        stack![feed, fab]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
```

- [ ] **Step 2: Add FAB messages to `AppMessage`**

```rust
pub enum AppMessage {
    // ... existing ...
    FabToggle,
    FabCompose,
    FabReminder,
    FabDismiss,
}
```

- [ ] **Step 3: Add `FabState` to app state**

```rust
pub struct AppState {
    // ... existing ...
    fab_state: FabState,
}
```

- [ ] **Step 4: Handle FAB messages in app update**

```rust
AppMessage::FabToggle => {
    self.fab_state.toggle();
}
AppMessage::FabDismiss => {
    self.fab_state.collapse();
}
AppMessage::FabCompose => {
    self.fab_state.collapse();
    // Navigate to compose view (from M23, or emit pending message)
    // For now, set a flag or transition to compose state
    return self.open_compose();
}
AppMessage::FabReminder => {
    self.fab_state.collapse();
    self.show_reminder_dialog = true;
    self.reminder_dialog_state = ReminderDialogState::default();
}
```

- [ ] **Step 5: Run `cargo check -p inboxly-ui`**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add scrim overlay when FAB is expanded`

---

### Task 12: Wire FAB actions to compose + reminder dialog

**Files:**
- Modify: `inboxly-ui/src/views/inbox_feed.rs` (or main app view)
- Modify: `inboxly-ui/src/app.rs` (or main app module)

Connect the FAB's Compose and Reminder actions to the actual features.

- [ ] **Step 1: Show reminder dialog when FAB→Reminder is selected**

In the view function, conditionally render the `ReminderDialog` as a modal overlay:

```rust
fn view(&self) -> Element<AppMessage> {
    let mut content = self.view_inbox_feed();
    content = self.view_content(content, &self.fab_state, &self.theme);

    if self.show_reminder_dialog {
        let dialog = reminder_dialog(&self.reminder_dialog_state, &self.theme)
            .map(|msg| AppMessage::ReminderDialog(msg));

        // Center the dialog over a scrim
        let dialog_scrim = mouse_area(
            container(Space::new(Length::Fill, Length::Fill))
                .style(|_: &iced::Theme| container::Style {
                    background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.5).into()),
                    ..Default::default()
                })
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_press(AppMessage::ReminderDialog(ReminderDialogMessage::Cancel));

        let centered_dialog = container(dialog)
            .center_x(Length::Fill)
            .center_y(Length::Fill);

        content = stack![content, opaque(dialog_scrim), centered_dialog]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }

    content
}
```

- [ ] **Step 2: Handle `ReminderDialog` messages in app update**

```rust
AppMessage::ReminderDialog(msg) => {
    let should_close = self.reminder_dialog_state.update(msg.clone());
    if should_close {
        match msg {
            ReminderDialogMessage::Confirm => {
                if self.reminder_dialog_state.is_valid() {
                    let title = self.reminder_dialog_state.title.clone();
                    let due_at = self.reminder_dialog_state.due_at.unwrap();
                    // Spawn async task to create the reminder
                    return iced::Task::perform(
                        async move {
                            // The actual creation uses ReminderService
                            (title, due_at)
                        },
                        |(title, due_at)| AppMessage::CreateReminderConfirmed { title, due_at },
                    );
                }
            }
            ReminderDialogMessage::Cancel => {}
            _ => {}
        }
        self.show_reminder_dialog = false;
    }
}

AppMessage::CreateReminderConfirmed { title, due_at } => {
    // Call ReminderService.create() via the async runtime
    let service = self.reminder_service.clone();
    return iced::Task::perform(
        async move { service.create(title, due_at).await },
        |result| match result {
            Ok(reminder) => AppMessage::ReminderCreated(reminder),
            Err(e) => AppMessage::Error(e.to_string()),
        },
    );
}

AppMessage::ReminderCreated(reminder) => {
    // Refresh the inbox feed to include the new reminder
    self.refresh_inbox_feed();
}
```

- [ ] **Step 3: Add state fields**

```rust
pub struct AppState {
    // ... existing ...
    show_reminder_dialog: bool,
    reminder_dialog_state: ReminderDialogState,
}
```

- [ ] **Step 4: Run `cargo check -p inboxly-ui`**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): wire FAB compose and reminder actions with dialog modal`

---

### Task 13: Implement reminder done action

**Files:**
- Modify: `inboxly-ui/src/app.rs` (or main app update handler)

When the user clicks Done on a `ReminderRow` (or swipes it), mark the reminder as done and show an undo snackbar (same UX as email Done from M19).

- [ ] **Step 1: Handle `ReminderDone` message**

```rust
AppMessage::ReminderDone(id) => {
    // Optimistically remove the reminder from the feed
    self.inbox_items.retain(|item| {
        !matches!(item, InboxItem::Reminder(r) if r.id == id)
    });

    // Show undo snackbar (reuse M19's snackbar infrastructure)
    self.show_undo_snackbar(
        format!("Reminder marked done"),
        AppMessage::ReminderUndone(id),
    );

    // Async: mark done in database
    let service = self.reminder_service.clone();
    return iced::Task::perform(
        async move { service.mark_done(id).await },
        |result| match result {
            Ok(()) => AppMessage::Noop,
            Err(e) => AppMessage::Error(e.to_string()),
        },
    );
}

AppMessage::ReminderUndone(id) => {
    // Undo: mark undone and refresh feed
    let service = self.reminder_service.clone();
    return iced::Task::perform(
        async move {
            service.mark_undone(id).await?;
            Ok(())
        },
        |result: std::result::Result<(), InboxlyError>| match result {
            Ok(()) => AppMessage::RefreshInboxFeed,
            Err(e) => AppMessage::Error(e.to_string()),
        },
    );
}
```

- [ ] **Step 2: Run `cargo check -p inboxly-ui`**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement reminder done action with undo snackbar`

---

### Task 14: Implement reminder snooze action

**Files:**
- Modify: `inboxly-ui/src/app.rs` (or main app update handler)

When the user clicks Snooze on a `ReminderRow`, open the snooze picker (from M21) targeting the reminder. When a preset is selected, snooze the reminder using `ReminderService::snooze()`.

- [ ] **Step 1: Handle `ShowSnoozePicker` with `SnoozeTarget::Reminder`**

The snooze picker from M21 should already handle `SnoozeTarget`. Extend the existing snooze confirm handler:

```rust
AppMessage::SnoozeConfirmed { target, until } => {
    match target {
        SnoozeTarget::Thread(thread_id) => {
            // existing M21 implementation
        }
        SnoozeTarget::Reminder(reminder_id) => {
            // Remove reminder from feed optimistically
            self.inbox_items.retain(|item| {
                !matches!(item, InboxItem::Reminder(r) if r.id == reminder_id)
            });

            let service = self.reminder_service.clone();
            let snooze_until = until.clone();
            return iced::Task::perform(
                async move { service.snooze(reminder_id, snooze_until).await },
                |result| match result {
                    Ok(()) => AppMessage::Noop,
                    Err(e) => AppMessage::Error(e.to_string()),
                },
            );
        }
    }
}
```

- [ ] **Step 2: Show snoozed reminders in the Snoozed view**

In the Snoozed view (from M21), add snoozed reminders:

```rust
// In the Snoozed view data loading:
let snoozed_reminders = reminder_service.list_all().await?
    .into_iter()
    .filter(|r| r.is_snoozed() && !r.done)
    .map(InboxItem::Reminder)
    .collect::<Vec<_>>();

// Merge into the snoozed items list
snoozed_items.extend(snoozed_reminders);
```

- [ ] **Step 3: Run `cargo check -p inboxly-ui`**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement reminder snooze with picker and snoozed view integration`

---

### Task 15: Add `RemindersView` to nav drawer

**Files:**
- Create: `inboxly-ui/src/views/reminders_view.rs`
- Modify: `inboxly-ui/src/views/mod.rs`
- Modify: `inboxly-ui/src/app.rs` (nav drawer click handler)

The nav drawer from M15 shows a "Reminders" item. Clicking it shows a filtered list of all reminders (active + done).

- [ ] **Step 1: Create `inboxly-ui/src/views/reminders_view.rs`**

```rust
use iced::widget::{column, container, scrollable, text, Column, Space};
use iced::{Element, Length, Padding};
use uuid::Uuid;

use inboxly_core::reminder::Reminder;

use crate::theme::InboxlyTheme;
use crate::widgets::reminder_row::{reminder_row, ReminderRowMessage};

/// Messages produced by the RemindersView.
#[derive(Debug, Clone)]
pub enum RemindersViewMessage {
    /// A reminder row action.
    Row(ReminderRowMessage),
}

/// State for the Reminders view.
#[derive(Debug, Clone, Default)]
pub struct RemindersViewState {
    /// All reminders (active + done).
    pub reminders: Vec<Reminder>,
    /// Which reminder is hovered.
    pub hovered_reminder: Option<Uuid>,
}

/// Render the Reminders view.
///
/// Layout:
/// ```text
/// ┌────────────────────────────────────┐
/// │  Reminders                         │  (view title, 20sp)
/// ├────────────────────────────────────┤
/// │  Active                            │  (section header)
/// │  ┌──────────────────────────────┐  │
/// │  │ 🔔 Reminder: Buy groceries  │  │
/// │  │ 🔔 Reminder: Call dentist   │  │
/// │  └──────────────────────────────┘  │
/// │  Done                              │  (section header)
/// │  ┌──────────────────────────────┐  │
/// │  │ ✓ Pick up package           │  │
/// │  └──────────────────────────────┘  │
/// └────────────────────────────────────┘
/// ```
pub fn reminders_view<'a>(
    state: &RemindersViewState,
    theme: &InboxlyTheme,
) -> Element<'a, RemindersViewMessage> {
    let title = text("Reminders")
        .size(20)
        .color(theme.primary_text);

    let (active, done): (Vec<_>, Vec<_>) = state
        .reminders
        .iter()
        .partition(|r| !r.done);

    let mut content = Column::new()
        .push(title)
        .push(Space::new(0, 16))
        .spacing(0);

    // Active section
    if !active.is_empty() {
        let header = text("Active")
            .size(14)
            .color(theme.secondary_text);
        content = content.push(header).push(Space::new(0, 8));

        for reminder in &active {
            let is_hovered = state.hovered_reminder == Some(reminder.id);
            let row = reminder_row(reminder, theme, is_hovered)
                .map(RemindersViewMessage::Row);
            content = content.push(row);
        }
        content = content.push(Space::new(0, 16));
    }

    // Done section
    if !done.is_empty() {
        let header = text("Done")
            .size(14)
            .color(theme.secondary_text);
        content = content.push(header).push(Space::new(0, 8));

        for reminder in &done {
            let is_hovered = state.hovered_reminder == Some(reminder.id);
            let row = reminder_row(reminder, theme, is_hovered)
                .map(RemindersViewMessage::Row);
            content = content.push(row);
        }
    }

    // Empty state
    if active.is_empty() && done.is_empty() {
        let empty = text("No reminders yet. Create one with the + button.")
            .size(14)
            .color(theme.secondary_text);
        content = content.push(Space::new(0, 32)).push(
            container(empty)
                .center_x(Length::Fill)
        );
    }

    scrollable(
        container(content)
            .padding(Padding::from(16))
            .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_empty() {
        let state = RemindersViewState::default();
        assert!(state.reminders.is_empty());
        assert!(state.hovered_reminder.is_none());
    }
}
```

- [ ] **Step 2: Register the view in `inboxly-ui/src/views/mod.rs`**

Add:

```rust
pub mod reminders_view;

pub use reminders_view::{reminders_view, RemindersViewMessage, RemindersViewState};
```

- [ ] **Step 3: Add nav drawer routing**

In the app's nav drawer click handler (from M15), handle the "Reminders" nav item:

```rust
NavItem::Reminders => {
    self.current_view = View::Reminders;
    // Load reminders
    let service = self.reminder_service.clone();
    return iced::Task::perform(
        async move { service.list_all().await },
        |result| match result {
            Ok(reminders) => AppMessage::RemindersLoaded(reminders),
            Err(e) => AppMessage::Error(e.to_string()),
        },
    );
}
```

Add the `View::Reminders` variant to the view enum:

```rust
pub enum View {
    Inbox,
    Snoozed,
    Done,
    Reminders,  // New
    // ... others from M15 ...
}
```

Handle `RemindersLoaded`:

```rust
AppMessage::RemindersLoaded(reminders) => {
    self.reminders_view_state.reminders = reminders;
}
```

- [ ] **Step 4: Render `RemindersView` when `current_view == View::Reminders`**

In the main view dispatch:

```rust
View::Reminders => {
    reminders_view(&self.reminders_view_state, &self.theme)
        .map(|msg| match msg {
            RemindersViewMessage::Row(row_msg) => match row_msg {
                ReminderRowMessage::Done(id) => AppMessage::ReminderDone(id),
                ReminderRowMessage::Snooze(id) => AppMessage::ShowSnoozePicker(
                    SnoozeTarget::Reminder(id),
                ),
                ReminderRowMessage::Clicked(id) => AppMessage::OpenReminder(id),
            },
        })
}
```

- [ ] **Step 5: Run `cargo check -p inboxly-ui`**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add RemindersView to nav drawer with active/done sections`

---

### Task 16: Integration tests

**Files:**
- Create: `inboxly-snooze/tests/reminder_integration.rs`

End-to-end tests that exercise the full reminder lifecycle: create → appear in feed → snooze → unsnooze → done → undo.

- [ ] **Step 1: Create integration test file**

```rust
//! Integration tests for the reminder system.
//!
//! Tests the full flow: ReminderService → ReminderStore (SQLite) → feed integration.

use chrono::{Duration, Utc};
use tokio::sync::mpsc;

use inboxly_core::inbox::{InboxItem, SnoozeUntil};
use inboxly_core::reminder::Reminder;
use inboxly_core::ReminderStore;
use inboxly_snooze::{ReminderEvent, ReminderService};
use inboxly_store::SqliteStore;

async fn setup() -> (ReminderService<SqliteStore>, mpsc::UnboundedReceiver<ReminderEvent>) {
    let store = SqliteStore::in_memory().await.unwrap();
    let (tx, rx) = mpsc::unbounded_channel();
    let service = ReminderService::new(store, tx);
    (service, rx)
}

#[tokio::test]
async fn create_and_list_for_inbox() {
    let (service, mut rx) = setup().await;

    let reminder = service
        .create("Buy groceries".into(), Utc::now() + Duration::hours(2))
        .await
        .unwrap();

    // Should emit ReminderCreated event
    let event = rx.try_recv().unwrap();
    assert!(matches!(event, ReminderEvent::ReminderCreated(_)));

    // Should appear in inbox feed list
    let feed = service.list_for_inbox().await.unwrap();
    assert_eq!(feed.len(), 1);
    assert_eq!(feed[0].title, "Buy groceries");
}

#[tokio::test]
async fn done_removes_from_feed() {
    let (service, mut rx) = setup().await;

    let reminder = service
        .create("Test".into(), Utc::now() + Duration::hours(1))
        .await
        .unwrap();
    let _ = rx.try_recv(); // consume created event

    service.mark_done(reminder.id).await.unwrap();

    let event = rx.try_recv().unwrap();
    assert!(matches!(event, ReminderEvent::ReminderDone(_)));

    let feed = service.list_for_inbox().await.unwrap();
    assert!(feed.is_empty());
}

#[tokio::test]
async fn undone_restores_to_feed() {
    let (service, mut rx) = setup().await;

    let reminder = service
        .create("Test".into(), Utc::now() + Duration::hours(1))
        .await
        .unwrap();
    let _ = rx.try_recv(); // consume created event

    service.mark_done(reminder.id).await.unwrap();
    let _ = rx.try_recv(); // consume done event

    service.mark_undone(reminder.id).await.unwrap();

    let event = rx.try_recv().unwrap();
    assert!(matches!(event, ReminderEvent::ReminderUndone(_)));

    let feed = service.list_for_inbox().await.unwrap();
    assert_eq!(feed.len(), 1);
}

#[tokio::test]
async fn snooze_hides_from_inbox_feed() {
    let (service, mut rx) = setup().await;

    let reminder = service
        .create("Test".into(), Utc::now() + Duration::hours(1))
        .await
        .unwrap();
    let _ = rx.try_recv(); // consume created event

    let snooze_until = SnoozeUntil::Time(Utc::now() + Duration::days(1));
    service.snooze(reminder.id, snooze_until).await.unwrap();

    let event = rx.try_recv().unwrap();
    assert!(matches!(event, ReminderEvent::ReminderSnoozed { .. }));

    // Snoozed reminders should NOT appear in inbox feed
    let feed = service.list_for_inbox().await.unwrap();
    assert!(feed.is_empty());

    // But should still appear in list_all
    let all = service.list_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].is_snoozed());
}

#[tokio::test]
async fn unsnooze_returns_to_feed() {
    let (service, mut rx) = setup().await;

    let reminder = service
        .create("Test".into(), Utc::now() + Duration::hours(1))
        .await
        .unwrap();
    let _ = rx.try_recv();

    let snooze_until = SnoozeUntil::Time(Utc::now() + Duration::days(1));
    service.snooze(reminder.id, snooze_until).await.unwrap();
    let _ = rx.try_recv();

    service.unsnooze(reminder.id).await.unwrap();

    let event = rx.try_recv().unwrap();
    assert!(matches!(event, ReminderEvent::ReminderUnsnoozed(_)));

    let feed = service.list_for_inbox().await.unwrap();
    assert_eq!(feed.len(), 1);
    assert!(!feed[0].is_snoozed());
}

#[tokio::test]
async fn create_rejects_empty_title() {
    let (service, _rx) = setup().await;
    let result = service.create("".into(), Utc::now()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn create_rejects_whitespace_title() {
    let (service, _rx) = setup().await;
    let result = service.create("   ".into(), Utc::now()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn update_title_and_due() {
    let (service, mut rx) = setup().await;

    let reminder = service
        .create("Original".into(), Utc::now() + Duration::hours(1))
        .await
        .unwrap();
    let _ = rx.try_recv();

    let new_due = Utc::now() + Duration::days(2);
    service
        .update(reminder.id, Some("Updated".into()), Some(new_due))
        .await
        .unwrap();

    let feed = service.list_for_inbox().await.unwrap();
    assert_eq!(feed[0].title, "Updated");
}

#[tokio::test]
async fn delete_removes_permanently() {
    let (service, mut rx) = setup().await;

    let reminder = service
        .create("Delete me".into(), Utc::now() + Duration::hours(1))
        .await
        .unwrap();
    let _ = rx.try_recv();

    service.delete(reminder.id).await.unwrap();

    let all = service.list_all().await.unwrap();
    assert!(all.is_empty());
}

#[tokio::test]
async fn inbox_item_conversion() {
    let reminder = Reminder::new("Test", Utc::now());
    let item = ReminderService::<SqliteStore>::to_inbox_item(&reminder);
    assert!(matches!(item, InboxItem::Reminder(_)));
}
```

- [ ] **Step 2: Run all tests**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test --workspace
```

- [ ] **Step 3: Run clippy**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo clippy --workspace -- -D warnings
```

**Commit:** `test(snooze): add reminder integration tests for full lifecycle`

---

## Final Verification

After all tasks are complete:

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test --workspace && cargo clippy --workspace -- -D warnings
```

**Expected state:**
- `inboxly-core`: `Reminder` struct, `ReminderLocation`, `ReminderStore` trait, `InboxItem::Reminder(Reminder)` variant
- `inboxly-store`: `ReminderStore` impl for `SqliteStore`, migration adding `created_at` + `snoozed_json` columns
- `inboxly-snooze`: `ReminderService` with full CRUD + snooze, `ReminderEvent` enum, scheduler integration
- `inboxly-ui`: `ReminderRow`, `SpeedDialFab`, `ReminderDialog`, `RemindersView` widgets, FAB scrim overlay, reminder done/snooze actions, nav drawer routing

## Licence

GPL-3.0
