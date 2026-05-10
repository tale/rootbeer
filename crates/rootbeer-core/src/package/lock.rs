use std::fmt;
use std::io;

use super::lockfile::{LockError, PackageLockEntry, RootbeerLock};
use super::{
    default_resolver_stack, resolver_stack_for_inputs, LockedPackage, PackageIntent,
    PackageLockInput, PackageRealizer, PackageRequest, PackageRequestResolver,
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
        let input_fingerprint = self.fingerprint_input(input)?;

        let mut entries = Vec::new();
        for intent in &input.intents {
            let entry = match intent {
                PackageIntent::Request(request) => {
                    let package = self.resolve_request(request, &input.context)?;
                    let locked = self.realize_locked_package(&package)?;
                    PackageLockEntry::resolved(request, &input.context, locked)?
                }

                PackageIntent::Locked(package) => {
                    PackageLockEntry::locked(self.realize_locked_package(package)?)
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
    ) -> Result<LockedPackage, LockBuildError> {
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
    ) -> Result<LockedPackage, LockBuildError> {
        let realized =
            self.realizer
                .realize_package(package)
                .map_err(|source| LockBuildError::Realize {
                    package: package.id(),
                    source,
                })?;

        let mut locked = package.clone();
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
        ArchiveFormat, GitHubRepositoryPin, LockedInstall, LockedSource, Provides, ResolverInput,
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
        ) -> Result<LockedPackage, ResolveError> {
            Ok(self.package.clone())
        }
    }

    struct FakeRealizer;

    impl PackageRealizerBackend for FakeRealizer {
        fn realize_package(&self, package: &LockedPackage) -> io::Result<RealizedPackage> {
            Ok(RealizedPackage {
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
            output_sha256: None,
        }
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
    }
}
