//! A content-addressed store for rootbeer packaging outputs. Each entry is a
//! directory containing the packaged files along with a manifest file with
//! metadata about the entry.
//!
//! In the name of determinism, the store normalizes file permissions to 755 for
//! executables then calculates the content hash based on the normalized tree.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::deterministic::DeterministicOutput;
pub mod deterministic;
pub mod layout;

/// Machine-wide root holding the shared store (`/opt/rootbeer`, or `ROOTBEER_ROOT`).
pub fn root_dir() -> PathBuf {
    std::env::var_os("ROOTBEER_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/opt/rootbeer"))
}

pub fn state_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(path).join("rootbeer");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/state/rootbeer");
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

const MANIFEST_DIR: &str = ".rootbeer";
const MANIFEST_FILE: &str = "manifest.json";

#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreEntry {
    pub path: PathBuf,
    pub name: String,
    pub version: String,
    pub output_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreManifest {
    pub schema: u32,
    pub name: String,
    pub version: String,
    pub output_sha256: String,
}

impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn add_tree(
        &self,
        name: impl Into<String>,
        version: impl Into<String>,
        src: impl AsRef<Path>,
    ) -> io::Result<StoreEntry> {
        let name = name.into();
        let version = version.into();
        let src = src.as_ref();
        let output_sha256 = hash_tree(src)?;
        let path = self.store_path(&output_sha256, &name, &version);

        if path.exists() {
            self.verify_entry(&path)?;
            return Ok(StoreEntry {
                path,
                name,
                version,
                output_sha256,
            });
        }

        fs::create_dir_all(&self.root)?;
        let tmp = self.temp_path(&name, &version);
        if tmp.exists() {
            fs::remove_dir_all(&tmp)?;
        }

        copy_normalized_tree(src, &tmp).map_err(|error| self.explain_denied(error))?;
        let manifest = StoreManifest {
            schema: 1,
            name: name.clone(),
            version: version.clone(),
            output_sha256: output_sha256.clone(),
        };

        write_manifest(&tmp, &manifest)?;
        match fs::rename(&tmp, &path) {
            Ok(()) => {}
            Err(_) if path.exists() => {
                // Another process may have created this exact store entry after
                // our initial existence check. Reuse it only if it verifies.
                fs::remove_dir_all(&tmp)?;
                self.verify_entry(&path)?;
            }

            Err(err) => return Err(err),
        }

        Ok(StoreEntry {
            path,
            name,
            version,
            output_sha256,
        })
    }

    pub fn verify_entry(&self, path: impl AsRef<Path>) -> io::Result<StoreManifest> {
        let path = path.as_ref();
        let manifest = read_manifest(path)?;
        let actual = hash_tree(path)?;
        if actual != manifest.output_sha256 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "store entry {} hash mismatch: expected {}, got {}",
                    path.display(),
                    manifest.output_sha256,
                    actual
                ),
            ));
        }

        let expected = self.store_path(&actual, &manifest.name, &manifest.version);
        if path.file_name() != expected.file_name() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "store entry identity/hash mismatch",
            ));
        }

        Ok(manifest)
    }

    pub fn store_path(&self, output_sha256: &str, name: &str, version: &str) -> PathBuf {
        self.root.join(format!(
            "sha256-{}-{}-{}",
            output_sha256,
            sanitize(name),
            sanitize(version)
        ))
    }

    fn explain_denied(&self, error: io::Error) -> io::Error {
        if error.kind() != io::ErrorKind::PermissionDenied {
            return error;
        }
        let Ok(metadata) = fs::metadata(&self.root) else {
            return error;
        };
        io::Error::new(
            error.kind(),
            format!(
                "{} is owned by uid {} and cannot be shared with other users yet",
                self.root.display(),
                metadata.uid()
            ),
        )
    }

    fn temp_path(&self, name: &str, version: &str) -> PathBuf {
        let pid = std::process::id();
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();

        self.root.join(format!(
            ".tmp-{}-{}-{pid}-{epoch}",
            sanitize(name),
            sanitize(version)
        ))
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new(root_dir().join("store"))
    }
}

