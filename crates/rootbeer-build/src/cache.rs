use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rootbeer_package::{BuildArtifact, CatalogRecipe, LockedPackage, LockedSource};
use rootbeer_store::hash_file;
use serde::{Deserialize, Serialize};

/// Persistent build results scoped to an explicitly identified host image and toolchain.
#[derive(Debug, Clone)]
pub struct BuildCache {
    pub directory: PathBuf,
    pub context: String,
    pub recheck: bool,
}

#[derive(Serialize, Deserialize)]
struct Record {
    key: String,
    artifact: BuildArtifact,
}

pub(crate) struct Entry {
    directory: PathBuf,
    key: String,
    should_rebuild: bool,
    _lock: fs::File,
}

impl BuildCache {
    pub(crate) fn entry(&self, key: String) -> Result<Entry, String> {
        if self.context.trim().is_empty() {
            return Err(
                "build cache requires an identity for the host image, SDK, and toolchain".into(),
            );
        }
        fs::create_dir_all(self.directory.join("locks")).map_err(|error| error.to_string())?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.directory.join("locks").join(&key))
            .map_err(|error| error.to_string())?;
        lock.lock().map_err(|error| error.to_string())?;
        Ok(Entry {
            directory: self.directory.join("results").join(&key),
            key,
            should_rebuild: self.recheck,
            _lock: lock,
        })
    }
}

