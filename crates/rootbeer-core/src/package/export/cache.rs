use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::package::{
    publication, ArtifactIndex, LockedSource, PackageCatalog, PackageRequest, PublishedArtifact,
};
use crate::store::{hash_bytes, hash_file};

/// Reuses successful exports from a trusted cache under an explicit build environment identity.
pub struct ExportCache {
    pub directory: PathBuf,
    pub context: String,
    pub recheck: bool,
}

#[derive(Serialize, Deserialize)]
struct Record {
    fingerprint: String,
    index: ArtifactIndex,
}

pub(super) struct Cache<'a> {
    options: &'a ExportCache,
    engine: String,
}

impl<'a> Cache<'a> {
    pub(super) fn new(options: &'a ExportCache) -> Result<Self, String> {
        if options.context.trim().is_empty() {
            return Err("export cache requires a build environment identity".into());
        }
        fs::create_dir_all(&options.directory).map_err(|e| e.to_string())?;
        let engine = hash_file(std::env::current_exe().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        Ok(Self { options, engine })
    }

    pub(super) fn fingerprint(
        &self,
        catalog: &PackageCatalog,
        key: &str,
        system: &str,
        registry: &str,
    ) -> Result<String, String> {
        fingerprint(
            catalog,
            key,
            system,
            registry,
            &self.engine,
            &self.options.context,
        )
    }

    pub(super) fn restore(
        &self,
        fingerprint: &str,
        key: &str,
        system: &str,
        destination: &Path,
    ) -> Result<Option<PublishedArtifact>, String> {
        if self.options.recheck {
            return Ok(None);
        }
        let directory = self.options.directory.join(fingerprint);
        if !directory.try_exists().map_err(|e| e.to_string())? {
            return Ok(None);
        }
        let record: Record = publication::read_json(&directory.join("record.json"))?;
        if record.fingerprint != fingerprint {
            return Err("export cache fingerprint mismatch".into());
        }
        record.index.validate()?;
        let artifact = record
            .index
            .artifacts
            .get(key)
            .and_then(|systems| systems.get(system))
            .ok_or("export cache is missing its package")?;
        copy_artifact(artifact, &directory, destination)?;
        Ok(Some(artifact.clone()))
    }

    pub(super) fn save(
        &self,
        fingerprint: &str,
        key: &str,
        system: &str,
        catalog: &PackageCatalog,
        artifact: &PublishedArtifact,
        source: &Path,
    ) -> Result<(), String> {
        let staging = tempfile::tempdir_in(&self.options.directory).map_err(|e| e.to_string())?;
        let bundle = staging.path().join("bundle");
        publication::create_bundle(&bundle)?;
        copy_artifact(artifact, source, &bundle)?;
        let record = Record {
            fingerprint: fingerprint.into(),
            index: ArtifactIndex {
                schema: 1,
                catalog: catalog.clone(),
                catalog_sha256: catalog.sha256(),
                artifacts: BTreeMap::from([(
                    key.into(),
                    BTreeMap::from([(system.into(), artifact.clone())]),
                )]),
            },
        };
        record.index.validate()?;
        publication::write_json(&bundle.join("record.json"), &record)?;
        let destination = self.options.directory.join(fingerprint);
        if destination.exists() {
            fs::remove_dir_all(&destination).map_err(|e| e.to_string())?;
        }
        fs::rename(bundle, destination).map_err(|e| e.to_string())
    }
}

fn copy_artifact(
    artifact: &PublishedArtifact,
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    publication::copy_verified(
        &source
            .join("receipts")
            .join(format!("{}.json", artifact.receipt_sha256)),
        &destination.join("receipts"),
        ".json",
    )?;
    if let LockedSource::Url { url, sha256 } = &artifact.package.source {
        if url.starts_with("ghcr://") {
            publication::copy_verified(
                &source.join("artifacts").join(format!("{sha256}.tar.gz")),
                &destination.join("artifacts"),
                ".tar.gz",
            )?;
        }
    }
    Ok(())
}

fn fingerprint(
    catalog: &PackageCatalog,
    key: &str,
    system: &str,
    registry: &str,
    engine: &str,
    context: &str,
) -> Result<String, String> {
    let mut recipes = BTreeMap::new();
    let mut pending = vec![key.to_string()];
    let mut visited = BTreeSet::new();
    while let Some(key) = pending.pop() {
        let request = PackageRequest::parse(&key);
        let package = catalog
            .find(&request.name)
            .ok_or_else(|| format!("unknown cache dependency {key}"))?;
        let version = request
            .version
            .as_deref()
            .ok_or("cache inputs must use exact versions")?;
        let canonical = format!("{}@{version}", package.name);
        if !visited.insert(canonical.clone()) {
            continue;
        }
        let recipe = package
            .versions
            .get(version)
            .ok_or_else(|| format!("unknown cache recipe {canonical}"))?;
        if !recipe.systems.iter().any(|target| target == system) {
            return Err(format!("{canonical} has no recipe for {system}"));
        }
        recipes.insert(canonical, recipe);
        if let Some(build) = &recipe.build {
            pending.extend(build.dependencies.iter().cloned());
        }
    }
    let bytes = serde_json::to_vec(&(
        "rootbeer-export-v1",
        engine,
        context,
        registry,
        system,
        recipes,
    ))
    .map_err(|e| e.to_string())?;
    Ok(hash_bytes(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::{bundle_artifacts, export_catalog_with_cache, ResolveContext};

    #[test]
    fn fingerprints_track_dependencies_without_invalidating_unrelated_packages() {
        let catalog = PackageCatalog::embedded().unwrap().clone();
        let key = "xz@5.8.3";
        let digest = |catalog: &PackageCatalog| {
            fingerprint(
                catalog,
                key,
                "aarch64-linux",
                "owner/index",
                "engine",
                "image",
            )
            .unwrap()
        };
        let original = digest(&catalog);
        let mut changed = catalog.clone();
        changed
            .packages
            .get_mut("fd")
            .unwrap()
            .versions
            .get_mut("10.5.0")
            .unwrap()
            .revision += 1;
        assert_eq!(original, digest(&changed));
        changed
            .packages
            .get_mut("xz")
            .unwrap()
            .description
            .push_str(" metadata");
        assert_eq!(original, digest(&changed));
        changed
            .packages
            .get_mut("xz")
            .unwrap()
            .versions
            .get_mut("5.8.3")
            .unwrap()
            .checks
            .push(vec!["xz".into(), "--help".into()]);
        assert_ne!(original, digest(&changed));

        let mut dependent = catalog.clone();
        dependent
            .packages
            .get_mut("xz")
            .unwrap()
            .versions
            .get_mut("5.8.3")
            .unwrap()
            .build
            .as_mut()
            .unwrap()
            .dependencies
            .push("fd@10.5.0".into());
        let before = digest(&dependent);
        dependent
            .packages
            .get_mut("fd")
            .unwrap()
            .versions
            .get_mut("10.5.0")
            .unwrap()
            .revision += 1;
        assert_ne!(before, digest(&dependent));
        let mut build = catalog.packages["xz"].versions["5.8.3"]
            .build
            .clone()
            .unwrap();
        build.dependencies = vec!["ripgrep@15.2.0".into()];
        let dependency = dependent
            .packages
            .get_mut("fd")
            .unwrap()
            .versions
            .get_mut("10.5.0")
            .unwrap();
        dependency.source = None;
        dependency.build = Some(build);
        let before = digest(&dependent);
        dependent
            .packages
            .get_mut("ripgrep")
            .unwrap()
            .versions
            .get_mut("15.2.0")
            .unwrap()
            .revision += 1;
        assert_ne!(before, digest(&dependent));
        for (system, registry, engine, context) in [
            ("x86_64-linux", "owner/index", "engine", "image"),
            ("aarch64-linux", "owner/other", "engine", "image"),
            ("aarch64-linux", "owner/index", "new-engine", "image"),
            ("aarch64-linux", "owner/index", "engine", "new-image"),
        ] {
            assert_ne!(
                original,
                fingerprint(&catalog, key, system, registry, engine, context).unwrap()
            );
        }
    }

    #[test]
    fn cached_export_preserves_receipts_and_rejects_corrupt_content() {
        let root = tempfile::tempdir().unwrap();
        let (catalog, receipt) = crate::package::bundle::tests::fixture(root.path());
        let system = ResolveContext::current().system;
        let mut receipt_value: serde_json::Value = publication::read_json(&receipt).unwrap();
        receipt_value["system"] = system.clone().into();
        publication::write_json(&receipt, &receipt_value).unwrap();
        let source = root.path().join("source");
        bundle_artifacts(&catalog, &[receipt], "ghcr://owner/index/xz", &source).unwrap();
        let index: ArtifactIndex = publication::read_json(&source.join("index.json")).unwrap();
        let key = "xz@5.8.3";
        let artifact = &index.artifacts[key][&system];
        let options = ExportCache {
            directory: root.path().join("cache"),
            context: "test-image".into(),
            recheck: false,
        };
        let cache = Cache::new(&options).unwrap();
        let fingerprint = cache
            .fingerprint(&catalog, key, &system, "owner/index")
            .unwrap();
        cache
            .save(&fingerprint, key, &system, &catalog, artifact, &source)
            .unwrap();
        let mut current = catalog.clone();
        current.packages.retain(|name, _| name == "xz");
        let output = root.path().join("output");
        export_catalog_with_cache(&current, "owner/index", &output, 2, Some(&options)).unwrap();
        let exported: ArtifactIndex = publication::read_json(&output.join("index.json")).unwrap();
        assert_eq!(exported.catalog_sha256, current.sha256());
        assert_eq!(
            exported.artifacts[key][&system].receipt_sha256,
            artifact.receipt_sha256
        );
        let receipt = format!("{}.json", artifact.receipt_sha256);
        assert_eq!(
            fs::read(output.join("receipts").join(&receipt)).unwrap(),
            fs::read(source.join("receipts").join(&receipt)).unwrap()
        );
        fs::write(
            options
                .directory
                .join(&fingerprint)
                .join("receipts")
                .join(&receipt),
            "tampered",
        )
        .unwrap();
        let failed = root.path().join("failed");
        assert!(
            export_catalog_with_cache(&current, "owner/index", &failed, 2, Some(&options)).is_err()
        );
        assert!(!failed.exists());
        cache
            .save(&fingerprint, key, &system, &catalog, artifact, &source)
            .unwrap();
        let LockedSource::Url { sha256, .. } = &artifact.package.source else {
            panic!("expected URL")
        };
        fs::write(
            options
                .directory
                .join(&fingerprint)
                .join("artifacts")
                .join(format!("{sha256}.tar.gz")),
            "tampered archive",
        )
        .unwrap();
        assert!(cache.restore(&fingerprint, key, &system, &output).is_err());
        let refresh = ExportCache {
            recheck: true,
            ..options
        };
        let cache = Cache::new(&refresh).unwrap();
        assert!(cache
            .restore(&fingerprint, key, &system, &output)
            .unwrap()
            .is_none());
    }
}
