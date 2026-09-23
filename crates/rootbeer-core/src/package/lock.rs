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
        self.build_selected(input, previous, true, false)
    }

    /// Reconciles declarations while preserving resolutions of unchanged requests.
    pub fn reconcile(
        &self,
        input: &PackageLockInput,
        previous: Option<&RootbeerLock>,
        is_offline: bool,
    ) -> Result<RootbeerLock, LockBuildError> {
        self.build_selected(input, previous, false, is_offline)
    }

    fn build_selected(
        &self,
        input: &PackageLockInput,
        previous: Option<&RootbeerLock>,
        should_update: bool,
        is_offline: bool,
    ) -> Result<RootbeerLock, LockBuildError> {
        let input_fingerprint = self.fingerprint_input(input)?;

        let mut entries = Vec::new();
        for intent in &input.intents {
            let entry = match intent {
                PackageIntent::Request(request) => {
                    let can_reuse = !should_update
                        && previous.is_some_and(|lock| {
                            lock.inputs
                                .same_package_authority(&input.resolver_inputs, request)
                        });
                    let saved = if can_reuse {
                        previous
                            .unwrap()
                            .resolution_for_request(request, &input.context)?
                    } else {
                        None
                    };
                    let mut resolution = match saved {
                        Some(resolution) => resolution,
                        None if is_offline => {
                            offline_resolution(request, &input.context, &input.resolver_inputs)
                                .map_err(|source| LockBuildError::Resolve {
                                    request: Box::new(request.clone()),
                                    source: Box::new(source),
                                })?
                        }
                        None => self.resolve_request(request, &input.context)?,
                    };
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

fn offline_resolution(
    request: &PackageRequest,
    context: &ResolveContext,
    inputs: &PackageResolverInputs,
) -> Result<PackageResolution, ResolveError> {
    let result = (|| -> Result<PackageResolution, String> {
        let is_canonical = request
            .resolver
            .as_deref()
            .is_none_or(|name| name == "rootbeer");
        if is_canonical
            && inputs
                .local_catalog()
                .is_some_and(|catalog| catalog.find(&request.name).is_some())
        {
            return Err(format!(
                "{request} has no matching local recipe lock; run without --offline first"
            ));
        }
        if let Some(saved) = super::standalone::cached_resolution(request, context)
            .map_err(|error| error.to_string())?
        {
            // Standalone resolutions came from the official repository.
            if !is_canonical || inputs.configured_repository().is_none() {
                return Ok(saved);
            }
        }
        if let (true, Some(pin)) = (is_canonical, inputs.repository()) {
            use super::PackageResolver;
            let repository = super::RepositoryResolver::with_cache(
                pin,
                crate::state_dir().join("downloads"),
                true,
            );
            if let Some(resolution) = repository.resolve(request, context)? {
                return Ok(resolution);
            }
        }
        Err(format!(
            "{request} has no cached resolution; run without --offline first"
        ))
    })();
    result.map_err(|reason| ResolveError::NotFound {
        request: Box::new(request.clone()),
        attempts: vec![super::ResolveAttempt {
            resolver: "offline cache".into(),
            reason,
        }],
    })
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
        ArchiveFormat, ExternalManagerProof, LockedInstall, LockedSource, Provides,
        ResolutionProof, ResolverInput,
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
                "rootbeer".to_string(),
                ResolverInput::Catalog {
                    sha256: "a".repeat(64),
                },
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
    struct TrackingResolver {
        calls: std::cell::RefCell<Vec<String>>,
    }

    impl PackageRequestResolver for TrackingResolver {
        fn resolve_package(
            &self,
            request: &PackageRequest,
            _: &ResolveContext,
        ) -> Result<PackageResolution, ResolveError> {
            self.calls.borrow_mut().push(request.to_string());
            let mut resolved = package();
            resolved.name = request.name.clone();
            resolved.version = request.version.clone().unwrap_or_else(|| "2.0.0".into());
            Ok(PackageResolution::new(resolved, proof()))
        }
    }

    #[test]
    fn reconciliation_preserves_unchanged_requests_and_refresh_is_explicit() {
        let context = ResolveContext::new("test-system");
        let request = PackageRequest::parse("fake:demo");
        let previous = RootbeerLock::from_package_entries([PackageLockEntry::resolved(
            &request,
            &context,
            PackageResolution::new(package(), proof()),
        )
        .unwrap()])
        .unwrap();
        let builder = PackageLockBuilder::new(
            TrackingResolver {
                calls: Default::default(),
            },
            FakeRealizer,
            context.clone(),
        );
        let input = PackageLockInput::with_resolver_inputs(
            context.clone(),
            Default::default(),
            vec![
                PackageIntent::request(request.clone()),
                PackageIntent::request(PackageRequest::parse("fake:added")),
            ],
        );
        let reconciled = builder.reconcile(&input, Some(&previous), false).unwrap();
        assert_eq!(
            reconciled
                .package_for_request(&request, &context)
                .unwrap()
                .version,
            "1.0.0"
        );
        assert_eq!(*builder.resolver.calls.borrow(), ["fake:added"]);
        let removed_input = PackageLockInput::with_resolver_inputs(
            context.clone(),
            Default::default(),
            vec![PackageIntent::request(request.clone())],
        );
        let offline = builder
            .reconcile(&removed_input, Some(&reconciled), true)
            .unwrap();
        assert_eq!(offline.packages.len(), 1);
        assert_eq!(*builder.resolver.calls.borrow(), ["fake:added"]);
        assert!(builder.reconcile(&input, Some(&previous), true).is_err());
        assert_eq!(*builder.resolver.calls.borrow(), ["fake:added"]);
        let updated = builder
            .build_with_previous(&removed_input, Some(&previous))
            .unwrap();
        assert_eq!(
            updated
                .package_for_request(&request, &context)
                .unwrap()
                .version,
            "2.0.0"
        );
        let pinned_request = request.version("3.0.0");
        let pinned_input = PackageLockInput::with_resolver_inputs(
            context.clone(),
            Default::default(),
            vec![PackageIntent::request(pinned_request.clone())],
        );
        let pinned = builder
            .reconcile(&pinned_input, Some(&updated), false)
            .unwrap();
        assert_eq!(
            pinned
                .package_for_request(&pinned_request, &context)
                .unwrap()
                .version,
            "3.0.0"
        );
    }

    #[test]
    fn local_recipe_edits_reconcile_only_local_requests_and_cannot_replay_offline() {
        let definition = crate::package::PackageDefinition::from_lua(
            r#"return {
            name = "demo", description = "Local demo",
            homepage = "https://example.invalid/demo", default_license = "MIT",
            prebuilt = { github = "owner/demo", tag = "v{version}", asset = "demo.tar.gz" },
            outputs = { bins = { "demo" }, checks = { { "demo", "--version" } } },
            platforms = { ["aarch64-macos"] = { default_version = "1" } },
            versions = { ["1"] = { digests = { ["aarch64-macos"] = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" } } },
        }"#,
        )
        .unwrap();
        let catalog = crate::package::PackageCatalog {
            extra: Default::default(),
            packages: BTreeMap::from([("demo".into(), definition.package)]),
        };
        let mut inputs = PackageResolverInputs::default();
        inputs.resolvers.insert(
            "local".into(),
            ResolverInput::LocalCatalog(Box::new(catalog.clone())),
        );
        let context = ResolveContext::new("test-system");
        let builder = PackageLockBuilder::new_with_inputs(
            TrackingResolver {
                calls: Default::default(),
            },
            FakeRealizer,
            context.clone(),
            inputs.clone(),
        );
        let input = PackageLockInput::with_resolver_inputs(
            context.clone(),
            inputs,
            vec![
                PackageIntent::request(PackageRequest::parse("demo")),
                PackageIntent::request(PackageRequest::parse("registry-tool")),
            ],
        );
        let lock = builder.build(&input).unwrap();
        builder.resolver.calls.borrow_mut().clear();
        builder.reconcile(&input, Some(&lock), true).unwrap();
        assert!(builder.resolver.calls.borrow().is_empty());
        let mut changed = input.clone();
        let mut catalog = catalog;
        catalog
            .packages
            .get_mut("demo")
            .unwrap()
            .versions
            .get_mut("1")
            .unwrap()
            .revision = 2;
        changed.resolver_inputs.resolvers.insert(
            "local".into(),
            ResolverInput::LocalCatalog(Box::new(catalog)),
        );
        assert!(builder.reconcile(&changed, Some(&lock), true).is_err());
        builder.reconcile(&changed, Some(&lock), false).unwrap();
        assert_eq!(*builder.resolver.calls.borrow(), ["demo"]);
        builder.resolver.calls.borrow_mut().clear();
        changed.resolver_inputs.resolvers.remove("local");
        builder.reconcile(&changed, Some(&lock), false).unwrap();
        assert_eq!(*builder.resolver.calls.borrow(), ["demo"]);
    }
}
