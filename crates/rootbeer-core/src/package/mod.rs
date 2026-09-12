//! Deterministic package realization primitives.
//!
//! The package layer consumes locked package facts, verifies their source
//! content, installs them into a normalized tree, and inserts that tree into
//! the content-addressed store. Higher-level package backends should lower to
//! these locked package facts before apply.

mod aqua;
mod build;
mod bundle;
mod catalog;
mod download;
mod github;
mod inputs;
mod intent;
mod lock;
pub mod lockfile;
pub mod profile;
mod realize;
mod resolve;
mod spec;

pub use aqua::AquaResolver;
pub use build::{build_package, BuildArtifact, BuildBackend, SourceBuild};
pub use bundle::{bundle_artifacts, ArtifactIndex, PublishedArtifact};
pub use catalog::{CatalogPackage, CatalogProof, CatalogRecipe, PackageCatalog};
pub use github::GitHubResolver;
pub use inputs::{GitHubRepositoryPin, PackageResolverInputs, ResolverInput};
pub use intent::{PackageIntent, PackageLockInput};
pub use lock::{LockBuildError, PackageLockBuilder, PackageRealizerBackend};
pub use realize::{PackageRealizer, RealizedPackage};
pub use resolve::{
    ArtifactProof, DependencyProof, ExternalManagerProof, GitReleaseProof, MetadataClosureProof,
    MetadataDocumentProof, PackageRequest, PackageRequestResolver, PackageResolution,
    PackageResolutionInput, PackageResolver, ResolutionProof, ResolveAttempt, ResolveContext,
    ResolveError, ResolverStack, SnapshotProof, SnapshotSource,
};
pub use spec::{
    ArchiveFormat, LockedInstall, LockedPackage, LockedSource, PackageRealizationInput, Provides,
};

pub fn default_resolver_stack() -> ResolverStack {
    resolver_stack_for_inputs(&PackageResolverInputs::default())
}

pub fn resolver_stack_for_inputs(inputs: &PackageResolverInputs) -> ResolverStack {
    let mut stack = backend_stack(inputs).with_implicit_resolver("rootbeer");
    stack.push(catalog::CatalogResolver::new(inputs, backend_stack(inputs)));
    stack
}

fn backend_stack(inputs: &PackageResolverInputs) -> ResolverStack {
    let mut stack = ResolverStack::new();
    stack.push(match inputs.aqua_registry() {
        Some(pin) => AquaResolver::from_registry_pin(pin),
        None => AquaResolver::new(),
    });
    stack.push(GitHubResolver::new());
    stack
}
