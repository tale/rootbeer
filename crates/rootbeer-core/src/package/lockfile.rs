#[cfg(test)]
use super::{LockedPackage, PackageRequest, PackageResolution, ResolutionProof};
use super::{PackageIntent, ResolveContext};
use crate::Op;
pub use rootbeer_package::lockfile::*;

pub fn from_ops(ops: &[Op]) -> Result<RootbeerLock, LockError> {
    RootbeerLock::from_packages(ops.iter().filter_map(|op| match op {
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

pub fn apply_to_ops(lock: &RootbeerLock, ops: &[Op]) -> Result<Vec<Op>, LockError> {
    let context = ResolveContext::current();
    ops.iter()
        .map(|op| match op {
            Op::Package { intent } => Ok(Op::RealizePackage {
                package: lock.package_for_intent(intent, &context)?.clone(),
            }),
            Op::RealizePackage { package } => {
                let id = package.id();
                let Some(locked) = lock.packages.get(&id) else {
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
                apps: BTreeMap::new(),
                bins: BTreeMap::from([("demo".to_string(), PathBuf::from("bin/demo"))]),
            },
            runtime_dependencies: Default::default(),
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
    fn changed_github_options_do_not_fall_back_to_version_only() {
        let request = PackageRequest::parse("github:owner/demo@1.0.0");
        let context = ResolveContext::new("aarch64-macos");
        let entry = PackageLockEntry::resolved(
            &request,
            &context,
            PackageResolution::new(package(), proof("github")),
        )
        .unwrap();
        let lock = RootbeerLock::from_package_entries([entry]).unwrap();
        let mut changed = request;
        changed.asset = Some("different.tar.gz".to_string());
        assert!(lock.package_for_request(&changed, &context).is_err());
    }

    #[test]
    fn collects_packages_from_ops_into_map() {
        let package = package();
        let lock = crate::package::lockfile::from_ops(&[Op::RealizePackage {
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

        let ops = crate::package::lockfile::apply_to_ops(
            &lock,
            &[Op::RealizePackage { package: planned }],
        )
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

        let ops = crate::package::lockfile::apply_to_ops(
            &lock,
            &[Op::Package {
                intent: PackageIntent::Request(request),
            }],
        )
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

        let err = crate::package::lockfile::apply_to_ops(
            &lock,
            &[Op::RealizePackage { package: planned }],
        )
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
