//! Default system bundle definitions with BigTop colours.
//!
//! Defines the 8 standard bundles (Social, Promos, Updates, Finance,
//! Purchases, Travel, Forums, Low Priority) and provides
//! [`ensure_system_bundles`] for idempotent creation at startup.

use inboxly_core::{BundleCategory, BundleId};
use inboxly_store::{BundleRow, Store};
use uuid::Uuid;

/// UUID v5 namespace for generating deterministic system bundle IDs.
///
/// This ensures the same [`BundleId`] is produced across reinstalls.
/// The bytes spell "inboxly-bundles!" in ASCII.
const SYSTEM_BUNDLE_NAMESPACE: Uuid = Uuid::from_bytes([
    0x69, 0x6e, 0x62, 0x6f, 0x78, 0x6c, 0x79, 0x2d,
    0x62, 0x75, 0x6e, 0x64, 0x6c, 0x65, 0x73, 0x21,
]);

/// A system bundle definition with its BigTop colour scheme.
pub struct SystemBundleDef {
    /// Bundle category.
    pub category: BundleCategory,
    /// Display name.
    pub name: &'static str,
    /// Title colour as CSS hex (e.g., "#d23f31").
    pub color: &'static str,
    /// Pastel badge background colour as CSS hex.
    pub badge_color: &'static str,
    /// Sort order in the bundle list (lower = first).
    pub sort_order: i64,
}

/// The 8 system bundles with BigTop colour definitions.
pub const SYSTEM_BUNDLES: &[SystemBundleDef] = &[
    SystemBundleDef {
        category: BundleCategory::Social,
        name: "Social",
        color: "#d23f31",
        badge_color: "#faebea",
        sort_order: 0,
    },
    SystemBundleDef {
        category: BundleCategory::Promos,
        name: "Promos",
        color: "#00acc1",
        badge_color: "#e5f6f9",
        sort_order: 1,
    },
    SystemBundleDef {
        category: BundleCategory::Updates,
        name: "Updates",
        color: "#f4511e",
        badge_color: "#feede8",
        sort_order: 2,
    },
    SystemBundleDef {
        category: BundleCategory::Finance,
        name: "Finance",
        color: "#558b2f",
        badge_color: "#eef3ea",
        sort_order: 3,
    },
    SystemBundleDef {
        category: BundleCategory::Purchases,
        name: "Purchases",
        color: "#6d4c41",
        badge_color: "#f0edec",
        sort_order: 4,
    },
    SystemBundleDef {
        category: BundleCategory::Travel,
        name: "Travel",
        color: "#8e24aa",
        badge_color: "#f3e9f6",
        sort_order: 5,
    },
    SystemBundleDef {
        category: BundleCategory::Forums,
        name: "Forums",
        color: "#3949ab",
        badge_color: "#ebecf6",
        sort_order: 6,
    },
    SystemBundleDef {
        category: BundleCategory::LowPriority,
        name: "Low Priority",
        color: "#212121",
        badge_color: "#e5e5e5",
        sort_order: 7,
    },
];

/// Generate a deterministic [`BundleId`] for a system bundle category.
///
/// Uses UUID v5 (SHA-1 namespace + category name) so the same ID is
/// produced every time, across reinstalls and machines.
pub fn system_bundle_id(category: &BundleCategory) -> BundleId {
    let name = category_key(category);
    BundleId(Uuid::new_v5(&SYSTEM_BUNDLE_NAMESPACE, name.as_bytes()))
}

/// Return the stable string key for a category (used for UUID generation).
fn category_key(category: &BundleCategory) -> &str {
    match category {
        BundleCategory::Social => "social",
        BundleCategory::Promos => "promos",
        BundleCategory::Updates => "updates",
        BundleCategory::Finance => "finance",
        BundleCategory::Purchases => "purchases",
        BundleCategory::Travel => "travel",
        BundleCategory::Forums => "forums",
        BundleCategory::LowPriority => "low-priority",
        BundleCategory::Saved => "saved",
        BundleCategory::Custom(s) => s.as_str(),
    }
}

