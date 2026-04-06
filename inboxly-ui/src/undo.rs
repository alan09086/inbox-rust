//! Undo state management -- timed undo for inbox actions.
//!
//! When a user marks a thread as Done or sweeps, the action is applied
//! optimistically to the local UI. A snackbar appears with an "Undo" button.
//! If the user clicks Undo within the timeout (default 7 seconds), the
//! action is reversed. Otherwise it commits.

use std::time::{Duration, Instant};

/// Default undo timeout (7 seconds, matching Google Inbox).
pub const UNDO_TIMEOUT: Duration = Duration::from_secs(7);

/// An undoable action stored while the snackbar is visible.
#[derive(Debug, Clone)]
pub enum UndoAction {
    /// A single thread was marked as Done.
    MarkDone {
        /// Thread ID that was marked done.
        thread_id: String,
    },
    /// A pin toggle was applied.
    TogglePin {
        /// Thread ID whose pin state was toggled.
        thread_id: String,
        /// The pin state *before* the toggle (true = was pinned).
        was_pinned: bool,
    },
    /// A sweep cleared all unpinned threads from the inbox.
    Sweep {
        /// Thread IDs that were marked as done by the sweep.
        thread_ids: Vec<String>,
    },
}

impl UndoAction {
    /// Human-readable description for the undo snackbar.
    pub fn description(&self) -> String {
        match self {
            Self::MarkDone { .. } => "Conversation marked done".to_owned(),
            Self::TogglePin { was_pinned, .. } => {
                if *was_pinned {
                    "Unpinned".to_owned()
                } else {
                    "Pinned".to_owned()
                }
            }
            Self::Sweep { thread_ids } => {
                let count = thread_ids.len();
                if count == 1 {
                    "1 conversation marked done".to_owned()
                } else {
                    format!("{count} conversations marked done")
                }
            }
        }
    }
}

/// Current undo state -- either empty or holding an undoable action.
#[derive(Debug, Clone)]
pub struct UndoState {
    /// The pending undoable action, if any.
    action: Option<UndoAction>,
    /// When the undo window started.
    started_at: Option<Instant>,
    /// Monotonically increasing counter, incremented on each `push()`.
    ///
    /// The UI's undo-expire timer captures this value when spawned and
    /// compares it when firing — if they differ, a newer action has
    /// replaced the one the timer was spawned for, so the timer does
    /// nothing (the newer action's timer will handle its own expiry).
    generation: u64,
}

impl Default for UndoState {
    fn default() -> Self {
        Self::new()
    }
}

impl UndoState {
    /// Create empty undo state.
    pub const fn new() -> Self {
        Self {
            action: None,
            started_at: None,
            generation: 0,
        }
    }

    /// Record an undoable action. Replaces any existing pending action
    /// (which is implicitly committed).
    pub fn push(&mut self, action: UndoAction) {
        self.generation = self.generation.wrapping_add(1);
        self.action = Some(action);
        self.started_at = Some(Instant::now());
    }

    /// Return the current generation counter.
    ///
    /// The UI's expire timer captures this on spawn and re-checks it
    /// when the sleep finishes; a mismatch means a newer push has
    /// occurred, so the old timer should no-op.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Take the pending action (for undo). Returns None if no action or if expired.
    pub fn take(&mut self) -> Option<UndoAction> {
        if self.is_expired() {
            self.clear();
            return None;
        }
        let action = self.action.take();
        self.started_at = None;
        action
    }

    /// Check if an undo action is pending and not expired.
    pub fn is_active(&self) -> bool {
        self.action.is_some() && !self.is_expired()
    }

    /// Get the description text for the snackbar.
    pub fn description(&self) -> Option<String> {
        if self.is_active() {
            self.action.as_ref().map(UndoAction::description)
        } else {
            None
        }
    }

    /// Clear the undo state (action committed or expired).
    pub fn clear(&mut self) {
        self.action = None;
        self.started_at = None;
    }

    /// Check if the undo window has expired.
    fn is_expired(&self) -> bool {
        self.started_at
            .is_some_and(|started| started.elapsed() >= UNDO_TIMEOUT)
    }

