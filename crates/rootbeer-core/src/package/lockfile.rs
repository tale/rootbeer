use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::LockedPackage;
use crate::Op;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootbeerLock {
    pub schema: u32,
    pub packages: Vec<LockedPackage>,
}

impl RootbeerLock {
    pub fn from_ops(ops: &[Op]) -> Self {
        let packages = ops
            .iter()
            .filter_map(|op| match op {
                Op::RealizePackage { package } => Some(package.clone()),
                _ => None,
            })
            .collect();

        Self {
            schema: 1,
            packages,
        }
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
        }
    }

    #[test]
    fn collects_packages_from_ops() {
        let package = package();
        let lock = RootbeerLock::from_ops(&[Op::RealizePackage {
            package: package.clone(),
        }]);

        assert_eq!(lock.schema, 1);
        assert_eq!(lock.packages, vec![package]);
    }

    #[test]
    fn writes_and_reads_lockfile() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("rootbeer.lock");
        let lock = RootbeerLock {
            schema: 1,
            packages: vec![package()],
        };

        lock.write(&path).unwrap();

        assert_eq!(RootbeerLock::read(&path).unwrap(), lock);
    }
}
