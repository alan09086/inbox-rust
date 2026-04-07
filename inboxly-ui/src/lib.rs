//! Inboxly desktop UI -- framework-agnostic state + Dioxus components.
//!
//! The state machine (app, nav, keyboard, undo, feed) is framework-independent.
//! Rendering is handled by the Dioxus component layer in `components/`.

pub mod app;
pub mod components;
pub mod feed;
pub mod keyboard;
pub mod loaded_thread;
pub mod nav;
pub mod theme;
pub mod undo;
