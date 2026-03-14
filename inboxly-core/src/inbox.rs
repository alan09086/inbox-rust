use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::bundle::Bundle;
use crate::highlight::TripBundle;
use crate::id::{BundleId, ThreadId};
use crate::thread::Thread;

/// A single item in the unified inbox feed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InboxItem {
    /// A conversation thread (possibly with highlight cards).
    Thread(Thread),
    /// A collapsed bundle grouping multiple threads.
    Bundle(Bundle),
    /// A user-created reminder (non-email task).
    Reminder {
        id: Uuid,
        title: String,
        due: DateTime<Utc>,
        done: bool,
    },
    /// Auto-grouped travel itinerary.
    TripBundle(TripBundle),
}

/// Per-thread state that lives in SQLite (local-only, not synced to IMAP).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadState {
    /// The thread this state applies to.
    pub thread_id: ThreadId,
    /// Pinned threads stay at the top and survive sweep.
    pub pinned: bool,
    /// Done (archived) — removed from inbox feed.
    pub done: bool,
    /// Snooze info, if this thread is snoozed.
    pub snoozed: Option<SnoozeInfo>,
    /// Bundle assignment, if categorised.
    pub bundle_id: Option<BundleId>,
    /// Extracted highlights for this thread.
    pub highlights: Vec<crate::highlight::Highlight>,
}

/// Information about a snoozed item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnoozeInfo {
    /// When (or where) to un-snooze.
    pub until: SnoozeUntil,
    /// Original inbox date (for restoring position context).
    pub original_date: DateTime<Utc>,
}

/// Snooze trigger — time-based or location-based.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SnoozeUntil {
    /// Un-snooze at a specific time.
    Time(DateTime<Utc>),
    /// Un-snooze when device enters a geofence.
    Location {
        lat: f64,
        lng: f64,
        radius_m: f64,
        label: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snooze_until_time() {
        let snooze = SnoozeUntil::Time(Utc::now() + chrono::Duration::hours(4));
        match &snooze {
            SnoozeUntil::Time(t) => assert!(*t > Utc::now()),
            _ => panic!("expected Time variant"),
        }
    }

    #[test]
    fn snooze_until_location() {
        let snooze = SnoozeUntil::Location {
            lat: 43.6532,
            lng: -79.3832,
            radius_m: 500.0,
            label: "Office".into(),
        };
        match &snooze {
            SnoozeUntil::Location { label, .. } => assert_eq!(label, "Office"),
            _ => panic!("expected Location variant"),
        }
    }

    #[test]
    fn thread_state_default_values() {
        let state = ThreadState {
            thread_id: ThreadId::new(),
            pinned: false,
            done: false,
            snoozed: None,
            bundle_id: None,
            highlights: vec![],
        };
        assert!(!state.pinned);
        assert!(!state.done);
        assert!(state.snoozed.is_none());
    }

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

    #[test]
    fn snooze_info_serde_roundtrip() {
        let info = SnoozeInfo {
            until: SnoozeUntil::Time(Utc::now()),
            original_date: Utc::now(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: SnoozeInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, back);
    }
}
