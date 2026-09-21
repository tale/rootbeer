use std::fs;
use std::path::Path;

use ring::signature::{Ed25519KeyPair, KeyPair};
use rootbeer_package::discovery::DiscoveryManifest;
use rootbeer_package::distribution::{
    read_record, verify_record, PackageRecord, SignedPackageRecord,
};
use rootbeer_package::{PackageCatalog, PackageIndexPin};
use rootbeer_store::hash_bytes;

/// Adds approved independent records to discovery without downloading or rebuilding package archives.
pub fn publish_records(
    catalog: &PackageCatalog,
    references: &[String],
    site: &Path,
    site_url: &str,
    key_der: &[u8],
    public_key: &str,
) -> Result<usize, String> {
    catalog.validate()?;
    rootbeer_package::index::validate_https(site_url)?;
    let key =
        Ed25519KeyPair::from_pkcs8(key_der).map_err(|_| "invalid Ed25519 PKCS#8 signing key")?;
    if key.public_key().as_ref() != rootbeer_package::official::decode_hex::<32>(public_key)? {
        return Err("signing key does not match discovery verification key".into());
    }
    let previous_path = site.join("current.json");
    let mut retained_records = Vec::new();
    let mut previous_catalog = None;
    let mut manifest = DiscoveryManifest {
        schema: 2,
        sequence: 1,
        catalog: catalog.clone(),
        records: Default::default(),
        signature: String::new(),
    };
    if previous_path.exists() {
        let bytes = fs::read(&previous_path).map_err(|error| error.to_string())?;
        if serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|error| error.to_string())?
            ["schema"]
            == 2
        {
            let previous = DiscoveryManifest::from_bytes(&bytes, public_key)?;
            previous_catalog = Some(previous.catalog.clone());
            manifest.sequence = previous
                .sequence
                .checked_add(1)
                .ok_or("discovery sequence overflow")?;
            for (id, mut platforms) in previous.records {
                platforms.retain(|system, _| can_retain(catalog, &previous.catalog, &id, system));
                if !platforms.is_empty() {
                    manifest.records.insert(id, platforms);
                }
            }
        } else {
            let previous: rootbeer_package::official::Manifest =
                serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            rootbeer_package::OfficialIndexSource {
                url: format!("{site_url}/current.json"),
                public_key: public_key.into(),
            }
            .verify(&previous)?;
            manifest.sequence = previous
                .sequence
                .checked_add(1)
                .ok_or("discovery sequence overflow")?;
            let downloads = tempfile::tempdir().map_err(|error| error.to_string())?;
            let snapshot = site
                .join("snapshots")
                .join(format!("{}.json", previous.index.sha256));
            let source = if snapshot.is_file() {
                format!(
                    "file://{}",
                    snapshot
                        .canonicalize()
                        .map_err(|error| error.to_string())?
                        .display()
                )
            } else {
                previous.index.url.clone()
            };
            let path = rootbeer_package::download::DownloadCache::new(downloads.path())
                .materialize_verified(&source, &previous.index.sha256)
                .map_err(|error| error.to_string())?;
            let index: rootbeer_package::ArtifactIndex =
                serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?;
            index.validate()?;
            previous_catalog = Some(index.catalog.clone());
            for (id, platforms) in &index.artifacts {
                for (system, artifact) in platforms {
                    if !can_retain(catalog, &index.catalog, id, system) {
                        continue;
                    }
                    let (_, _, approved) = rootbeer_package::graph::find_recipe_for_system(
                        &index.catalog,
                        id,
                        system,
                    )?;
                    let record = PackageRecord {
                        extra: Default::default(),
                        schema: 1,
                        system: system.clone(),
                        recipe: approved.clone(),
                        artifact: artifact.clone(),
                        provenance: rootbeer_package::distribution::PackageProvenance::Retained(
                            Box::new(rootbeer_package::distribution::RetainedProvenance {
                                approval: previous.clone(),
                                receipt: PackageIndexPin {
                                    url: format!(
                                        "{}/receipts/{}.json",
                                        site_url.trim_end_matches('/'),
                                        artifact.receipt_sha256
                                    ),
                                    sha256: artifact.receipt_sha256.clone(),
                                },
                            }),
                        ),
                    };
                    let bytes = crate::sign_package_record(&record, key_der, public_key)?;
                    let sha256 = hash_bytes(&bytes);
                    manifest.records.entry(id.clone()).or_default().insert(
                        system.clone(),
                        PackageIndexPin {
                            url: format!(
                                "{}/records/{sha256}.json",
                                site_url.trim_end_matches('/')
                            ),
                            sha256: sha256.clone(),
                        },
                    );
                    retained_records.push((sha256, bytes));
                }
            }
        }
    }
    let mut records = std::collections::BTreeMap::new();
    for reference in references {
        let bytes = read_record(reference)?;
        let signed: SignedPackageRecord =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let identity: PackageRecord =
            serde_json::from_str(signed.record.get()).map_err(|error| error.to_string())?;
        let record = verify_record(
            &bytes,
            public_key,
            &identity.artifact.package.id(),
            &identity.system,
        )?;
        let id = record.artifact.package.id();
        let (_, _, recipe) =
            rootbeer_package::graph::find_recipe_for_system(catalog, &id, &record.system)?;
        if recipe != record.recipe {
            return Err(format!(
                "{id}: signed record differs from the approved recipe"
            ));
        }
        let sha256 = hash_bytes(&bytes);
        let pin = PackageIndexPin {
            url: format!("{}/records/{sha256}.json", site_url.trim_end_matches('/')),
            sha256: sha256.clone(),
        };
        let identity = (id.clone(), record.system.clone());
        if records
            .get(&identity)
            .is_some_and(|(previous, _)| previous != &pin)
        {
            return Err(format!(
                "{id}: conflicting package records in one publication"
            ));
        }
        records.insert(identity, (pin, bytes));
    }
    if records.is_empty() && manifest.records.is_empty() {
        return Err("discovery requires at least one published package record".into());
    }
    for ((id, system), (pin, _)) in &records {
        manifest
            .records
            .entry(id.clone())
            .or_default()
            .insert(system.clone(), pin.clone());
    }
    if let Some(previous) = previous_catalog {
        for package in manifest.catalog.packages.values_mut() {
            let Some(old) = previous.packages.get(&package.name) else {
                continue;
            };
            for system in ["aarch64-macos", "aarch64-linux", "x86_64-linux"] {
                let available = |version: &str| {
                    manifest
                        .records
                        .get(&format!("{}@{version}", package.name))
                        .is_some_and(|platforms| platforms.contains_key(system))
                };
                let preferred = package.default_version_for(system);
                let retained = old.default_version_for(system);
                if !available(preferred) && available(retained) {
                    package
                        .default_versions
                        .insert(system.into(), retained.into());
                }
            }
        }
    }
    manifest.validate()?;
    manifest.signature = key
        .sign(&manifest.signing_message()?)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let bytes = serde_json::to_vec(&manifest).map_err(|error| error.to_string())?;
    DiscoveryManifest::from_bytes(&bytes, public_key)?;
    fs::create_dir_all(site.join("records")).map_err(|error| error.to_string())?;
    fs::create_dir_all(site.join("manifests")).map_err(|error| error.to_string())?;
    for (sha256, bytes) in retained_records {
        fs::write(site.join("records").join(format!("{sha256}.json")), bytes)
            .map_err(|error| error.to_string())?;
    }
    for (pin, record) in records.values() {
        fs::write(
            site.join("records").join(format!("{}.json", pin.sha256)),
            record,
        )
        .map_err(|error| error.to_string())?;
    }
    fs::write(
        site.join("manifests")
            .join(format!("{}.json", hash_bytes(&bytes))),
        &bytes,
    )
    .map_err(|error| error.to_string())?;
    let temporary = site.join("current.json.tmp");
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    fs::rename(temporary, previous_path).map_err(|error| error.to_string())?;
    Ok(manifest
        .records
        .values()
        .map(|platforms| platforms.len())
        .sum())
}

