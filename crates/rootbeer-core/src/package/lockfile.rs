use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{
    LockedPackage, PackageIntent, PackageRequest, PackageResolution, PackageResolverInputs,
    ResolutionProof, ResolveContext,
};
use crate::deterministic::DeterministicInput;
use crate::Op;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootbeerLock {
    pub schema: u32,
    #[serde(default, skip_serializing_if = "PackageResolverInputs::is_empty")]
    pub inputs: PackageResolverInputs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resolutions: BTreeMap<String, PackageLockResolution>,
    pub packages: BTreeMap<String, LockedPackage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageLockResolution {
    pub package: String,
    pub proof: ResolutionProof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageLockEntry {
    pub package: LockedPackage,
    pub id: Option<String>,
    pub resolution_fingerprint: Option<String>,
    pub proof: Option<ResolutionProof>,
}

impl PackageLockEntry {
    pub fn locked(package: LockedPackage) -> Self {
        Self {
            package,
            id: None,
            resolution_fingerprint: None,
            proof: None,
        }
    }

    pub fn resolved(
        request: &PackageRequest,
        context: &ResolveContext,
        resolution: PackageResolution,
    ) -> Result<Self, LockError> {
        let id = package_id_for_request(request, &resolution.package);
        Ok(Self {
            package: resolution.package,
            id: Some(id),
            resolution_fingerprint: Some(resolution_fingerprint(request, context)?),
            proof: Some(resolution.proof),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockError {
    DuplicatePackage { id: String },
    DuplicateResolution { fingerprint: String },
    MissingResolutionProof { id: String },
    MissingPackage { id: String },
    PackageChanged { id: String },
    MissingLockfile { path: std::path::PathBuf },
    StaleLockfile { path: std::path::PathBuf },
    Fingerprint { kind: &'static str, error: String },
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockError::DuplicatePackage { id } => write!(f, "duplicate package `{id}`"),
            LockError::DuplicateResolution { fingerprint } => {
                write!(f, "duplicate package resolution `{fingerprint}`")
            }
            LockError::MissingResolutionProof { id } => {
                write!(f, "package resolution `{id}` is missing a proof")
            }
            LockError::MissingPackage { id } => {
                write!(f, "package `{id}` is not present in rootbeer.lock")
            }
            LockError::PackageChanged { id } => write!(
                f,
                "package `{id}` differs from the facts recorded in rootbeer.lock"
            ),
            LockError::MissingLockfile { path } => {
                write!(f, "package lockfile {} is missing", path.display())
            }
            LockError::StaleLockfile { path } => write!(
                f,
                "package lockfile {} is stale for the current plan",
                path.display()
            ),
            LockError::Fingerprint { kind, error } => {
                write!(f, "failed to fingerprint {kind}: {error}")
            }
        }
    }
}

impl std::error::Error for LockError {}

impl RootbeerLock {
    pub fn from_packages(
        packages: impl IntoIterator<Item = LockedPackage>,
    ) -> Result<Self, LockError> {
        Self::from_package_entries(packages.into_iter().map(PackageLockEntry::locked))
    }

    pub fn from_package_entries(
        entries: impl IntoIterator<Item = PackageLockEntry>,
    ) -> Result<Self, LockError> {
        let mut map = BTreeMap::new();
        let mut resolutions = BTreeMap::new();
        for entry in entries {
            let id = entry.id.unwrap_or_else(|| entry.package.id());
            if map.insert(id.clone(), entry.package).is_some() {
                return Err(LockError::DuplicatePackage { id });
            }

            if let Some(fingerprint) = entry.resolution_fingerprint {
                let proof = entry
                    .proof
                    .ok_or_else(|| LockError::MissingResolutionProof { id: id.clone() })?;
                let resolution = PackageLockResolution {
                    package: id.clone(),
                    proof,
                };
                if let Some(previous) = resolutions.insert(fingerprint.clone(), resolution) {
                    if previous.package != id {
                        return Err(LockError::DuplicateResolution { fingerprint });
                    }
                }
            }
        }

        Ok(Self {
            schema: 1,
            inputs: PackageResolverInputs::default(),
            input_fingerprint: None,
            resolutions,
            packages: map,
        })
    }

    pub fn with_input_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.input_fingerprint = Some(fingerprint.into());
        self
    }

    pub fn with_resolver_inputs(mut self, inputs: PackageResolverInputs) -> Self {
        self.inputs = inputs;
        self
    }

    pub fn matches_input_fingerprint(&self, fingerprint: &str) -> bool {
        self.input_fingerprint.as_deref() == Some(fingerprint)
    }

    pub fn from_ops(ops: &[Op]) -> Result<Self, LockError> {
        Self::from_packages(ops.iter().filter_map(|op| match op {
            Op::RealizePackage { package } => Some(package.clone()),
            Op::Package {
                intent: PackageIntent::Locked(package),
            } => Some(package.clone()),
            _ => None,
        }))
    }

    pub fn has_package_ops(ops: &[Op]) -> bool {
        ops.iter()
            .any(|op| matches!(op, Op::Package { .. } | Op::RealizePackage { .. }))
    }

    pub fn apply_to_ops(&self, ops: &[Op]) -> Result<Vec<Op>, LockError> {
        let context = ResolveContext::current();
        ops.iter()
            .map(|op| match op {
                Op::Package { intent } => Ok(Op::RealizePackage {
                    package: self.package_for_intent(intent, &context)?.clone(),
                }),
                Op::RealizePackage { package } => {
                    let id = package.id();
                    let Some(locked) = self.packages.get(&id) else {
                        return Err(LockError::MissingPackage { id });
                    };

                    if !locked.same_realization_input(package) {
                        return Err(LockError::PackageChanged { id });
                    }

                    Ok(Op::RealizePackage {
                        package: locked.clone(),
                    })
                }
                op => Ok(op.clone()),
            })
            .collect()
    }

    pub fn package_for_intent(
        &self,
        intent: &PackageIntent,
        context: &ResolveContext,
    ) -> Result<&LockedPackage, LockError> {
        match intent {
            PackageIntent::Request(request) => self.package_for_request(request, context),
            PackageIntent::Locked(package) => self.package_for_locked_spec(package),
        }
    }

    pub fn package_for_request(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<&LockedPackage, LockError> {
        let fingerprint = resolution_fingerprint(request, context)?;
        if let Some(resolution) = self.resolutions.get(&fingerprint) {
            let id = &resolution.package;
            return self
                .packages
                .get(id)
                .ok_or_else(|| LockError::MissingPackage { id: id.clone() });
        }

        if let Some(version) = &request.version {
            let id = package_id_for_request_version(request, version);
            if let Some(package) = self.packages.get(&id) {
                return Ok(package);
            }
        }

        Err(LockError::MissingPackage {
            id: request.to_string(),
        })
    }

    fn package_for_locked_spec(
        &self,
        package: &LockedPackage,
    ) -> Result<&LockedPackage, LockError> {
        let id = package.id();
        let Some(locked) = self.packages.get(&id) else {
            return Err(LockError::MissingPackage { id });
        };

        if !locked.same_realization_input(package) {
            return Err(LockError::PackageChanged { id });
        }

        Ok(locked)
    }

    pub fn read(path: impl AsRef<Path>) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(io::Error::other)
    }

    pub fn write(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(path, format!("{json}\n"))
    }
}

fn package_id_for_request(request: &PackageRequest, package: &LockedPackage) -> String {
    package_id_for_request_version(request, &package.version)
}

fn package_id_for_request_version(request: &PackageRequest, version: &str) -> String {
    match &request.resolver {
        Some(resolver) => format!("{resolver}:{}@{version}", request.name),
        None => format!("{}@{version}", request.name),
    }
}

fn resolution_fingerprint(
    request: &PackageRequest,
    context: &ResolveContext,
) -> Result<String, LockError> {
    request
        .resolution_input(context)
        .fingerprint()
        .map(|fingerprint| fingerprint.into_string())
        .map_err(|e| LockError::Fingerprint {
            kind: "package.resolution",
            error: e.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;
    use crate::package::{
        ArchiveFormat, ExternalManagerProof, LockedInstall, LockedSource, Provides,
    };

    fn package() -> LockedPackage {
        LockedPackage {
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
            source: LockedSource::Url {
                url: "file:///tmp/demo.tar.gz".to_string(),
                sha256: "abc123".to_string(),
            },
            install: LockedInstall::Archive {
                format: ArchiveFormat::TarGz,
                strip_prefix: Some(PathBuf::from("demo")),
            },
            provides: Provides {
                bins: BTreeMap::from([("demo".to_string(), PathBuf::from("bin/demo"))]),
            },
            output_sha256: None,
        }
    }

    fn proof(manager: &str) -> ResolutionProof {
        ResolutionProof::ExternalManager(ExternalManagerProof {
            manager: manager.to_string(),
            inputs: BTreeMap::new(),
            notes: vec!["test resolver".to_string()],
        })
    }

    #[test]
    fn collects_packages_from_ops_into_map() {
        let package = package();
        let lock = RootbeerLock::from_ops(&[Op::RealizePackage {
            package: package.clone(),
        }])
        .unwrap();

        assert_eq!(lock.schema, 1);
        assert!(lock.inputs.is_empty());
        assert!(lock.input_fingerprint.is_none());
        assert!(lock.resolutions.is_empty());
        assert_eq!(lock.packages.get("demo@1.0.0"), Some(&package));
    }

    #[test]
    fn rejects_duplicate_packages() {
        let package = package();
        let err = RootbeerLock::from_packages([package.clone(), package]).unwrap_err();

        assert_eq!(
            err,
            LockError::DuplicatePackage {
                id: "demo@1.0.0".to_string()
            }
        );
    }

    #[test]
    fn applies_locked_output_hash_to_ops() {
        let mut locked = package();
        locked.output_sha256 = Some("out".to_string());
        let planned = package();
        let lock = RootbeerLock::from_packages([locked]).unwrap();

        let ops = lock
            .apply_to_ops(&[Op::RealizePackage { package: planned }])
            .unwrap();

        let [Op::RealizePackage { package }] = ops.as_slice() else {
            panic!("expected package op");
        };
        assert_eq!(package.output_sha256.as_deref(), Some("out"));
    }

    #[test]
    fn applies_locked_resolution_to_package_request_op() {
        let request = PackageRequest::new("demo").resolver("aqua");
        let context = ResolveContext::current();
        let mut locked = package();
        locked.output_sha256 = Some("out".to_string());
        let lock = RootbeerLock::from_package_entries([PackageLockEntry::resolved(
            &request,
            &context,
            PackageResolution::new(locked, proof("aqua")),
        )
        .unwrap()])
        .unwrap();

        let ops = lock
            .apply_to_ops(&[Op::Package {
                intent: PackageIntent::Request(request),
            }])
            .unwrap();

        let [Op::RealizePackage { package }] = ops.as_slice() else {
            panic!("expected package op");
        };
        assert_eq!(package.output_sha256.as_deref(), Some("out"));
    }

    #[test]
    fn explicit_resolver_entries_are_namespaced_by_resolver() {
        let context = ResolveContext::current();
        let aqua_request = PackageRequest::new("demo").resolver("aqua");
        let other_request = PackageRequest::new("demo").resolver("other");
        let mut aqua_package = package();
        aqua_package.source = LockedSource::Url {
            url: "file:///tmp/aqua.tar.gz".to_string(),
            sha256: "abc123".to_string(),
        };
        let mut other_package = package();
        other_package.source = LockedSource::Url {
            url: "file:///tmp/other.tar.gz".to_string(),
            sha256: "abc123".to_string(),
        };

        let lock = RootbeerLock::from_package_entries([
            PackageLockEntry::resolved(
                &aqua_request,
                &context,
                PackageResolution::new(aqua_package, proof("aqua")),
            )
            .unwrap(),
            PackageLockEntry::resolved(
                &other_request,
                &context,
                PackageResolution::new(other_package, proof("other")),
            )
            .unwrap(),
        ])
        .unwrap();

        assert!(lock.packages.contains_key("aqua:demo@1.0.0"));
        assert!(lock.packages.contains_key("other:demo@1.0.0"));
        assert!(lock
            .resolutions
            .values()
            .any(|resolution| resolution.proof == proof("aqua")));
    }

    #[test]
    fn rejects_changed_package() {
        let locked = package();
        let mut planned = package();
        planned.source = LockedSource::Url {
            url: "file:///tmp/other.tar.gz".to_string(),
            sha256: "abc123".to_string(),
        };
        let lock = RootbeerLock::from_packages([locked]).unwrap();

        let err = lock
            .apply_to_ops(&[Op::RealizePackage { package: planned }])
            .unwrap_err();

        assert_eq!(
            err,
            LockError::PackageChanged {
                id: "demo@1.0.0".to_string()
            }
        );
    }

    #[test]
    fn writes_and_reads_lockfile() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("rootbeer.lock");
        let lock = RootbeerLock::from_packages([package()]).unwrap();

        lock.write(&path).unwrap();

        assert_eq!(RootbeerLock::read(&path).unwrap(), lock);
    }

    #[test]
    fn writes_and_reads_resolution_proofs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("rootbeer.lock");
        let request = PackageRequest::new("demo").resolver("aqua");
        let context = ResolveContext::current();
        let lock = RootbeerLock::from_package_entries([PackageLockEntry::resolved(
            &request,
            &context,
            PackageResolution::new(package(), proof("aqua")),
        )
        .unwrap()])
        .unwrap();

        lock.write(&path).unwrap();

        let read = RootbeerLock::read(&path).unwrap();
        assert_eq!(read, lock);
        assert_eq!(
            read.resolutions.values().next().unwrap().proof,
            proof("aqua")
        );
    }
}
