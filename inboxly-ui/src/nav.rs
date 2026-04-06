//! Navigation drawer types.
//!
//! Pure data types for navigation targets -- no framework dependencies.
//! Rendering is handled by the Dioxus component layer.

use crate::theme::ActiveView;

/// Secondary navigation destinations (folders, not primary views).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavSection {
    Drafts,
    Sent,
    Reminders,
    Trash,
    Spam,
}

impl NavSection {
    /// Human-readable label for this section.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Drafts => "Drafts",
            Self::Sent => "Sent",
            Self::Reminders => "Reminders",
            Self::Trash => "Trash",
            Self::Spam => "Spam",
        }
    }

    /// All secondary nav items in display order.
    pub fn all() -> &'static [Self] {
        &[
            Self::Drafts,
            Self::Sent,
            Self::Reminders,
            Self::Trash,
            Self::Spam,
        ]
    }
}

/// A bundle category entry for the nav drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavBundleCategory {
    pub name: String,
}

/// Default bundle categories shown in the nav drawer.
pub fn default_bundle_categories() -> Vec<NavBundleCategory> {
    vec![
        NavBundleCategory {
            name: "Social".into(),
        },
        NavBundleCategory {
            name: "Promos".into(),
        },
        NavBundleCategory {
            name: "Updates".into(),
        },
        NavBundleCategory {
            name: "Finance".into(),
        },
        NavBundleCategory {
            name: "Purchases".into(),
        },
        NavBundleCategory {
            name: "Travel".into(),
        },
        NavBundleCategory {
            name: "Forums".into(),
        },
        NavBundleCategory {
            name: "Low Priority".into(),
        },
    ]
}

/// Unified navigation target -- any clickable item in the nav drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavTarget {
    /// Primary views (Inbox, Snoozed, Done) -- changes toolbar colour.
    View(ActiveView),
    /// Secondary nav (Drafts, Sent, etc.) -- loads folder content.
    Section(NavSection),
    /// Bundle category filter -- shows emails in that bundle.
    BundleCategory(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Inboxly, Message};
    use inboxly_core::config::{AccountConfig, AuthMethod};

    fn make_test_account(email: &str, display_name: &str) -> AccountConfig {
        AccountConfig {
            email: email.to_string(),
            display_name: display_name.to_string(),
            provider: "generic".to_string(),
            auth_method: AuthMethod::Password,
            imap_host: "imap.example.com".to_string(),
            imap_port: 993,
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
        }
    }

    #[test]
    fn account_switcher_starts_collapsed() {
        let app = Inboxly::default();
        assert!(!app.account_switcher_open);
    }

    #[test]
    fn account_switcher_toggles() {
        let mut app = Inboxly::default();
        app.update(Message::ToggleAccountSwitcher);
        assert!(app.account_switcher_open);
        app.update(Message::ToggleAccountSwitcher);
        assert!(!app.account_switcher_open);
    }

    #[test]
    fn switch_to_same_account_still_closes_switcher() {
        let mut app = Inboxly::default();
        app.accounts = vec![make_test_account("test@example.com", "Test User")];
        app.account_switcher_open = true;
        app.update(Message::SwitchAccount(0));
        assert!(!app.account_switcher_open);
        assert_eq!(app.active_account_index, 0);
    }

    #[test]
    fn switch_account_updates_index_and_closes() {
        let mut app = Inboxly::default();
        app.accounts = vec![
            make_test_account("first@example.com", "First"),
            make_test_account("second@example.com", "Second"),
        ];
        app.account_switcher_open = true;
        app.update(Message::SwitchAccount(1));
        assert_eq!(app.active_account_index, 1);
        assert_eq!(app.active_email(), "second@example.com");
        assert!(!app.account_switcher_open);
    }

    #[test]
    fn navigate_closes_account_switcher() {
        let mut app = Inboxly::default();
        app.account_switcher_open = true;
        app.update(Message::Navigate(NavTarget::View(ActiveView::Inbox)));
        assert!(!app.account_switcher_open);
    }

    #[test]
    fn with_accounts_sets_accounts() {
        let accounts = vec![make_test_account("test@example.com", "Test")];
        let app = Inboxly::with_accounts(accounts);
        assert_eq!(app.accounts.len(), 1);
        assert_eq!(app.active_email(), "test@example.com");
    }
}
