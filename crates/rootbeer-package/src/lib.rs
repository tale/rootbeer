//! Package definitions, resolution, and verified installation.

pub use rootbeer_store as store;
pub use rootbeer_store::{deterministic, state_dir};
mod aqua;
mod artifact;
mod build_spec;
pub mod catalog;
pub mod definition;
pub mod discovery;
pub mod distribution;
mod dmg;
pub mod download;
mod execution;
pub use execution::Execution;
pub mod ghcr;
pub mod github;
pub mod graph;
pub mod index;
mod inputs;
mod intent;
pub mod official;
pub mod pdr;
pub mod realize;
mod resolve;
pub mod self_update;
mod source;
mod spec;
pub use source::{GitSource, SourceBuildProof, SourceSelection};
pub mod staging;
pub mod upstream;

pub use aqua::AquaResolver;
pub use artifact::{ArtifactIndex, PublishedArtifact};
pub use build_spec::{
    BuildArtifact, BuildBackend, BuildDependency, BuildEnvironmentInput, BuildEnvironmentLock,
    BuildSteps, DependencyKind, GoBuild, RustBuild, SourceBuild,
};
pub use catalog::{
    Bins, CatalogPackage, CatalogProof, CatalogRecipe, CatalogVersion, ExtraFields, PackageCatalog,
};
pub use definition::{PackageDefinition, PackageUpstream, UpstreamProvider};
pub use github::GitHubResolver;
pub use index::{PackageIndexPin, PublishedIndexProof};
pub use inputs::{GitHubRepositoryPin, PackageResolverInputs, ResolverInput};
pub use intent::{PackageIntent, PackageLockInput};
pub use official::{IndexSelection, OfficialIndexSource};
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
    if let Some(catalog) = inputs.local_catalog() {
        stack.push(
            catalog::CatalogResolver::new(catalog, inputs, backend_stack(inputs))
                .with_fallback(inputs.package_index()),
        );
    } else if let Some(pin) = inputs.discovery() {
        stack.push(discovery::DiscoveryResolver::new(pin));
    } else if let Some(pin) = inputs.package_index() {
        stack.push(index::IndexResolver::new(pin));
    }
    stack
}

pub fn backend_stack(inputs: &PackageResolverInputs) -> ResolverStack {
    let mut stack = ResolverStack::new();
    stack.push(match inputs.aqua_registry() {
        Some(pin) => AquaResolver::from_registry_pin(pin),
        None => AquaResolver::new(),
    });
    stack.push(GitHubResolver::new());
    stack
}

pub mod lockfile;

pub mod runtime;

#[cfg(test)]
extern crate self as rootbeer_package;

#[cfg(test)]
#[path = "../tests/support/catalog.rs"]
mod test_catalog;
