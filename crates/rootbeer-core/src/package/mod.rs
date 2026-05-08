//! Deterministic package realization primitives.
//!
//! The package layer consumes locked package facts, verifies their source
//! content, installs them into a normalized tree, and inserts that tree into
//! the content-addressed store. Higher-level backends such as GitHub, aqua, or
//! npm should eventually lower to these locked package facts before apply.

mod realize;
mod spec;

pub use realize::{PackageRealizer, RealizedPackage};
pub use spec::{LockedInstall, LockedPackage, LockedSource, Provides};
