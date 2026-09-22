use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::{ArtifactIndex, LockedSource, PackageCatalog, PublishedArtifact};
use rootbeer_store::{hash_bytes, hash_file};

pub(super) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
        .map_err(|e| e.to_string())
}

pub(super) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    fs::write(path, serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

pub(crate) use rootbeer_package::staging::staging;

pub(super) fn create_bundle(path: &Path) -> Result<(), String> {
    fs::create_dir(path).map_err(|e| e.to_string())?;
    for folder in ["artifacts", "receipts", "qualifications"] {
        fs::create_dir(path.join(folder)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(super) fn verify_file(source: &Path, suffix: &str) -> Result<String, String> {
    if !fs::symlink_metadata(source)
        .map_err(|e| format!("{}: {e}", source.display()))?
        .is_file()
    {
        return Err(format!(
            "bundle files must be regular files: {}",
            source.display()
        ));
    }
    let digest = hash_file(source).map_err(|e| format!("{}: {e}", source.display()))?;
    let name = format!("{digest}{suffix}");
    if source
        .file_name()
        .is_none_or(|actual| actual != name.as_str())
    {
        return Err(format!("bundle digest mismatch: {}", source.display()));
    }
    Ok(digest)
}

pub(super) fn copy_verified(source: &Path, destination: &Path, suffix: &str) -> Result<(), String> {
    let digest = verify_file(source, suffix)?;
    let name = format!("{digest}{suffix}");
    let target = destination.join(name);
    match fs::symlink_metadata(&target) {
        Ok(metadata) if !metadata.is_file() => {
            return Err("bundle destination must be a regular file".into())
        }
        Ok(_) if hash_file(&target).map_err(|e| e.to_string())? != digest => {
            return Err("immutable bundle file collision".into())
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.to_string()),
        _ => {}
    }
    fs::copy(source, &target).map_err(|e| e.to_string())?;
    if hash_file(target).map_err(|e| e.to_string())? != digest {
        return Err("bundle file changed while copying".into());
    }
    Ok(())
}

pub(super) fn artifact_files(
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

pub(crate) fn check_files(index: &ArtifactIndex, bundle: &Path) -> Result<(), String> {
    for systems in index.artifacts.values() {
        for (system, artifact) in systems {
            for (path, suffix) in artifact_files(artifact, bundle)? {
                verify_file(&path, suffix)?;
            }
            let package = &artifact.package;
            let Some(recipe) =
                index.catalog.packages[&package.name].versions[&package.version].for_system(system)
            else {
                continue;
            };
            if recipe.build.is_none() && recipe.mirror {
                check_mirror_receipt(artifact, recipe, system, bundle)?;
            }
        }
    }
    Ok(())
}

/// Validates publication coverage and local bundle contents against the selected catalog.
/// The caller must establish the bundle's producer trust separately.
pub fn verify_bundle(bundle: &Path, catalog: &PackageCatalog) -> Result<(), String> {
    catalog.validate()?;
    let index: ArtifactIndex = read_json(&bundle.join("index.json"))?;
    index.validate_complete()?;
    if index.catalog_sha256 != catalog.sha256() {
        return Err("bundle does not match the selected catalog".into());
    }

    check_files(&index, bundle)
}

/// Validates a complete bundle and its per-package qualification evidence without execution.
/// Producer admission remains the caller's responsibility.
pub fn verify_candidate(bundle: &Path, catalog: &PackageCatalog) -> Result<(), String> {
    verify_bundle(bundle, catalog)?;
    let index: ArtifactIndex = read_json(&bundle.join("index.json"))?;
    crate::export::verify_qualifications(bundle, &index)
}

#[derive(Deserialize)]
struct MirrorReceipt {
    schema: u32,
    revision: u32,
    system: String,
    package: super::LockedPackage,
    upstream_package: super::LockedPackage,
}

fn check_mirror_receipt(
    artifact: &super::PublishedArtifact,
    recipe: &super::CatalogRecipe,
    system: &str,
    bundle: &Path,
) -> Result<(), String> {
    let path = bundle
        .join("receipts")
        .join(format!("{}.json", artifact.receipt_sha256));
    let receipt: MirrorReceipt = read_json(&path)
        .map_err(|error| format!("invalid mirror receipt {}: {error}", path.display()))?;
    let upstream = &receipt.upstream_package;
    let has_pinned_source = matches!(
        &upstream.source,
        LockedSource::Url { url, sha256 }
            if url.starts_with("https://") && recipe.sha256.as_ref() == Some(sha256)
    );
    if receipt.schema != 1
        || receipt.revision != artifact.revision
        || receipt.system != system
        || receipt.package != artifact.package
        || upstream.name != artifact.package.name
        || upstream.version != artifact.package.version
        || upstream.provides != artifact.package.provides
        || upstream.output_sha256 != artifact.package.output_sha256
        || !has_pinned_source
    {
        return Err(format!(
            "mirror receipt differs from the package contract: {}",
            path.display()
        ));
    }
    Ok(())
}

/// Combines platform bundles, validating content hashes and complete recipe coverage.
pub fn assemble_indexes(inputs: &Path, output: &Path) -> Result<(), String> {
    let mut paths = fs::read_dir(inputs)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|entry| entry.path().join("index.json")))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    paths.retain(|path| path.is_file());
    paths.sort();
    if paths.is_empty() {
        return Err("no platform indexes".into());
    }
    let staging = staging(output)?;
    let destination = staging.path().join("bundle");
    create_bundle(&destination)?;
    let mut combined: Option<ArtifactIndex> = None;
    for path in paths {
        let mut fragment: ArtifactIndex = read_json(&path)?;
        fragment.validate_fragment()?;
        check_files(&fragment, path.parent().unwrap())?;
        for (folder, suffix) in [
            ("artifacts", ".tar.gz"),
            ("receipts", ".json"),
            ("qualifications", ".json"),
        ] {
            let directory = path.parent().unwrap().join(folder);
            let files = match fs::read_dir(&directory) {
                Ok(files) => files,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(format!("{}: {error}", directory.display())),
            };
            for file in files {
                copy_verified(
                    &file
                        .map_err(|e| format!("{}: {e}", directory.display()))?
                        .path(),
                    &destination.join(folder),
                    suffix,
                )?;
            }
        }
        let Some(index) = &mut combined else {
            combined = Some(fragment);
            continue;
        };
        if index.catalog_sha256 != fragment.catalog_sha256 {
            return Err("platforms used different catalog snapshots".into());
        }
        index.schema = index.schema.max(fragment.schema);
        for (key, systems) in std::mem::take(&mut fragment.artifacts) {
            for (system, artifact) in systems {
                if index
                    .artifacts
                    .entry(key.clone())
                    .or_default()
                    .insert(system.clone(), artifact)
                    .is_some()
                {
                    return Err(format!("duplicate platform artifact: {key} {system}"));
                }
            }
        }
    }
    let mut index = combined.unwrap();
    index.schema = index.schema.max(7);
    index.validate_complete()?;
    write_json(&destination.join("index.json"), &index)?;
    fs::rename(destination, output).map_err(|e| e.to_string())
}

/// Coordinates for a signed Pages publication. ORAS supplies publisher authentication.
pub struct PublishOptions<'a> {
    pub bundle: &'a Path,
    pub site: &'a Path,
    pub site_url: &'a str,
    pub manifest_name: &'a str,
    pub registry: &'a str,
    pub repository_url: &'a str,
    pub sequence: u64,
    pub key: &'a Path,
    pub public_key: &'a str,
}

fn publication_manifest(public: &Path, name: &str) -> Result<PathBuf, String> {
    let path = Path::new(name);
    if path.file_name().is_none_or(|file| file != name)
        || path.extension().is_none_or(|extension| extension != "json")
        || name.as_bytes().contains(&0)
    {
        return Err("manifest must be a .json basename without directory components".into());
    }
    match fs::symlink_metadata(public) {
        Ok(metadata) if !metadata.is_dir() => {
            return Err("Pages output directory must not be a symlink or file".into());
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.to_string()),
        _ => {}
    }
    let destination = public.join(name);
    match fs::symlink_metadata(&destination) {
        Ok(metadata) if !metadata.is_file() => {
            return Err("manifest destination must be a regular file".into());
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.to_string()),
        _ => {}
    }
    Ok(destination)
}

/// Uploads source blobs, verifies anonymous reads, then installs the signed Pages snapshot.
/// Never executes package artifacts. Git commits and Pages deployment remain CI operations.
pub fn publish_index(opts: &PublishOptions<'_>) -> Result<(), String> {
    let public = opts.site.join("public");
    let manifest_path = publication_manifest(&public, opts.manifest_name)?;
    rootbeer_package::ghcr::validate_repository(opts.registry)?;
    rootbeer_package::index::validate_https(opts.site_url)?;
    rootbeer_package::index::validate_https(opts.repository_url)?;
    let bundle = opts.bundle.canonicalize().map_err(|e| e.to_string())?;
    let bytes = fs::read(bundle.join("index.json")).map_err(|e| e.to_string())?;
    let index: ArtifactIndex = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    index.validate_complete()?;
    check_files(&index, &bundle)?;
    let previous = match fs::read(&manifest_path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    let digest = hash_bytes(&bytes);
    let url = format!(
        "{}/snapshots/{digest}.json",
        opts.site_url.trim_end_matches('/')
    );
    let key = fs::read(opts.key).map_err(|e| e.to_string())?;
    let manifest = super::sign_index(
        &bytes,
        &url,
        opts.sequence,
        &key,
        opts.public_key,
        previous.as_deref(),
    )?;
    let mut blobs = BTreeSet::new();
    for systems in index.artifacts.values() {
        for artifact in systems.values() {
            for package in rootbeer_package::runtime::closure(&artifact.package)?
                .into_iter()
                .chain(std::iter::once(&artifact.package))
            {
                let LockedSource::Url { url, .. } = &package.source else {
                    unreachable!()
                };
                if !url.starts_with("ghcr://") {
                    continue;
                }
                let blob = rootbeer_package::ghcr::GhcrBlob::parse(url)?;
                if !blob.repository.starts_with(&format!("{}/", opts.registry)) {
                    return Err("artifact is outside the publication namespace".into());
                }
                blobs.insert((blob.repository, blob.sha256));
            }
        }
    }
    for (repository, digest) in blobs {
        let status = Command::new("oras")
            .args([
                "push",
                &format!("ghcr.io/{repository}:sha256-{digest}"),
                "--artifact-type",
                "application/vnd.rootbeer.package.v1",
                "--annotation",
                &format!("org.opencontainers.image.source={}", opts.repository_url),
                &format!("{digest}.tar.gz:application/gzip"),
            ])
            .env_remove("INDEX_SIGNING_KEY")
            .current_dir(bundle.join("artifacts"))
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("ORAS upload failed: {status}"));
        }
        let mut reader = rootbeer_package::ghcr::GhcrBlob {
            repository,
            sha256: digest.clone(),
        }
        .reader()
        .map_err(|e| e.to_string())?;
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        let mut buffer = [0; 8192];
        loop {
            let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        if format!("{:x}", hasher.finalize()) != digest {
            return Err("published GHCR blob hash mismatch".into());
        }
    }
    fs::create_dir_all(opts.site).map_err(|e| e.to_string())?;
    for directory in [&public, &public.join("snapshots"), &public.join("receipts")] {
        match fs::symlink_metadata(directory) {
            Ok(metadata) if !metadata.is_dir() => {
                return Err("Pages output directories must not be symlinks or files".into())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(directory).map_err(|e| e.to_string())?
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    let snapshot = public.join("snapshots").join(format!("{digest}.json"));
    match fs::symlink_metadata(&snapshot) {
        Ok(metadata) if !metadata.is_file() => {
            return Err("snapshot destination must be a regular file".into())
        }
        Ok(_) if fs::read(&snapshot).map_err(|e| e.to_string())? != bytes => {
            return Err("immutable snapshot collision".into())
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.to_string()),
        _ => {}
    }
    fs::write(snapshot, bytes).map_err(|e| e.to_string())?;
    for file in fs::read_dir(bundle.join("receipts")).map_err(|e| e.to_string())? {
        copy_verified(
            &file.map_err(|e| e.to_string())?.path(),
            &public.join("receipts"),
            ".json",
        )?;
    }
    let mut latest = tempfile::NamedTempFile::new_in(&public).map_err(|e| e.to_string())?;
    latest.write_all(&manifest).map_err(|e| e.to_string())?;
    latest.as_file().sync_all().map_err(|e| e.to_string())?;
    latest.persist(&manifest_path).map_err(|e| e.to_string())?;
    if !public
        .join(".nojekyll")
        .try_exists()
        .map_err(|e| e.to_string())?
    {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(public.join(".nojekyll"))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn fragment(root: &Path, system: &str) -> PathBuf {
        let directory = root.join(system);
        fs::create_dir_all(&directory).unwrap();
        let (catalog, receipt) = super::super::bundle::tests::fixture(&directory);
        let bundle = directory.join("result");
        super::super::bundle_artifacts(&catalog, &[receipt], "https://example.org", &bundle)
            .unwrap();
        let mut index: ArtifactIndex = read_json(&bundle.join("index.json")).unwrap();
        index.catalog.packages.retain(|name, _| name == "xz");
        for recipe in index
            .catalog
            .packages
            .get_mut("xz")
            .unwrap()
            .versions
            .values_mut()
        {
            recipe.systems = vec!["aarch64-linux".into(), "x86_64-linux".into()];
        }
        index.catalog_sha256 = index.catalog.sha256();
        let artifact = index
            .artifacts
            .values_mut()
            .next()
            .unwrap()
            .remove("aarch64-linux")
            .unwrap();
        *index.artifacts.values_mut().next().unwrap() = BTreeMap::from([(system.into(), artifact)]);
        write_json(&bundle.join("index.json"), &index).unwrap();
        bundle
    }

    fn complete(root: &Path) -> PathBuf {
        let inputs = root.join("inputs");
        fs::create_dir(&inputs).unwrap();
        for system in ["aarch64-linux", "x86_64-linux"] {
            let source = fragment(root, system);
            fs::rename(source, inputs.join(system)).unwrap();
        }
        let output = root.join("bundle");
        assemble_indexes(&inputs, &output).unwrap();
        output
    }

    #[test]
    fn verifies_bundle_against_selected_catalog_without_mutation() {
        let root = tempfile::tempdir().unwrap();
        let bundle = complete(root.path());
        let bytes = fs::read(bundle.join("index.json")).unwrap();
        let index: ArtifactIndex = serde_json::from_slice(&bytes).unwrap();
        verify_bundle(&bundle, &index.catalog).unwrap();

        let mut changed = index.catalog.clone();
        changed
            .packages
            .get_mut("xz")
            .unwrap()
            .description
            .push_str(" changed");
        let error = verify_bundle(&bundle, &changed).unwrap_err();
        assert!(
            error.contains("does not match the selected catalog"),
            "{error}"
        );
        assert_eq!(bytes, fs::read(bundle.join("index.json")).unwrap());
    }

    #[test]
    fn bundle_verification_requires_publication_coverage() {
        let root = tempfile::tempdir().unwrap();
        let bundle = fragment(root.path(), "aarch64-linux");
        let index: ArtifactIndex = read_json(&bundle.join("index.json")).unwrap();
        let error = verify_bundle(&bundle, &index.catalog).unwrap_err();
        assert!(error.contains("incomplete publication"), "{error}");
    }

    #[test]
    fn bundle_verification_rejects_missing_corrupt_and_symlinked_content() {
        for folder in ["artifacts", "receipts"] {
            for corruption in ["missing", "corrupt", "symlink"] {
                let root = tempfile::tempdir().unwrap();
                let bundle = complete(root.path());
                let mut index: ArtifactIndex = read_json(&bundle.join("index.json")).unwrap();
                for artifact in index
                    .artifacts
                    .values_mut()
                    .flat_map(|systems| systems.values_mut())
                {
                    let LockedSource::Url { url, sha256 } = &mut artifact.package.source else {
                        panic!("expected URL source");
                    };
                    *url = format!("ghcr://owner/index/xz@sha256:{sha256}");
                }
                write_json(&bundle.join("index.json"), &index).unwrap();
                verify_bundle(&bundle, &index.catalog).unwrap();

                let artifact = &index.artifacts.values().next().unwrap()["aarch64-linux"];
                let LockedSource::Url { sha256, .. } = &artifact.package.source else {
                    panic!("expected URL source");
                };
                let filename = if folder == "artifacts" {
                    format!("{sha256}.tar.gz")
                } else {
                    format!("{}.json", artifact.receipt_sha256)
                };
                let file = bundle.join(folder).join(filename);
                match corruption {
                    "missing" => fs::remove_file(&file).unwrap(),
                    "corrupt" => fs::write(&file, "tampered").unwrap(),
                    "symlink" => {
                        let original = root.path().join("original");
                        fs::rename(&file, &original).unwrap();
                        std::os::unix::fs::symlink(original, &file).unwrap();
                    }
                    _ => unreachable!(),
                }

                let error = verify_bundle(&bundle, &index.catalog).unwrap_err();
                assert!(
                    error.contains(&file.display().to_string()),
                    "{folder} {corruption}: {error}"
                );
            }
        }
    }

    #[test]
    fn mirror_receipts_bind_upstream_pins_and_published_packages_after_rehashing() {
        let root = tempfile::tempdir().unwrap();
        let bundle = fragment(root.path(), "aarch64-linux");
        let mut index: ArtifactIndex = read_json(&bundle.join("index.json")).unwrap();
        let artifact = index
            .artifacts
            .values_mut()
            .next()
            .unwrap()
            .get_mut("aarch64-linux")
            .unwrap();
        let upstream = artifact.package.clone();
        let LockedSource::Url { sha256, .. } = &upstream.source else {
            panic!("expected upstream URL");
        };
        artifact.package.source = LockedSource::Url {
            url: format!("ghcr://owner/index/xz@sha256:{sha256}"),
            sha256: sha256.clone(),
        };
        let package = index.catalog.packages.get_mut("xz").unwrap();
        let recipe = package.versions.get_mut(&package.default_version).unwrap();
        recipe.build = None;
        recipe.source = Some("github:owner/xz@v1".into());
        recipe.mirror = true;
        recipe.checksums = recipe
            .systems
            .iter()
            .map(|system| (system.clone(), sha256.clone()))
            .collect();
        index.catalog_sha256 = index.catalog.sha256();
        let original = serde_json::json!({
            "schema": 1,
            "revision": artifact.revision,
            "system": "aarch64-linux",
            "package": artifact.package,
            "upstream_package": upstream,
        });
        let bytes = serde_json::to_vec(&original).unwrap();
        artifact.receipt_sha256 = hash_bytes(&bytes);
        fs::write(
            bundle
                .join("receipts")
                .join(format!("{}.json", artifact.receipt_sha256)),
            bytes,
        )
        .unwrap();
        index.validate().unwrap();
        check_files(&index, &bundle).unwrap();

        for field in [
            "upstream_hash",
            "upstream_output",
            "package",
            "revision",
            "system",
            "missing_upstream",
        ] {
            let mut changed = original.clone();
            match field {
                "upstream_hash" => {
                    changed["upstream_package"]["source"]["Url"]["sha256"] = "b".repeat(64).into()
                }
                "upstream_output" => {
                    changed["upstream_package"]["output_sha256"] = "c".repeat(64).into()
                }
                "package" => changed["package"]["output_sha256"] = "d".repeat(64).into(),
                "revision" => changed["revision"] = 99.into(),
                "system" => changed["system"] = "x86_64-linux".into(),
                "missing_upstream" => {
                    changed.as_object_mut().unwrap().remove("upstream_package");
                }
                _ => unreachable!(),
            }
            let bytes = serde_json::to_vec(&changed).unwrap();
            let digest = hash_bytes(&bytes);
            fs::write(
                bundle.join("receipts").join(format!("{digest}.json")),
                bytes,
            )
            .unwrap();
            index
                .artifacts
                .values_mut()
                .next()
                .unwrap()
                .get_mut("aarch64-linux")
                .unwrap()
                .receipt_sha256 = digest;
            index.validate().unwrap();
            let error = check_files(&index, &bundle).unwrap_err();
            assert!(error.contains("mirror receipt"), "{field}: {error}");
        }
    }

    #[test]
    fn assembles_complete_platforms_and_preserves_existing_output() {
        let root = tempfile::tempdir().unwrap();
        let bundle = complete(root.path());
        let index: ArtifactIndex = read_json(&bundle.join("index.json")).unwrap();
        index.validate_complete().unwrap();
        assert_eq!(index.schema, 2);
        assert_eq!(index.artifacts.values().next().unwrap().len(), 2);
        let bytes = fs::read(bundle.join("index.json")).unwrap();
        assert!(assemble_indexes(&root.path().join("inputs"), &bundle)
            .unwrap_err()
            .contains("already exists"));
        assert_eq!(bytes, fs::read(bundle.join("index.json")).unwrap());
    }

    #[test]
    fn source_only_publication_preserves_schema_seven() {
        let root = tempfile::tempdir().unwrap();
        let inputs = root.path().join("inputs");
        fs::create_dir(&inputs).unwrap();
        let source = fragment(root.path(), "aarch64-linux");
        let mut index: ArtifactIndex = read_json(&source.join("index.json")).unwrap();
        index.schema = 7;
        index.artifacts.clear();
        write_json(&source.join("index.json"), &index).unwrap();
        fs::rename(source, inputs.join("source")).unwrap();
        let output = root.path().join("source-only");
        assemble_indexes(&inputs, &output).unwrap();
        let published: ArtifactIndex = read_json(&output.join("index.json")).unwrap();
        assert_eq!(published.schema, 7);
        assert!(published.artifacts.is_empty());
        published.validate_complete().unwrap();
        for package in index.catalog.packages.values_mut() {
            for recipe in package.versions.values_mut() {
                recipe.build = None;
                recipe.source = Some("github:owner/xz@v1".into());
            }
        }
        index.catalog_sha256 = index.catalog.sha256();
        assert!(index.validate_complete().is_err());
    }

    #[test]
    fn accepts_empty_shards_without_allowing_empty_publication() {
        let root = tempfile::tempdir().unwrap();
        let bundle = complete(root.path());
        let mut empty: ArtifactIndex = read_json(&bundle.join("index.json")).unwrap();
        empty.artifacts.clear();
        assert!(empty.validate().is_err());
        assert!(empty.validate_complete().is_err());

        let directory = root.path().join("inputs/empty");
        fs::create_dir(&directory).unwrap();
        write_json(&directory.join("index.json"), &empty).unwrap();
        assemble_indexes(
            &root.path().join("inputs"),
            &root.path().join("with-empty-shard"),
        )
        .unwrap();

        empty.catalog_sha256 = "0".repeat(64);
        write_json(&directory.join("index.json"), &empty).unwrap();
        assert!(assemble_indexes(
            &root.path().join("inputs"),
            &root.path().join("invalid-empty-shard"),
        )
        .is_err());
        assert!(!root.path().join("invalid-empty-shard").exists());
    }

    #[test]
    fn assembles_downloaded_shards_without_unreferenced_artifact_directories() {
        let root = tempfile::tempdir().unwrap();
        let bundle = complete(root.path());
        for system in ["aarch64-linux", "x86_64-linux"] {
            fs::remove_dir_all(root.path().join("inputs").join(system).join("artifacts")).unwrap();
        }
        let output = root.path().join("downloaded");
        assemble_indexes(&root.path().join("inputs"), &output).unwrap();
        assert_eq!(
            fs::read(bundle.join("index.json")).unwrap(),
            fs::read(output.join("index.json")).unwrap()
        );
        assert!(output.join("artifacts").is_dir());
    }

    #[test]
    fn rejects_downloaded_shards_missing_referenced_files() {
        for folder in ["artifacts", "receipts"] {
            let root = tempfile::tempdir().unwrap();
            complete(root.path());
            let shard = root.path().join("inputs/aarch64-linux");
            let mut index: ArtifactIndex = read_json(&shard.join("index.json")).unwrap();
            let artifact = index
                .artifacts
                .values_mut()
                .next()
                .unwrap()
                .values_mut()
                .next()
                .unwrap();
            let LockedSource::Url { url, sha256 } = &mut artifact.package.source else {
                panic!("expected URL source");
            };
            *url = format!("ghcr://owner/index/xz@sha256:{sha256}");
            let file = if folder == "artifacts" {
                format!("{sha256}.tar.gz")
            } else {
                format!("{}.json", artifact.receipt_sha256)
            };
            write_json(&shard.join("index.json"), &index).unwrap();
            fs::remove_dir_all(shard.join(folder)).unwrap();
            let output = root.path().join("missing");
            let error = assemble_indexes(&root.path().join("inputs"), &output).unwrap_err();
            assert!(
                error.contains(&shard.join(folder).join(file).display().to_string()),
                "{error}"
            );
            assert!(!output.exists());
        }
    }

    #[test]
    fn rejects_missing_platform_tampering_and_duplicate_platforms_atomically() {
        let root = tempfile::tempdir().unwrap();
        let inputs = root.path().join("inputs");
        fs::create_dir(&inputs).unwrap();
        fs::rename(fragment(root.path(), "aarch64-linux"), inputs.join("first")).unwrap();
        let output = root.path().join("output");
        assert!(assemble_indexes(&inputs, &output)
            .unwrap_err()
            .contains("incomplete"));
        assert!(!output.exists());
        let second = fragment(root.path(), "x86_64-linux");
        let first: ArtifactIndex = read_json(&inputs.join("first/index.json")).unwrap();
        write_json(&second.join("index.json"), &first).unwrap();
        for file in fs::read_dir(inputs.join("first/receipts")).unwrap() {
            let file = file.unwrap();
            fs::copy(file.path(), second.join("receipts").join(file.file_name())).unwrap();
        }
        fs::rename(second, inputs.join("second")).unwrap();
        assert!(assemble_indexes(&inputs, &output)
            .unwrap_err()
            .contains("duplicate"));
        let receipt = fs::read_dir(inputs.join("first/receipts"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        fs::write(receipt, "tampered").unwrap();
        assert!(assemble_indexes(&inputs, &output).is_err());
        assert!(!output.exists());
    }

    #[test]
    fn publisher_signs_retained_snapshots_and_rejects_bad_sequence_before_mutation() {
        let root = tempfile::tempdir().unwrap();
        let bundle = complete(root.path());
        let der = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let key_pair = Ed25519KeyPair::from_pkcs8(der.as_ref()).unwrap();
        let public_key: String = key_pair
            .public_key()
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let key = root.path().join("key.der");
        fs::write(&key, der.as_ref()).unwrap();
        let site = root.path().join("site");
        let mut opts = PublishOptions {
            bundle: &bundle,
            site: &site,
            site_url: "https://example.org",
            manifest_name: "latest.json",
            registry: "tale/rootbeer-index",
            repository_url: "https://github.com/tale/rootbeer-index",
            sequence: 1,
            key: &key,
            public_key: &public_key,
        };
        publish_index(&opts).unwrap();
        let first = fs::read(site.join("public/latest.json")).unwrap();
        assert!(publish_index(&opts).unwrap_err().contains("increase"));
        assert_eq!(first, fs::read(site.join("public/latest.json")).unwrap());
        opts.sequence = 2;
        publish_index(&opts).unwrap();
        assert_eq!(
            fs::read_dir(site.join("public/snapshots")).unwrap().count(),
            1
        );
        let latest: serde_json::Value = read_json(&site.join("public/latest.json")).unwrap();
        assert_eq!(latest["sequence"], 2);
        assert_eq!(
            latest["index"]["sha256"],
            hash_bytes(&fs::read(bundle.join("index.json")).unwrap())
        );
        let legacy = fs::read(site.join("public/latest.json")).unwrap();
        let legacy_snapshot = site.join("public/snapshots").join(format!(
            "{}.json",
            latest["index"]["sha256"].as_str().unwrap()
        ));
        let receipts = fs::read_dir(site.join("public/receipts"))
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                (path.clone(), fs::read(path).unwrap())
            })
            .collect::<Vec<_>>();
        let mut updated: ArtifactIndex = read_json(&bundle.join("index.json")).unwrap();
        updated
            .catalog
            .packages
            .get_mut("xz")
            .unwrap()
            .description
            .push_str(" (updated)");
        updated.catalog_sha256 = updated.catalog.sha256();
        write_json(&bundle.join("index.json"), &updated).unwrap();
        opts.manifest_name = "latest-v2.json";
        opts.sequence = 1;
        publish_index(&opts).unwrap();
        let versioned = fs::read(site.join("public/latest-v2.json")).unwrap();
        assert_eq!(legacy, fs::read(site.join("public/latest.json")).unwrap());
        assert_eq!(
            fs::read_dir(site.join("public/snapshots")).unwrap().count(),
            2
        );
        assert!(legacy_snapshot.is_file());
        for (path, contents) in receipts {
            assert_eq!(fs::read(path).unwrap(), contents);
        }
        assert!(publish_index(&opts).unwrap_err().contains("increase"));
        assert_eq!(
            versioned,
            fs::read(site.join("public/latest-v2.json")).unwrap()
        );
        opts.sequence = 2;
        publish_index(&opts).unwrap();
        assert_eq!(legacy, fs::read(site.join("public/latest.json")).unwrap());
        opts.manifest_name = "latest.json";
        assert!(publish_index(&opts).unwrap_err().contains("increase"));
        assert_eq!(legacy, fs::read(site.join("public/latest.json")).unwrap());
    }
    #[test]
    fn rejects_symlinks_in_bundle_content_and_destinations() {
        let root = tempfile::tempdir().unwrap();
        let digest = hash_bytes(b"receipt");
        let name = format!("{digest}.json");
        let source = root.path().join(&name);
        fs::write(&source, b"receipt").unwrap();
        let output = root.path().join("output");
        fs::create_dir(&output).unwrap();
        let outside = root.path().join("outside");
        std::os::unix::fs::symlink(&outside, output.join(&name)).unwrap();
        assert!(copy_verified(&source, &output, ".json").is_err());
        assert!(!outside.exists());
        fs::remove_file(output.join(&name)).unwrap();
        fs::remove_file(&source).unwrap();
        fs::write(&outside, b"receipt").unwrap();
        std::os::unix::fs::symlink(&outside, &source).unwrap();
        assert!(copy_verified(&source, &output, ".json").is_err());
    }

    #[test]
    fn manifest_names_reject_traversal_directories_and_symlinks() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let public = root.path().join("public");
        fs::create_dir(&public).unwrap();
        for name in [
            "",
            "latest",
            ".json",
            "../latest.json",
            "sub/latest.json",
            "/tmp/latest.json",
            "./latest.json",
            "latest.json/",
            "bad\0.json",
        ] {
            assert!(publication_manifest(&public, name).is_err(), "{name:?}");
        }
        assert_eq!(
            publication_manifest(&public, "latest-v2.json").unwrap(),
            public.join("latest-v2.json")
        );
        fs::create_dir(public.join("directory.json")).unwrap();
        assert!(publication_manifest(&public, "directory.json").is_err());
        let outside = root.path().join("outside.json");
        fs::write(&outside, b"legacy").unwrap();
        symlink(&outside, public.join("latest-v2.json")).unwrap();
        assert!(publication_manifest(&public, "latest-v2.json").is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"legacy");
        symlink(root.path().join("missing"), public.join("dangling.json")).unwrap();
        assert!(publication_manifest(&public, "dangling.json").is_err());
        symlink(&public, root.path().join("linked-public")).unwrap();
        assert!(publication_manifest(&root.path().join("linked-public"), "latest.json").is_err());
    }
}
