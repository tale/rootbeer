use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;

use super::{ArchiveFormat, LockedInstall, LockedPackage, LockedSource, Provides};
use crate::state_dir;
use crate::store::{hash_bytes, hash_file, hash_tree, Store, StoreEntry};

#[derive(Debug, Clone)]
pub struct PackageRealizer {
    store: Store,
    downloads: PathBuf,
    temp_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealizedPackage {
    pub package: LockedPackage,
    pub store_entry: StoreEntry,
    pub bins: BTreeMap<String, PathBuf>,
}

impl PackageRealizer {
    pub fn new(store: Store) -> Self {
        Self::with_dirs(
            store,
            state_dir().join("downloads"),
            state_dir().join("tmp/package"),
        )
    }

    pub fn with_dirs(
        store: Store,
        downloads: impl Into<PathBuf>,
        temp_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            store,
            downloads: downloads.into(),
            temp_dir: temp_dir.into(),
        }
    }

    pub fn default() -> Self {
        Self::new(Store::default())
    }

    pub fn realize(&self, package: &LockedPackage) -> io::Result<RealizedPackage> {
        let source = self.fetch_source(&package.source)?;
        let extracted;
        let install_source = match (&source, &package.install) {
            (SourceMaterial::Tree(path), LockedInstall::Directory { .. }) => path.as_path(),
            (SourceMaterial::File(path), LockedInstall::Archive { format, .. }) => {
                fs::create_dir_all(&self.temp_dir)?;
                extracted = tempfile::Builder::new()
                    .prefix("realize-")
                    .tempdir_in(&self.temp_dir)?;
                extract_archive(path, *format, extracted.path())?;
                extracted.path()
            }

            (SourceMaterial::Tree(_), LockedInstall::Archive { .. }) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "archive install requires a file or URL source",
                ));
            }

            (SourceMaterial::File(_), LockedInstall::Directory { .. }) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "directory install requires a path source",
                ));
            }
        };

        let install_root = install_root(install_source, &package.install)?;
        validate_provides(&install_root, &package.provides)?;

        let store_entry =
            self.store
                .add_tree(package.name.clone(), package.version.clone(), &install_root)?;
        if let Some(expected) = &package.output_sha256 {
            if store_entry.output_sha256 != *expected {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "package {} output hash mismatch: expected {}, got {}",
                        package.id(),
                        expected,
                        store_entry.output_sha256
                    ),
                ));
            }
        }

        let bins = package
            .provides
            .bins
            .iter()
            .map(|(name, rel)| (name.clone(), store_entry.path.join(rel)))
            .collect();

        Ok(RealizedPackage {
            package: package.clone(),
            store_entry,
            bins,
        })
    }

    fn fetch_source(&self, source: &LockedSource) -> io::Result<SourceMaterial> {
        match source {
            LockedSource::Path { path, sha256 } => {
                let actual = hash_tree(path)?;
                if actual != *sha256 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "source {} hash mismatch: expected {}, got {}",
                            path.display(),
                            sha256,
                            actual
                        ),
                    ));
                }

                Ok(SourceMaterial::Tree(path.clone()))
            }

            LockedSource::File { path, sha256 } => {
                verify_file_hash(path, sha256)?;
                Ok(SourceMaterial::File(path.clone()))
            }

            LockedSource::Url { url, sha256 } => {
                let path = self.fetch_url(url, sha256)?;
                Ok(SourceMaterial::File(path))
            }
        }
    }

    fn fetch_url(&self, url: &str, sha256: &str) -> io::Result<PathBuf> {
        fs::create_dir_all(&self.downloads)?;
        let cached = self.downloads.join(format!("sha256-{sha256}"));

        if cached.exists() {
            if hash_file(&cached)? == sha256 {
                return Ok(cached);
            }

            fs::remove_file(&cached)?;
        }

        let bytes = read_url(url)?;
        let actual = hash_bytes(&bytes);
        if actual != sha256 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("source {url} hash mismatch: expected {sha256}, got {actual}"),
            ));
        }

        let tmp = self.downloads.join(format!(".tmp-sha256-{sha256}"));
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, &cached)?;

        Ok(cached)
    }
}

#[derive(Debug)]
enum SourceMaterial {
    Tree(PathBuf),
    File(PathBuf),
}

