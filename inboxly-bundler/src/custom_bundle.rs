//! Custom user-defined bundles with name and colour.
//!
//! Re-uses [`BundleVisibility`] and [`BundleThrottle`] from `inboxly-core`.
//! Defines CRUD parameter types, the [`BundleStore`] trait, and
//! [`BundleInfo`] for summary views.

use inboxly_core::{BundleThrottle, BundleVisibility};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Parameters for creating a custom bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBundleParams {
    /// User-visible name (e.g., "Work", "Freelance", "Side Project").
    pub name: String,
    /// Title colour as CSS hex string (e.g., "#e06055").
    pub color: String,
    /// Badge background colour as CSS hex string (e.g., "#faebea").
    pub badge_color: String,
    /// Visibility setting.
    pub visibility: BundleVisibility,
    /// Throttle setting.
    pub throttle: BundleThrottle,
}

/// Parameters for updating a custom bundle.  All fields are optional --
/// only `Some` values are applied.
#[derive(Debug, Clone, Default)]
pub struct UpdateBundleParams {
    /// New display name.
    pub name: Option<String>,
    /// New title colour.
    pub color: Option<String>,
    /// New badge colour.
    pub badge_color: Option<String>,
    /// New visibility setting.
    pub visibility: Option<BundleVisibility>,
    /// New throttle setting.
    pub throttle: Option<BundleThrottle>,
    /// New sort order.
    pub sort_order: Option<i64>,
}

/// Summary info for a bundle (system or custom).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleInfo {
    /// Unique bundle identifier.
    pub id: Uuid,
    /// Display name.
    pub name: String,
    /// Category key (e.g., "Social", "Promos", or custom name).
    pub category: String,
    /// Title colour as CSS hex.
    pub color: String,
    /// Badge background colour as CSS hex.
    pub badge_color: String,
    /// How this bundle appears in the inbox.
    pub visibility: BundleVisibility,
    /// Delivery frequency.
    pub throttle: BundleThrottle,
    /// Whether this is a user-created custom bundle.
    pub is_custom: bool,
    /// Sort order (lower = shown first).
    pub sort_order: i64,
}

/// Errors from bundle store operations.
#[derive(Debug, thiserror::Error)]
pub enum BundleStoreError {
    /// The requested bundle was not found.
    #[error("bundle not found: {0}")]
    NotFound(Uuid),

    /// A bundle with this name already exists.
    #[error("bundle name already exists: {0}")]
    DuplicateName(String),

    /// Cannot delete a built-in system bundle.
    #[error("cannot delete built-in bundle: {0}")]
    BuiltIn(Uuid),

    /// An error from the underlying database.
    #[error("database error: {0}")]
    Database(String),
}

/// Trait for custom bundle persistence.
///
/// Implemented by `inboxly-store::Store` for production use.
/// A mock implementation is used in tests.
pub trait BundleStore {
    /// Create a new custom bundle.  Returns the bundle ID.
    ///
    /// # Errors
    ///
    /// Returns [`BundleStoreError::DuplicateName`] if a bundle with the
    /// same name already exists.
    fn create_bundle(&self, params: CreateBundleParams) -> Result<Uuid, BundleStoreError>;

    /// Update a custom bundle's settings.
    ///
    /// # Errors
    ///
    /// Returns [`BundleStoreError::NotFound`] if the bundle does not exist.
    fn update_bundle(
        &self,
        id: Uuid,
        params: UpdateBundleParams,
    ) -> Result<(), BundleStoreError>;

    /// Delete a custom bundle and all its rules.
    ///
    /// # Errors
    ///
    /// Returns [`BundleStoreError::BuiltIn`] if the bundle is a system bundle.
    /// Returns [`BundleStoreError::NotFound`] if it does not exist.
    fn delete_bundle(&self, id: Uuid) -> Result<(), BundleStoreError>;

    /// List all bundles (built-in + custom), ordered by sort_order.
    ///
    /// # Errors
    ///
    /// Returns [`BundleStoreError::Database`] on database failure.
    fn list_bundles(&self) -> Result<Vec<BundleInfo>, BundleStoreError>;
}

