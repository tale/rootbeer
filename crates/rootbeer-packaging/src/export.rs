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

pub use cache::ExportCache;

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

/// Qualifies recipes concurrently, limiting source compilation to one package at a time.
pub fn export_catalog_with_workers(
    catalog: &PackageCatalog,
    registry: &str,
    output: &Path,
    build_options: &BuildOptions,
    workers: usize,
    cache_options: Option<&ExportCache>,
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
    let staging = publication::staging(output)?;
    let destination = staging.path().join("bundle");
    publication::create_bundle(&destination)?;
    let context = ResolveContext::current();
    let cache = cache_options.map(cache::Cache::new).transpose()?;
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
                let fingerprint = cache
                    .as_ref()
                    .filter(|_| recipe.build.is_none() && !build_options.is_isolated)
                    .map(|cache| cache.fingerprint(catalog, &key, &context.system, registry))
                    .transpose()?;
                let restored = match (&cache, &fingerprint) {
                    (Some(cache), Some(fingerprint)) => {
                        cache.restore(fingerprint, &key, &context.system, recipe, &destination)?
                    }
                    _ => None,
                };
                Ok::<_, String>((fingerprint, restored))
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
                Ok((fingerprint, None)) => tasks.push((
                    recipe.build.is_some(),
                    (key, &package.name, recipe, fingerprint),
                )),
                Err(error) => {
                    failures.insert(key, error);
                }
            }
        }
    }
    if !tasks.is_empty() {
        let inputs = export_inputs(catalog, tasks.iter().map(|(_, (key, ..))| key.as_str()))?;
        let build_cache = cache_options.map(|cache| rootbeer_build::BuildCache {
            directory: cache.directory.join("builds"),
            context: cache.context.clone(),
            recheck: cache.recheck,
        });
        let environment = ExportEnvironment {
            catalog,
            registry,
            build_options,
            inputs: &inputs,
            cache: build_cache.as_ref(),
        };
        run_workers(
            tasks,
            workers,
            |(key, name, recipe, fingerprint)| {
                let result = (|| {
                    let work = tempfile::tempdir_in(staging.path()).map_err(|e| e.to_string())?;
                    let bundle = work.path().join("bundle");
                    publication::create_bundle(&bundle)?;
                    let artifact =
                        export_recipe(&environment, &key, name, recipe, work.path(), &bundle)?;
                    Ok::<_, String>((work, artifact))
                })();
                (key, recipe, fingerprint, result)
            },
            |(key, recipe, fingerprint, result)| {
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
                    if let (Some(cache), Some(fingerprint)) = (&cache, &fingerprint) {
                        cache.save(
                            fingerprint,
                            &key,
                            &context.system,
                            recipe,
                            &artifact,
                            &bundle,
                        )?;
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
                    }
                    Err(error) => {
                        eprintln!("FAIL {key} on {}: {error}", context.system);
                        failures.insert(key, error);
                    }
                }
            },
        );
    }
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
        needs_aqua |= recipe
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