fn install_root(source_root: &Path, install: &LockedInstall) -> io::Result<PathBuf> {
    match install {
        LockedInstall::Directory { strip_prefix } | LockedInstall::Archive { strip_prefix, .. } => {
            let root = match strip_prefix {
                Some(prefix) => {
                    validate_relative_path("strip_prefix", prefix)?;
                    source_root.join(prefix)
                }
                None => source_root.to_path_buf(),
            };

            if !root.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("install root {} is not a directory", root.display()),
                ));
            }

            Ok(root)
        }
    }
}

fn verify_file_hash(path: &Path, sha256: &str) -> io::Result<()> {
    let actual = hash_file(path)?;
    if actual != sha256 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "source {} hash mismatch: expected {}, got {}",
                path.display(),
                sha256,
                actual
            ),
        ));
    }

    Ok(())
}

fn read_url(url: &str) -> io::Result<Vec<u8>> {
    if let Some(path) = url.strip_prefix("file://") {
        return fs::read(path);
    }

    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported source URL `{url}`"),
        ));
    }

    ureq::get(url)
        .call()
        .map_err(|e| io::Error::other(format!("failed to fetch {url}: {e}")))?
        .into_body()
        .read_to_vec()
        .map_err(io::Error::other)
}

fn extract_archive(archive: &Path, format: ArchiveFormat, dest: &Path) -> io::Result<()> {
    match format {
        ArchiveFormat::TarGz => {
            let file = fs::File::open(archive)?;
            let decoder = GzDecoder::new(file);
            let mut archive = tar::Archive::new(decoder);

            for entry in archive.entries()? {
                let mut entry = entry?;
                if !entry.unpack_in(dest)? {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "archive entry escapes extraction directory",
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Contrary to seeming like this does something with dependency, this just
/// validates that the provided binaries exist in the install root and have
/// valid relative paths.
fn validate_provides(root: &Path, provides: &Provides) -> io::Result<()> {
    for (name, rel) in &provides.bins {
        if name.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "provided binary name cannot be empty",
            ));
        }

        validate_relative_path("provided binary path", rel)?;
        let path = root.join(rel);

        if !path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "provided binary `{name}` points to missing file {}",
                    path.display()
                ),
            ));
        }
    }

    Ok(())
}

