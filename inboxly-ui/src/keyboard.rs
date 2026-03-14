//! Keyboard shortcut definitions for the application.
//!
//! Defines the standard keyboard shortcuts matching Google Inbox conventions.
//! Binding these to Iced keyboard events is done in the application's
//! subscription system.

/// Standard keyboard shortcuts.
pub struct Shortcuts;

impl Shortcuts {
    /// Archive / Mark Done (e or #).
    pub const DONE: &'static str = "e";
    /// Pin toggle (=).
    pub const PIN: &'static str = "=";
    /// Compose new email (c).
    pub const COMPOSE: &'static str = "c";
    /// Search (/).
    pub const SEARCH: &'static str = "/";
    /// Undo (Ctrl+Z).
    pub const UNDO: &'static str = "Ctrl+Z";
    /// Go to inbox (g then i).
    pub const GO_INBOX: &'static str = "g i";
    /// Go to snoozed (g then s).
    pub const GO_SNOOZED: &'static str = "g s";
    /// Go to done (g then d).
    pub const GO_DONE: &'static str = "g d";
    /// Select next (j or Down).
    pub const NEXT: &'static str = "j";
    /// Select previous (k or Up).
    pub const PREVIOUS: &'static str = "k";
    /// Open thread (o or Enter).
    pub const OPEN: &'static str = "o";
    /// Snooze (b).
    pub const SNOOZE: &'static str = "b";
    /// Refresh (r).
    pub const REFRESH: &'static str = "r";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_are_single_keys() {
        // Most shortcuts should be single characters.
        assert_eq!(Shortcuts::DONE.len(), 1);
        assert_eq!(Shortcuts::PIN.len(), 1);
        assert_eq!(Shortcuts::COMPOSE.len(), 1);
        assert_eq!(Shortcuts::SEARCH.len(), 1);
        assert_eq!(Shortcuts::NEXT.len(), 1);
        assert_eq!(Shortcuts::PREVIOUS.len(), 1);
        assert_eq!(Shortcuts::OPEN.len(), 1);
        assert_eq!(Shortcuts::SNOOZE.len(), 1);
        assert_eq!(Shortcuts::REFRESH.len(), 1);
    }
}
