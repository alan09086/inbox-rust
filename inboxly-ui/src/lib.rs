//! Iced-based desktop UI for Inboxly.
//!
//! Implements the Inbox by Google-style interface using Iced's elm architecture:
//! Model (app state) -> Message (events) -> Update (state changes) -> View (render).

pub mod app;
pub mod feed;
pub mod keyboard;
pub mod nav;
pub mod theme;
pub mod toolbar;
pub mod undo;
pub mod views;
pub mod widgets;
