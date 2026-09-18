use std::fmt;
use std::io;

use super::lockfile::{LockError, PackageLockEntry, RootbeerLock};
use super::{
    default_resolver_stack, resolver_stack_for_inputs, LockedPackage, PackageIntent,
    PackageLockInput, PackageRealizer, PackageRequest, PackageRequestResolver, PackageResolution,
    PackageResolverInputs, RealizedPackage, ResolveContext, ResolveError,
};
use crate::deterministic::DeterministicInput;
use crate::Op;

pub trait PackageRealizerBackend {
    fn realize_package(&self, package: &LockedPackage) -> io::Result<RealizedPackage>;
}

impl PackageRealizerBackend for PackageRealizer {
    fn realize_package(&self, package: &LockedPackage) -> io::Result<RealizedPackage> {
        self.realize(package)
    }
}

pub struct PackageLockBuilder<R = super::ResolverStack, B = PackageRealizer> {
    resolver: R,
    realizer: B,
    context: ResolveContext,
    resolver_inputs: PackageResolverInputs,
}

impl PackageLockBuilder {
    pub fn current_system() -> Self {
        Self::new(
            default_resolver_stack(),
            PackageRealizer::default(),
            ResolveContext::current(),
        )
    }

    pub fn current_system_with_inputs(inputs: PackageResolverInputs) -> Self {
        Self::new_with_inputs(
            resolver_stack_for_inputs(&inputs),
            PackageRealizer::default(),
            ResolveContext::current(),
            inputs,
        )
    }
}

impl Default for PackageLockBuilder {
    fn default() -> Self {
        Self::current_system()
    }
}

impl<R, B> PackageLockBuilder<R, B> {
    pub fn new(resolver: R, realizer: B, context: ResolveContext) -> Self {
        Self::new_with_inputs(
            resolver,
            realizer,
            context,
            PackageResolverInputs::default(),
        )
    }

    pub fn new_with_inputs(
        resolver: R,
        realizer: B,
        context: ResolveContext,
        resolver_inputs: PackageResolverInputs,
    ) -> Self {
        Self {
            resolver,
            realizer,
            context,
            resolver_inputs,
        }
    }

    pub fn context(&self) -> &ResolveContext {
        &self.context
    }
}

impl<R, B> PackageLockBuilder<R, B>
where
    R: PackageRequestResolver,
    B: PackageRealizerBackend,
{
    pub fn lock_input_from_ops(&self, ops: &[Op]) -> PackageLockInput {
        PackageLockInput::with_resolver_inputs(
            self.context.clone(),
            self.resolver_inputs.clone(),
            package_intents(ops),
        )
    }

    pub fn fingerprint_input(&self, input: &PackageLockInput) -> Result<String, LockBuildError> {
        input
            .fingerprint()
            .map(|fingerprint| fingerprint.into_string())
            .map_err(|e| LockError::Fingerprint {
                kind: PackageLockInput::KIND,
                error: e.to_string(),
            })
            .map_err(Into::into)
    }

    pub fn build_from_ops(&self, ops: &[Op]) -> Result<RootbeerLock, LockBuildError> {
        let input = self.lock_input_from_ops(ops);
        self.build(&input)
    }

    pub fn build(&self, input: &PackageLockInput) -> Result<RootbeerLock, LockBuildError> {
        self.build_with_previous(input, None)
    }

    /// Refreshes resolution while retaining verified output identities for unchanged inputs.
    pub fn build_with_previous(
        &self,
        input: &PackageLockInput,
        previous: Option<&RootbeerLock>,
    ) -> Result<RootbeerLock, LockBuildError> {
        let input_fingerprint = self.fingerprint_input(input)?;

        let mut entries = Vec::new();
        for intent in &input.intents {
            let entry = match intent {
                PackageIntent::Request(request) => {
                    let mut resolution = self.resolve_request(request, &input.context)?;
                    resolution.package =
                        self.realize_locked_package(&resolution.package, previous)?;
                    PackageLockEntry::resolved(request, &input.context, resolution)?
                }

                PackageIntent::Locked(package) => {
                    PackageLockEntry::locked(self.realize_locked_package(package, previous)?)
                }
            };

            entries.push(entry);
        }

        Ok(RootbeerLock::from_package_entries(entries)?
            .with_resolver_inputs(input.resolver_inputs.clone())
            .with_input_fingerprint(input_fingerprint))
    }

    fn resolve_request(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<PackageResolution, LockBuildError> {
        self.resolver
            .resolve_package(request, context)
            .map_err(|source| LockBuildError::Resolve {
                request: Box::new(request.clone()),
                source: Box::new(source),
            })
    }

    fn realize_locked_package(
        &self,
        package: &LockedPackage,
        previous: Option<&RootbeerLock>,
    ) -> Result<LockedPackage, LockBuildError> {
        let mut locked = package.clone();
        if locked.output_sha256.is_none() {
            locked.output_sha256 = previous
                .into_iter()
                .flat_map(|lock| lock.packages.values())
                .find(|old| old.same_realization_input(package))
                .and_then(|old| old.output_sha256.clone());
        }
        let realized =
            self.realizer
                .realize_package(&locked)
                .map_err(|source| LockBuildError::Realize {
                    package: package.id(),
                    source,
                })?;

        locked.output_sha256 = Some(realized.store_entry.output_sha256);
        Ok(locked)
    }
}

fn package_intents(ops: &[Op]) -> Vec<PackageIntent> {
    ops.iter()
        .filter_map(|op| match op {
            Op::Package { intent } => Some(intent.clone()),
            Op::RealizePackage { package } => Some(PackageIntent::locked(package.clone())),
            _ => None,
        })
        .collect()
}

#[derive(Debug)]
pub enum LockBuildError {
    Lock(Box<LockError>),
    Resolve {
        request: Box<PackageRequest>,
        source: Box<ResolveError>,
    },
    Realize {
        package: String,
        source: io::Error,
    },
}

impl fmt::Display for LockBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockBuildError::Lock(err) => write!(f, "{err}"),
            LockBuildError::Resolve { request, source } => {
                write!(f, "failed to resolve package {request}: {source}")
            }
            LockBuildError::Realize { package, source } => {
                write!(f, "failed to realize package {package}: {source}")
            }
        }
    }
}

