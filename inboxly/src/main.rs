//! Inboxly -- main binary entry point.
//!
//! Launches the Dioxus Desktop application with the nav drawer, toolbar,
//! and view switching. Loads account configuration from `~/.config/inboxly/config.toml`.

use std::sync::OnceLock;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use inboxly_core::config::{AccountConfig, AppConfig};

/// Accounts loaded from config, consumed once during app initialisation.
pub static STARTUP_ACCOUNTS: OnceLock<Vec<AccountConfig>> = OnceLock::new();

fn main() {
    // Load accounts from config file (fallback to empty on error).
    let accounts = match AppConfig::load() {
        Ok(config) => config.accounts,
        Err(e) => {
            eprintln!("warning: failed to load config: {e}");
            Vec::new()
        }
    };
    let _ = STARTUP_ACCOUNTS.set(accounts);

    // Default window size: 1280x800 (modern laptop-scale). The nav drawer
    // is 264 px wide so anything narrower than ~700 px eats the content
    // area entirely. Users can resize freely after launch; this is the
    // fresh-start default so the app isn't cramped on first run.
    let window = WindowBuilder::new()
        .with_title("Inboxly")
        .with_inner_size(LogicalSize::new(1280.0, 800.0));
    let cfg = Config::new().with_window(window).with_menu(None);
    dioxus::LaunchBuilder::desktop()
        .with_cfg(cfg)
        .launch(inboxly_ui::components::app::App);
}
