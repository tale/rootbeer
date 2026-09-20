use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::time::Duration;

use super::lockfile::{PackageLockEntry, RootbeerLock};
use super::*;
use rootbeer_package::download::DownloadCache;
use rootbeer_store::{hash_bytes, hash_file, Store};

mod cache;
use rootbeer_build::scheduler;

pub(crate) use cache::verify_qualifications;
pub use cache::{
    candidate_files, import_results, import_results_for_system, CandidateFiles, ExportCache,
};

/// Current-platform qualification decisions against a trusted local result cache.
#[derive(Debug, serde::Serialize)]
pub struct ExportPlan {
    pub schema: u32,
    pub catalog_sha256: String,
    pub system: String,
    pub packages: BTreeMap<String, ExportDecision>,
}

/// Reuses intact qualification evidence or schedules qualification for unmatched inputs.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ExportDecision {
    Reuse {
        qualification_sha256: String,
        receipt_sha256: String,
    },
    Qualify {
        qualification_sha256: Option<String>,
        reason: &'static str,
    },
}

/// Inspects local qualification evidence without downloads, builds, or package execution.
pub fn plan_export(
    catalog: &PackageCatalog,
    registry: &str,
    build_options: &BuildOptions,
    cache_options: Option<&ExportCache>,
    shard: Option<ExportShard>,
) -> Result<ExportPlan, String> {
    validate_export(catalog, registry, build_options, 1, shard)?;
    let system = ResolveContext::current().system;
    let cache = cache_options
        .map(|options| cache::Cache::new(options, build_options))
        .transpose()?;
    let mut packages = BTreeMap::new();
    for package in catalog.packages.values() {
        for (version, recipe) in &package.versions {
            let key = format!("{}@{version}", package.name);
            if !recipe.systems.contains(&system) || shard.is_some_and(|shard| !shard.contains(&key))
            {
                continue;
            }
            let Some(cache) = &cache else {
                packages.insert(
                    key,
                    ExportDecision::Qualify {
                        qualification_sha256: None,
                        reason: "cache_disabled",
                    },
                );
                continue;
            };
            let inputs = cache.inputs(catalog, &key, &system, registry)?;
            let qualification_sha256 = inputs.fingerprint()?;
            let decision = match cache.inspect(&inputs)? {
                Some((qualification_sha256, artifact)) => ExportDecision::Reuse {
                    qualification_sha256,
                    receipt_sha256: artifact.receipt_sha256,
                },
                None => ExportDecision::Qualify {
                    qualification_sha256: Some(qualification_sha256),
                    reason: if cache_options.is_some_and(|options| options.recheck) {
                        "explicit_recheck"
                    } else {
                        "no_matching_result"
                    },
                },
            };
            packages.insert(key, decision);
        }
    }
    Ok(ExportPlan {
        schema: 1,
        catalog_sha256: catalog.sha256(),
        system,
        packages,
    })
}

/// A zero-based partition of package versions, stable when unrelated recipes change.
#[derive(Clone, Copy, Debug)]
pub struct ExportShard {
    pub index: usize,
    pub count: usize,
}

impl ExportShard {
    fn validate(self) -> Result<(), String> {
        if self.count == 0 || self.index >= self.count {
            return Err("shards must be positive and shard must be less than shards".into());
        }
        Ok(())
    }

    fn contains(self, key: &str) -> bool {
        let digest = hash_bytes(key.as_bytes());
        let value = u64::from_str_radix(&digest[..16], 16).unwrap();
        value % self.count as u64 == self.index as u64
    }
}

fn validate_export(
    catalog: &PackageCatalog,
    registry: &str,
    build_options: &BuildOptions,
    workers: usize,
    shard: Option<ExportShard>,
) -> Result<(), String> {
    catalog.validate()?;
    rootbeer_package::ghcr::validate_repository(registry)?;
    if build_options.jobs == 0 || build_options.jobs > 64 {
        return Err("jobs must be between 1 and 64".into());
    }
    if workers == 0 || workers > 64 {
        return Err("workers must be between 1 and 64".into());
    }
    if let Some(shard) = shard {
        shard.validate()?;
    }
    if build_options.is_isolated && build_options.environment.is_none() {
        return Err("isolated export requires a pinned environment".into());
    }
    if build_options.is_isolated {
        rootbeer_build::Sandbox::identity()?;
    }
    if let Some(lock) = &build_options.environment {
        rootbeer_build::verify_environment(lock)?;
    }
    Ok(())
}

/// Builds or imports the current platform's recipes and tests their locked offline outputs.
/// Exports a platform bundle only after every applicable package passes.
pub fn export_catalog(
    catalog: &PackageCatalog,
    registry: &str,
    output: &Path,
    jobs: usize,
) -> Result<(), String> {
    export_catalog_with_cache(catalog, registry, output, jobs, None)
}

/// Exports a complete platform bundle, optionally reusing previously verified package results.
pub fn export_catalog_with_cache(
    catalog: &PackageCatalog,
    registry: &str,
    output: &Path,
    jobs: usize,
    cache_options: Option<&ExportCache>,
) -> Result<(), String> {
    export_catalog_shard(catalog, registry, output, jobs, cache_options, None)
}

