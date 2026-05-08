use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::LockedPackage;
use crate::Op;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootbeerLock {
    pub schema: u32,
    pub packages: BTreeMap<String, LockedPackage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockError {
    DuplicatePackage { id: String },
    MissingPackage { id: String },
    PackageChanged { id: String },
    MissingLockfile { path: std::path::PathBuf },
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockError::DuplicatePackage { id } => write!(f, "duplicate package `{id}`"),
            LockError::MissingPackage { id } => {
                write!(f, "package `{id}` is not locked; run `rb package lock`")
            }
            LockError::PackageChanged { id } => write!(
                f,
                "package `{id}` differs from rootbeer.lock; run `rb package lock`"
            ),
            LockError::MissingLockfile { path } => write!(
                f,
                "package lockfile {} is missing; run `rb package lock`",
                path.display()
            ),
        }
    }
}

impl std::error::Error for LockError {}

impl RootbeerLock {
    pub fn from_packages(
        packages: impl IntoIterator<Item = LockedPackage>,
    ) -> Result<Self, LockError> {
        let mut map = BTreeMap::new();
        for package in packages {
            let id = package.id();
            if map.insert(id.clone(), package).is_some() {
                return Err(LockError::DuplicatePackage { id });
            }
        }

        Ok(Self {
            schema: 1,
            packages: map,
        })
    }

    pub fn from_ops(ops: &[Op]) -> Result<Self, LockError> {
        Self::from_packages(ops.iter().filter_map(|op| match op {
            Op::RealizePackage { package } => Some(package.clone()),
            _ => None,
        }))
    }

    pub fn has_package_ops(ops: &[Op]) -> bool {
        ops.iter().any(|op| matches!(op, Op::RealizePackage { .. }))
    }

    pub fn apply_to_ops(&self, ops: &[Op]) -> Result<Vec<Op>, LockError> {
        ops.iter()
            .map(|op| match op {
                Op::RealizePackage { package } => {
                    let id = package.id();
                    let Some(locked) = self.packages.get(&id) else {
                        return Err(LockError::MissingPackage { id });
                    };

                    if locked.without_output_hash() != package.without_output_hash() {
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;
    use crate::package::{ArchiveFormat, LockedInstall, LockedSource, Provides};

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

    #[test]
    fn collects_packages_from_ops_into_map() {
        let package = package();
        let lock = RootbeerLock::from_ops(&[Op::RealizePackage {
            package: package.clone(),
        }])
        .unwrap();

        assert_eq!(lock.schema, 1);
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
}