// ---------------------------------------------------------------------------
// In-memory mock for tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod mock {
    use super::*;
    use std::sync::Mutex;

    /// In-memory mock implementation of [`BundleStore`] for unit tests.
    pub struct MockBundleStore {
        bundles: Mutex<Vec<BundleInfo>>,
        next_sort_order: Mutex<i64>,
    }

    impl MockBundleStore {
        /// Create a new mock store with the 8 system bundles pre-populated.
        pub fn new_with_system_bundles() -> Self {
            use crate::system_bundles::{system_bundle_id, SYSTEM_BUNDLES};

            let bundles: Vec<BundleInfo> = SYSTEM_BUNDLES
                .iter()
                .map(|def| {
                    let id = system_bundle_id(&def.category);
                    BundleInfo {
                        id: id.0,
                        name: def.name.to_owned(),
                        category: def.name.to_owned(),
                        color: def.color.to_owned(),
                        badge_color: def.badge_color.to_owned(),
                        visibility: BundleVisibility::Bundled,
                        throttle: BundleThrottle::Immediate,
                        is_custom: false,
                        sort_order: def.sort_order,
                    }
                })
                .collect();
            Self {
                next_sort_order: Mutex::new(bundles.len() as i64),
                bundles: Mutex::new(bundles),
            }
        }
    }

    impl BundleStore for MockBundleStore {
        fn create_bundle(&self, params: CreateBundleParams) -> Result<Uuid, BundleStoreError> {
            let mut bundles = self.bundles.lock().expect("mock lock poisoned");
            if bundles.iter().any(|b| b.name == params.name) {
                return Err(BundleStoreError::DuplicateName(params.name));
            }
            let id = Uuid::new_v4();
            let mut order = self.next_sort_order.lock().expect("mock lock poisoned");
            bundles.push(BundleInfo {
                id,
                name: params.name.clone(),
                category: params.name,
                color: params.color,
                badge_color: params.badge_color,
                visibility: params.visibility,
                throttle: params.throttle,
                is_custom: true,
                sort_order: *order,
            });
            *order = order.saturating_add(1);
            Ok(id)
        }

        fn update_bundle(
            &self,
            id: Uuid,
            params: UpdateBundleParams,
        ) -> Result<(), BundleStoreError> {
            let mut bundles = self.bundles.lock().expect("mock lock poisoned");
            let bundle = bundles
                .iter_mut()
                .find(|b| b.id == id)
                .ok_or(BundleStoreError::NotFound(id))?;
            if let Some(name) = params.name {
                bundle.name = name;
            }
            if let Some(color) = params.color {
                bundle.color = color;
            }
            if let Some(badge_color) = params.badge_color {
                bundle.badge_color = badge_color;
            }
            if let Some(visibility) = params.visibility {
                bundle.visibility = visibility;
            }
            if let Some(throttle) = params.throttle {
                bundle.throttle = throttle;
            }
            if let Some(sort_order) = params.sort_order {
                bundle.sort_order = sort_order;
            }
            Ok(())
        }

        fn delete_bundle(&self, id: Uuid) -> Result<(), BundleStoreError> {
            let mut bundles = self.bundles.lock().expect("mock lock poisoned");
            let pos = bundles
                .iter()
                .position(|b| b.id == id)
                .ok_or(BundleStoreError::NotFound(id))?;
            if !bundles[pos].is_custom {
                return Err(BundleStoreError::BuiltIn(id));
            }
            bundles.remove(pos);
            Ok(())
        }

        fn list_bundles(&self) -> Result<Vec<BundleInfo>, BundleStoreError> {
            let mut bundles = self.bundles.lock().expect("mock lock poisoned").clone();
            bundles.sort_by_key(|b| b.sort_order);
            Ok(bundles)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mock::MockBundleStore;

    #[test]
    fn create_custom_bundle() {
        let store = MockBundleStore::new_with_system_bundles();
        let id = store
            .create_bundle(CreateBundleParams {
                name: "Work".into(),
                color: "#336699".into(),
                badge_color: "#eef4fa".into(),
                visibility: BundleVisibility::Bundled,
                throttle: BundleThrottle::Immediate,
            })
            .expect("create");
        assert_ne!(id, Uuid::nil());

        let bundles = store.list_bundles().expect("list");
        let work = bundles.iter().find(|b| b.name == "Work").expect("find Work");
        assert!(work.is_custom);
        assert_eq!(work.color, "#336699");
    }

    #[test]
    fn duplicate_name_rejected() {
        let store = MockBundleStore::new_with_system_bundles();
        store
            .create_bundle(CreateBundleParams {
                name: "Work".into(),
                color: "#000".into(),
                badge_color: "#fff".into(),
                visibility: BundleVisibility::Bundled,
                throttle: BundleThrottle::Immediate,
            })
            .expect("create first");
        let result = store.create_bundle(CreateBundleParams {
            name: "Work".into(),
            color: "#111".into(),
            badge_color: "#eee".into(),
            visibility: BundleVisibility::Bundled,
            throttle: BundleThrottle::Immediate,
        });
        assert!(matches!(result, Err(BundleStoreError::DuplicateName(_))));
    }

    #[test]
    fn update_custom_bundle() {
        let store = MockBundleStore::new_with_system_bundles();
        let id = store
            .create_bundle(CreateBundleParams {
                name: "Work".into(),
                color: "#000".into(),
                badge_color: "#fff".into(),
                visibility: BundleVisibility::Bundled,
                throttle: BundleThrottle::Immediate,
            })
            .expect("create");
        store
            .update_bundle(
                id,
                UpdateBundleParams {
                    name: Some("Office".into()),
                    color: Some("#123456".into()),
                    ..UpdateBundleParams::default()
                },
            )
            .expect("update");
        let bundles = store.list_bundles().expect("list");
        let office = bundles.iter().find(|b| b.id == id).expect("find");
        assert_eq!(office.name, "Office");
        assert_eq!(office.color, "#123456");
    }

    #[test]
    fn delete_custom_bundle() {
        let store = MockBundleStore::new_with_system_bundles();
        let initial_count = store.list_bundles().expect("list").len();
        let id = store
            .create_bundle(CreateBundleParams {
                name: "Temp".into(),
                color: "#000".into(),
                badge_color: "#fff".into(),
                visibility: BundleVisibility::Bundled,
                throttle: BundleThrottle::Immediate,
            })
            .expect("create");
        assert_eq!(store.list_bundles().expect("list").len(), initial_count + 1);
        store.delete_bundle(id).expect("delete");
        assert_eq!(store.list_bundles().expect("list").len(), initial_count);
    }

    #[test]
    fn cannot_delete_system_bundle() {
        let store = MockBundleStore::new_with_system_bundles();
        let bundles = store.list_bundles().expect("list");
        let system = bundles.iter().find(|b| !b.is_custom).expect("find system");
        let result = store.delete_bundle(system.id);
        assert!(matches!(result, Err(BundleStoreError::BuiltIn(_))));
    }

    #[test]
    fn list_returns_sorted_by_order() {
        let store = MockBundleStore::new_with_system_bundles();
        let bundles = store.list_bundles().expect("list");
        for window in bundles.windows(2) {
            assert!(
                window[0].sort_order <= window[1].sort_order,
                "bundles should be sorted by sort_order"
            );
        }
    }
}