/// Exports selected package versions while retaining the full catalog for assembly validation.
pub fn export_catalog_shard(
    catalog: &PackageCatalog,
    registry: &str,
    output: &Path,
    jobs: usize,
    cache_options: Option<&ExportCache>,
    shard: Option<ExportShard>,
) -> Result<(), String> {
    export_catalog_with_workers(
        catalog,
        registry,
        output,
        &BuildOptions {
            jobs,
            ..Default::default()
        },
        2,
        cache_options,
        shard,
    )
}

/// Qualifies dependency-ready recipes concurrently within the total build job budget.
pub fn export_catalog_with_workers(
    catalog: &PackageCatalog,
    registry: &str,
    output: &Path,
    build_options: &BuildOptions,
    workers: usize,
    cache_options: Option<&ExportCache>,
    shard: Option<ExportShard>,
) -> Result<(), String> {
    validate_export(catalog, registry, build_options, workers, shard)?;
    let staging = publication::staging(output)?;
    let destination = staging.path().join("bundle");
    publication::create_bundle(&destination)?;
    let context = ResolveContext::current();
    let cache = cache_options
        .map(|options| cache::Cache::new(options, build_options))
        .transpose()?;
    let mut index = ArtifactIndex {
        schema: ArtifactIndex::schema_for(catalog),
        catalog: catalog.clone(),
        catalog_sha256: catalog.sha256(),
        artifacts: BTreeMap::new(),
    };
    let mut tasks = Vec::new();
    let mut failures = BTreeMap::new();
    for package in catalog.packages.values() {
        for (version, recipe) in &package.versions {
            let key = format!("{}@{version}", package.name);
            if !recipe.systems.contains(&context.system)
                || shard.is_some_and(|shard| !shard.contains(&key))
            {
                continue;
            }
            let cached = (|| {
                let inputs = cache
                    .as_ref()
                    .map(|cache| cache.inputs(catalog, &key, &context.system, registry))
                    .transpose()?;
                let restored = match (&cache, &inputs) {
                    (Some(cache), Some(inputs)) => cache.restore(inputs, &destination)?,
                    _ => None,
                };
                Ok::<_, String>((inputs, restored))
            })();
            match cached {
                Ok((_, Some(artifact))) => {
                    index
                        .artifacts
                        .entry(key.clone())
                        .or_default()
                        .insert(context.system.clone(), artifact);
                    eprintln!("REUSE {key} on {}", context.system);
                }
                Ok((inputs, None)) => {
                    tasks.push((recipe.build.is_some(), (key, &package.name, recipe, inputs)))
                }
                Err(error) => {
                    failures.insert(key, error);
                }
            }
        }
    }
    if !tasks.is_empty() {
        let inputs = export_inputs(catalog, tasks.iter().map(|(_, (key, ..))| key.as_str()))?;
        let build_cache = rootbeer_build::BuildCache {
            directory: cache_options
                .map(|cache| cache.directory.join("builds"))
                .unwrap_or_else(|| staging.path().join("build-cache")),
            context: cache_options
                .map(|cache| cache.context.clone())
                .unwrap_or_else(|| "export-session".into()),
            recheck: cache_options.is_some_and(|cache| cache.recheck),
        };
        let session = rootbeer_build::BuildSession::default();
        let scheduled = tasks
            .iter()
            .map(|(is_source, (key, _, recipe, _))| scheduler::Task {
                key: key.clone(),
                dependencies: recipe
                    .build
                    .as_ref()
                    .map(|build| {
                        build
                            .dependencies
                            .iter()
                            .map(|dependency| dependency.package().to_string())
                            .collect()
                    })
                    .unwrap_or_default(),
                is_source: *is_source,
            })
            .collect();
        let metadata: BTreeMap<_, _> = tasks
            .into_iter()
            .map(|(_, (key, name, recipe, inputs))| (key, (name, recipe, inputs)))
            .collect();
        scheduler::run(
            scheduled,
            workers,
            build_options.jobs,
            &build_options.execution,
            |key, jobs| {
                build_options
                    .execution
                    .check()
                    .map_err(|error| error.to_string())?;
                let (name, recipe, _) = &metadata[key];
                let options = BuildOptions {
                    jobs,
                    downloads: cache_options
                        .map(|cache| cache.directory.join("builds/downloads"))
                        .unwrap_or_else(|| build_options.downloads.clone()),
                    session: Some(session.clone()),
                    ..build_options.clone()
                };
                let environment = ExportEnvironment {
                    catalog,
                    registry,
                    build_options: &options,
                    inputs: &inputs,
                    cache: Some(&build_cache),
                };
                let work = tempfile::tempdir_in(staging.path()).map_err(|e| e.to_string())?;
                let bundle = work.path().join("bundle");
                publication::create_bundle(&bundle)?;
                let artifact =
                    export_recipe(&environment, key, name, recipe, work.path(), &bundle)?;
                Ok((work, artifact))
            },
            |key, result| {
                let (_, _, inputs) = &metadata[&key];
                let result = result.and_then(|(work, artifact)| {
                    let bundle = work.path().join("bundle");
                    for (directory, extension) in [("receipts", ".json"), ("artifacts", ".tar.gz")]
                    {
                        for entry in
                            fs::read_dir(bundle.join(directory)).map_err(|e| e.to_string())?
                        {
                            publication::copy_verified(
                                &entry.map_err(|e| e.to_string())?.path(),
                                &destination.join(directory),
                                extension,
                            )?;
                        }
                    }
                    if let (Some(cache), Some(inputs)) = (&cache, &inputs) {
                        cache.save(catalog, inputs, &artifact, &bundle)?;
                        inputs.retain(&artifact, &destination)?;
                    }
                    Ok(artifact)
                });
                match result {
                    Ok(artifact) => {
                        index
                            .artifacts
                            .entry(key.clone())
                            .or_default()
                            .insert(context.system.clone(), artifact);
                        eprintln!("PASS {key} on {}", context.system);
                        true
                    }
                    Err(error) => {
                        eprintln!("FAIL {key} on {}: {error}", context.system);
                        failures.insert(key, error);
                        false
                    }
                }
            },
        )?;
    }
    build_options
        .execution
        .check()
        .map_err(|error| error.to_string())?;
    if !failures.is_empty() {
        return Err(format!(
            "{} package(s) failed:\n{}",
            failures.len(),
            failures
                .into_iter()
                .map(|(key, error)| format!("{key}: {error}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if shard.is_some() {
        index.validate_fragment()?;
    } else {
        index.validate()?;
    }
    publication::write_json(&destination.join("index.json"), &index)?;
    fs::rename(destination, output).map_err(|e| e.to_string())
}

fn export_inputs<'a>(
    catalog: &PackageCatalog,
    keys: impl Iterator<Item = &'a str>,
) -> Result<PackageResolverInputs, String> {
    let mut pending: Vec<String> = keys.map(str::to_owned).collect();
    let mut visited = std::collections::BTreeSet::new();
    let mut needs_aqua = false;
    while let Some(key) = pending.pop() {
        if !visited.insert(key.clone()) {
            continue;
        }
        let request = PackageRequest::parse(&key);
        let package = catalog
            .find(&request.name)
            .ok_or_else(|| format!("unknown package {key}"))?;
        let recipe = package
            .versions
            .get(
                request
                    .version
                    .as_deref()
                    .ok_or("export dependencies must use exact versions")?,
            )
            .ok_or_else(|| format!("unknown recipe {key}"))?;
        needs_aqua |= recipe.build.is_none()
            && recipe
                .source
                .as_deref()
                .is_some_and(|source| source.starts_with("aqua:"));
        if let Some(build) = &recipe.build {
            pending.extend(
                build
                    .dependencies
                    .iter()
                    .map(|dependency| dependency.package().to_string()),
            );
        }
    }
    let mut inputs = if needs_aqua {
        PackageResolverInputs::resolve_current().map_err(|e| e.to_string())?
    } else {
        PackageResolverInputs::default()
    };
    inputs.resolvers.insert(
        "rootbeer".into(),
        ResolverInput::Catalog {
            sha256: catalog.sha256(),
        },
    );
    Ok(inputs)
}

#[derive(Clone, Copy)]
struct ExportEnvironment<'a> {
    catalog: &'a PackageCatalog,
    registry: &'a str,
    build_options: &'a BuildOptions,
    inputs: &'a PackageResolverInputs,
    cache: Option<&'a rootbeer_build::BuildCache>,
}

fn export_recipe(
    environment: &ExportEnvironment<'_>,
    key: &str,
    package_name: &str,
    recipe: &CatalogRecipe,
    root: &Path,
    destination: &Path,
) -> Result<PublishedArtifact, String> {
    let ExportEnvironment {
        catalog,
        registry,
        build_options,
        inputs,
        cache,
    } = *environment;
    let context = ResolveContext::current();
    let mut resolver = rootbeer_package::backend_stack(inputs).with_implicit_resolver("rootbeer");
    resolver.push(rootbeer_package::catalog::CatalogResolver::new(
        catalog,
        inputs,
        rootbeer_package::backend_stack(inputs),
    ));
    let downloads = root.join("downloads");
    let realizer = PackageRealizer::with_dirs(
        Store::new(root.join("store")),
        &downloads,
        root.join("install"),
    )
    .with_execution(build_options.execution.clone());
    let (mut artifact, receipt_bytes, proof) = if recipe.build.is_some() {
        let build = root.join("build");
        rootbeer_build::BuildPlan::resolve(catalog, key, inputs)?.execute(
            &build,
            &rootbeer_build::BuildOptions {
                cache: cache.cloned(),
                ..build_options.clone()
            },
        )?;
        let (_, artifact, receipt) = super::bundle::prepare_artifact(
            catalog,
            &build.join("receipt.json"),
            &format!("ghcr://{registry}/{}", package_name),
            destination,
            &realizer,
        )?;
        let LockedSource::Url { sha256, .. } = &artifact.package.source else {
            unreachable!()
        };
        let archive = destination
            .join("artifacts")
            .join(format!("{sha256}.tar.gz"));
        DownloadCache::new(&downloads)
            .with_execution(build_options.execution.clone())
            .materialize(&format!("file://{}", archive.display()), Some(sha256))
            .map_err(|e| e.to_string())?;
        (artifact, receipt, None)
    } else {
        let resolution = resolver
            .resolve_package(&PackageRequest::parse(key), &context)
            .map_err(|e| e.to_string())?;
        let mut locked = resolution.package;
        let realized = realizer.realize(&locked).map_err(|e| e.to_string())?;
        locked.output_sha256 = Some(realized.store_entry.output_sha256);
        let upstream = recipe.mirror.then(|| locked.clone());
        if recipe.mirror {
            mirror_package(
                &mut locked,
                &realized.store_entry.path,
                registry,
                destination,
                &downloads,
            )?;
        }
        let mut receipt = serde_json::json!({"schema": 1, "catalog_sha256": catalog.sha256(), "revision": recipe.revision, "system": context.system, "package": locked, "proof": resolution.proof, "resolver_inputs": inputs});
        if build_options.is_isolated {
            receipt["isolation"] = rootbeer_build::Sandbox::identity()?.into();
            receipt["environment"] = serde_json::to_value(&build_options.environment)
                .map_err(|error| error.to_string())?;
        }
        if let Some(upstream) = upstream {
            receipt["upstream_package"] =
                serde_json::to_value(upstream).map_err(|e| e.to_string())?;
        }
        let receipt = serde_json::to_vec(&receipt).map_err(|e| e.to_string())?;
        let artifact = PublishedArtifact {
            revision: recipe.revision,
            receipt_sha256: hash_bytes(&receipt),
            package: locked,
        };
        (artifact, receipt, Some(resolution.proof))
    };
    let realized = realizer
        .realize(&artifact.package)
        .map_err(|e| e.to_string())?;
    if let Some(build) = &recipe.build {
        rootbeer_build::dependencies::validate(&realized.store_entry.path, &build.libraries)?;
    }
    artifact.package.output_sha256 = Some(realized.store_entry.output_sha256.clone());
    let profile = root.join("profile");
    fs::create_dir(&profile).map_err(|e| e.to_string())?;
    for (name, path) in &realized.bins {
        symlink(path, profile.join(name)).map_err(|e| e.to_string())?;
    }
    let check_workspace = tempfile::tempdir_in(root).map_err(|error| error.to_string())?;
    let environment = BTreeMap::from([
        (
            "HOME",
            check_workspace.path().to_string_lossy().into_owned(),
        ),
        (
            "PATH",
            if build_options.is_isolated {
                profile.display().to_string()
            } else {
                format!("{}:/usr/bin:/bin", profile.display())
            },
        ),
        ("LC_ALL", "C".into()),
        (
            "TMPDIR",
            check_workspace.path().to_string_lossy().into_owned(),
        ),
    ]);
    let mut runtime_roots = Vec::new();
    for dependency in rootbeer_package::runtime::closure(&artifact.package)? {
        runtime_roots.push(
            realizer
                .realize(dependency)
                .map_err(|e| e.to_string())?
                .store_entry
                .path,
        );
    }
    let sandbox = if build_options.is_isolated {
        Some(rootbeer_build::Sandbox::new(
            build_options
                .environment
                .as_ref()
                .ok_or("missing build environment")?,
            [profile.clone(), realized.store_entry.path.clone()]
                .into_iter()
                .chain(runtime_roots),
            check_workspace.path(),
        )?)
    } else {
        None
    };
    for check in &recipe.checks {
        let mut command = check.clone();
        command[0] = profile.join(&command[0]).to_string_lossy().into_owned();
        rootbeer_build::run_with_execution(
            &command,
            check_workspace.path(),
            &environment,
            &root.join("checks.log"),
            Duration::from_secs(30),
            sandbox.as_ref(),
            &build_options.execution,
        )?;
    }
    if let Some(environment) = &build_options.environment {
        rootbeer_build::verify_environment(environment)?;
    }
    let entry = match proof {
        Some(proof) => PackageLockEntry::resolved(
            &PackageRequest::parse(key),
            &context,
            PackageResolution::new(artifact.package.clone(), proof),
        )
        .map_err(|e| e.to_string())?,
        None => PackageLockEntry::locked(artifact.package.clone()),
    };
    let lock = RootbeerLock::from_package_entries([entry]).map_err(|e| e.to_string())?;
    let path = root.join("rootbeer.lock");
    lock.write(&path).map_err(|e| e.to_string())?;
    let locked_bytes = fs::read(&path).map_err(|e| e.to_string())?;
    fs::remove_dir_all(&profile).map_err(|e| e.to_string())?;
    fs::remove_dir_all(root.join("store")).map_err(|e| e.to_string())?;
    let replay = RootbeerLock::read(&path).map_err(|e| e.to_string())?;
    let offline = PackageRealizer::with_dirs_and_offline(
        Store::new(root.join("store")),
        &downloads,
        root.join("offline"),
        true,
    );
    let restored = offline
        .realize(replay.packages.values().next().unwrap())
        .map_err(|e| e.to_string())?;
    if let Some(build) = &recipe.build {
        rootbeer_build::dependencies::validate(&restored.store_entry.path, &build.libraries)?;
    }
    fs::create_dir(&profile).map_err(|e| e.to_string())?;
    for (name, path) in &restored.bins {
        symlink(path, profile.join(name)).map_err(|e| e.to_string())?;
    }
    if fs::read(&path).map_err(|e| e.to_string())? != locked_bytes
        || recipe.bins.iter().any(|bin| !profile.join(bin).is_file())
    {
        return Err(format!(
            "{key}: offline replay changed the lock or lost commands"
        ));
    }
    let receipt_sha256 = hash_bytes(&receipt_bytes);
    if receipt_sha256 != artifact.receipt_sha256 {
        return Err(format!("{key}: receipt digest mismatch"));
    }
    fs::write(
        destination
            .join("receipts")
            .join(format!("{receipt_sha256}.json")),
        receipt_bytes,
    )
    .map_err(|e| e.to_string())?;
    Ok(artifact)
}

fn mirror_package(
    package: &mut LockedPackage,
    installed: &Path,
    registry: &str,
    destination: &Path,
    downloads: &Path,
) -> Result<(), String> {
    let archive = tempfile::NamedTempFile::new_in(destination).map_err(|e| e.to_string())?;
    rootbeer_build::pack(installed, archive.path()).map_err(|e| e.to_string())?;
    let sha256 = hash_file(archive.path()).map_err(|e| e.to_string())?;
    let path = destination
        .join("artifacts")
        .join(format!("{sha256}.tar.gz"));
    fs::copy(archive.path(), &path).map_err(|e| e.to_string())?;
    DownloadCache::new(downloads)
        .materialize(&format!("file://{}", path.display()), Some(&sha256))
        .map_err(|e| e.to_string())?;
    package.source = LockedSource::Url {
        url: format!("ghcr://{registry}/{}@sha256:{sha256}", package.name),
        sha256,
    };
    package.install = LockedInstall::Archive {
        format: ArchiveFormat::TarGz,
        strip_prefix: None,
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_qualification_reuses_results_and_requalifies_changed_inputs() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(source.join("fixture")).unwrap();
        let counter = root.path().join("compilations");
        fs::write(
            source.join("fixture/configure"),
            format!(
                r#"#!/bin/sh
echo built >> '{}'
cat > Makefile <<'MAKE'
all:
	true
check:
	true
install:
	mkdir -p $(DESTDIR)/bin
	printf '#!/bin/sh\n[ "$$1" != "--fail" ]\n' > $(DESTDIR)/bin/xz
	chmod +x $(DESTDIR)/bin/xz
MAKE
"#,
                counter.display()
            ),
        )
        .unwrap();
        let archive = root.path().join("source.tar.gz");
        rootbeer_build::pack(&source, &archive).unwrap();
        let downloads = root.path().join("cache/builds/downloads");
        let downloaded = DownloadCache::new(&downloads)
            .materialize(&format!("file://{}", archive.display()), None)
            .unwrap();
        let mut catalog = crate::test_catalog::catalog().clone();
        catalog.packages.retain(|name, _| name == "xz");
        let package = catalog.packages.get_mut("xz").unwrap();
        let recipe = package.versions.get_mut(&package.default_version).unwrap();
        recipe.systems = vec![ResolveContext::current().system];
        recipe.bins = vec!["xz".into()];
        recipe.checks = vec![vec!["xz".into(), "--version".into()]];
        let build = recipe.build.as_mut().unwrap();
        build.url = "https://example.invalid/shared-source.tar.gz".into();
        build.sha256 = downloaded.sha256;
        build.strip_prefix = "fixture".into();
        build.configure.clear();
        build.dependencies.clear();
        let package = package.clone();
        for name in ["consumer-a", "consumer-b"] {
            let mut consumer = package.clone();
            consumer.name = name.into();
            consumer
                .versions
                .get_mut(&consumer.default_version)
                .unwrap()
                .build
                .as_mut()
                .unwrap()
                .dependencies = vec![format!("xz@{}", package.default_version).into()];
            catalog.packages.insert(name.into(), consumer);
        }
        let mut cache = ExportCache {
            directory: root.path().join("cache"),
            context: "shared-test".into(),
            recheck: false,
        };
        let options = BuildOptions {
            jobs: 3,
            downloads,
            ..Default::default()
        };
        let export = |catalog: &PackageCatalog, name, cache: Option<&ExportCache>, expected| {
            export_catalog_with_workers(
                catalog,
                "owner/index",
                &root.path().join(name),
                &options,
                2,
                cache,
                None,
            )
            .unwrap();
            assert_eq!(
                fs::read_to_string(&counter).unwrap().lines().count(),
                expected,
                "{name}"
            );
            publication::read_json::<ArtifactIndex>(&root.path().join(name).join("index.json"))
                .unwrap()
        };
        let missing = plan_export(&catalog, "owner/index", &options, Some(&cache), None).unwrap();
        assert!(missing.packages.values().all(|decision| matches!(
            decision,
            ExportDecision::Qualify {
                reason: "no_matching_result",
                ..
            }
        )));
        assert!(!counter.exists());
        let first = export(&catalog, "first", Some(&cache), 3);
        let candidate = root.path().join("first");
        crate::verify_candidate(&candidate, &catalog).unwrap();
        let imported = ExportCache {
            directory: root.path().join("imported"),
            context: cache.context.clone(),
            recheck: false,
        };
        assert_eq!(
            crate::import_results(&candidate, &imported.directory).unwrap(),
            3
        );
        assert_eq!(
            crate::import_results(&candidate, &imported.directory).unwrap(),
            3
        );
        let restored = export(&catalog, "imported-export", Some(&imported), 3);
        assert_eq!(
            serde_json::to_value(&first).unwrap(),
            serde_json::to_value(restored).unwrap()
        );
        assert!(!imported.directory.join("builds").exists());
        let record_path = fs::read_dir(candidate.join("qualifications"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let original = fs::read(&record_path).unwrap();
        let mut record: serde_json::Value = serde_json::from_slice(&original).unwrap();
        record["inputs"]["environment"] = "invalid".into();
        let bytes = serde_json::to_vec(&record).unwrap();
        fs::remove_file(&record_path).unwrap();
        let changed_path = candidate
            .join("qualifications")
            .join(format!("{}.json", hash_bytes(&bytes)));
        fs::write(&changed_path, bytes).unwrap();
        assert!(crate::verify_candidate(&candidate, &catalog).is_err());
        assert!(crate::import_results(&candidate, &root.path().join("rejected")).is_err());
        assert!(!root.path().join("rejected").exists());
        fs::remove_file(changed_path).unwrap();
        assert!(crate::verify_candidate(&candidate, &catalog).is_err());
        fs::write(&record_path, &original).unwrap();
        let mut duplicate: serde_json::Value = serde_json::from_slice(&original).unwrap();
        duplicate["inputs"]["environment"] = "0".repeat(64).into();
        let bytes = serde_json::to_vec(&duplicate).unwrap();
        let duplicate_path = candidate
            .join("qualifications")
            .join(format!("{}.json", hash_bytes(&bytes)));
        fs::write(&duplicate_path, bytes).unwrap();
        assert!(crate::verify_candidate(&candidate, &catalog)
            .unwrap_err()
            .contains("duplicate qualification"));
        fs::remove_file(duplicate_path).unwrap();
        for pointer in ["/schema", "/artifact/revision"] {
            let mut changed: serde_json::Value = serde_json::from_slice(&original).unwrap();
            *changed.pointer_mut(pointer).unwrap() = 999.into();
            let bytes = serde_json::to_vec(&changed).unwrap();
            let changed_path = candidate
                .join("qualifications")
                .join(format!("{}.json", hash_bytes(&bytes)));
            fs::remove_file(&record_path).unwrap();
            fs::write(&changed_path, bytes).unwrap();
            assert!(crate::verify_candidate(&candidate, &catalog).is_err());
            fs::remove_file(changed_path).unwrap();
            fs::write(&record_path, &original).unwrap();
        }
        let reusable = plan_export(&catalog, "owner/index", &options, Some(&cache), None).unwrap();
        assert!(reusable
            .packages
            .values()
            .all(|decision| matches!(decision, ExportDecision::Reuse { .. })));
        let repeated = export(&catalog, "repeated", Some(&cache), 3);
        assert_eq!(
            serde_json::to_value(&first).unwrap(),
            serde_json::to_value(&repeated).unwrap()
        );

        catalog
            .packages
            .get_mut("consumer-a")
            .unwrap()
            .versions
            .get_mut("5.8.3")
            .unwrap()
            .checks
            .push(vec!["xz".into(), "--help".into()]);
        let planned = plan_export(&catalog, "owner/index", &options, Some(&cache), None).unwrap();
        assert!(matches!(
            planned.packages["consumer-a@5.8.3"],
            ExportDecision::Qualify { .. }
        ));
        assert!(matches!(
            planned.packages["consumer-b@5.8.3"],
            ExportDecision::Reuse { .. }
        ));
        let changed = export(&catalog, "checks", Some(&cache), 3);
        let system = ResolveContext::current().system;
        assert_ne!(
            first.artifacts["consumer-a@5.8.3"][&system].receipt_sha256,
            changed.artifacts["consumer-a@5.8.3"][&system].receipt_sha256
        );
        for key in ["xz@5.8.3", "consumer-b@5.8.3"] {
            assert_eq!(
                first.artifacts[key][&system].receipt_sha256,
                changed.artifacts[key][&system].receipt_sha256
            );
        }
        export(&catalog, "checks-repeated", Some(&cache), 3);

        catalog
            .packages
            .get_mut("consumer-a")
            .unwrap()
            .versions
            .get_mut("5.8.3")
            .unwrap()
            .checks
            .push(vec!["xz".into(), "--fail".into()]);
        let failed_output = root.path().join("failed-checks");
        let error = export_catalog_with_workers(
            &catalog,
            "owner/index",
            &failed_output,
            &options,
            2,
            Some(&cache),
            None,
        )
        .unwrap_err();
        assert!(error.contains("consumer-a@5.8.3"), "{error}");
        assert!(!failed_output.exists());
        assert_eq!(fs::read_to_string(&counter).unwrap().lines().count(), 3);
        let planned = plan_export(&catalog, "owner/index", &options, Some(&cache), None).unwrap();
        assert!(matches!(
            planned.packages["consumer-a@5.8.3"],
            ExportDecision::Qualify { .. }
        ));
        catalog
            .packages
            .get_mut("consumer-a")
            .unwrap()
            .versions
            .get_mut("5.8.3")
            .unwrap()
            .checks
            .pop();

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
            .configure
            .push("--changed-input".into());
        let dependency_changed = export(&catalog, "dependency-changed", Some(&cache), 4);
        for key in ["xz@5.8.3", "consumer-a@5.8.3", "consumer-b@5.8.3"] {
            assert_ne!(
                changed.artifacts[key][&system].receipt_sha256,
                dependency_changed.artifacts[key][&system].receipt_sha256
            );
        }

        cache.recheck = true;
        let planned = plan_export(&catalog, "owner/index", &options, Some(&cache), None).unwrap();
        assert!(planned.packages.values().all(|decision| matches!(
            decision,
            ExportDecision::Qualify {
                reason: "explicit_recheck",
                ..
            }
        )));
        export(&catalog, "recheck", Some(&cache), 7);
        export(&catalog, "temporary", None, 10);
    }

    #[test]
    fn failed_recipes_do_not_discard_other_verified_results() {
        retain_completed_recipes(false);
    }

    #[test]
    fn cancelled_exports_reuse_completed_qualifications() {
        retain_completed_recipes(true);
    }

    fn retain_completed_recipes(should_cancel: bool) {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(source.join("fixture")).unwrap();
        fs::write(
            source.join("fixture/configure"),
            r#"#!/bin/sh
cat > Makefile <<'MAKE'
all:
	true
check:
	true
install:
	mkdir -p $(DESTDIR)/bin
	printf '#!/bin/sh\nif [ "$$1" = "--wait" ]; then /bin/sleep 30; fi\n[ "$$1" != "--fail" ]\n' > $(DESTDIR)/bin/xz
	chmod +x $(DESTDIR)/bin/xz
MAKE
"#,
        )
        .unwrap();
        let archive = root.path().join("source.tar.gz");
        rootbeer_build::pack(&source, &archive).unwrap();
        let downloaded = DownloadCache::new(root.path().join("cache/builds/downloads"))
            .materialize(&format!("file://{}", archive.display()), None)
            .unwrap();
        let mut catalog = crate::test_catalog::catalog().clone();
        catalog.packages.retain(|name, _| name == "xz");
        let package = catalog.packages.get_mut("xz").unwrap();
        let recipe = package.versions.get_mut(&package.default_version).unwrap();
        recipe.systems = vec![ResolveContext::current().system];
        recipe.bins = vec!["xz".into()];
        recipe.checks = vec![vec!["xz".into(), "--version".into()]];
        recipe.source = Some("github:fixture/should-not-fetch@1".into());
        recipe
            .assets
            .insert(ResolveContext::current().system, "upstream.tar.gz".into());
        recipe.mirror = true;
        recipe
            .checksums
            .insert(ResolveContext::current().system, "a".repeat(64));
        let build = recipe.build.as_mut().unwrap();
        build.url = "https://example.invalid/rootbeer-worker-test.tar.gz".into();
        build.sha256 = downloaded.sha256;
        build.strip_prefix = "fixture".into();
        build.configure.clear();
        build.dependencies.clear();
        let mut failed = package.clone();
        failed.name = if should_cancel { "z-wait" } else { "a-failure" }.into();
        failed
            .versions
            .get_mut(&failed.default_version)
            .unwrap()
            .checks = vec![vec![
            "xz".into(),
            if should_cancel { "--wait" } else { "--fail" }.into(),
        ]];
        catalog.packages.insert(failed.name.clone(), failed);
        let options = ExportCache {
            directory: root.path().join("cache"),
            context: "worker-test".into(),
            recheck: false,
        };
        let output = root.path().join("output");
        let execution = if should_cancel {
            Execution::with_timeout(Duration::from_secs(3)).unwrap()
        } else {
            Execution::default()
        };
        let error = export_catalog_with_workers(
            &catalog,
            "owner/index",
            &output,
            &BuildOptions {
                jobs: 1,
                execution,
                ..Default::default()
            },
            if should_cancel { 1 } else { 2 },
            Some(&options),
            None,
        )
        .unwrap_err();
        assert!(
            error.contains(if should_cancel {
                "deadline"
            } else {
                "a-failure@"
            }),
            "{error}"
        );
        assert!(!output.exists());
        assert_eq!(
            fs::read_dir(options.directory.join("builds/results"))
                .unwrap()
                .filter(|entry| entry
                    .as_ref()
                    .unwrap()
                    .path()
                    .join("receipt.json")
                    .is_file())
                .count(),
            1
        );
        let original_catalog_sha256 = catalog.sha256();
        let plan = plan_export(
            &catalog,
            "owner/index",
            &BuildOptions::default(),
            Some(&options),
            None,
        )
        .unwrap();
        let ExportDecision::Reuse { receipt_sha256, .. } = &plan.packages["xz@5.8.3"] else {
            panic!("completed qualification was lost");
        };
        if should_cancel {
            let package = catalog.packages.get_mut("z-wait").unwrap();
            package
                .versions
                .get_mut(&package.default_version)
                .unwrap()
                .checks = vec![vec!["xz".into(), "--version".into()]];
        } else {
            fs::remove_dir_all(options.directory.join("builds")).unwrap();
            catalog.packages.remove("a-failure");
        }
        export_catalog_with_workers(
            &catalog,
            "owner/index",
            &output,
            &BuildOptions {
                jobs: 1,
                ..Default::default()
            },
            2,
            Some(&options),
            None,
        )
        .unwrap();
        let index: ArtifactIndex = publication::read_json(&output.join("index.json")).unwrap();
        assert_eq!(index.artifacts.len(), if should_cancel { 2 } else { 1 });
        let published = &index.artifacts["xz@5.8.3"][&ResolveContext::current().system];
        assert_eq!(&published.receipt_sha256, receipt_sha256);
        assert!(
            matches!(&published.package.source, LockedSource::Url { url, .. } if url.starts_with("ghcr://owner/index/xz@sha256:"))
        );
        let receipt: rootbeer_package::BuildArtifact = publication::read_json(
            &output
                .join("receipts")
                .join(format!("{}.json", published.receipt_sha256)),
        )
        .unwrap();
        assert!(receipt.build_key.is_some());
        assert_eq!(receipt.catalog_sha256, original_catalog_sha256);
        assert_ne!(receipt.catalog_sha256, catalog.sha256());
        assert_eq!(options.directory.join("builds").exists(), should_cancel);
        assert_eq!(
            receipt.build.sha256,
            catalog.packages["xz"].versions["5.8.3"]
                .build
                .as_ref()
                .unwrap()
                .sha256
        );

        let mut shard_artifacts = BTreeMap::new();
        for shard in 0..8 {
            let shard_output = root.path().join(format!("shard-{shard}"));
            export_catalog_shard(
                &catalog,
                "owner/index",
                &shard_output,
                1,
                Some(&options),
                Some(ExportShard {
                    index: shard,
                    count: 8,
                }),
            )
            .unwrap();
            let fragment: ArtifactIndex =
                publication::read_json(&shard_output.join("index.json")).unwrap();
            assert_eq!(fragment.catalog_sha256, index.catalog_sha256);
            for (key, artifact) in fragment.artifacts {
                assert!(shard_artifacts.insert(key, artifact).is_none());
            }
        }
        assert_eq!(shard_artifacts.len(), index.artifacts.len());
        for (key, systems) in shard_artifacts {
            for (system, artifact) in systems {
                assert_eq!(artifact.package, index.artifacts[&key][&system].package);
                assert_eq!(artifact.revision, index.artifacts[&key][&system].revision);
            }
        }
    }

    #[test]
    fn mirrored_archives_replay_without_upstream_and_preserve_resources() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir_all(source.join("App.app/Contents/MacOS")).unwrap();
        fs::create_dir_all(source.join("App.app/Contents/Resources")).unwrap();
        let command = "App.app/Contents/MacOS/client";
        fs::write(source.join(command), "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(source.join(command), fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            source.join("App.app/Contents/Resources/data"),
            "runtime data",
        )
        .unwrap();
        symlink("Resources", source.join("App.app/Contents/current")).unwrap();
        let mut package = LockedPackage {
            name: "app".into(),
            version: "0.1.0-main+abcdef1".into(),
            source: LockedSource::Path {
                path: source.clone(),
                sha256: rootbeer_store::hash_tree(&source).unwrap(),
            },
            install: LockedInstall::Directory { strip_prefix: None },
            provides: Provides {
                apps: Default::default(),
                bins: BTreeMap::from([("app".into(), command.into())]),
            },
            runtime_dependencies: Default::default(),
            output_sha256: None,
        };
        if cfg!(target_os = "macos") {
            package
                .provides
                .apps
                .insert("App.app".into(), "App.app".into());
        }
        let downloads = root.path().join("downloads");
        let store = root.path().join("store");
        let realizer =
            PackageRealizer::with_dirs(Store::new(&store), &downloads, root.path().join("install"));
        let original = realizer.realize(&package).unwrap();
        package.output_sha256 = Some(original.store_entry.output_sha256.clone());
        let bundle = root.path().join("bundle");
        super::super::publication::create_bundle(&bundle).unwrap();
        mirror_package(
            &mut package,
            &original.store_entry.path,
            "owner/index",
            &bundle,
            &downloads,
        )
        .unwrap();
        let LockedSource::Url { url, sha256 } = &package.source else {
            panic!("expected mirror")
        };
        assert_eq!(url, &format!("ghcr://owner/index/app@sha256:{sha256}"));
        assert_eq!(
            hash_file(bundle.join("artifacts").join(format!("{sha256}.tar.gz"))).unwrap(),
            *sha256
        );
        let lock = RootbeerLock::from_package_entries([PackageLockEntry::locked(package.clone())])
            .unwrap();
        let lock_path = root.path().join("rootbeer.lock");
        lock.write(&lock_path).unwrap();
        let replay = RootbeerLock::read(&lock_path).unwrap();
        assert_eq!(
            replay.packages.values().next().unwrap().provides.apps,
            package.provides.apps
        );
        fs::remove_dir_all(source).unwrap();
        fs::remove_dir_all(&store).unwrap();
        let restored = PackageRealizer::with_dirs_and_offline(
            Store::new(&store),
            &downloads,
            root.path().join("offline"),
            true,
        )
        .realize(&package)
        .unwrap();
        assert_eq!(
            restored.store_entry.output_sha256,
            original.store_entry.output_sha256
        );
        assert_eq!(
            fs::read_to_string(
                restored
                    .store_entry
                    .path
                    .join("App.app/Contents/Resources/data")
            )
            .unwrap(),
            "runtime data"
        );
        assert_eq!(
            fs::read_link(restored.store_entry.path.join("App.app/Contents/current")).unwrap(),
            Path::new("Resources")
        );
        assert!(std::process::Command::new(&restored.bins["app"])
            .status()
            .unwrap()
            .success());
    }

    #[test]
    fn shards_cover_each_recipe_exactly_once() {
        let catalog = crate::test_catalog::catalog();
        for count in [1, 2, 8, 256] {
            for package in catalog.packages.values() {
                for version in package.versions.keys() {
                    let key = format!("{}@{version}", package.name);
                    let owners = (0..count)
                        .filter(|&index| ExportShard { index, count }.contains(&key))
                        .count();
                    assert_eq!(owners, 1, "{key} across {count} shards");
                }
            }
        }
    }

    #[test]
    fn rejects_invalid_shards_before_creating_output() {
        let catalog = crate::test_catalog::catalog();
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("output");
        for (index, count) in [(0, 0), (1, 1), (8, 8)] {
            let error = export_catalog_shard(
                catalog,
                "owner/index",
                &output,
                2,
                None,
                Some(ExportShard { index, count }),
            )
            .unwrap_err();
            assert!(error.contains("shard"));
            assert!(!output.exists());
        }
    }
}
