//! Snooze date-picker popup state.
//!
//! Extracted from `Inboxly` in M35a. Tracks which thread (if any) has
//! the snooze picker popup open and where to anchor it.

use crate::app::Point;

/// State backing the snooze-picker popup.
pub struct SnoozeState {
    /// Thread ID whose snooze date-picker popup is currently open.
    pub picker_thread: Option<String>,
    /// Cursor position where the snooze picker was triggered (popup anchor).
    pub picker_position: Point,
}

impl Default for SnoozeState {
    fn default() -> Self {
        Self::new()
    }
}

impl SnoozeState {
    /// Create empty snooze state.
    pub const fn new() -> Self {
        Self {
            picker_thread: None,
            picker_position: Point::ORIGIN,
        }
    }
}
