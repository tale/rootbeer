use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};

use super::{LockedInstall, LockedPackage, LockedSource, Provides};
use crate::store::{hash_tree, Store, StoreEntry};

#[derive(Debug, Clone)]
pub struct PackageRealizer {
    store: Store,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealizedPackage {
    pub package: LockedPackage,
    pub store_entry: StoreEntry,
    pub bins: BTreeMap<String, PathBuf>,
}

impl PackageRealizer {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    pub fn default() -> Self {
        Self::new(Store::default())
    }

    pub fn realize(&self, package: &LockedPackage) -> io::Result<RealizedPackage> {
        let source_root = self.verify_source(&package.source)?;
        let install_root = install_root(&source_root, &package.install)?;
        validate_provides(&install_root, &package.provides)?;

        let store_entry =
            self.store
                .add_tree(package.name.clone(), package.version.clone(), &install_root)?;

        let bins = package
            .provides
            .bins
            .iter()
            .map(|(name, rel)| (name.clone(), store_entry.path.join(rel)))
            .collect();

        Ok(RealizedPackage {
            package: package.clone(),
            store_entry,
            bins,
        })
    }

    fn verify_source(&self, source: &LockedSource) -> io::Result<PathBuf> {
        match source {
            LockedSource::Path { path, sha256 } => {
                let actual = hash_tree(path)?;
                if actual != *sha256 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "source {} hash mismatch: expected {}, got {}",
                            path.display(),
                            sha256,
                            actual
                        ),
                    ));
                }

                Ok(path.clone())
            }
        }
    }
}

fn install_root(source_root: &Path, install: &LockedInstall) -> io::Result<PathBuf> {
    match install {
        LockedInstall::Directory { strip_prefix } => {
            let root = match strip_prefix {
                Some(prefix) => {
                    validate_relative_path("strip_prefix", prefix)?;
                    source_root.join(prefix)
                }
                None => source_root.to_path_buf(),
            };

            if !root.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("install root {} is not a directory", root.display()),
                ));
            }

            Ok(root)
        }
    }
}

/// Contrary to seeming like this does something with dependency, this just
/// validates that the provided binaries exist in the install root and have
/// valid relative paths.
fn validate_provides(root: &Path, provides: &Provides) -> io::Result<()> {
    for (name, rel) in &provides.bins {
        if name.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "provided binary name cannot be empty",
            ));
        }

        validate_relative_path("provided binary path", rel)?;
        let path = root.join(rel);

        if !path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "provided binary `{name}` points to missing file {}",
                    path.display()
                ),
            ));
        }
    }

    Ok(())
}

fn validate_relative_path(label: &str, path: &Path) -> io::Result<()> {
    let mut has_component = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => has_component = true,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "{label} must be a relative path without `..`: {}",
                        path.display()
                    ),
                ));
            }
        }
    }

    if !has_component {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{label} cannot be empty"),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn package(source: &Path, source_hash: String) -> LockedPackage {
        LockedPackage {
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
            source: LockedSource::Path {
                path: source.to_path_buf(),
                sha256: source_hash,
            },
            install: LockedInstall::Directory {
                strip_prefix: Some(PathBuf::from("pkg")),
            },
            provides: Provides {
                bins: BTreeMap::from([("demo".to_string(), PathBuf::from("bin/demo"))]),
            },
        }
    }

    fn source_tree() -> tempfile::TempDir {
        let source = tempfile::tempdir().unwrap();
        fs::create_dir_all(source.path().join("pkg/bin")).unwrap();
        fs::write(source.path().join("pkg/bin/demo"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(
            source.path().join("pkg/bin/demo"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        source
    }

    #[test]
    fn realizes_verified_path_source_into_store() {
        let store_root = tempfile::tempdir().unwrap();
        let source = source_tree();
        let package = package(source.path(), hash_tree(source.path()).unwrap());

        let realizer = PackageRealizer::new(Store::new(store_root.path().join("store")));
        let realized = realizer.realize(&package).unwrap();

        assert!(realized.store_entry.path.exists());
        assert!(realized.store_entry.path.join("bin/demo").is_file());
        assert_eq!(
            realized.bins.get("demo").unwrap(),
            &realized.store_entry.path.join("bin/demo")
        );
        assert_eq!(
            fs::metadata(realized.store_entry.path.join("bin/demo"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }

    #[test]
    fn rejects_source_hash_mismatch() {
        let store_root = tempfile::tempdir().unwrap();
        let source = source_tree();
        let package = package(source.path(), "bad-hash".to_string());

        let realizer = PackageRealizer::new(Store::new(store_root.path().join("store")));
        let err = realizer.realize(&package).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("hash mismatch"));
    }

    #[test]
    fn rejects_missing_provided_binary() {
        let store_root = tempfile::tempdir().unwrap();
        let source = source_tree();
        let mut package = package(source.path(), hash_tree(source.path()).unwrap());
        package
            .provides
            .bins
            .insert("missing".to_string(), PathBuf::from("bin/missing"));

        let realizer = PackageRealizer::new(Store::new(store_root.path().join("store")));
        let err = realizer.realize(&package).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(err.to_string().contains("provided binary `missing`"));
    }

    #[test]
    fn rejects_unsafe_relative_paths() {
        let store_root = tempfile::tempdir().unwrap();
        let source = source_tree();
        let mut package = package(source.path(), hash_tree(source.path()).unwrap());
        package.install = LockedInstall::Directory {
            strip_prefix: Some(PathBuf::from("../pkg")),
        };

        let realizer = PackageRealizer::new(Store::new(store_root.path().join("store")));
        let err = realizer.realize(&package).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("strip_prefix"));
    }
}
