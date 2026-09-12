use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;

use serde::{de::DeserializeOwned, Serialize};

use super::{ArtifactIndex, LockedSource};
use crate::store::{hash_bytes, hash_file};

pub(super) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
        .map_err(|e| e.to_string())
}

pub(super) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    fs::write(path, serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

pub(super) fn staging(output: &Path) -> Result<tempfile::TempDir, String> {
    if output.try_exists().map_err(|e| e.to_string())? {
        return Err(format!("output already exists: {}", output.display()));
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    tempfile::tempdir_in(parent).map_err(|e| e.to_string())
}

pub(super) fn create_bundle(path: &Path) -> Result<(), String> {
    fs::create_dir(path).map_err(|e| e.to_string())?;
    for folder in ["artifacts", "receipts"] {
        fs::create_dir(path.join(folder)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(super) fn copy_verified(source: &Path, destination: &Path, suffix: &str) -> Result<(), String> {
    if !fs::symlink_metadata(source)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("bundle files must be regular files".into());
    }
    let digest = hash_file(source).map_err(|e| e.to_string())?;
    let name = format!("{digest}{suffix}");
    if source
        .file_name()
        .is_none_or(|actual| actual != name.as_str())
    {
        return Err(format!("bundle digest mismatch: {}", source.display()));
    }
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

fn check_files(index: &ArtifactIndex, bundle: &Path) -> Result<(), String> {
    for systems in index.artifacts.values() {
        for artifact in systems.values() {
            for (digest, folder, suffix) in
                std::iter::once((artifact.receipt_sha256.as_str(), "receipts", ".json")).chain(
                    match &artifact.package.source {
                        LockedSource::Url { url, sha256 } if url.starts_with("ghcr://") => {
                            Some((sha256.as_str(), "artifacts", ".tar.gz"))
                        }
                        _ => None,
                    },
                )
            {
                let file = bundle.join(folder).join(format!("{digest}{suffix}"));
                if !fs::symlink_metadata(&file)
                    .map_err(|e| e.to_string())?
                    .is_file()
                    || hash_file(file).map_err(|e| e.to_string())? != digest
                {
                    return Err("missing or invalid bundle content".into());
                }
            }
        }
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
        fragment.validate()?;
        check_files(&fragment, path.parent().unwrap())?;
        for (folder, suffix) in [("artifacts", ".tar.gz"), ("receipts", ".json")] {
            for file in
                fs::read_dir(path.parent().unwrap().join(folder)).map_err(|e| e.to_string())?
            {
                copy_verified(
                    &file.map_err(|e| e.to_string())?.path(),
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
    let index = combined.unwrap();
    index.validate_complete()?;
    write_json(&destination.join("index.json"), &index)?;
    fs::rename(destination, output).map_err(|e| e.to_string())
}

/// Coordinates for a signed Pages publication. ORAS supplies publisher authentication.
pub struct PublishOptions<'a> {
    pub bundle: &'a Path,
    pub site: &'a Path,
    pub site_url: &'a str,
    pub registry: &'a str,
    pub repository_url: &'a str,
    pub sequence: u64,
    pub key: &'a Path,
    pub public_key: &'a str,
}

/// Uploads source blobs, verifies anonymous reads, then installs the signed Pages snapshot.
/// Never executes package artifacts. Git commits and Pages deployment remain CI operations.
pub fn publish_index(opts: &PublishOptions<'_>) -> Result<(), String> {
    super::ghcr::validate_repository(opts.registry)?;
    super::index::validate_https(opts.site_url)?;
    super::index::validate_https(opts.repository_url)?;
    let bundle = opts.bundle.canonicalize().map_err(|e| e.to_string())?;
    let bytes = fs::read(bundle.join("index.json")).map_err(|e| e.to_string())?;
    let index: ArtifactIndex = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    index.validate_complete()?;
    check_files(&index, &bundle)?;
    let public = opts.site.join("public");
    let previous = match fs::read(public.join("latest.json")) {
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
            let LockedSource::Url { url, .. } = &artifact.package.source else {
                unreachable!()
            };
            if !url.starts_with("ghcr://") {
                continue;
            }
            let blob = super::ghcr::GhcrBlob::parse(url)?;
            if !blob.repository.starts_with(&format!("{}/", opts.registry)) {
                return Err("artifact is outside the publication namespace".into());
            }
            blobs.insert((blob.repository, blob.sha256));
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
        let mut reader = super::ghcr::GhcrBlob {
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
    latest
        .persist(public.join("latest.json"))
        .map_err(|e| e.to_string())?;
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
    fn assembles_complete_platforms_and_preserves_existing_output() {
        let root = tempfile::tempdir().unwrap();
        let bundle = complete(root.path());
        let index: ArtifactIndex = read_json(&bundle.join("index.json")).unwrap();
        index.validate_complete().unwrap();
        assert_eq!(index.artifacts.values().next().unwrap().len(), 2);
        let bytes = fs::read(bundle.join("index.json")).unwrap();
        assert!(assemble_indexes(&root.path().join("inputs"), &bundle)
            .unwrap_err()
            .contains("already exists"));
        assert_eq!(bytes, fs::read(bundle.join("index.json")).unwrap());
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
}
