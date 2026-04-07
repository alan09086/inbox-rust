//! Overflow + right-click context menu state.
//!
//! Extracted from `Inboxly` in M35a. Both menus share the
//! `thread_sender` field because they are mutually exclusive — only one
//! menu can be open at a time. The `close()` helper replaces the old
//! `Inboxly::close_menus()` private method so handlers can clear all
//! three fields through a single entry point.

use crate::app::Point;

/// State backing the overflow (three-dot) and right-click context menus.
pub struct MenuState {
    /// Thread ID whose overflow (three-dot) menu is currently open.
    pub overflow_thread: Option<String>,
    /// Cursor position where the overflow menu was triggered (popup anchor).
    pub overflow_position: Point,
    /// Thread ID whose right-click context menu is currently open.
    pub context_thread: Option<String>,
    /// Cursor position where the context menu was triggered.
    pub context_position: Point,
    /// Sender address of the thread whose menu is open (for
    /// `BlockSender`, `CreateRuleFromSender`). Shared between overflow
    /// and context menus — they are mutually exclusive.
    pub thread_sender: Option<String>,
}

impl Default for MenuState {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuState {
    /// Create empty menu state.
    pub const fn new() -> Self {
        Self {
            overflow_thread: None,
            overflow_position: Point::ORIGIN,
            context_thread: None,
            context_position: Point::ORIGIN,
            thread_sender: None,
        }
    }

    /// Clear both menus (overflow + context) and their shared sender field.
    ///
    /// Every message handler that resolves a thread action should call
    /// this so the three-field menu-state invariant stays
    /// self-enforcing — new handlers can't accidentally clear only two
    /// of the three fields.
    pub fn close(&mut self) {
        self.overflow_thread = None;
        self.context_thread = None;
        self.thread_sender = None;
    }
}
