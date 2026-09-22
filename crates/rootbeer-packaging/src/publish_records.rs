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
                let (Some(preferred), Some(retained)) = (
                    package.default_version_for(system),
                    old.default_version_for(system),
                ) else {
                    continue;
                };
                if !available(preferred) && available(retained) {
                    let retained = retained.to_string();
                    package.default_versions.insert(system.into(), retained);
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

    #[test]
    fn platform_changes_retain_only_unchanged_approvals() {
        let root = tempfile::tempdir().unwrap();
        let (catalog, _) = crate::bundle::tests::fixture(root.path());
        let package = catalog.packages.values().next().unwrap();
        let system = rootbeer_package::ResolveContext::current().system;
        let version = package.default_version_for(&system).unwrap().to_string();
        let id = format!("{}@{version}", package.name);
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
            .get_mut(&version)
            .unwrap();
        let mut variant = recipe.platforms[&system].clone();
        variant.checks.push(vec!["other-platform".into()]);
        recipe.platforms.insert(other.into(), variant);
        let original = &package.versions[&version];
        assert_eq!(
            rootbeer_package::distribution::input_key(
                &id,
                &system,
                original.revision,
                &original.platforms[&system],
                "engine",
                "environment"
            ),
            rootbeer_package::distribution::input_key(
                &id,
                &system,
                recipe.revision,
                &recipe.platforms[&system],
                "engine",
                "environment"
            )
        );
        assert!(can_retain(&changed, &catalog, &id, &system));
        assert!(!can_retain(&changed, &catalog, &id, other));
    }
}