pub(crate) fn key(
    name: &str,
    recipe: &CatalogRecipe,
    system: &str,
    dependencies: &BTreeMap<String, LockedPackage>,
    context: &str,
    jobs: usize,
    exports: &BTreeMap<String, rootbeer_package::graph::DependencyExports>,
) -> Result<String, String> {
    let outputs = dependencies
        .iter()
        .map(|(name, package)| {
            let hash = package
                .output_sha256
                .as_deref()
                .ok_or_else(|| format!("{name}: dependency output is not locked"))?;
            Ok((name, (hash, &package.provides, exports.get(name))))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let engine = rootbeer_store::hash_bytes(
        concat!(
            include_str!("lib.rs"),
            include_str!("cache.rs"),
            include_str!("plan.rs"),
            include_str!("dependencies.rs"),
            include_str!("runner.rs"),
            include_str!("archive.rs"),
            include_str!("backend/mod.rs"),
            include_str!("backend/autotools.rs"),
            include_str!("backend/custom.rs"),
            include_str!("backend/zig.rs"),
            include_str!("../../rootbeer-package/src/build_spec.rs"),
            include_str!("../../rootbeer-package/src/realize.rs"),
            include_str!("../../rootbeer-package/src/graph.rs"),
            include_str!("../../rootbeer-store/src/lib.rs"),
            include_str!("../../../Cargo.lock")
        )
        .as_bytes(),
    );
    let bytes = serde_json::to_vec(&(
        "rootbeer-build-v1",
        engine,
        name,
        recipe,
        system,
        outputs,
        context,
        jobs,
    ))
    .map_err(|error| error.to_string())?;
    Ok(rootbeer_store::hash_bytes(&bytes))
}

impl Entry {
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    pub(crate) fn restore(&self, output: &Path) -> Result<Option<BuildArtifact>, String> {
        if self.should_rebuild
            || !self
                .directory
                .try_exists()
                .map_err(|error| error.to_string())?
        {
            return Ok(None);
        }
        let bytes =
            fs::read(self.directory.join("receipt.json")).map_err(|error| error.to_string())?;
        let mut record: Record =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if record.key != self.key {
            return Err("build cache key mismatch".into());
        }
        let LockedSource::File { path, sha256 } = &mut record.artifact.package.source else {
            return Err("build cache artifact must be a local archive".into());
        };
        let archive = self.directory.join("package.tar.gz");
        if hash_file(&archive).map_err(|error| error.to_string())? != *sha256 {
            return Err("build cache archive hash mismatch".into());
        }
        *path = output.join("package.tar.gz");
        fs::copy(archive, path).map_err(|error| error.to_string())?;
        eprintln!("reuse build: {}", record.artifact.package.id());
        Ok(Some(record.artifact))
    }

    pub(crate) fn save(&self, artifact: &BuildArtifact, source: &Path) -> Result<(), String> {
        let parent = self.directory.parent().unwrap();
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let staging = tempfile::tempdir_in(parent).map_err(|error| error.to_string())?;
        fs::copy(
            source.join("package.tar.gz"),
            staging.path().join("package.tar.gz"),
        )
        .map_err(|error| error.to_string())?;
        fs::write(
            staging.path().join("receipt.json"),
            serde_json::to_vec(&Record {
                key: self.key.clone(),
                artifact: artifact.clone(),
            })
            .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if self.directory.exists() {
            fs::remove_dir_all(&self.directory).map_err(|error| error.to_string())?;
        }
        fs::rename(staging.path(), &self.directory).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rootbeer_package::{LockedInstall, PackageCatalog, Provides};

    #[test]
    fn keys_track_build_inputs_without_tracking_dependency_locations() {
        let catalog = PackageCatalog::embedded().unwrap();
        let recipe = catalog.packages["xz"].versions.values().next().unwrap();
        let mut dependencies = BTreeMap::from([(
            "compiler@1".into(),
            LockedPackage {
                name: "compiler".into(),
                version: "1".into(),
                source: LockedSource::File {
                    path: "/first/compiler.tar.gz".into(),
                    sha256: "a".repeat(64),
                },
                install: LockedInstall::Directory { strip_prefix: None },
                provides: Provides::default(),
                output_sha256: Some("b".repeat(64)),
            },
        )]);
        let digest = |recipe: &CatalogRecipe, dependencies: &BTreeMap<String, LockedPackage>| {
            key(
                "xz@1",
                recipe,
                "aarch64-macos",
                dependencies,
                "sdk-v1",
                2,
                &BTreeMap::new(),
            )
            .unwrap()
        };
        let original = digest(recipe, &dependencies);
        dependencies.get_mut("compiler@1").unwrap().source = LockedSource::File {
            path: "/second/compiler.tar.gz".into(),
            sha256: "c".repeat(64),
        };
        assert_eq!(original, digest(recipe, &dependencies));
        let libraries = BTreeMap::from([(
            "compiler@1".into(),
            rootbeer_package::graph::DependencyExports {
                has_bins: true,
                has_libraries: true,
                libraries: vec![PathBuf::from("lib/libcompiler.a")],
            },
        )]);
        assert_ne!(
            original,
            key(
                "xz@1",
                recipe,
                "aarch64-macos",
                &dependencies,
                "sdk-v1",
                2,
                &libraries
            )
            .unwrap()
        );
        dependencies.get_mut("compiler@1").unwrap().output_sha256 = Some("d".repeat(64));
        assert_ne!(original, digest(recipe, &dependencies));
        dependencies.get_mut("compiler@1").unwrap().output_sha256 = None;
        assert!(key(
            "xz@1",
            recipe,
            "aarch64-macos",
            &dependencies,
            "sdk-v1",
            2,
            &BTreeMap::new()
        )
        .is_err());
        let mut changed = recipe.clone();
        changed
            .build
            .as_mut()
            .unwrap()
            .patches
            .push("different patch".into());
        assert_ne!(
            digest(recipe, &BTreeMap::new()),
            digest(&changed, &BTreeMap::new())
        );
    }

    #[test]
    fn concurrent_builds_hold_one_lock_per_key() {
        let root = tempfile::tempdir().unwrap();
        let cache = BuildCache {
            directory: root.path().into(),
            context: "test".into(),
            recheck: false,
        };
        let first = cache.entry("same".into()).unwrap();
        let (started, started_receiver) = std::sync::mpsc::channel();
        let (acquired, acquired_receiver) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            started.send(()).unwrap();
            let _second = cache.entry("same".into()).unwrap();
            acquired.send(()).unwrap();
        });
        started_receiver.recv().unwrap();
        assert!(acquired_receiver
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err());
        drop(first);
        acquired_receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        thread.join().unwrap();
    }
}