impl std::error::Error for LockBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LockBuildError::Lock(err) => Some(err.as_ref()),
            LockBuildError::Resolve { source, .. } => Some(source.as_ref()),
            LockBuildError::Realize { source, .. } => Some(source),
        }
    }
}

impl From<LockError> for LockBuildError {
    fn from(err: LockError) -> Self {
        Self::Lock(Box::new(err))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;
    use crate::package::{
        ArchiveFormat, ExternalManagerProof, GitHubRepositoryPin, LockedInstall, LockedSource,
        Provides, ResolutionProof, ResolverInput,
    };
    use crate::store::StoreEntry;

    struct FakeResolver {
        package: LockedPackage,
    }

    impl PackageRequestResolver for FakeResolver {
        fn resolve_package(
            &self,
            _request: &PackageRequest,
            _context: &ResolveContext,
        ) -> Result<PackageResolution, ResolveError> {
            Ok(PackageResolution::new(self.package.clone(), proof()))
        }
    }

    struct FakeRealizer;

    impl PackageRealizerBackend for FakeRealizer {
        fn realize_package(&self, package: &LockedPackage) -> io::Result<RealizedPackage> {
            Ok(RealizedPackage {
                apps: std::collections::BTreeMap::new(),
                package: package.clone(),
                store_entry: StoreEntry {
                    path: PathBuf::from("/store/demo"),
                    name: package.name.clone(),
                    version: package.version.clone(),
                    output_sha256: "output".to_string(),
                },
                bins: BTreeMap::new(),
            })
        }
    }

    fn package() -> LockedPackage {
        LockedPackage {
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
            source: LockedSource::Url {
                url: "https://example.com/demo.tar.gz".to_string(),
                sha256: "source".to_string(),
            },
            install: LockedInstall::Archive {
                format: ArchiveFormat::TarGz,
                strip_prefix: Some(PathBuf::from("demo")),
            },
            provides: Provides::default(),
            runtime_dependencies: Default::default(),
            output_sha256: None,
        }
    }

    fn proof() -> ResolutionProof {
        ResolutionProof::ExternalManager(ExternalManagerProof {
            manager: "fake".to_string(),
            inputs: BTreeMap::new(),
            notes: vec!["test resolver".to_string()],
        })
    }

    #[test]
    fn refresh_reuses_unchanged_outputs_and_still_detects_tampering() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("tool");
        std::fs::write(&source, b"tool bytes").unwrap();
        let mut package = package();
        package.source = LockedSource::File {
            path: source.clone(),
            sha256: crate::store::hash_file(&source).unwrap(),
        };
        package.install = LockedInstall::Binary {
            path: "tool".into(),
        };
        package.provides.bins.insert("tool".into(), "tool".into());
        let store = crate::store::Store::new(root.path().join("store"));
        let builder = PackageLockBuilder::new(
            FakeResolver {
                package: package.clone(),
            },
            PackageRealizer::with_dirs(
                store.clone(),
                root.path().join("downloads"),
                root.path().join("tmp"),
            ),
            ResolveContext::new("test-system"),
        );
        let input = builder.lock_input_from_ops(&[Op::Package {
            intent: PackageIntent::request(PackageRequest::new("demo").resolver("fake")),
        }]);
        let previous = builder.build(&input).unwrap();
        std::fs::remove_file(&source).unwrap();
        let refreshed = builder
            .build_with_previous(&input, Some(&previous))
            .unwrap();
        assert_eq!(refreshed.packages, previous.packages);
        let locked = refreshed.packages.values().next().unwrap();
        let output = store.store_path(
            locked.output_sha256.as_ref().unwrap(),
            &locked.name,
            &locked.version,
        );
        std::fs::write(output.join("tool"), b"tampered").unwrap();
        assert!(builder
            .build_with_previous(&input, Some(&previous))
            .is_err());
        let mut changed = input.clone();
        package.version = "2.0.0".into();
        changed.intents = vec![PackageIntent::locked(package)];
        assert!(builder
            .build_with_previous(&changed, Some(&previous))
            .is_err());
    }

    #[test]
    fn builder_uses_injected_resolver_and_realizer() {
        let inputs = PackageResolverInputs {
            resolvers: BTreeMap::from([(
                "aqua".to_string(),
                ResolverInput::AquaRegistry(GitHubRepositoryPin {
                    owner: "aquaproj".to_string(),
                    repo: "aqua-registry".to_string(),
                    rev: "abc123".to_string(),
                }),
            )]),
        };
        let builder = PackageLockBuilder::new_with_inputs(
            FakeResolver { package: package() },
            FakeRealizer,
            ResolveContext::new("test-system"),
            inputs.clone(),
        );
        let lock = builder
            .build_from_ops(&[Op::Package {
                intent: PackageIntent::request(PackageRequest::new("demo").resolver("fake")),
            }])
            .unwrap();

        let package = lock.packages.get("fake:demo@1.0.0").unwrap();
        assert_eq!(package.output_sha256.as_deref(), Some("output"));
        assert_eq!(lock.inputs, inputs);
        assert!(lock.input_fingerprint.is_some());
        assert_eq!(lock.resolutions.len(), 1);
        assert_eq!(lock.resolutions.values().next().unwrap().proof, proof());
    }
}
