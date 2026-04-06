//! Custom user-defined bundles with name and colour.
//!
//! Re-uses [`BundleVisibility`] and [`BundleThrottle`] from `inboxly-core`.
//! Defines CRUD parameter types, the [`BundleStore`] trait, and
//! [`BundleInfo`] for summary views.

use crate::bundle::{BundleThrottle, BundleVisibility};
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
    fn update_bundle(&self, id: Uuid, params: UpdateBundleParams) -> Result<(), BundleStoreError>;

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
