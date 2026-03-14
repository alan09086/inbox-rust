//! Bundler event types for UI notification.
//!
//! These events are emitted by the bundler subsystem (throttle changes,
//! bundle reassignments) and consumed by the UI layer to refresh views.

use inboxly_core::throttle::BundleThrottle;
use inboxly_core::BundleId;
use uuid::Uuid;

/// Events emitted by the bundler subsystem.
///
/// The UI subscribes to these events to know when to refresh the inbox
/// feed, bundle list, or thread views.
#[derive(Debug, Clone)]
pub enum BundlerEvent {
    /// A bundle's throttle setting was changed by the user.
    ThrottleChanged {
        /// Which bundle had its throttle updated.
        bundle_id: BundleId,
        /// The new throttle setting.
        throttle: BundleThrottle,
    },

    /// A thread's bundle assignment changed (e.g., due to body re-evaluation
    /// or user manual move).
    BundleChanged {
        /// The thread whose assignment changed.
        thread_id: Uuid,
        /// The previous bundle (None if uncategorised).
        old_bundle: Option<BundleId>,
        /// The new bundle (None if uncategorised).
        new_bundle: Option<BundleId>,
    },

    /// One or more throttle windows have opened. The UI should refresh
    /// the inbox feed to show newly-visible bundles.
    ThrottleWindowOpened {
        /// Bundle IDs whose delivery windows just opened.
        bundle_ids: Vec<BundleId>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    #[test]
    fn throttle_changed_event_construction() {
        let event = BundlerEvent::ThrottleChanged {
            bundle_id: BundleId::new(),
            throttle: BundleThrottle::Daily {
                delivery_time: NaiveTime::from_hms_opt(17, 0, 0).expect("valid time"),
            },
        };
        assert!(matches!(event, BundlerEvent::ThrottleChanged { .. }));
    }

    #[test]
    fn bundle_changed_event_construction() {
        let event = BundlerEvent::BundleChanged {
            thread_id: Uuid::new_v4(),
            old_bundle: Some(BundleId::new()),
            new_bundle: Some(BundleId::new()),
        };
        assert!(matches!(event, BundlerEvent::BundleChanged { .. }));
    }

    #[test]
    fn throttle_window_opened_event_construction() {
        let event = BundlerEvent::ThrottleWindowOpened {
            bundle_ids: vec![BundleId::new(), BundleId::new()],
        };
        if let BundlerEvent::ThrottleWindowOpened { bundle_ids } = event {
            assert_eq!(bundle_ids.len(), 2);
        } else {
            panic!("wrong variant");
        }
    }
}
