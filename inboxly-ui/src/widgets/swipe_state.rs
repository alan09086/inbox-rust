//! Swipe gesture state tracking for inbox rows.
//!
//! Tracks per-row swipe direction, drag offset, and threshold states.
//! The actual rendering is handled by wrapping email rows in a container
//! that applies the offset. Full custom Widget swipe with arm/commit
//! thresholds is deferred to M25 polish.

use std::collections::HashMap;

/// Swipe direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDirection {
    /// Right drag = Done (green + checkmark).
    Right,
    /// Left drag = Snooze (orange + clock).
    Left,
}

/// Per-row swipe state.
#[derive(Debug, Clone)]
pub struct SwipeState {
    /// Current drag offset in logical pixels (positive = right).
    pub offset: f32,
    /// Direction determined from initial drag.
    pub direction: Option<SwipeDirection>,
    /// Whether the swipe has crossed the arm threshold (50% of row width).
    pub armed: bool,
}

impl Default for SwipeState {
    fn default() -> Self {
        Self {
            offset: 0.0,
            direction: None,
            armed: false,
        }
    }
}

impl SwipeState {
    /// Reset swipe state (drag ended without commit).
    pub fn reset(&mut self) {
        self.offset = 0.0;
        self.direction = None;
        self.armed = false;
    }

    /// Update offset from a drag event.
    pub fn update_offset(&mut self, delta_x: f32, row_width: f32) {
        self.offset += delta_x;

        // Determine direction from offset sign.
        if self.offset.abs() > 5.0 {
            self.direction = Some(if self.offset > 0.0 {
                SwipeDirection::Right
            } else {
                SwipeDirection::Left
            });
        }

        // Arm at 50% of row width.
        let threshold = row_width * 0.5;
        self.armed = self.offset.abs() >= threshold;
    }
}

/// Collection of swipe states keyed by row identifier.
#[derive(Debug, Clone, Default)]
pub struct SwipeStates {
    states: HashMap<String, SwipeState>,
}

impl SwipeStates {
    /// Get or create a swipe state for a row.
    pub fn get_mut(&mut self, row_id: &str) -> &mut SwipeState {
        self.states
            .entry(row_id.to_owned())
            .or_default()
    }

    /// Reset swipe state for a row.
    pub fn reset(&mut self, row_id: &str) {
        if let Some(state) = self.states.get_mut(row_id) {
            state.reset();
        }
    }

    /// Clear all swipe states (e.g., after feed reload).
    pub fn clear(&mut self) {
        self.states.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_neutral() {
        let state = SwipeState::default();
        assert_eq!(state.offset, 0.0);
        assert!(state.direction.is_none());
        assert!(!state.armed);
    }

    #[test]
    fn right_drag_sets_direction() {
        let mut state = SwipeState::default();
        state.update_offset(10.0, 400.0);
        assert_eq!(state.direction, Some(SwipeDirection::Right));
    }

    #[test]
    fn left_drag_sets_direction() {
        let mut state = SwipeState::default();
        state.update_offset(-10.0, 400.0);
        assert_eq!(state.direction, Some(SwipeDirection::Left));
    }

    #[test]
    fn arms_at_50_percent() {
        let mut state = SwipeState::default();
        state.update_offset(200.0, 400.0); // 200 = 50% of 400
        assert!(state.armed);
    }

    #[test]
    fn does_not_arm_below_threshold() {
        let mut state = SwipeState::default();
        state.update_offset(100.0, 400.0); // 100 = 25% of 400
        assert!(!state.armed);
    }

    #[test]
    fn reset_clears_state() {
        let mut state = SwipeState::default();
        state.update_offset(200.0, 400.0);
        assert!(state.armed);
        state.reset();
        assert_eq!(state.offset, 0.0);
        assert!(!state.armed);
    }

    #[test]
    fn swipe_states_collection() {
        let mut states = SwipeStates::default();
        let s = states.get_mut("thread-1");
        s.update_offset(10.0, 400.0);
        assert_eq!(
            states.get_mut("thread-1").direction,
            Some(SwipeDirection::Right)
        );

        states.reset("thread-1");
        assert_eq!(states.get_mut("thread-1").offset, 0.0);
    }

    #[test]
    fn clear_removes_all() {
        let mut states = SwipeStates::default();
        states.get_mut("t1");
        states.get_mut("t2");
        states.clear();
        // After clear, getting a key creates a fresh default.
        assert_eq!(states.get_mut("t1").offset, 0.0);
    }
}
