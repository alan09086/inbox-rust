//! Store trait definitions for cross-crate implementation.
//!
//! These traits are defined here (in `inboxly-core`) so that `inboxly-store`
//! can implement them without depending on `inboxly-bundler`, breaking the
//! would-be circular dependency.
mod affinity_store;
mod bundle_store;
mod rule_store;

pub use affinity_store::*;
pub use bundle_store::*;
pub use rule_store::*;