fn validate_relative_path(label: &str, path: &Path) -> io::Result<()> {
    let mut has_component = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => has_component = true,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "{label} must be a relative path without `..`: {}",
                        path.display()
                    ),
                ));
            }
        }
    }

    if !has_component {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{label} cannot be empty"),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use flate2::write::GzEncoder;
    use flate2::Compression;

    use super::*;

    fn package(source: &Path, source_hash: String) -> LockedPackage {
        LockedPackage {
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
            source: LockedSource::Path {
                path: source.to_path_buf(),
                sha256: source_hash,
            },
            install: LockedInstall::Directory {
                strip_prefix: Some(PathBuf::from("pkg")),
            },
            provides: Provides {
                bins: BTreeMap::from([("demo".to_string(), PathBuf::from("bin/demo"))]),
            },
            output_sha256: None,
        }
    }

    fn source_tree() -> tempfile::TempDir {
        let source = tempfile::tempdir().unwrap();
        fs::create_dir_all(source.path().join("pkg/bin")).unwrap();
        fs::write(source.path().join("pkg/bin/demo"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(
            source.path().join("pkg/bin/demo"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        source
    }

    fn realizer(root: &Path) -> PackageRealizer {
        PackageRealizer::with_dirs(
            Store::new(root.join("store")),
            root.join("downloads"),
            root.join("tmp"),
        )
    }

    fn archive_source() -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let archive_path = root.path().join("demo.tar.gz");
        let file = fs::File::create(&archive_path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        let body = b"#!/bin/sh\n";
        header.set_size(body.len() as u64);
        header.set_mode(0o700);
        header.set_cksum();
        builder
            .append_data(&mut header, "pkg/bin/demo", &body[..])
            .unwrap();
        builder.finish().unwrap();
        builder.into_inner().unwrap().finish().unwrap();

        (root, archive_path)
    }

    fn archive_package(archive: &Path, source_hash: String) -> LockedPackage {
        LockedPackage {
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
            source: LockedSource::File {
                path: archive.to_path_buf(),
                sha256: source_hash,
            },
            install: LockedInstall::Archive {
                format: ArchiveFormat::TarGz,
                strip_prefix: Some(PathBuf::from("pkg")),
            },
            provides: Provides {
                bins: BTreeMap::from([("demo".to_string(), PathBuf::from("bin/demo"))]),
            },
            output_sha256: None,
        }
    }

    #[test]
    fn realizes_verified_path_source_into_store() {
        let store_root = tempfile::tempdir().unwrap();
        let source = source_tree();
        let package = package(source.path(), hash_tree(source.path()).unwrap());

        let realizer = realizer(store_root.path());
        let realized = realizer.realize(&package).unwrap();

        assert!(realized.store_entry.path.exists());
        assert!(realized.store_entry.path.join("bin/demo").is_file());
        assert_eq!(
            realized.bins.get("demo").unwrap(),
            &realized.store_entry.path.join("bin/demo")
        );
        assert_eq!(
            fs::metadata(realized.store_entry.path.join("bin/demo"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }

    #[test]
    fn rejects_source_hash_mismatch() {
        let store_root = tempfile::tempdir().unwrap();
        let source = source_tree();
        let package = package(source.path(), "bad-hash".to_string());

        let realizer = realizer(store_root.path());
        let err = realizer.realize(&package).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("hash mismatch"));
    }

    #[test]
    fn rejects_missing_provided_binary() {
        let store_root = tempfile::tempdir().unwrap();
        let source = source_tree();
        let mut package = package(source.path(), hash_tree(source.path()).unwrap());
        package
            .provides
            .bins
            .insert("missing".to_string(), PathBuf::from("bin/missing"));

        let realizer = realizer(store_root.path());
        let err = realizer.realize(&package).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(err.to_string().contains("provided binary `missing`"));
    }

    #[test]
    fn rejects_unsafe_relative_paths() {
        let store_root = tempfile::tempdir().unwrap();
        let source = source_tree();
        let mut package = package(source.path(), hash_tree(source.path()).unwrap());
        package.install = LockedInstall::Directory {
            strip_prefix: Some(PathBuf::from("../pkg")),
        };

        let realizer = realizer(store_root.path());
        let err = realizer.realize(&package).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("strip_prefix"));
    }

    #[test]
    fn realizes_verified_tar_gz_source_into_store() {
        let store_root = tempfile::tempdir().unwrap();
        let (_archive_root, archive) = archive_source();
        let package = archive_package(&archive, hash_file(&archive).unwrap());

        let realized = realizer(store_root.path()).realize(&package).unwrap();

        assert!(realized.store_entry.path.join("bin/demo").is_file());
        assert_eq!(
            fs::metadata(realized.store_entry.path.join("bin/demo"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }

    #[test]
    fn realizes_file_url_source_through_download_cache() {
        let store_root = tempfile::tempdir().unwrap();
        let (_archive_root, archive) = archive_source();
        let mut package = archive_package(&archive, hash_file(&archive).unwrap());
        package.source = LockedSource::Url {
            url: format!("file://{}", archive.display()),
            sha256: hash_file(&archive).unwrap(),
        };

        let realized = realizer(store_root.path()).realize(&package).unwrap();

        assert!(store_root
            .path()
            .join("downloads")
            .join(format!("sha256-{}", hash_file(&archive).unwrap()))
            .is_file());
        assert!(realized.store_entry.path.join("bin/demo").is_file());
    }

    #[test]
    fn rejects_archive_hash_mismatch() {
        let store_root = tempfile::tempdir().unwrap();
        let (_archive_root, archive) = archive_source();
        let package = archive_package(&archive, "bad-hash".to_string());

        let err = realizer(store_root.path()).realize(&package).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("hash mismatch"));
    }

    #[test]
    fn rejects_output_hash_mismatch() {
        let store_root = tempfile::tempdir().unwrap();
        let (_archive_root, archive) = archive_source();
        let mut package = archive_package(&archive, hash_file(&archive).unwrap());
        package.output_sha256 = Some("bad-output-hash".to_string());

        let err = realizer(store_root.path()).realize(&package).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("output hash mismatch"));
    }

    #[test]
    fn rejects_unsafe_archive_entries() {
        let store_root = tempfile::tempdir().unwrap();
        let archive_root = tempfile::tempdir().unwrap();
        let archive = archive_root.path().join("bad.tar.gz");
        let file = fs::File::create(&archive).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        let body = b"bad";
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.as_mut_bytes()[0..9].copy_from_slice(b"../escape");
        header.set_cksum();
        builder.append(&header, &body[..]).unwrap();
        builder.finish().unwrap();
        builder.into_inner().unwrap().finish().unwrap();

        let package = archive_package(&archive, hash_file(&archive).unwrap());
        let err = realizer(store_root.path()).realize(&package).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("escapes extraction directory"));
    }
}