    /// Time remaining in the undo window (for progress display).
    pub fn time_remaining(&self) -> Duration {
        match self.started_at {
            Some(started) => UNDO_TIMEOUT.saturating_sub(started.elapsed()),
            None => Duration::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn new_state_is_inactive() {
        let state = UndoState::new();
        assert!(!state.is_active());
        assert!(state.description().is_none());
    }

    #[test]
    fn push_makes_active() {
        let mut state = UndoState::new();
        state.push(UndoAction::MarkDone {
            thread_id: "t1".into(),
        });
        assert!(state.is_active());
        assert!(state.description().is_some());
    }

    #[test]
    fn take_returns_action() {
        let mut state = UndoState::new();
        state.push(UndoAction::MarkDone {
            thread_id: "t1".into(),
        });
        let action = state.take();
        assert!(action.is_some());
        assert!(!state.is_active());
    }

    #[test]
    fn take_after_clear_returns_none() {
        let mut state = UndoState::new();
        state.push(UndoAction::MarkDone {
            thread_id: "t1".into(),
        });
        state.clear();
        assert!(state.take().is_none());
    }

    #[test]
    fn description_for_mark_done() {
        let action = UndoAction::MarkDone {
            thread_id: "t1".into(),
        };
        assert_eq!(action.description(), "Conversation marked done");
    }

    #[test]
    fn description_for_sweep() {
        let action = UndoAction::Sweep {
            thread_ids: vec!["t1".into(), "t2".into(), "t3".into()],
        };
        assert_eq!(action.description(), "3 conversations marked done");
    }

    #[test]
    fn description_for_single_sweep() {
        let action = UndoAction::Sweep {
            thread_ids: vec!["t1".into()],
        };
        assert_eq!(action.description(), "1 conversation marked done");
    }

    #[test]
    fn description_for_pin() {
        let pin = UndoAction::TogglePin {
            thread_id: "t1".into(),
            was_pinned: false,
        };
        assert_eq!(pin.description(), "Pinned");

        let unpin = UndoAction::TogglePin {
            thread_id: "t1".into(),
            was_pinned: true,
        };
        assert_eq!(unpin.description(), "Unpinned");
    }

    #[test]
    fn push_replaces_existing() {
        let mut state = UndoState::new();
        state.push(UndoAction::MarkDone {
            thread_id: "t1".into(),
        });
        state.push(UndoAction::MarkDone {
            thread_id: "t2".into(),
        });
        let action = state.take().expect("should have action");
        if let UndoAction::MarkDone { thread_id } = action {
            assert_eq!(thread_id, "t2");
        } else {
            panic!("expected MarkDone");
        }
    }

    #[test]
    fn expired_action_not_returned() {
        let mut state = UndoState::new();
        state.push(UndoAction::MarkDone {
            thread_id: "t1".into(),
        });
        // Override started_at to simulate expiry.
        state.started_at = Some(Instant::now() - UNDO_TIMEOUT - Duration::from_secs(1));
        assert!(!state.is_active());
        assert!(state.take().is_none());
    }

    #[test]
    fn generation_increments_on_push() {
        let mut state = UndoState::new();
        assert_eq!(state.generation(), 0);
        state.push(UndoAction::MarkDone {
            thread_id: "t1".into(),
        });
        let gen1 = state.generation();
        assert_eq!(gen1, 1);
        state.push(UndoAction::MarkDone {
            thread_id: "t2".into(),
        });
        assert!(state.generation() > gen1);
    }

    #[test]
    fn generation_preserved_on_take_and_clear() {
        let mut state = UndoState::new();
        state.push(UndoAction::MarkDone {
            thread_id: "t1".into(),
        });
        let gen_before = state.generation();
        let _ = state.take();
        assert_eq!(
            state.generation(),
            gen_before,
            "take must not change generation"
        );
        state.push(UndoAction::MarkDone {
            thread_id: "t2".into(),
        });
        assert!(state.generation() > gen_before);
        state.clear();
        assert_eq!(
            state.generation(),
            gen_before + 1,
            "clear must not change generation"
        );
    }
}
