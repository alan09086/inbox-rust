//! Inboxly -- main binary entry point.
//!
//! Launches the Iced desktop application with the nav drawer, toolbar,
//! and view switching. Loads account configuration from `~/.config/inboxly/config.toml`.

use std::sync::OnceLock;

use iced::{Size, Task};
use inboxly_core::config::{AccountConfig, AppConfig};
use inboxly_ui::app::{Inboxly, Message};

/// Accounts loaded from config, consumed once during app initialisation.
static STARTUP_ACCOUNTS: OnceLock<Vec<AccountConfig>> = OnceLock::new();

fn new() -> (Inboxly, Task<Message>) {
    let accounts = STARTUP_ACCOUNTS.get().cloned().unwrap_or_default();
    let mut app = Inboxly::with_accounts(accounts);
    app.theme = inboxly_ui::theme::InboxlyTheme::from_system();
    (app, Task::none())
}

fn main() -> iced::Result {
    // Load accounts from config file (fallback to empty on error).
    let accounts = match AppConfig::load() {
        Ok(config) => config.accounts,
        Err(e) => {
            eprintln!("warning: failed to load config: {e}");
            Vec::new()
        }
    };
    let _ = STARTUP_ACCOUNTS.set(accounts);

    iced::application(new, Inboxly::update, Inboxly::view)
        .title(Inboxly::title)
        .window_size(Size::new(1280.0, 800.0))
        .theme(Inboxly::theme)
        .run()
}
