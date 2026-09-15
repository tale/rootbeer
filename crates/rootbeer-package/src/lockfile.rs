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

        for package in map.values() {
            crate::runtime::closure(package).map_err(|error| LockError::Fingerprint {
                kind: "package.runtime",
                error,
            })?;
        }
        let schema = if map
            .values()
            .any(|package| !package.runtime_dependencies.is_empty())
        {
            3
        } else if map
            .values()
            .any(|package| !package.provides.apps.is_empty())
        {
            2
        } else {
            1
        };
        Ok(Self {
            schema,
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

        if let Some(version) = request.version.as_ref().filter(|_| {
            request.source.is_none() && request.asset.is_none() && request.bins.is_empty()
        }) {
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

    /// Store entries retained by these roots, including their complete runtime closure.
    pub fn store_paths(
        &self,
        store: &rootbeer_store::Store,
    ) -> Result<std::collections::BTreeSet<std::path::PathBuf>, String> {
        let mut paths = std::collections::BTreeSet::new();
        for package in self.packages.values() {
            for entry in crate::runtime::closure(package)?
                .into_iter()
                .chain(std::iter::once(package))
            {
                paths.insert(store.root().join(crate::runtime::store_directory(entry)?));
            }
        }
        Ok(paths)
    }

    pub fn read(path: impl AsRef<Path>) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        let lock: Self = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if !matches!(lock.schema, 1..=3) {
            return Err(io::Error::other(
                "unsupported package lock schema; update Rootbeer",
            ));
        }
        for package in lock.packages.values() {
            if lock.schema < 3 && !package.runtime_dependencies.is_empty() {
                return Err(io::Error::other(
                    "runtime dependencies require package lock schema 3",
                ));
            }
            crate::runtime::closure(package).map_err(io::Error::other)?;
        }
        Ok(lock)
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
