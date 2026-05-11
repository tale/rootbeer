use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::deterministic::DeterministicInput;

use super::{GitHubRepositoryPin, LockedPackage};

/// A high-level package request before it has been lowered to a locked package
/// fact. The resolver prefix is optional: `ripgrep` is implicit, while
/// `aqua:ripgrep` or `github:BurntSushi/ripgrep` is explicit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRequest {
    pub name: String,
    pub version: Option<String>,
    pub resolver: Option<String>,
}

impl DeterministicInput for PackageRequest {
    const KIND: &'static str = "package.request";
}

impl fmt::Display for PackageRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(resolver) = &self.resolver {
            write!(f, "{resolver}:")?;
        }

        write!(f, "{}", self.name)?;

        if let Some(version) = &self.version {
            write!(f, "@{version}")?;
        }

        Ok(())
    }
}

impl PackageRequest {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            resolver: None,
        }
    }

    pub fn parse(input: &str) -> Self {
        let (resolver, input) = match input.split_once(':') {
            Some((resolver, name)) if !resolver.is_empty() && !name.is_empty() => {
                (Some(resolver.to_string()), name)
            }
            _ => (None, input),
        };

        let (name, version) = match input.rsplit_once('@') {
            Some((name, version)) if !name.is_empty() && !version.is_empty() => {
                (name.to_string(), Some(version.to_string()))
            }
            _ => (input.to_string(), None),
        };

        Self {
            name,
            version,
            resolver,
        }
    }

    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn resolver(mut self, resolver: impl Into<String>) -> Self {
        self.resolver = Some(resolver.into());
        self
    }

    pub fn is_explicit(&self) -> bool {
        self.resolver.is_some()
    }

    pub fn resolution_input<'a>(
        &'a self,
        context: &'a ResolveContext,
    ) -> PackageResolutionInput<'a> {
        PackageResolutionInput {
            request: self,
            context,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveContext {
    pub system: String,
}

impl ResolveContext {
    pub fn new(system: impl Into<String>) -> Self {
        Self {
            system: system.into(),
        }
    }

    pub fn current() -> Self {
        Self::new(format!(
            "{}-{}",
            std::env::consts::ARCH,
            std::env::consts::OS
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PackageResolutionInput<'a> {
    pub request: &'a PackageRequest,
    pub context: &'a ResolveContext,
}

impl DeterministicInput for PackageResolutionInput<'_> {
    const KIND: &'static str = "package.resolution";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageResolution {
    pub package: LockedPackage,
    pub proof: ResolutionProof,
}

impl PackageResolution {
    pub fn new(package: LockedPackage, proof: ResolutionProof) -> Self {
        Self { package, proof }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResolutionProof {
    Snapshot(SnapshotProof),
    MetadataClosure(MetadataClosureProof),
    GitRelease(GitReleaseProof),
    ExternalManager(ExternalManagerProof),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotProof {
    pub resolver: String,
    pub source: SnapshotSource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<MetadataDocumentProof>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SnapshotSource {
    GitHubRepository(GitHubRepositoryPin),
    Url { url: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataClosureProof {
    pub resolver: String,
    pub root: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<MetadataDocumentProof>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactProof>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<DependencyProof>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyProof {
    pub name: String,
    pub version: String,
    pub proof: Box<ResolutionProof>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataDocumentProof {
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactProof {
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitReleaseProof {
    pub host: String,
    pub owner: String,
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<MetadataDocumentProof>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalManagerProof {
    pub manager: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveAttempt {
    pub resolver: String,
    pub reason: String,
}

impl ResolveAttempt {
    fn not_found(resolver: impl Into<String>) -> Self {
        Self {
            resolver: resolver.into(),
            reason: "not found".to_string(),
        }
    }

    fn failed(resolver: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            resolver: resolver.into(),
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    NoResolvers,
    UnknownExplicitResolver {
        resolver: String,
        available: Vec<String>,
    },
    NotFound {
        request: PackageRequest,
        attempts: Vec<ResolveAttempt>,
    },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::NoResolvers => write!(f, "no package resolvers are configured"),
            ResolveError::UnknownExplicitResolver {
                resolver,
                available,
            } => {
                write!(f, "unknown package resolver `{resolver}`")?;
                if !available.is_empty() {
                    write!(f, " (available: {})", available.join(", "))?;
                }
                Ok(())
            }

            ResolveError::NotFound { request, attempts } => {
                if let Some(resolver) = &request.resolver {
                    write!(f, "package `{}` not found via `{resolver}`", request.name)?;
                } else {
                    write!(f, "package `{}` not found", request.name)?;
                }

                if !attempts.is_empty() {
                    write!(f, "; attempted ")?;
                    for (i, attempt) in attempts.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{} ({})", attempt.resolver, attempt.reason)?;
                    }
                }

                Ok(())
            }
        }
    }
}

impl std::error::Error for ResolveError {}

/// Implementations can use any kind of strategy to resolve a package but must
/// return either Ok(None) if not found or a locked package with a proof if found.
pub trait PackageResolver {
    fn name(&self) -> &str;

    fn resolve(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<Option<PackageResolution>, String>;
}

/// Resolves package requests into locked package facts. This is intentionally
/// separate from `PackageResolver`, which models a single backend that may miss.
pub trait PackageRequestResolver {
    fn resolve_package(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<PackageResolution, ResolveError>;
}

/// Ordered resolver orchestration. This is the only policy encoded here:
/// explicit requests use exactly one named resolver, while implicit requests
/// try resolvers in configured order and surface every failed attempt.
#[derive(Default)]
pub struct ResolverStack {
    resolvers: Vec<Box<dyn PackageResolver>>,
}

impl ResolverStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push<R>(&mut self, resolver: R)
    where
        R: PackageResolver + 'static,
    {
        self.resolvers.push(Box::new(resolver));
    }

    pub fn resolver_names(&self) -> Vec<String> {
        self.resolvers
            .iter()
            .map(|resolver| resolver.name().to_string())
            .collect()
    }

    pub fn resolve(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<PackageResolution, ResolveError> {
        if self.resolvers.is_empty() {
            return Err(ResolveError::NoResolvers);
        }

        if let Some(explicit) = &request.resolver {
            let Some(resolver) = self
                .resolvers
                .iter()
                .find(|resolver| resolver.name() == explicit)
            else {
                return Err(ResolveError::UnknownExplicitResolver {
                    resolver: explicit.clone(),
                    available: self.resolver_names(),
                });
            };

            return match resolver.resolve(request, context) {
                Ok(Some(package)) => Ok(package),
                Ok(None) => Err(ResolveError::NotFound {
                    request: request.clone(),
                    attempts: vec![ResolveAttempt::not_found(resolver.name())],
                }),

                Err(reason) => Err(ResolveError::NotFound {
                    request: request.clone(),
                    attempts: vec![ResolveAttempt::failed(resolver.name(), reason)],
                }),
            };
        }

        let mut attempts = Vec::new();
        for resolver in &self.resolvers {
            match resolver.resolve(request, context) {
                Ok(Some(package)) => return Ok(package),
                Ok(None) => attempts.push(ResolveAttempt::not_found(resolver.name())),
                Err(reason) => attempts.push(ResolveAttempt::failed(resolver.name(), reason)),
            }
        }

        Err(ResolveError::NotFound {
            request: request.clone(),
            attempts,
        })
    }
}

impl PackageRequestResolver for ResolverStack {
    fn resolve_package(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<PackageResolution, ResolveError> {
        self.resolve(request, context)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;
    use crate::package::{ArchiveFormat, LockedInstall, LockedSource, Provides};

    struct FakeResolver {
        name: &'static str,
        resolution: Option<PackageResolution>,
        error: Option<&'static str>,
    }

    impl FakeResolver {
        fn miss(name: &'static str) -> Self {
            Self {
                name,
                resolution: None,
                error: None,
            }
        }

        fn hit(name: &'static str, package: LockedPackage) -> Self {
            Self {
                name,
                resolution: Some(PackageResolution::new(package, proof(name))),
                error: None,
            }
        }

        fn error(name: &'static str, error: &'static str) -> Self {
            Self {
                name,
                resolution: None,
                error: Some(error),
            }
        }
    }

    impl PackageResolver for FakeResolver {
        fn name(&self) -> &str {
            self.name
        }

        fn resolve(
            &self,
            _request: &PackageRequest,
            _context: &ResolveContext,
        ) -> Result<Option<PackageResolution>, String> {
            if let Some(error) = self.error {
                Err(error.to_string())
            } else {
                Ok(self.resolution.clone())
            }
        }
    }

    fn proof(resolver: &str) -> ResolutionProof {
        ResolutionProof::ExternalManager(ExternalManagerProof {
            manager: resolver.to_string(),
            inputs: BTreeMap::new(),
            notes: vec!["test resolver".to_string()],
        })
    }

    fn package(name: &str) -> LockedPackage {
        LockedPackage {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            source: LockedSource::Url {
                url: format!("https://example.com/{name}.tar.gz"),
                sha256: "abc123".to_string(),
            },
            install: LockedInstall::Archive {
                format: ArchiveFormat::TarGz,
                strip_prefix: Some(PathBuf::from(name)),
            },
            provides: Provides {
                bins: BTreeMap::from([(name.to_string(), PathBuf::from(format!("bin/{name}")))]),
            },
            output_sha256: None,
        }
    }

    #[test]
    fn parses_implicit_and_explicit_requests() {
        assert_eq!(
            PackageRequest::parse("ripgrep@14.1.1"),
            PackageRequest::new("ripgrep").version("14.1.1")
        );
        assert_eq!(
            PackageRequest::parse("aqua:BurntSushi/ripgrep@14.1.1"),
            PackageRequest::new("BurntSushi/ripgrep")
                .resolver("aqua")
                .version("14.1.1")
        );
        assert_eq!(
            PackageRequest::parse("@scope/tool"),
            PackageRequest::new("@scope/tool")
        );
        assert_eq!(
            PackageRequest::parse("@scope/tool@1.2.3"),
            PackageRequest::new("@scope/tool").version("1.2.3")
        );
        assert_eq!(
            PackageRequest::parse("registry:@scope/tool"),
            PackageRequest::new("@scope/tool").resolver("registry")
        );
        assert_eq!(
            PackageRequest::parse("registry:@scope/tool@1.2.3"),
            PackageRequest::new("@scope/tool")
                .resolver("registry")
                .version("1.2.3")
        );
    }

    #[test]
    fn package_resolution_input_fingerprint_includes_request_and_context() {
        let request = PackageRequest::new("ripgrep").resolver("aqua");
        let darwin = ResolveContext::new("aarch64-macos");
        let linux = ResolveContext::new("x86_64-linux");

        assert_ne!(
            request.resolution_input(&darwin).fingerprint().unwrap(),
            request.resolution_input(&linux).fingerprint().unwrap()
        );
    }

    #[test]
    fn implicit_resolution_tries_resolvers_in_order() {
        let mut stack = ResolverStack::new();
        stack.push(FakeResolver::miss("aqua"));
        stack.push(FakeResolver::hit("ubi", package("ripgrep")));
        stack.push(FakeResolver::hit("github", package("wrong")));

        let resolved = stack
            .resolve(
                &PackageRequest::new("ripgrep"),
                &ResolveContext::new("aarch64-darwin"),
            )
            .unwrap();

        assert_eq!(resolved.package.name, "ripgrep");
    }

    #[test]
    fn explicit_resolution_only_uses_named_resolver() {
        let mut stack = ResolverStack::new();
        stack.push(FakeResolver::hit("aqua", package("wrong")));
        stack.push(FakeResolver::hit("github", package("ripgrep")));

        let resolved = stack
            .resolve(
                &PackageRequest::new("ripgrep").resolver("github"),
                &ResolveContext::new("aarch64-darwin"),
            )
            .unwrap();

        assert_eq!(resolved.package.name, "ripgrep");
    }

    #[test]
    fn unknown_explicit_resolver_lists_available_resolvers() {
        let mut stack = ResolverStack::new();
        stack.push(FakeResolver::miss("aqua"));
        stack.push(FakeResolver::miss("github"));

        let err = stack
            .resolve(
                &PackageRequest::new("ripgrep").resolver("ubi"),
                &ResolveContext::new("aarch64-darwin"),
            )
            .unwrap_err();

        assert_eq!(
            err.to_string(),
            "unknown package resolver `ubi` (available: aqua, github)"
        );
    }

    #[test]
    fn failed_implicit_resolution_surfaces_each_attempt() {
        let mut stack = ResolverStack::new();
        stack.push(FakeResolver::miss("aqua"));
        stack.push(FakeResolver::error("ubi", "unsupported platform"));
        stack.push(FakeResolver::miss("github"));

        let err = stack
            .resolve(
                &PackageRequest::new("ripgrep"),
                &ResolveContext::new("aarch64-darwin"),
            )
            .unwrap_err();

        assert_eq!(
            err.to_string(),
            "package `ripgrep` not found; attempted aqua (not found), ubi (unsupported platform), github (not found)"
        );
    }
}