impl DeterministicOutput for StoreEntry {
    const KIND: &'static str = "store.entry";

    fn output_sha256(&self) -> &str {
        &self.output_sha256
    }
}

impl DeterministicOutput for StoreManifest {
    const KIND: &'static str = "store.manifest";

    fn output_sha256(&self) -> &str {
        &self.output_sha256
    }
}

pub fn hash_tree(path: impl AsRef<Path>) -> io::Result<String> {
    let path = path.as_ref();
    let mut hasher = Sha256::new();

    let mut entries = Vec::new();
    collect_entries(path, path, &mut entries)?;
    entries.sort_by(|a, b| a.relative.cmp(&b.relative));

    for entry in entries {
        hasher.update(entry.kind.as_bytes());
        hasher.update([0]);

        hasher.update(entry.relative.as_bytes());
        hasher.update([0]);

        match entry.kind.as_str() {
            "dir" => {}
            "file" => {
                hasher.update(if entry.executable { b"755" } else { b"644" });
                hasher.update([0]);

                let mut file = fs::File::open(path.join(&entry.relative))?;
                let mut buf = [0u8; 8192];

                loop {
                    let n = file.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }

                    hasher.update(&buf[..n]);
                }
            }
            "symlink" => {
                let target = fs::read_link(path.join(&entry.relative))?;
                hasher.update(target.as_os_str().as_encoded_bytes());
            }
            _ => unreachable!(),
        }

        hasher.update([0xff]);
    }

    Ok(hex(hasher.finalize().as_slice()))
}

pub fn hash_file(path: impl AsRef<Path>) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }

        hasher.update(&buf[..n]);
    }

    Ok(hex(hasher.finalize().as_slice()))
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    hex(Sha256::digest(bytes).as_slice())
}

#[derive(Debug)]
struct TreeEntry {
    relative: String,
    kind: String,
    executable: bool,
}

fn collect_entries(root: &Path, path: &Path, entries: &mut Vec<TreeEntry>) -> io::Result<()> {
    let mut children = Vec::new();
    for child in fs::read_dir(path)? {
        let child = child?;
        if path == root && child.file_name() == OsStr::new(MANIFEST_DIR) {
            continue;
        }

        children.push(child.path());
    }

    // Without this, the hash was non-deterministic, oops!
    children.sort();

    for child in children {
        let meta = fs::symlink_metadata(&child)?;
        let relative = child
            .strip_prefix(root)
            .expect("child should be under root")
            .to_string_lossy()
            .replace('\\', "/");

        if meta.file_type().is_symlink() {
            entries.push(TreeEntry {
                relative,
                kind: "symlink".to_string(),
                executable: false,
            });
        } else if meta.is_dir() {
            entries.push(TreeEntry {
                relative: relative.clone(),
                kind: "dir".to_string(),
                executable: false,
            });
            collect_entries(root, &child, entries)?;
        } else if meta.is_file() {
            entries.push(TreeEntry {
                relative,
                kind: "file".to_string(),
                executable: meta.permissions().mode() & 0o111 != 0,
            });
        }
    }

    Ok(())
}

fn copy_normalized_tree(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    fs::set_permissions(dst, fs::Permissions::from_mode(0o755))?;

    for child in fs::read_dir(src)? {
        let child = child?;
        if child.file_name() == OsStr::new(MANIFEST_DIR) {
            continue;
        }

        let src_path = child.path();
        let dst_path = dst.join(child.file_name());
        let meta = fs::symlink_metadata(&src_path)?;

        // This is a pretty ugly block of code below
        if meta.file_type().is_symlink() {
            symlink(fs::read_link(&src_path)?, &dst_path)?;
        } else if meta.is_dir() {
            copy_normalized_tree(&src_path, &dst_path)?;
        } else if meta.is_file() {
            fs::copy(&src_path, &dst_path)?;
            let mode = if meta.permissions().mode() & 0o111 != 0 {
                0o755
            } else {
                0o644
            };

            fs::set_permissions(&dst_path, fs::Permissions::from_mode(mode))?;
        }
    }

    Ok(())
}

