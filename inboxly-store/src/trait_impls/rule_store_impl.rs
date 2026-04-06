//! Implements [`RuleStore`] for [`Store`].
//!
//! Bridges the SQL layer in `bundle_rules.rs` (which works with
//! [`BundleRuleRow`]) to the [`RuleStore`] trait (which works with
//! [`BundleRule`] and [`CreateRuleParams`]).
//!
//! Field and operator values are stored as lowercase strings produced by the
//! [`Display`] impls on [`UserRuleField`] and [`UserRuleOp`], and parsed
//! back via [`FromStr`].

use std::str::FromStr;

use inboxly_core::store_traits::{
    BundleRule, CreateRuleParams, RuleId, RuleStore, RuleStoreError, UpdateRuleParams, UserRuleField,
    UserRuleOp, validate_rule,
};
use uuid::Uuid;

use crate::bundle_rules::BundleRuleRow;
use crate::error::StoreError;
use crate::store::Store;

// ---------------------------------------------------------------------------
// Row ↔ BundleRule conversion
// ---------------------------------------------------------------------------

/// Convert a [`BundleRuleRow`] into a [`BundleRule`].
///
/// # Errors
///
/// Returns [`RuleStoreError::InvalidField`] or [`RuleStoreError::InvalidOperator`]
/// if the stored strings cannot be parsed.
fn row_to_rule(row: BundleRuleRow) -> Result<BundleRule, RuleStoreError> {
    let id = Uuid::parse_str(&row.id)
        .map_err(|e| RuleStoreError::Database(format!("invalid rule UUID {}: {e}", row.id)))?;
    let bundle_id = Uuid::parse_str(&row.bundle_id).map_err(|e| {
        RuleStoreError::Database(format!("invalid bundle UUID {}: {e}", row.bundle_id))
    })?;
    let field = UserRuleField::from_str(&row.field)
        .map_err(|_| RuleStoreError::InvalidField(row.field.clone()))?;
    let operator = UserRuleOp::from_str(&row.operator)
        .map_err(|_| RuleStoreError::InvalidOperator(row.operator.clone()))?;

    Ok(BundleRule {
        id,
        bundle_id,
        field,
        operator,
        value: row.value,
        priority: row.priority,
    })
}

/// Map a [`StoreError::NotFound`] to [`RuleStoreError::NotFound`]; all other
/// errors become [`RuleStoreError::Database`].
fn map_store_err(id: RuleId, e: StoreError) -> RuleStoreError {
    match e {
        StoreError::NotFound(_) => RuleStoreError::NotFound(id),
        other => RuleStoreError::Database(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// RuleStore impl
// ---------------------------------------------------------------------------

impl RuleStore for Store {
    fn create_rule(&self, params: CreateRuleParams) -> Result<BundleRule, RuleStoreError> {
        // Validate regex before touching the database.
        validate_rule(&params.field, &params.operator, &params.value)?;

        let id = Uuid::new_v4();
        let row = BundleRuleRow {
            id: id.to_string(),
            bundle_id: params.bundle_id.to_string(),
            field: params.field.to_string(),
            operator: params.operator.to_string(),
            value: params.value,
            priority: params.priority,
        };

        self.insert_bundle_rule(&row)
            .map_err(|e| RuleStoreError::Database(e.to_string()))?;

        // Re-fetch so callers get the canonical persisted state.
        self.get_rule(id)
    }

    fn get_rule(&self, id: RuleId) -> Result<BundleRule, RuleStoreError> {
        let row = self
            .get_bundle_rule(&id.to_string())
            .map_err(|e| map_store_err(id, e))?;
        row_to_rule(row)
    }

    fn list_rules(&self) -> Result<Vec<BundleRule>, RuleStoreError> {
        let rows = self
            .get_all_bundle_rules()
            .map_err(|e| RuleStoreError::Database(e.to_string()))?;
        rows.into_iter().map(row_to_rule).collect()
    }

    fn list_rules_for_bundle(&self, bundle_id: Uuid) -> Result<Vec<BundleRule>, RuleStoreError> {
        let rows = self
            .get_rules_for_bundle(&bundle_id.to_string())
            .map_err(|e| RuleStoreError::Database(e.to_string()))?;
        rows.into_iter().map(row_to_rule).collect()
    }

    fn update_rule(&self, id: RuleId, params: UpdateRuleParams) -> Result<BundleRule, RuleStoreError> {
        // Fetch existing row so we can apply partial updates.
        let existing = self.get_rule(id)?;

        let new_field = params.field.unwrap_or(existing.field);
        let new_operator = params.operator.unwrap_or(existing.operator);
        let new_value = params.value.unwrap_or(existing.value);
        let new_priority = params.priority.unwrap_or(existing.priority);
        let new_bundle_id = params.bundle_id.unwrap_or(existing.bundle_id);

        // Validate regex after merging.
        validate_rule(&new_field, &new_operator, &new_value)?;

        let row = BundleRuleRow {
            id: id.to_string(),
            bundle_id: new_bundle_id.to_string(),
            field: new_field.to_string(),
            operator: new_operator.to_string(),
            value: new_value,
            priority: new_priority,
        };

        self.update_bundle_rule(&row)
            .map_err(|e| map_store_err(id, e))?;

        // Re-fetch for the canonical result.
        self.get_rule(id)
    }

    fn delete_rule(&self, id: RuleId) -> Result<(), RuleStoreError> {
        self.delete_bundle_rule(&id.to_string())
            .map_err(|e| map_store_err(id, e))
    }
}
