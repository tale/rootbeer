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
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    inputs: Inputs,
    artifact: PublishedArtifact,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Inputs {
    package: String,
    system: String,
    registry: String,
    engine: String,
    environment: String,
    recipes: BTreeMap<String, CatalogRecipe>,
}

impl Inputs {
    pub(super) fn fingerprint(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(&("rootbeer-qualification-v1", self))
            .map_err(|error| error.to_string())?;
        Ok(hash_bytes(&bytes))
    }

    fn validate(&self, artifact: &PublishedArtifact) -> Result<(), String> {
        artifact.validate(&self.package, &self.system, &self.recipes[&self.package])?;
        for dependency in rootbeer_package::runtime::closure(&artifact.package)? {
            let key = dependency.id();
            let recipe = self.recipes.get(&key).ok_or_else(|| {
                format!("{key}: runtime dependency missing from qualification inputs")
            })?;
            PublishedArtifact {
                revision: recipe.revision,
                receipt_sha256: artifact.receipt_sha256.clone(),
                package: dependency.clone(),
            }
            .validate(&key, &self.system, recipe)?;
        }
        Ok(())
    }
}

pub(super) struct Cache<'a> {
    options: &'a ExportCache,
    build_options: &'a rootbeer_build::BuildOptions,
    engine: String,
}

impl<'a> Cache<'a> {
    pub(super) fn new(
        options: &'a ExportCache,
        build_options: &'a rootbeer_build::BuildOptions,
    ) -> Result<Self, String> {
        if options.context.trim().is_empty() {
            return Err("export cache requires a build environment identity".into());
        }
        let engine = env!("ROOTBEER_ENGINE_IDENTITY").to_string();
        Ok(Self {
            options,
            build_options,
            engine,
        })
    }

    pub(super) fn inputs(
        &self,
        catalog: &PackageCatalog,
        key: &str,
        system: &str,
        registry: &str,
    ) -> Result<Inputs, String> {
        let environment =
            self.build_options
                .environment_identity(catalog, key, &self.options.context)?;
        inputs(catalog, key, system, registry, &self.engine, &environment)
    }

    fn load(&self, inputs: &Inputs) -> Result<Option<(PathBuf, PublishedArtifact)>, String> {
        if self.options.recheck {
            return Ok(None);
        }
        let directory = self.options.directory.join(inputs.fingerprint()?);
        if !directory.try_exists().map_err(|e| e.to_string())? {
            return Ok(None);
        }
        let record: Record = publication::read_json(&directory.join("record.json"))?;
        if record.schema != 1 || record.inputs != *inputs {
            return Err("qualification cache schema or inputs mismatch".into());
        }
        inputs.validate(&record.artifact)?;
        Ok(Some((directory, record.artifact)))
    }

    pub(super) fn inspect(&self, inputs: &Inputs) -> Result<Option<PublishedArtifact>, String> {
        let Some((directory, artifact)) = self.load(inputs)? else {
            return Ok(None);
        };
        for (path, suffix) in artifact_files(&artifact, &directory)? {
            publication::verify_file(&path, suffix)?;
        }
        Ok(Some(artifact))
    }

    pub(super) fn restore(
        &self,
        inputs: &Inputs,
        destination: &Path,
    ) -> Result<Option<PublishedArtifact>, String> {
        let Some((directory, artifact)) = self.load(inputs)? else {
            return Ok(None);
        };
        copy_artifact(&artifact, &directory, destination)?;
        Ok(Some(artifact))
    }