fn write_manifest(path: &Path, manifest: &StoreManifest) -> io::Result<()> {
    let dir = path.join(MANIFEST_DIR);
    fs::create_dir_all(&dir)?;

    let mut file = fs::File::create(dir.join(MANIFEST_FILE))?;
    let json = serde_json::to_string_pretty(manifest).map_err(io::Error::other)?;
    file.write_all(json.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

fn read_manifest(path: &Path) -> io::Result<StoreManifest> {
    let bytes = fs::read(path.join(MANIFEST_DIR).join(MANIFEST_FILE))?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '-' | '+' => c,
            _ => '-',
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    const CHARS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        out.push(CHARS[(byte >> 4) as usize] as char);
        out.push(CHARS[(byte & 0x0f) as usize] as char);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwritable_store_names_its_owner() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("bin"), "hello").unwrap();
        let root = tmp.path().join("store");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o555)).unwrap();

        let error = Store::new(&root)
            .add_tree("demo", "1", &source)
            .unwrap_err();

        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(error.to_string().contains("is owned by uid"), "{error}");
    }

    #[test]
    fn tree_hash_ignores_manifest_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("bin"), "hello").unwrap();
        let before = hash_tree(tmp.path()).unwrap();

        write_manifest(
            tmp.path(),
            &StoreManifest {
                schema: 1,
                name: "demo".to_string(),
                version: "1.0.0".to_string(),
                output_sha256: before.clone(),
            },
        )
        .unwrap();

        let after = hash_tree(tmp.path()).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn tree_hash_tracks_executable_bit() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        fs::write(a.path().join("tool"), "hello").unwrap();
        fs::write(b.path().join("tool"), "hello").unwrap();
        fs::set_permissions(b.path().join("tool"), fs::Permissions::from_mode(0o755)).unwrap();

        assert_ne!(hash_tree(a.path()).unwrap(), hash_tree(b.path()).unwrap());
    }

    #[test]
    fn add_tree_writes_content_addressed_store_entry() {
        let root = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        fs::create_dir(src.path().join("bin")).unwrap();
        fs::write(src.path().join("bin/rg"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(src.path().join("bin/rg"), fs::Permissions::from_mode(0o700)).unwrap();

        let store = Store::new(root.path().join("store"));
        let entry = store.add_tree("ripgrep", "14.1.1", src.path()).unwrap();

        assert!(entry.path.exists());
        assert!(entry
            .path
            .ends_with(format!("sha256-{}-ripgrep-14.1.1", entry.output_sha256)));
        assert_eq!(
            fs::metadata(entry.path.join("bin/rg"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );

        let manifest = store.verify_entry(&entry.path).unwrap();
        assert_eq!(manifest.name, "ripgrep");
        assert_eq!(manifest.version, "14.1.1");
        assert_eq!(manifest.output_sha256, entry.output_sha256);
    }

    #[test]
    fn add_tree_reuses_existing_matching_entry() {
        let root = tempfile::tempdir().unwrap();
        let src = tempfile::tempdir().unwrap();
        fs::write(src.path().join("tool"), "hello").unwrap();

        let store = Store::new(root.path().join("store"));
        let first = store.add_tree("tool", "1.0.0", src.path()).unwrap();
        let second = store.add_tree("tool", "1.0.0", src.path()).unwrap();

        assert_eq!(first, second);
    }
    #[test]
    fn changed_tree_and_manifest_cannot_reuse_the_original_address() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("file"), "original").unwrap();
        let store = Store::new(directory.path().join("store"));
        let entry = store.add_tree("example", "1", &source).unwrap();
        fs::write(entry.path.join("file"), "changed").unwrap();
        let mut manifest = read_manifest(&entry.path).unwrap();
        manifest.output_sha256 = hash_tree(&entry.path).unwrap();
        write_manifest(&entry.path, &manifest).unwrap();
        assert!(store
            .verify_entry(&entry.path)
            .unwrap_err()
            .to_string()
            .contains("identity/hash mismatch"));
    }
}
