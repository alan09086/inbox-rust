//! Implements [`BundleStore`] for [`Store`].
//!
//! Bridges the SQL layer in `bundles.rs` (which works with [`BundleRow`]) to
//! the [`BundleStore`] trait (which works with [`BundleInfo`] and typed params).
//!
//! ## Enum storage format
//!
//! [`BundleVisibility`] is stored as a plain string:
//! - `"Bundled"` → [`BundleVisibility::Bundled`]
//! - `"Unbundled"` → [`BundleVisibility::Unbundled`]
//! - `"SkipInbox"` → [`BundleVisibility::SkipInbox`]
//!
//! [`BundleThrottle`] is stored as a tagged JSON string matching the
//! `#[serde(tag = "mode")]` layout defined in `inboxly-core::throttle`.

use inboxly_core::bundle::BundleVisibility;
use inboxly_core::store_traits::{
    BundleInfo, BundleStore, BundleStoreError, CreateBundleParams, UpdateBundleParams,
};
use inboxly_core::throttle::BundleThrottle;
use uuid::Uuid;

use crate::bundles::BundleRow;
use crate::error::StoreError;
use crate::store::Store;

// ---------------------------------------------------------------------------
// Known system category labels (used to determine is_custom).
//
// These match the strings produced by `category_label()` in
// `inboxly-bundler::system_bundles`, which are the values written into the
// `bundles.category` column for built-in bundles.
// ---------------------------------------------------------------------------

const SYSTEM_CATEGORIES: &[&str] = &[
    "Social",
    "Promos",
    "Updates",
    "Finance",
    "Purchases",
    "Travel",
    "Forums",
    "LowPriority",
    "Saved",
];

fn is_system_category(category: &str) -> bool {
    SYSTEM_CATEGORIES.contains(&category)
}

// ---------------------------------------------------------------------------
// Enum → string helpers
// ---------------------------------------------------------------------------

fn visibility_to_str(v: BundleVisibility) -> &'static str {
    match v {
        BundleVisibility::Bundled => "Bundled",
        BundleVisibility::Unbundled => "Unbundled",
        BundleVisibility::SkipInbox => "SkipInbox",
    }
}

fn visibility_from_str(s: &str) -> BundleVisibility {
    match s {
        "Bundled" => BundleVisibility::Bundled,
        "Unbundled" => BundleVisibility::Unbundled,
        "SkipInbox" => BundleVisibility::SkipInbox,
        other => {
            tracing::warn!(
                value = other,
                "unknown BundleVisibility value; defaulting to Bundled"
            );
            BundleVisibility::Bundled
        }
    }
}

// ---------------------------------------------------------------------------
// Row ↔ BundleInfo conversion
// ---------------------------------------------------------------------------

/// Convert a [`BundleRow`] to a [`BundleInfo`].
///
/// # Errors
///
/// Returns [`BundleStoreError::Database`] if the stored UUIDs or throttle
/// JSON cannot be parsed.
fn row_to_info(row: BundleRow) -> Result<BundleInfo, BundleStoreError> {
    let id = Uuid::parse_str(&row.id)
        .map_err(|e| BundleStoreError::Database(format!("invalid bundle UUID {}: {e}", row.id)))?;

    let visibility = visibility_from_str(&row.visibility);

    let throttle: BundleThrottle = serde_json::from_str(&row.throttle).map_err(|e| {
        BundleStoreError::Database(format!(
            "invalid throttle JSON for bundle {}: {e}",
            row.id
        ))
    })?;

    let is_custom = !is_system_category(&row.category);

    Ok(BundleInfo {
        id,
        name: row.name,
        category: row.category,
        color: row.color,
        badge_color: row.badge_color,
        visibility,
        throttle,
        is_custom,
        sort_order: row.sort_order,
    })
}

/// Map a [`StoreError::NotFound`] to [`BundleStoreError::NotFound`];
/// all other errors become [`BundleStoreError::Database`].
fn map_store_err(id: Uuid, e: StoreError) -> BundleStoreError {
    match e {
        StoreError::NotFound(_) => BundleStoreError::NotFound(id),
        other => BundleStoreError::Database(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// BundleStore impl
// ---------------------------------------------------------------------------

impl BundleStore for Store {
    fn create_bundle(&self, params: CreateBundleParams) -> Result<Uuid, BundleStoreError> {
        // Check for duplicate name.
        let existing = self
            .list_bundle_rows()
            .map_err(|e| BundleStoreError::Database(e.to_string()))?;
        if existing.iter().any(|r| r.name == params.name) {
            return Err(BundleStoreError::DuplicateName(params.name));
        }

        let id = Uuid::new_v4();
        let throttle_json = serde_json::to_string(&params.throttle)
            .map_err(|e| BundleStoreError::Database(format!("failed to serialize throttle: {e}")))?;

        let row = BundleRow {
            id: id.to_string(),
            // For custom bundles, category == name (consistent with the mock).
            category: params.name.clone(),
            name: params.name,
            color: params.color,
            badge_color: params.badge_color,
            visibility: visibility_to_str(params.visibility).to_string(),
            throttle: throttle_json,
            sort_order: existing.len() as i64,
        };

        self.insert_bundle_row(&row)
            .map_err(|e| BundleStoreError::Database(e.to_string()))?;

        Ok(id)
    }

    fn update_bundle(&self, id: Uuid, params: UpdateBundleParams) -> Result<(), BundleStoreError> {
        // Fetch existing row to apply partial updates.
        let existing = self
            .get_bundle_row(&id.to_string())
            .map_err(|e| map_store_err(id, e))?;

        let new_name = params.name.unwrap_or(existing.name);
        let new_color = params.color.unwrap_or(existing.color);
        let new_badge_color = params.badge_color.unwrap_or(existing.badge_color);
        let new_sort_order = params.sort_order.unwrap_or(existing.sort_order);

        let new_visibility = params
            .visibility
            .map(|v| visibility_to_str(v).to_string())
            .unwrap_or(existing.visibility);

        let new_throttle = if let Some(throttle) = params.throttle {
            serde_json::to_string(&throttle)
                .map_err(|e| BundleStoreError::Database(format!("failed to serialize throttle: {e}")))?
        } else {
            existing.throttle
        };

        let updated = BundleRow {
            id: existing.id,
            category: existing.category,
            name: new_name,
            color: new_color,
            badge_color: new_badge_color,
            visibility: new_visibility,
            throttle: new_throttle,
            sort_order: new_sort_order,
        };

        self.update_bundle_row(&updated)
            .map_err(|e| map_store_err(id, e))
    }

    fn delete_bundle(&self, id: Uuid) -> Result<(), BundleStoreError> {
        // Verify it exists first (also catches BuiltIn case if needed).
        let row = self
            .get_bundle_row(&id.to_string())
            .map_err(|e| map_store_err(id, e))?;

        // Prevent deletion of system bundles.
        if is_system_category(&row.category) {
            return Err(BundleStoreError::BuiltIn(id));
        }

        self.delete_bundle_row(&id.to_string())
            .map_err(|e| map_store_err(id, e))
    }

    fn list_bundles(&self) -> Result<Vec<BundleInfo>, BundleStoreError> {
        let rows = self
            .list_bundle_rows()
            .map_err(|e| BundleStoreError::Database(e.to_string()))?;
        rows.into_iter().map(row_to_info).collect()
    }
}
