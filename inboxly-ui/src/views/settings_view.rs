//! Settings view data types and adapters.

use crate::theme::{SettingsReader, SettingsWriter};

/// The six tabs in the settings sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SettingsTab {
    #[default]
    General,
    Accounts,
    Bundles,
    Notifications,
    KeyboardShortcuts,
    DataStorage,
}

impl SettingsTab {
    /// Display label for the sidebar.
    pub fn label(&self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Accounts => "Accounts",
            Self::Bundles => "Bundles",
            Self::Notifications => "Notifications",
            Self::KeyboardShortcuts => "Keyboard Shortcuts",
            Self::DataStorage => "Data & Storage",
        }
    }

    /// All tabs in display order.
    pub fn all() -> &'static [Self] {
        &[
            Self::General,
            Self::Accounts,
            Self::Bundles,
            Self::Notifications,
            Self::KeyboardShortcuts,
            Self::DataStorage,
        ]
    }
}

/// Adapter that wraps an `inboxly_store::Store` reference and implements
/// the `SettingsReader`/`SettingsWriter` traits from `inboxly-ui`.
///
/// This avoids a circular dependency between `inboxly-store` and `inboxly-ui`.
pub struct StoreSettingsAdapter<'a> {
    pub store: &'a inboxly_store::Store,
}

impl SettingsReader for StoreSettingsAdapter<'_> {
    fn get_setting(&self, key: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
        self.store
            .get_setting(key)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}

impl SettingsWriter for StoreSettingsAdapter<'_> {
    fn set_setting(&self, key: &str, value: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.store
            .set_setting(key, value)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_tab_default_is_general() {
        assert_eq!(SettingsTab::default(), SettingsTab::General);
    }

    #[test]
    fn settings_tab_labels() {
        assert_eq!(SettingsTab::General.label(), "General");
        assert_eq!(SettingsTab::Accounts.label(), "Accounts");
        assert_eq!(SettingsTab::Bundles.label(), "Bundles");
        assert_eq!(SettingsTab::Notifications.label(), "Notifications");
        assert_eq!(SettingsTab::KeyboardShortcuts.label(), "Keyboard Shortcuts");
        assert_eq!(SettingsTab::DataStorage.label(), "Data & Storage");
    }

    #[test]
    fn settings_tab_all_returns_six_tabs() {
        let all = SettingsTab::all();
        assert_eq!(all.len(), 6);
        assert_eq!(all[0], SettingsTab::General);
        assert_eq!(all[5], SettingsTab::DataStorage);
    }

    #[test]
    fn store_settings_adapter_read_write() {
        let store = inboxly_store::Store::open_in_memory().expect("in-memory store");
        let adapter = StoreSettingsAdapter { store: &store };

        // Initially empty
        let val = adapter.get_setting("theme").expect("get_setting");
        assert!(val.is_none());

        // Write and read back
        adapter.set_setting("theme", "dark").expect("set_setting");
        let val = adapter.get_setting("theme").expect("get_setting");
        assert_eq!(val.as_deref(), Some("dark"));
    }

    #[test]
    fn store_settings_adapter_missing_key() {
        let store = inboxly_store::Store::open_in_memory().expect("in-memory store");
        let adapter = StoreSettingsAdapter { store: &store };

        let val = adapter.get_setting("nonexistent").expect("get_setting");
        assert!(val.is_none());
    }
}
