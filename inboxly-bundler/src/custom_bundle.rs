//! Custom user-defined bundles with name and colour.
//!
//! The type definitions and [`BundleStore`] trait have moved to
//! [`inboxly_core::store_traits`].  This module re-exports everything for
//! backwards compatibility.

pub use inboxly_core::store_traits::{
    BundleInfo, BundleStore, BundleStoreError, CreateBundleParams, UpdateBundleParams,
};

// ---------------------------------------------------------------------------
// In-memory mock for tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod mock {
    use super::*;
    use inboxly_core::{BundleThrottle, BundleVisibility};
    use std::sync::Mutex;
    use uuid::Uuid;

    /// In-memory mock implementation of [`BundleStore`] for unit tests.
    pub struct MockBundleStore {
        bundles: Mutex<Vec<BundleInfo>>,
        next_sort_order: Mutex<i64>,
    }

    impl MockBundleStore {
        /// Create a new mock store with the 8 system bundles pre-populated.
        pub fn new_with_system_bundles() -> Self {
            use crate::system_bundles::{SYSTEM_BUNDLES, system_bundle_id};

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
    use inboxly_core::{BundleThrottle, BundleVisibility};
    use mock::MockBundleStore;
    use uuid::Uuid;

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
        let work = bundles
            .iter()
            .find(|b| b.name == "Work")
            .expect("find Work");
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
