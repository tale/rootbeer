//! Deterministic package realization primitives.
//!
//! The package layer consumes locked package facts, verifies their source
//! content, installs them into a normalized tree, and inserts that tree into
//! the content-addressed store. Higher-level backends such as GitHub, aqua, or
//! npm should eventually lower to these locked package facts before apply.

mod realize;
mod resolve;
mod spec;

pub use realize::{PackageRealizer, RealizedPackage};
pub use resolve::{
    PackageRequest, PackageResolver, ResolveAttempt, ResolveContext, ResolveError, ResolverStack,
};
pub use spec::{ArchiveFormat, LockedInstall, LockedPackage, LockedSource, Provides};
