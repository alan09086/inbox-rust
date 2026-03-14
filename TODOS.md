# Inboxly — TODOs

## Priority: Wire bundler traits to Store

**What:** Implement `RuleStore`, `AffinityStore`, `BundleStore` (from `inboxly-bundler`) on `inboxly-store::Store`. Delete the aspirational `inboxly-core::traits` module (Store/Bundler/Extractor traits that were never implemented).

**Why:** The M13 `BundlerEngine` accepts `&dyn RuleStore` etc. but can only receive mock implementations today. The real `Store` has the underlying SQL methods (`bundle_rules.rs`, `sender_affinity.rs`, `bundles.rs`) but doesn't implement the traits. Without this, the bundler can't run against real data.

**How to apply:**
1. In `inboxly-store`, add `impl inboxly_bundler::RuleStore for Store { ... }` delegating to existing `bundle_rules.rs` methods
2. Same for `AffinityStore` (delegating to `sender_affinity.rs`) and `BundleStore` (delegating to `bundles.rs`)
3. Add integration tests in `inboxly-store/tests/` that exercise the full bundler pipeline through real SQLite
4. Delete `inboxly-core/src/traits.rs` and remove from `lib.rs` re-exports
5. This introduces a circular dependency risk: `inboxly-store` would need to depend on `inboxly-bundler` for the trait definitions. **Solution**: Move the trait definitions to `inboxly-core` (replacing the current aspirational traits with the concrete bundler traits), or use a separate `inboxly-traits` crate.

**Blocked by:** Nothing
**Blocks:** M15 (UI wiring needs Store to provide bundler functionality)