    pub(super) fn save(
        &self,
        catalog: &PackageCatalog,
        inputs: &Inputs,
        artifact: &PublishedArtifact,
        source: &Path,
    ) -> Result<(), String> {
        if self.inputs(catalog, &inputs.package, &inputs.system, &inputs.registry)? != *inputs {
            return Err("qualification inputs changed during export".into());
        }
        fs::create_dir_all(&self.options.directory).map_err(|error| error.to_string())?;
        let staging = tempfile::tempdir_in(&self.options.directory).map_err(|e| e.to_string())?;
        let bundle = staging.path().join("bundle");
        publication::create_bundle(&bundle)?;
        copy_artifact(artifact, source, &bundle)?;
        let record = Record {
            schema: 1,
            inputs: inputs.clone(),
            artifact: artifact.clone(),
        };
        inputs.validate(&record.artifact)?;
        publication::write_json(&bundle.join("record.json"), &record)?;
        let destination = self.options.directory.join(inputs.fingerprint()?);
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
    for (path, suffix) in artifact_files(artifact, source)? {
        let folder = path.parent().unwrap().file_name().unwrap();
        publication::copy_verified(&path, &destination.join(folder), suffix)?;
    }
    Ok(())
}

fn artifact_files(
    artifact: &PublishedArtifact,
    source: &Path,
) -> Result<Vec<(PathBuf, &'static str)>, String> {
    let mut files = vec![(
        source
            .join("receipts")
            .join(format!("{}.json", artifact.receipt_sha256)),
        ".json",
    )];
    let packages = rootbeer_package::runtime::closure(&artifact.package)?;
    for package in packages.into_iter().chain([&artifact.package]) {
        if let LockedSource::Url { url, sha256 } = &package.source {
            if !url.starts_with("ghcr://") {
                continue;
            }
            files.push((
                source.join("artifacts").join(format!("{sha256}.tar.gz")),
                ".tar.gz",
            ));
        }
    }
    Ok(files)
}

fn inputs(
    catalog: &PackageCatalog,
    key: &str,
    system: &str,
    registry: &str,
    engine: &str,
    context: &str,
) -> Result<Inputs, String> {
    let graph = rootbeer_package::graph::DependencyGraph::new(catalog, &[key.into()], system)?;
    let recipes = graph
        .order
        .iter()
        .map(|key| {
            let (_, _, recipe) = rootbeer_package::graph::find_recipe(catalog, key)?;
            Ok((key.clone(), recipe.clone()))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    Ok(Inputs {
        package: key.into(),
        engine: engine.into(),
        environment: context.into(),
        registry: registry.into(),
        system: system.into(),
        recipes,
    })
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
            inputs(
                catalog,
                key,
                "aarch64-linux",
                "owner/index",
                "engine",
                "image",
            )
            .unwrap()
            .fingerprint()
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
                inputs(&catalog, key, system, registry, engine, context)
                    .unwrap()
                    .fingerprint()
                    .unwrap()
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
        let options = ExportCache {
            directory: root.path().join("cache"),
            context: "test-image".into(),
            recheck: false,
        };
        let build_options = rootbeer_build::BuildOptions::default();
        let cache = Cache::new(&options, &build_options).unwrap();
        let inputs = cache.inputs(&catalog, key, &system, "owner/index").unwrap();
        let mut changed_inputs = inputs.clone();
        changed_inputs.environment = "environment-before-tool-change".into();
        assert!(cache
            .save(&catalog, &changed_inputs, artifact, &source)
            .is_err());
        assert!(!options.directory.exists());
        cache.save(&catalog, &inputs, artifact, &source).unwrap();
        let fingerprint = inputs.fingerprint().unwrap();
        let record_path = options.directory.join(&fingerprint).join("record.json");
        let record: serde_json::Value = publication::read_json(&record_path).unwrap();
        assert!(record.get("index").is_none());
        assert!(record.get("catalog").is_none());
        for field in ["revision", "receipt_sha256", "package", "schema", "inputs"] {
            let mut changed = record.clone();
            match field {
                "revision" => changed["artifact"][field] = 0.into(),
                "receipt_sha256" => changed["artifact"][field] = "invalid".into(),
                "package" => {
                    changed["artifact"][field]["provides"]["bins"]["xz"] = "../escape".into()
                }
                "schema" => changed[field] = 99.into(),
                "inputs" => changed[field]["environment"] = "different".into(),
                _ => unreachable!(),
            }
            publication::write_json(&record_path, &changed).unwrap();
            assert!(cache.restore(&inputs, &source).is_err(), "{field}");
            assert!(cache.inspect(&inputs).is_err(), "{field}");
        }
        publication::write_json(&record_path, &record).unwrap();
        let output = root.path().join("output");
        publication::create_bundle(&output).unwrap();
        let restored = cache.restore(&inputs, &output).unwrap().unwrap();
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
        assert!(cache.restore(&inputs, &failed).is_err());
        assert!(!failed.exists());
        cache.save(&catalog, &inputs, artifact, &source).unwrap();
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
        assert!(cache.restore(&inputs, &output).is_err());
        assert!(cache.inspect(&inputs).is_err());
        let refresh = ExportCache {
            recheck: true,
            ..options
        };
        let cache = Cache::new(&refresh, &build_options).unwrap();
        assert!(cache.restore(&inputs, &output).unwrap().is_none());
    }

    #[test]
    fn qualification_inputs_verify_tools_and_track_environment_changes() {
        let root = tempfile::tempdir().unwrap();
        let tool = root.path().join("tool");
        fs::copy("/bin/sh", &tool).unwrap();
        let lock = rootbeer_build::BuildEnvironment {
            tools: ["sh", "cc", "make", "patch"]
                .into_iter()
                .map(|name| (name.into(), tool.clone()))
                .collect(),
            ..Default::default()
        }
        .pin()
        .unwrap();
        let catalog = crate::test_catalog::catalog();
        let system = ResolveContext::current().system;
        let options = ExportCache {
            directory: root.path().join("cache"),
            context: "image".into(),
            recheck: false,
        };
        let mut build_options = rootbeer_build::BuildOptions {
            environment: Some(lock),
            ..Default::default()
        };
        let original = Cache::new(&options, &build_options)
            .unwrap()
            .inputs(catalog, "xz@5.8.3", &system, "owner/index")
            .unwrap();
        build_options.jobs = 8;
        assert_eq!(
            original,
            Cache::new(&options, &build_options)
                .unwrap()
                .inputs(catalog, "xz@5.8.3", &system, "owner/index")
                .unwrap()
        );
        build_options
            .environment
            .as_mut()
            .unwrap()
            .variables
            .insert("CFLAGS".into(), "-O1".into());
        assert_ne!(
            original,
            Cache::new(&options, &build_options)
                .unwrap()
                .inputs(catalog, "xz@5.8.3", &system, "owner/index")
                .unwrap()
        );
        build_options
            .environment
            .as_mut()
            .unwrap()
            .variables
            .clear();
        build_options.is_isolated = true;
        let isolated = Cache::new(&options, &build_options).unwrap().inputs(
            catalog,
            "xz@5.8.3",
            &system,
            "owner/index",
        );
        if let Ok(isolated) = isolated {
            assert_ne!(original, isolated);
        }
        build_options.is_isolated = false;
        fs::write(tool, "changed tool").unwrap();
        assert!(Cache::new(&options, &build_options)
            .unwrap()
            .inputs(catalog, "xz@5.8.3", &system, "owner/index")
            .is_err());
        assert!(!options.directory.exists());
    }

    #[test]
    fn cached_artifacts_retain_and_verify_the_entire_runtime_closure() {
        let root = tempfile::tempdir().unwrap();
        let (catalog, receipt) = crate::bundle::tests::fixture(root.path());
        let source = root.path().join("source");
        bundle_artifacts(&catalog, &[receipt], "ghcr://owner/index/xz", &source).unwrap();
        let index: ArtifactIndex = publication::read_json(&source.join("index.json")).unwrap();
        let mut artifact = index.artifacts["xz@5.8.3"].values().next().unwrap().clone();
        let mut dependency = artifact.package.clone();
        let mut hashes = Vec::new();
        for name in ["runtime-leaf", "runtime-parent"] {
            let tree = root.path().join(name);
            fs::create_dir(&tree).unwrap();
            fs::write(tree.join("resource"), name).unwrap();
            let archive = root.path().join(format!("{name}.tar.gz"));
            rootbeer_build::pack(&tree, &archive).unwrap();
            let sha256 = rootbeer_store::hash_file(&archive).unwrap();
            fs::copy(
                &archive,
                source.join("artifacts").join(format!("{sha256}.tar.gz")),
            )
            .unwrap();
            dependency.name = name.into();
            dependency.source = LockedSource::Url {
                url: format!("ghcr://owner/index/{name}@sha256:{sha256}"),
                sha256: sha256.clone(),
            };
            hashes.push(sha256);
            artifact.package.runtime_dependencies =
                BTreeMap::from([(dependency.id(), dependency.clone())]);
            dependency = artifact.package.clone();
        }
        let output = root.path().join("output");
        publication::create_bundle(&output).unwrap();
        copy_artifact(&artifact, &source, &output).unwrap();
        for sha256 in hashes {
            let path = PathBuf::from("artifacts").join(format!("{sha256}.tar.gz"));
            let bytes = fs::read(source.join(&path)).unwrap();
            assert_eq!(fs::read(output.join(&path)).unwrap(), bytes);
            fs::remove_file(source.join(&path)).unwrap();
            assert!(copy_artifact(&artifact, &source, &output).is_err());
            fs::write(source.join(&path), "corrupt").unwrap();
            assert!(copy_artifact(&artifact, &source, &output).is_err());
            fs::write(source.join(path), bytes).unwrap();
        }
    }
}
