//! Grouped UI state sub-structs extracted from the top-level `Inboxly`.
//!
//! Each sub-struct owns a cohesive slice of UI state (settings, menus,
//! snooze picker) so the `Inboxly` god-object stays legible as more
//! feature state is added. Mirrors the existing `UndoState` pattern from
//! `crate::undo`.

pub mod menu_state;
pub mod settings_state;
pub mod snooze_state;

pub use menu_state::MenuState;
pub use settings_state::SettingsState;
pub use snooze_state::SnoozeState;
