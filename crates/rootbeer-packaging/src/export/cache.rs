use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{publication, CatalogRecipe, LockedSource, PackageCatalog, PublishedArtifact};
use rootbeer_store::hash_bytes;

/// Reuses successful exports from a trusted cache under an explicit build environment identity.
pub struct ExportCache {
    pub directory: PathBuf,
    pub context: String,
    pub recheck: bool,
}

#[derive(Serialize, Deserialize)]
struct Record {
    fingerprint: String,
    artifact: PublishedArtifact,
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
        let engine = env!("ROOTBEER_ENGINE_IDENTITY").to_string();
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
        recipe: &CatalogRecipe,
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
        record.artifact.validate(key, system, recipe)?;
        copy_artifact(&record.artifact, &directory, destination)?;
        Ok(Some(record.artifact))
    }

    pub(super) fn save(
        &self,
        fingerprint: &str,
        key: &str,
        system: &str,
        recipe: &CatalogRecipe,
        artifact: &PublishedArtifact,
        source: &Path,
    ) -> Result<(), String> {
        let staging = tempfile::tempdir_in(&self.options.directory).map_err(|e| e.to_string())?;
        let bundle = staging.path().join("bundle");
        publication::create_bundle(&bundle)?;
        copy_artifact(artifact, source, &bundle)?;
        let record = Record {
            fingerprint: fingerprint.into(),
            artifact: artifact.clone(),
        };
        record.artifact.validate(key, system, recipe)?;
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
    let graph = rootbeer_package::graph::DependencyGraph::new(catalog, &[key.into()], system)?;
    let recipes = graph
        .order
        .iter()
        .map(|key| {
            let (_, _, recipe) = rootbeer_package::graph::find_recipe(catalog, key)?;
            Ok((key, recipe))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let bytes = serde_json::to_vec(&(
        "rootbeer-export-v3",
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
    use crate::{bundle_artifacts, ArtifactIndex, ResolveContext};

    #[test]
    fn fingerprints_track_dependencies_without_invalidating_unrelated_packages() {
        let catalog = crate::test_catalog::catalog().clone();
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
        dependency.assets.clear();
        dependency.checksums.clear();
        dependency.bin_paths.clear();
        dependency.mirror = false;
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
        let (catalog, receipt) = crate::bundle::tests::fixture(root.path());
        let system = ResolveContext::current().system;
        let mut receipt_value: serde_json::Value = publication::read_json(&receipt).unwrap();
        receipt_value["system"] = system.clone().into();
        publication::write_json(&receipt, &receipt_value).unwrap();
        let source = root.path().join("source");
        bundle_artifacts(&catalog, &[receipt], "ghcr://owner/index/xz", &source).unwrap();
        let index: ArtifactIndex = publication::read_json(&source.join("index.json")).unwrap();
        let key = "xz@5.8.3";
        let artifact = &index.artifacts[key][&system];
        let recipe = &catalog.packages["xz"].versions["5.8.3"];
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
            .save(&fingerprint, key, &system, recipe, artifact, &source)
            .unwrap();
        let record_path = options.directory.join(&fingerprint).join("record.json");
        let record: serde_json::Value = publication::read_json(&record_path).unwrap();
        assert!(record.get("index").is_none());
        assert!(record.get("catalog").is_none());
        for field in ["revision", "receipt_sha256", "package"] {
            let mut changed = record.clone();
            match field {
                "revision" => changed["artifact"][field] = 0.into(),
                "receipt_sha256" => changed["artifact"][field] = "invalid".into(),
                "package" => {
                    changed["artifact"][field]["provides"]["bins"]["xz"] = "../escape".into()
                }
                _ => unreachable!(),
            }
            publication::write_json(&record_path, &changed).unwrap();
            assert!(
                cache
                    .restore(&fingerprint, key, &system, recipe, &source)
                    .is_err(),
                "{field}"
            );
        }
        publication::write_json(&record_path, &record).unwrap();
        let output = root.path().join("output");
        publication::create_bundle(&output).unwrap();
        let restored = cache
            .restore(&fingerprint, key, &system, recipe, &output)
            .unwrap()
            .unwrap();
        assert_eq!(restored.receipt_sha256, artifact.receipt_sha256);
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
        assert!(cache
            .restore(&fingerprint, key, &system, recipe, &failed)
            .is_err());
        assert!(!failed.exists());
        cache
            .save(&fingerprint, key, &system, recipe, artifact, &source)
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
        assert!(cache
            .restore(&fingerprint, key, &system, recipe, &output)
            .is_err());
        let refresh = ExportCache {
            recheck: true,
            ..options
        };
        let cache = Cache::new(&refresh).unwrap();
        assert!(cache
            .restore(&fingerprint, key, &system, recipe, &output)
            .unwrap()
            .is_none());
    }
}
