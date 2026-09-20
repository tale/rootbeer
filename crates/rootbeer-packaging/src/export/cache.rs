use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{publication, CatalogRecipe, PackageCatalog, PublishedArtifact};
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
    pub(super) fn retain(&self, artifact: &PublishedArtifact, bundle: &Path) -> Result<(), String> {
        let record = Record {
            schema: 1,
            inputs: self.clone(),
            artifact: artifact.clone(),
        };
        let bytes = serde_json::to_vec(&record).map_err(|error| error.to_string())?;
        let directory = bundle.join("qualifications");
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        fs::write(
            directory.join(format!("{}.json", hash_bytes(&bytes))),
            bytes,
        )
        .map_err(|error| error.to_string())
    }

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
}

impl<'a> Cache<'a> {
    pub(super) fn new(
        options: &'a ExportCache,
        build_options: &'a rootbeer_build::BuildOptions,
    ) -> Result<Self, String> {
        if options.context.trim().is_empty() {
            return Err("export cache requires a build environment identity".into());
        }
        Ok(Self {
            options,
            build_options,
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
        let engine = engine_identity(catalog, key, system)?;
        inputs(catalog, key, system, registry, &engine, &environment)
    }

    fn load(&self, inputs: &Inputs) -> Result<Option<(PathBuf, Record)>, String> {
        if self.options.recheck {
            return Ok(None);
        }
        let mut expected = inputs.clone();
        let mut directory = self.options.directory.join(expected.fingerprint()?);
        if !directory.try_exists().map_err(|e| e.to_string())? {
            let Some(compatible) = compatible_inputs(inputs)? else {
                return Ok(None);
            };
            expected = compatible;
            directory = self.options.directory.join(expected.fingerprint()?);
            if !directory.try_exists().map_err(|e| e.to_string())? {
                return Ok(None);
            }
        }
        let record: Record = publication::read_json(&directory.join("record.json"))?;
        if record.schema != 1 || record.inputs != expected {
            return Err("qualification cache schema or inputs mismatch".into());
        }
        inputs.validate(&record.artifact)?;
        Ok(Some((directory, record)))
    }

    pub(super) fn inspect(
        &self,
        inputs: &Inputs,
    ) -> Result<Option<(String, PublishedArtifact)>, String> {
        let Some((directory, record)) = self.load(inputs)? else {
            return Ok(None);
        };
        for (path, suffix) in publication::artifact_files(&record.artifact, &directory)? {
            publication::verify_file(&path, suffix)?;
        }
        Ok(Some((record.inputs.fingerprint()?, record.artifact)))
    }

    pub(super) fn restore(
        &self,
        inputs: &Inputs,
        destination: &Path,
    ) -> Result<Option<PublishedArtifact>, String> {
        let Some((directory, record)) = self.load(inputs)? else {
            return Ok(None);
        };
        copy_artifact(&record.artifact, &directory, destination)?;
        record.inputs.retain(&record.artifact, destination)?;
        Ok(Some(record.artifact))
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

fn compatible_inputs(inputs: &Inputs) -> Result<Option<Inputs>, String> {
    let shared = env!("ROOTBEER_COMPATIBLE_ENGINE_IDENTITY");
    if shared.is_empty() {
        return Ok(None);
    }
    let implementations = inputs
        .recipes
        .values()
        .map(|recipe| {
            rootbeer_build::compatible_engine_identity(
                recipe.build.as_ref().map(|build| &build.backend),
            )
        })
        .collect::<Option<std::collections::BTreeSet<_>>>();
    let Some(implementations) = implementations else {
        return Ok(None);
    };
    let mut compatible = inputs.clone();
    compatible.engine = hash_bytes(
        &serde_json::to_vec(&("rootbeer-qualification-engine-v2", shared, implementations))
            .map_err(|error| error.to_string())?,
    );
    Ok(Some(compatible))
}

pub(crate) fn verify_qualifications(
    bundle: &Path,
    index: &crate::ArtifactIndex,
) -> Result<(), String> {
    qualifications(bundle, index).map(|_| ())
}

fn qualifications(bundle: &Path, index: &crate::ArtifactIndex) -> Result<Vec<Record>, String> {
    let directory = bundle.join("qualifications");
    if !fs::symlink_metadata(&directory)
        .map_err(|error| error.to_string())?
        .is_dir()
    {
        return Err("qualification records must be in a regular directory".into());
    }
    let mut records = Vec::new();
    let mut covered = std::collections::BTreeSet::new();
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        publication::verify_file(&path, ".json")?;
        let record: Record = publication::read_json(&path)?;
        let claimed = &record.inputs;
        if !rootbeer_package::index::is_sha256(&claimed.engine)
            || !rootbeer_package::index::is_sha256(&claimed.environment)
        {
            return Err("qualification requires engine and environment digests".into());
        }
        rootbeer_package::ghcr::validate_repository(&claimed.registry)?;
        let expected = inputs(
            &index.catalog,
            &claimed.package,
            &claimed.system,
            &claimed.registry,
            &claimed.engine,
            &claimed.environment,
        )?;
        if record.schema != 1 || claimed != &expected {
            return Err("qualification inputs do not match the bundle catalog".into());
        }
        claimed.validate(&record.artifact)?;
        let artifact = index
            .artifacts
            .get(&claimed.package)
            .and_then(|systems| systems.get(&claimed.system))
            .ok_or("qualification has no matching platform artifact")?;
        if serde_json::to_value(artifact).map_err(|error| error.to_string())?
            != serde_json::to_value(&record.artifact).map_err(|error| error.to_string())?
        {
            return Err("qualification differs from the bundled artifact".into());
        }
        if !covered.insert((claimed.package.clone(), claimed.system.clone())) {
            return Err("duplicate qualification for a platform artifact".into());
        }
        records.push(record);
    }
    if covered.len()
        != index
            .artifacts
            .values()
            .map(|systems| systems.len())
            .sum::<usize>()
    {
        return Err("bundle has incomplete qualification coverage".into());
    }
    Ok(records)
}

/// Files needed to import one platform from an authenticated candidate's metadata.
#[derive(Debug, Serialize)]
pub struct CandidateFiles {
    pub schema: u32,
    pub system: String,
    pub files: std::collections::BTreeSet<PathBuf>,
}

fn validate_system(system: &str) -> Result<(), String> {
    if !matches!(system, "aarch64-macos" | "aarch64-linux" | "x86_64-linux") {
        return Err(format!("unsupported candidate system: {system}"));
    }
    Ok(())
}

/// Plans platform receipt/archive transfer using only the index and all qualification records.
/// The caller must authenticate those metadata bytes before using the resulting paths.
pub fn candidate_files(bundle: &Path, system: &str) -> Result<CandidateFiles, String> {
    validate_system(system)?;
    let index: crate::ArtifactIndex = publication::read_json(&bundle.join("index.json"))?;
    index.validate_complete()?;
    let records = qualifications(bundle, &index)?;
    let mut files = std::collections::BTreeSet::new();
    for record in records
        .iter()
        .filter(|record| record.inputs.system == system)
    {
        for (path, _) in publication::artifact_files(&record.artifact, Path::new(""))? {
            files.insert(path);
        }
    }
    Ok(CandidateFiles {
        schema: 1,
        system: system.into(),
        files,
    })
}

/// Imports all qualifications from a candidate whose producer and catalog the caller approved.
/// Verifies contents without executing package code.
pub fn import_results(bundle: &Path, directory: &Path) -> Result<usize, String> {
    import_selected(bundle, directory, None)
}

/// Imports one platform's approved results, including its runtime archives.
/// Other platforms require complete metadata but their archive bytes need not be present.
pub fn import_results_for_system(
    bundle: &Path,
    directory: &Path,
    system: &str,
) -> Result<usize, String> {
    validate_system(system)?;
    import_selected(bundle, directory, Some(system))
}

fn import_selected(bundle: &Path, directory: &Path, system: Option<&str>) -> Result<usize, String> {
    let mut index: crate::ArtifactIndex = publication::read_json(&bundle.join("index.json"))?;
    index.validate_complete()?;
    let mut records = qualifications(bundle, &index)?;
    if let Some(system) = system {
        records.retain(|record| record.inputs.system == system);
        for systems in index.artifacts.values_mut() {
            systems.retain(|name, _| name == system);
        }
    }
    publication::check_files(&index, bundle)?;
    if directory.is_symlink() {
        return Err("result cache must not be a symlink".into());
    }
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    for record in &records {
        let destination = directory.join(record.inputs.fingerprint()?);
        if destination
            .try_exists()
            .map_err(|error| error.to_string())?
        {
            if !fs::symlink_metadata(&destination)
                .map_err(|error| error.to_string())?
                .is_dir()
            {
                return Err("result cache entry must be a regular directory".into());
            }
            let existing: Record = publication::read_json(&destination.join("record.json"))?;
            if existing.schema != 1 || existing.inputs != record.inputs {
                return Err("existing qualification inputs mismatch".into());
            }
            existing.inputs.validate(&existing.artifact)?;
            for (path, suffix) in publication::artifact_files(&existing.artifact, &destination)? {
                publication::verify_file(&path, suffix)?;
            }
            continue;
        }
        let staging = tempfile::tempdir_in(directory).map_err(|error| error.to_string())?;
        let entry = staging.path().join("entry");
        publication::create_bundle(&entry)?;
        copy_artifact(&record.artifact, bundle, &entry)?;
        publication::write_json(&entry.join("record.json"), record)?;
        fs::rename(entry, destination).map_err(|error| error.to_string())?;
    }
    Ok(records.len())
}

fn copy_artifact(
    artifact: &PublishedArtifact,
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    for (path, suffix) in publication::artifact_files(artifact, source)? {
        let folder = path.parent().unwrap().file_name().unwrap();
        publication::copy_verified(&path, &destination.join(folder), suffix)?;
    }
    Ok(())
}

fn engine_identity(catalog: &PackageCatalog, key: &str, system: &str) -> Result<String, String> {
    let graph = rootbeer_package::graph::DependencyGraph::new(catalog, &[key.into()], system)?;
    let implementations = graph
        .order
        .iter()
        .map(|key| {
            let (_, _, recipe) = rootbeer_package::graph::find_recipe(catalog, key)?;
            Ok(rootbeer_build::engine_identity(
                recipe.build.as_ref().map(|build| &build.backend),
            ))
        })
        .collect::<Result<std::collections::BTreeSet<_>, String>>()?;
    let engine = hash_bytes(
        &serde_json::to_vec(&(
            "rootbeer-qualification-engine-v2",
            env!("ROOTBEER_ENGINE_IDENTITY"),
            implementations,
        ))
        .map_err(|error| error.to_string())?,
    );
    Ok(engine)
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
    use crate::{bundle_artifacts, ArtifactIndex, LockedSource, ResolveContext};

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
    fn qualifications_include_dependency_backends_but_not_unrelated_backends() {
        let mut catalog = crate::test_catalog::catalog().clone();
        let original = engine_identity(&catalog, "xz@5.8.3", "aarch64-linux").unwrap();
        let mut rust = catalog.packages["xz"].versions["5.8.3"]
            .build
            .clone()
            .unwrap();
        rust.backend = crate::BuildBackend::Rust;
        rust.configure.clear();
        rust.args.clear();
        rust.libraries.clear();
        rust.rust = Some(crate::RustBuild {
            packages: vec!["fd".into()],
            ..Default::default()
        });
        let dependency = catalog
            .packages
            .get_mut("fd")
            .unwrap()
            .versions
            .get_mut("10.5.0")
            .unwrap();
        dependency.build = Some(rust);
        dependency.source = None;
        dependency.mirror = false;
        dependency.assets.clear();
        dependency.checksums.clear();
        dependency.bin_paths.clear();
        assert_eq!(
            original,
            engine_identity(&catalog, "xz@5.8.3", "aarch64-linux").unwrap()
        );
        catalog
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
        assert_ne!(
            original,
            engine_identity(&catalog, "xz@5.8.3", "aarch64-linux").unwrap()
        );
    }

    #[test]
    fn platform_import_requires_all_metadata_but_only_selected_contents() {
        let root = tempfile::tempdir().unwrap();
        let (catalog, receipt) = crate::bundle::tests::fixture(root.path());
        let bundle = root.path().join("candidate");
        bundle_artifacts(&catalog, &[receipt], "ghcr://owner/index/xz", &bundle).unwrap();
        let mut index: ArtifactIndex = publication::read_json(&bundle.join("index.json")).unwrap();
        index.catalog.packages.retain(|name, _| name == "xz");
        index
            .catalog
            .packages
            .get_mut("xz")
            .unwrap()
            .versions
            .get_mut("5.8.3")
            .unwrap()
            .systems = vec!["aarch64-linux".into(), "x86_64-linux".into()];
        index.catalog_sha256 = index.catalog.sha256();
        let first = index.artifacts["xz@5.8.3"]["aarch64-linux"].clone();
        let mut second = first.clone();
        let bytes = b"other platform receipt";
        second.receipt_sha256 = hash_bytes(bytes);
        fs::write(
            bundle
                .join("receipts")
                .join(format!("{}.json", second.receipt_sha256)),
            bytes,
        )
        .unwrap();
        let bytes = b"other platform archive";
        let digest = hash_bytes(bytes);
        fs::write(
            bundle.join("artifacts").join(format!("{digest}.tar.gz")),
            bytes,
        )
        .unwrap();
        second.package.source = LockedSource::Url {
            url: format!("ghcr://owner/index/xz@sha256:{digest}"),
            sha256: digest,
        };
        index
            .artifacts
            .get_mut("xz@5.8.3")
            .unwrap()
            .insert("x86_64-linux".into(), second.clone());
        publication::write_json(&bundle.join("index.json"), &index).unwrap();
        for (system, artifact) in [("aarch64-linux", &first), ("x86_64-linux", &second)] {
            inputs(
                &index.catalog,
                "xz@5.8.3",
                system,
                "owner/index",
                &"a".repeat(64),
                &"b".repeat(64),
            )
            .unwrap()
            .retain(artifact, &bundle)
            .unwrap();
        }
        crate::verify_candidate(&bundle, &index.catalog).unwrap();
        let plan = candidate_files(&bundle, "aarch64-linux").unwrap();
        assert_eq!(2, plan.files.len());
        for (path, _) in publication::artifact_files(&first, Path::new("")).unwrap() {
            assert!(plan.files.contains(&path));
        }
        for (path, _) in publication::artifact_files(&second, &bundle).unwrap() {
            fs::remove_file(path).unwrap();
        }
        assert_eq!(
            plan.files,
            candidate_files(&bundle, "aarch64-linux").unwrap().files
        );
        assert_eq!(
            1,
            import_results_for_system(&bundle, &root.path().join("cache"), "aarch64-linux")
                .unwrap()
        );
        assert!(import_results(&bundle, &root.path().join("all")).is_err());
        assert!(!root.path().join("all").exists());
        assert!(candidate_files(&bundle, "unknown-platform").is_err());
        let missing = plan
            .files
            .iter()
            .find(|path| path.starts_with("artifacts"))
            .unwrap();
        fs::remove_file(bundle.join(missing)).unwrap();
        assert!(
            import_results_for_system(&bundle, &root.path().join("missing"), "aarch64-linux")
                .is_err()
        );
        assert!(!root.path().join("missing").exists());
        let record = fs::read_dir(bundle.join("qualifications"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        fs::write(record, "corrupt metadata").unwrap();
        assert!(candidate_files(&bundle, "aarch64-linux").is_err());
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
        if let Some(predecessor) = compatible_inputs(&inputs).unwrap() {
            let mut original: Record = publication::read_json(&record_path).unwrap();
            original.inputs = predecessor.clone();
            let retained = options.directory.join(predecessor.fingerprint().unwrap());
            fs::rename(record_path.parent().unwrap(), &retained).unwrap();
            publication::write_json(&retained.join("record.json"), &original).unwrap();
            let output = root.path().join("predecessor");
            publication::create_bundle(&output).unwrap();
            cache.restore(&inputs, &output).unwrap().unwrap();
            let preserved = fs::read_dir(output.join("qualifications"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            let preserved: Record = publication::read_json(&preserved).unwrap();
            assert_eq!(preserved.inputs, predecessor);
            assert_eq!(preserved.artifact.receipt_sha256, artifact.receipt_sha256);
            assert_eq!(
                cache.inspect(&inputs).unwrap().unwrap().0,
                predecessor.fingerprint().unwrap()
            );
            let mut changed = inputs.clone();
            changed.environment.push_str("-changed");
            assert!(cache.inspect(&changed).unwrap().is_none());
            changed = inputs.clone();
            changed.recipes.get_mut(key).unwrap().revision += 1;
            assert!(cache.inspect(&changed).unwrap().is_none());
            changed = inputs.clone();
            changed
                .recipes
                .get_mut(key)
                .unwrap()
                .build
                .as_mut()
                .unwrap()
                .backend = crate::BuildBackend::Go;
            assert!(compatible_inputs(&changed).unwrap().is_none());
            let refresh = ExportCache {
                recheck: true,
                directory: options.directory.clone(),
                context: options.context.clone(),
            };
            assert!(Cache::new(&refresh, &build_options)
                .unwrap()
                .inspect(&inputs)
                .unwrap()
                .is_none());
            cache.save(&catalog, &inputs, artifact, &source).unwrap();
        }
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
