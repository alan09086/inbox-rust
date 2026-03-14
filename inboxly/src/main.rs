//! Inboxly -- main binary entry point.
//!
//! Launches the Iced desktop application with the nav drawer, toolbar,
//! and view switching.

use iced::Size;
use inboxly_ui::app::Inboxly;

fn main() -> iced::Result {
    iced::application(Inboxly::new, Inboxly::update, Inboxly::view)
        .title(Inboxly::title)
        .window_size(Size::new(1280.0, 800.0))
        .theme(Inboxly::theme)
        .run()
}
