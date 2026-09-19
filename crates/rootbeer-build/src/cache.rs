use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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

/// Shares completed cache entries across builds in one invocation, including forced rechecks.
#[derive(Debug, Clone, Default)]
pub struct BuildSession {
    completed: Arc<Mutex<BTreeSet<PathBuf>>>,
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
    session: Option<BuildSession>,
    _lock: fs::File,
}

impl BuildCache {
    pub(crate) fn entry(
        &self,
        key: String,
        session: Option<&BuildSession>,
    ) -> Result<Entry, String> {
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
        let directory = self.directory.join("results").join(&key);
        let is_completed =
            session.is_some_and(|session| session.completed.lock().unwrap().contains(&directory));
        Ok(Entry {
            directory,
            key,
            should_rebuild: self.recheck && !is_completed,
            session: session.cloned(),
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
    let engine = env!("ROOTBEER_ENGINE_IDENTITY");
    let mut compilation_recipe = recipe.clone();
    compilation_recipe.checks.clear();
    let bytes = serde_json::to_vec(&(
        "rootbeer-build-v3",
        engine,
        name,
        compilation_recipe,
        system,
        outputs,
        context,
    ))
    .map_err(|error| error.to_string())?;
    Ok(rootbeer_store::hash_bytes(&bytes))
}

impl Entry {
    pub(crate) fn complete(&self) {
        if let Some(session) = &self.session {
            session
                .completed
                .lock()
                .unwrap()
                .insert(self.directory.clone());
        }
    }

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
    use rootbeer_package::{LockedInstall, Provides};

    #[test]
    fn keys_track_build_inputs_without_tracking_dependency_locations() {
        let catalog = crate::test_catalog::catalog();
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
                runtime_dependencies: Default::default(),
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
                &BTreeMap::new(),
            )
            .unwrap()
        };
        let original = digest(recipe, &dependencies);
        let mut checks_changed = recipe.clone();
        checks_changed
            .checks
            .push(vec!["xz".into(), "--help".into()]);
        assert_eq!(original, digest(&checks_changed, &dependencies));
        checks_changed.revision += 1;
        assert_ne!(original, digest(&checks_changed, &dependencies));
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
            recheck: true,
        };
        let session = BuildSession::default();
        let first = cache.entry("same".into(), Some(&session)).unwrap();
        assert!(first.should_rebuild);
        let (started, started_receiver) = std::sync::mpsc::channel();
        let (acquired, acquired_receiver) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            started.send(()).unwrap();
            let second = cache.entry("same".into(), Some(&session)).unwrap();
            assert!(!second.should_rebuild);
            acquired.send(()).unwrap();
        });
        started_receiver.recv().unwrap();
        assert!(acquired_receiver
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err());
        first.complete();
        drop(first);
        acquired_receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        thread.join().unwrap();
    }
}