/// Ensure all 8 system bundles exist in the store.
///
/// Idempotent -- safe to call on every app startup. Checks by category
/// name rather than ID so it gracefully handles any pre-existing bundles.
///
/// Returns the list of [`BundleId`]s for system bundles.
///
/// # Errors
///
/// Returns [`crate::BundlerError::Store`] if any database operation fails.
pub fn ensure_system_bundles(store: &Store) -> crate::Result<Vec<BundleId>> {
    let mut ids = Vec::with_capacity(SYSTEM_BUNDLES.len());

    for def in SYSTEM_BUNDLES {
        let id = system_bundle_id(&def.category);
        let id_str = id.0.to_string();
        let category_str = category_label(&def.category);

        // Check if this category already exists
        if store.get_bundle_by_category(&category_str)?.is_some() {
            tracing::debug!(category = category_str, "system bundle already exists");
            ids.push(id);
            continue;
        }

        let row = BundleRow {
            id: id_str,
            category: category_str.clone(),
            name: def.name.to_string(),
            color: def.color.to_string(),
            badge_color: def.badge_color.to_string(),
            visibility: "Bundled".to_string(),
            throttle: default_throttle(&def.category).to_string(),
            sort_order: def.sort_order,
        };

        store.insert_bundle(&row)?;
        tracing::info!(bundle_name = def.name, "created system bundle");
        ids.push(id);
    }

    Ok(ids)
}

/// Return the serde-compatible category label for a category.
fn category_label(category: &BundleCategory) -> String {
    // Must match the serde representation used in BundleRow.category
    match category {
        BundleCategory::Social => "Social".to_string(),
        BundleCategory::Promos => "Promos".to_string(),
        BundleCategory::Updates => "Updates".to_string(),
        BundleCategory::Finance => "Finance".to_string(),
        BundleCategory::Purchases => "Purchases".to_string(),
        BundleCategory::Travel => "Travel".to_string(),
        BundleCategory::Forums => "Forums".to_string(),
        BundleCategory::LowPriority => "LowPriority".to_string(),
        BundleCategory::Saved => "Saved".to_string(),
        BundleCategory::Custom(s) => s.clone(),
    }
}

/// Return the default throttle setting for a category.
fn default_throttle(category: &BundleCategory) -> &'static str {
    match category {
        BundleCategory::Promos | BundleCategory::Forums => "Daily",
        BundleCategory::LowPriority => "Weekly",
        _ => "Immediate",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_bundle_ids_are_deterministic() {
        let id1 = system_bundle_id(&BundleCategory::Social);
        let id2 = system_bundle_id(&BundleCategory::Social);
        assert_eq!(id1, id2);
    }

    #[test]
    fn all_categories_have_unique_ids() {
        let ids: Vec<_> = SYSTEM_BUNDLES
            .iter()
            .map(|d| system_bundle_id(&d.category))
            .collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(
            ids.len(),
            unique.len(),
            "all system bundle IDs must be unique"
        );
    }

    #[test]
    fn eight_system_bundles_defined() {
        assert_eq!(SYSTEM_BUNDLES.len(), 8);
    }

    #[test]
    fn ensure_system_bundles_creates_all_eight() {
        let store = Store::open_in_memory().expect("open in-memory store");
        let ids = ensure_system_bundles(&store).expect("ensure bundles");
        assert_eq!(ids.len(), 8);

        // Verify each exists
        let all = store.list_bundles().expect("list bundles");
        assert_eq!(all.len(), 8);
    }

    #[test]
    fn ensure_system_bundles_is_idempotent() {
        let store = Store::open_in_memory().expect("open in-memory store");

        let ids1 = ensure_system_bundles(&store).expect("first call");
        let ids2 = ensure_system_bundles(&store).expect("second call");

        assert_eq!(ids1, ids2, "bundle IDs should be identical across calls");

        // Should still only have 8 bundles total
        let all = store.list_bundles().expect("list bundles");
        assert_eq!(all.len(), 8);
    }

    #[test]
    fn system_bundle_sort_order() {
        // Social first, Low Priority last
        assert_eq!(SYSTEM_BUNDLES[0].sort_order, 0);
        assert_eq!(SYSTEM_BUNDLES[7].sort_order, 7);
    }
}
