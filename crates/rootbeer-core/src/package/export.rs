use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::time::Duration;

use super::download::DownloadCache;
use super::lockfile::{PackageLockEntry, RootbeerLock};
use super::*;
use crate::store::{hash_bytes, Store};

mod cache;

pub use cache::ExportCache;

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
    catalog.validate()?;
    super::ghcr::validate_repository(registry)?;
    if jobs == 0 {
        return Err("jobs must be positive".into());
    }
    let staging = publication::staging(output)?;
    let destination = staging.path().join("bundle");
    publication::create_bundle(&destination)?;
    let context = ResolveContext::current();
    let cache = cache_options.map(cache::Cache::new).transpose()?;
    let mut index = ArtifactIndex {
        schema: 1,
        catalog: catalog.clone(),
        catalog_sha256: catalog.sha256(),
        artifacts: BTreeMap::new(),
    };
    let mut backend = None;
    for package in catalog.packages.values() {
        for (version, recipe) in &package.versions {
            if !recipe.systems.contains(&context.system) {
                continue;
            }
            let key = format!("{}@{version}", package.name);
            let fingerprint = cache
                .as_ref()
                .map(|cache| cache.fingerprint(catalog, &key, &context.system, registry))
                .transpose()?;
            if let (Some(cache), Some(fingerprint)) = (&cache, &fingerprint) {
                if let Some(artifact) =
                    cache.restore(fingerprint, &key, &context.system, &destination)?
                {
                    index
                        .artifacts
                        .entry(key.clone())
                        .or_default()
                        .insert(context.system.clone(), artifact);
                    eprintln!("REUSE {key} on {}", context.system);
                    continue;
                }
            }
            if backend.is_none() {
                let mut inputs = if catalog.packages.values().any(|package| {
                    package.versions.values().any(|recipe| {
                        recipe
                            .source
                            .as_deref()
                            .is_some_and(|source| source.starts_with("aqua:"))
                            && recipe.systems.contains(&context.system)
                    })
                }) {
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
                let mut resolver = super::backend_stack(&inputs).with_implicit_resolver("rootbeer");
                resolver.push(
                    super::catalog::CatalogResolver::new(&inputs, super::backend_stack(&inputs))
                        .with_catalog(catalog),
                );
                backend = Some((inputs, resolver));
            }
            let (inputs, resolver) = backend.as_ref().unwrap();
            let work = tempfile::tempdir_in(staging.path()).map_err(|e| e.to_string())?;
            let root = work.path();
            let downloads = root.join("downloads");
            let realizer = PackageRealizer::with_dirs(
                Store::new(root.join("store")),
                &downloads,
                root.join("install"),
            );
            let (mut artifact, receipt_bytes, proof) = if recipe.build.is_some() {
                let build = root.join("build");
                build_package(catalog, &key, &build, jobs)?;
                let bundle = root.join("source-bundle");
                bundle_artifacts(
                    catalog,
                    &[build.join("receipt.json")],
                    &format!("ghcr://{registry}/{}", package.name),
                    &bundle,
                )?;
                let mut built: ArtifactIndex = publication::read_json(&bundle.join("index.json"))?;
                let artifact = built
                    .artifacts
                    .get_mut(&key)
                    .unwrap()
                    .remove(&context.system)
                    .unwrap();
                let LockedSource::Url { sha256, .. } = &artifact.package.source else {
                    unreachable!()
                };
                let archive = bundle.join("artifacts").join(format!("{sha256}.tar.gz"));
                DownloadCache::new(&downloads)
                    .materialize(&format!("file://{}", archive.display()), Some(sha256))
                    .map_err(|e| e.to_string())?;
                publication::copy_verified(&archive, &destination.join("artifacts"), ".tar.gz")?;
                let receipt = fs::read(build.join("receipt.json")).map_err(|e| e.to_string())?;
                (artifact, receipt, None)
            } else {
                let resolution = resolver
                    .resolve_package(&PackageRequest::parse(&key), &context)
                    .map_err(|e| e.to_string())?;
                let mut locked = resolution.package;
                let realized = realizer.realize(&locked).map_err(|e| e.to_string())?;
                locked.output_sha256 = Some(realized.store_entry.output_sha256);
                let receipt = serde_json::to_vec(&serde_json::json!({"schema": 1, "catalog_sha256": catalog.sha256(), "revision": recipe.revision, "system": context.system, "package": locked, "proof": resolution.proof, "resolver_inputs": inputs})).map_err(|e| e.to_string())?;
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
            artifact.package.output_sha256 = Some(realized.store_entry.output_sha256.clone());
            let profile = root.join("profile");
            fs::create_dir(&profile).map_err(|e| e.to_string())?;
            for (name, path) in &realized.bins {
                symlink(path, profile.join(name)).map_err(|e| e.to_string())?;
            }
            let environment = BTreeMap::from([
                ("HOME", root.to_string_lossy().into_owned()),
                ("PATH", format!("{}:/usr/bin:/bin", profile.display())),
                ("LC_ALL", "C".into()),
                ("TMPDIR", root.to_string_lossy().into_owned()),
            ]);
            for check in &recipe.checks {
                let mut command = check.clone();
                command[0] = profile.join(&command[0]).to_string_lossy().into_owned();
                super::build::run(
                    &command,
                    root,
                    &environment,
                    &root.join("checks.log"),
                    Duration::from_secs(30),
                )?;
            }
            let entry = match proof {
                Some(proof) => PackageLockEntry::resolved(
                    &PackageRequest::parse(&key),
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
            if let (Some(cache), Some(fingerprint)) = (&cache, &fingerprint) {
                cache.save(
                    fingerprint,
                    &key,
                    &context.system,
                    catalog,
                    &artifact,
                    &destination,
                )?;
            }
            index
                .artifacts
                .entry(key.clone())
                .or_default()
                .insert(context.system.clone(), artifact);
            eprintln!("PASS {key} on {}", context.system);
        }
    }
    index.validate()?;
    publication::write_json(&destination.join("index.json"), &index)?;
    fs::rename(destination, output).map_err(|e| e.to_string())
}