fn run_workers<T: Send, R: Send>(
    tasks: Vec<(bool, T)>,
    workers: usize,
    operation: impl Fn(T) -> R + Sync,
    mut complete: impl FnMut(R),
) {
    use std::sync::{mpsc, Condvar, Mutex};
    let queue = Mutex::new((std::collections::VecDeque::from(tasks), false));
    let ready = Condvar::new();
    let (sender, receiver) = mpsc::sync_channel(workers);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let (queue, ready, operation, sender) = (&queue, &ready, &operation, sender.clone());
            scope.spawn(move || loop {
                let (is_source, task) = {
                    let mut state = queue.lock().unwrap();
                    loop {
                        if state.0.is_empty() {
                            return;
                        }
                        if let Some(position) = state
                            .0
                            .iter()
                            .position(|(is_source, _)| !is_source || !state.1)
                        {
                            let task = state.0.remove(position).unwrap();
                            state.1 |= task.0;
                            break task;
                        }
                        state = ready.wait(state).unwrap();
                    }
                };
                let result = operation(task);
                if is_source {
                    queue.lock().unwrap().1 = false;
                    ready.notify_all();
                }
                if sender.send(result).is_err() {
                    return;
                }
            });
        }
        drop(sender);
        for result in receiver {
            complete(result);
        }
    });
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
    resolver.push(
        rootbeer_package::catalog::CatalogResolver::new(
            inputs,
            rootbeer_package::backend_stack(inputs),
        )
        .with_catalog(catalog),
    );
    let downloads = root.join("downloads");
    let realizer = PackageRealizer::with_dirs(
        Store::new(root.join("store")),
        &downloads,
        root.join("install"),
    );
    let (mut artifact, receipt_bytes, proof) = if recipe.build.is_some() {
        let build = root.join("build");
        rootbeer_build::BuildPlan::resolve(catalog, key, inputs)?.execute(
            &build,
            &rootbeer_build::BuildOptions {
                cache: cache.cloned(),
                downloads: cache
                    .map(|cache| cache.directory.join("downloads"))
                    .unwrap_or_else(|| rootbeer_store::state_dir().join("downloads")),
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
        rootbeer_build::run_with_sandbox(
            &command,
            check_workspace.path(),
            &environment,
            &root.join("checks.log"),
            Duration::from_secs(30),
            sandbox.as_ref(),
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
    fn workers_skip_waiting_sources_and_bound_parallelism() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc, Mutex,
        };
        let (sender, receiver) = mpsc::channel();
        let receiver = Mutex::new(receiver);
        let active = AtomicUsize::new(0);
        let source_active = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);
        let mut completed = Vec::new();
        run_workers(
            vec![(true, 0), (true, 1), (false, 2), (false, 3)],
            2,
            |task| {
                let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(count, Ordering::SeqCst);
                if task < 2 {
                    assert_eq!(source_active.fetch_add(1, Ordering::SeqCst), 0);
                }
                if task == 0 {
                    receiver
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                }
                if task == 2 {
                    sender.send(()).unwrap();
                }
                if task < 2 {
                    source_active.fetch_sub(1, Ordering::SeqCst);
                }
                active.fetch_sub(1, Ordering::SeqCst);
                task
            },
            |task| completed.push(task),
        );
        completed.sort();
        assert_eq!(completed, [0, 1, 2, 3]);
        assert_eq!(maximum.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn failed_recipes_do_not_discard_other_verified_results() {
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
	printf '#!/bin/sh\n[ "$$1" != "--fail" ]\n' > $(DESTDIR)/bin/xz
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
        let mut catalog = PackageCatalog::embedded().unwrap().clone();
        catalog.packages.retain(|name, _| name == "xz");
        let package = catalog.packages.get_mut("xz").unwrap();
        let recipe = package.versions.get_mut(&package.default_version).unwrap();
        recipe.systems = vec![ResolveContext::current().system];
        recipe.bins = vec!["xz".into()];
        recipe.checks = vec![vec!["xz".into(), "--version".into()]];
        let build = recipe.build.as_mut().unwrap();
        build.url = "https://example.invalid/rootbeer-worker-test.tar.gz".into();
        build.sha256 = downloaded.sha256;
        build.strip_prefix = "fixture".into();
        build.configure.clear();
        build.dependencies.clear();
        let mut failed = package.clone();
        failed.name = "a-failure".into();
        failed
            .versions
            .get_mut(&failed.default_version)
            .unwrap()
            .checks = vec![vec!["xz".into(), "--fail".into()]];
        catalog.packages.insert(failed.name.clone(), failed);
        let options = ExportCache {
            directory: root.path().join("cache"),
            context: "worker-test".into(),
            recheck: false,
        };
        let output = root.path().join("output");
        let error = export_catalog_with_workers(
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
        .unwrap_err();
        assert!(error.contains("a-failure@"), "{error}");
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
        catalog.packages.remove("a-failure");
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
        assert_eq!(index.artifacts.len(), 1);
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
        let catalog = PackageCatalog::embedded().unwrap();
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
        let catalog = PackageCatalog::embedded().unwrap();
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