fn can_retain(current: &PackageCatalog, approved: &PackageCatalog, id: &str, system: &str) -> bool {
    let mut pending = vec![id.to_owned()];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id.clone()) {
            continue;
        }
        let Ok((_, _, previous)) =
            rootbeer_package::graph::find_recipe_for_system(approved, &id, system)
        else {
            return false;
        };
        if !rootbeer_package::graph::find_recipe_for_system(current, &id, system)
            .is_ok_and(|(_, _, recipe)| recipe == previous)
        {
            return false;
        }
        if let Some(build) = &previous.build {
            pending.extend(
                build
                    .dependencies
                    .iter()
                    .map(|dependency| dependency.package().to_owned()),
            );
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use rootbeer_package::discovery::{DiscoveryPin, DiscoveryResolver};
    use rootbeer_package::{PackageRequest, PackageResolver, ResolveContext};

    #[test]
    fn platform_changes_retain_only_unchanged_approvals() {
        let root = tempfile::tempdir().unwrap();
        let (catalog, _) = crate::bundle::tests::fixture(root.path());
        let package = catalog.packages.values().next().unwrap();
        let id = format!("{}@{}", package.name, package.default_version);
        let system = rootbeer_package::ResolveContext::current().system;
        let other = if system == "aarch64-macos" {
            "x86_64-linux"
        } else {
            "aarch64-macos"
        };
        let mut changed = catalog.clone();
        let recipe = changed
            .packages
            .get_mut(&package.name)
            .unwrap()
            .versions
            .get_mut(&package.default_version)
            .unwrap();
        let mut variant = recipe.clone();
        variant.systems = vec![other.into()];
        variant.revision += 1;
        recipe.platforms.insert(other.into(), variant);
        let original = &package.versions[&package.default_version];
        assert_eq!(
            rootbeer_package::distribution::input_key(
                &id,
                &system,
                original,
                "engine",
                "environment"
            ),
            rootbeer_package::distribution::input_key(
                &id,
                &system,
                recipe,
                "engine",
                "environment"
            )
        );
        assert!(can_retain(&changed, &catalog, &id, &system));
        assert!(!can_retain(&changed, &catalog, &id, other));
    }

    #[test]
    fn promotes_existing_approved_artifacts_without_builds_and_resolves_each_record_offline() {
        let root = tempfile::tempdir().unwrap();
        let (catalog, receipt) = crate::bundle::tests::fixture(root.path());
        let bundle = root.path().join("bundle");
        let digest =
            crate::bundle_artifacts(&catalog, &[receipt], "https://example.com", &bundle).unwrap();
        let bytes = fs::read(bundle.join("index.json")).unwrap();
        let index: rootbeer_package::ArtifactIndex = serde_json::from_slice(&bytes).unwrap();
        let key = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(key.as_ref()).unwrap();
        let public_key: String = pair
            .public_key()
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let pin = PackageIndexPin {
            url: format!("https://example.com/snapshots/{digest}.json"),
            sha256: digest.clone(),
        };
        let message = serde_json::to_vec(&("rootbeer-index-v1", 1, &pin.url, &pin.sha256)).unwrap();
        let approval = rootbeer_package::official::Manifest {
            schema: 1,
            sequence: 1,
            index: pin,
            signature: pair
                .sign(&message)
                .as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        };
        let site = root.path().join("site");
        fs::create_dir_all(site.join("snapshots")).unwrap();
        fs::write(
            site.join("snapshots").join(format!("{digest}.json")),
            &bytes,
        )
        .unwrap();
        fs::write(
            site.join("current.json"),
            serde_json::to_vec(&approval).unwrap(),
        )
        .unwrap();
        assert_eq!(
            publish_records(
                &catalog,
                &[],
                &site,
                "https://example.com",
                key.as_ref(),
                &public_key
            )
            .unwrap(),
            1
        );
        let bytes = fs::read(site.join("current.json")).unwrap();
        let manifest = DiscoveryManifest::from_bytes(&bytes, &public_key).unwrap();
        let cache = root.path().join("downloads");
        fs::create_dir(&cache).unwrap();
        let sha256 = hash_bytes(&bytes);
        fs::write(cache.join(format!("sha256-{sha256}")), bytes).unwrap();
        let (id, platforms) = manifest.records.first_key_value().unwrap();
        let (system, record) = platforms.first_key_value().unwrap();
        let record_bytes =
            fs::read(site.join("records").join(format!("{}.json", record.sha256))).unwrap();
        fs::write(
            cache.join(format!("sha256-{}", record.sha256)),
            record_bytes,
        )
        .unwrap();
        let resolver = DiscoveryResolver::with_cache(
            &DiscoveryPin {
                manifest: PackageIndexPin {
                    url: format!("https://example.com/manifests/{sha256}.json"),
                    sha256,
                },
                public_key: public_key.clone(),
            },
            &cache,
            true,
        );
        let resolved = resolver
            .resolve(&PackageRequest::parse(id), &ResolveContext::new(system))
            .unwrap()
            .unwrap();
        assert_eq!(resolved.package, index.artifacts[id][system].package);
        assert_eq!(
            publish_records(
                &catalog,
                &[],
                &site,
                "https://example.com",
                key.as_ref(),
                &public_key
            )
            .unwrap(),
            1
        );
        assert_eq!(
            DiscoveryManifest::from_bytes(
                &fs::read(site.join("current.json")).unwrap(),
                &public_key
            )
            .unwrap()
            .sequence,
            3
        );
        let mut pending = catalog.clone();
        let package = pending.packages.get_mut(&resolved.package.name).unwrap();
        package.versions.insert(
            "99.0".into(),
            package.versions[&resolved.package.version].clone(),
        );
        package.default_version = "99.0".into();
        publish_records(
            &pending,
            &[],
            &site,
            "https://example.com",
            key.as_ref(),
            &public_key,
        )
        .unwrap();
        let discovery = DiscoveryManifest::from_bytes(
            &fs::read(site.join("current.json")).unwrap(),
            &public_key,
        )
        .unwrap();
        assert_eq!(
            discovery.catalog.packages[&resolved.package.name].default_version_for(system),
            resolved.package.version
        );
        let mut changed = catalog.clone();
        let name = &resolved.package.name;
        changed
            .packages
            .get_mut(name)
            .unwrap()
            .versions
            .get_mut(&resolved.package.version)
            .unwrap()
            .checks
            .push(vec!["xz".into(), "changed".into()]);
        assert!(publish_records(
            &changed,
            &[],
            &site,
            "https://example.com",
            key.as_ref(),
            &public_key
        )
        .unwrap_err()
        .contains("at least one"));
        assert_eq!(fs::read_dir(site.join("records")).unwrap().count(), 1);
    }
}
