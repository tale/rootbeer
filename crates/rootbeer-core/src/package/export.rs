use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::time::Duration;

use super::download::DownloadCache;
use super::lockfile::{PackageLockEntry, RootbeerLock};
use super::*;
use crate::store::{hash_bytes, hash_file, Store};

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
    catalog.validate()?;
    super::ghcr::validate_repository(registry)?;
    if jobs == 0 || jobs > 64 {
        return Err("jobs must be between 1 and 64".into());
    }
    if let Some(shard) = shard {
        shard.validate()?;
    }
    let staging = publication::staging(output)?;
    let destination = staging.path().join("bundle");
    publication::create_bundle(&destination)?;
    let context = ResolveContext::current();
    let cache = cache_options.map(cache::Cache::new).transpose()?;
    let mut index = ArtifactIndex {
        schema: 2,
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
            if shard.is_some_and(|shard| !shard.contains(&key)) {
                continue;
            }
            let fingerprint = cache
                .as_ref()
                .map(|cache| cache.fingerprint(catalog, &key, &context.system, registry))
                .transpose()?;
            if let (Some(cache), Some(fingerprint)) = (&cache, &fingerprint) {
                if let Some(artifact) =
                    cache.restore(fingerprint, &key, &context.system, recipe, &destination)?
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
                let (_, artifact, receipt) = super::bundle::prepare_artifact(
                    catalog,
                    &build.join("receipt.json"),
                    &format!("ghcr://{registry}/{}", package.name),
                    &destination,
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
                    .resolve_package(&PackageRequest::parse(&key), &context)
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
                        &destination,
                        &downloads,
                    )?;
                }
                let mut receipt = serde_json::json!({"schema": 1, "catalog_sha256": catalog.sha256(), "revision": recipe.revision, "system": context.system, "package": locked, "proof": resolution.proof, "resolver_inputs": inputs});
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
                    recipe,
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
    if shard.is_some() {
        index.validate_fragment()?;
    } else {
        index.validate()?;
    }
    publication::write_json(&destination.join("index.json"), &index)?;
    fs::rename(destination, output).map_err(|e| e.to_string())
}

fn mirror_package(
    package: &mut LockedPackage,
    installed: &Path,
    registry: &str,
    destination: &Path,
    downloads: &Path,
) -> Result<(), String> {
    let archive = tempfile::NamedTempFile::new_in(destination).map_err(|e| e.to_string())?;
    super::build::pack(installed, archive.path()).map_err(|e| e.to_string())?;
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
                sha256: crate::store::hash_tree(&source).unwrap(),
            },
            install: LockedInstall::Directory { strip_prefix: None },
            provides: Provides {
                bins: BTreeMap::from([("app".into(), command.into())]),
            },
            output_sha256: None,
        };
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
