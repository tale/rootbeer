use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::state_dir;
use crate::store::hash_file;

const USER_AGENT: &str = concat!("rootbeer/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub(super) struct DownloadCache {
    root: PathBuf,
    offline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DownloadedFile {
    pub path: PathBuf,
    pub sha256: String,
}

impl DownloadCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            offline: false,
        }
    }

    pub fn offline(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            offline: true,
        }
    }

    pub fn materialize(
        &self,
        url: &str,
        expected_sha256: Option<&str>,
    ) -> io::Result<DownloadedFile> {
        fs::create_dir_all(&self.root)?;

        if let Some(sha256) = expected_sha256 {
            if let Some(downloaded) = self.valid_cached(sha256)? {
                return Ok(downloaded);
            }
        }

        if self.offline {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                match expected_sha256 {
                    Some(sha256) => format!(
                        "source {url} with sha256 {sha256} is not in the download cache and offline mode is enabled"
                    ),
                    None => format!("source {url} cannot be fetched in offline mode"),
                },
            ));
        }

        let (tmp, actual_sha256) = self.download_to_temp(url)?;
        let result = self.finish_download(&tmp, actual_sha256, expected_sha256, url);
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }

        result
    }

    pub fn materialize_verified(&self, url: &str, sha256: &str) -> io::Result<PathBuf> {
        self.materialize(url, Some(sha256))
            .map(|downloaded| downloaded.path)
    }

    fn valid_cached(&self, sha256: &str) -> io::Result<Option<DownloadedFile>> {
        let path = self.cached_path(sha256);
        if !path.exists() {
            return Ok(None);
        }

        if hash_file(&path)? == sha256 {
            return Ok(Some(DownloadedFile {
                path,
                sha256: sha256.to_string(),
            }));
        }

        fs::remove_file(path)?;
        Ok(None)
    }

    fn download_to_temp(&self, url: &str) -> io::Result<(PathBuf, String)> {
        for attempt in 0..16 {
            let tmp = self.temp_path(attempt);
            let mut file = match OpenOptions::new().write(true).create_new(true).open(&tmp) {
                Ok(file) => file,
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(err),
            };

            match copy_url_to_writer(url, &mut file) {
                Ok(sha256) => {
                    file.sync_all()?;
                    return Ok((tmp, sha256));
                }

                Err(err) => {
                    let _ = fs::remove_file(&tmp);
                    return Err(err);
                }
            }
        }

        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "failed to allocate a temporary download path in {}",
                self.root.display()
            ),
        ))
    }

    fn finish_download(
        &self,
        tmp: &Path,
        actual_sha256: String,
        expected_sha256: Option<&str>,
        url: &str,
    ) -> io::Result<DownloadedFile> {
        if let Some(expected) = expected_sha256 {
            if actual_sha256 != expected {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("source {url} hash mismatch: expected {expected}, got {actual_sha256}"),
                ));
            }
        }

        let cached = self.cached_path(&actual_sha256);
        if cached.exists() {
            if hash_file(&cached)? == actual_sha256 {
                fs::remove_file(tmp)?;
                return Ok(DownloadedFile {
                    path: cached,
                    sha256: actual_sha256,
                });
            }

            fs::remove_file(&cached)?;
        }

        match fs::rename(tmp, &cached) {
            Ok(()) => Ok(DownloadedFile {
                path: cached,
                sha256: actual_sha256,
            }),

            Err(err) if cached.exists() => {
                if hash_file(&cached)? == actual_sha256 {
                    let _ = fs::remove_file(tmp);
                    Ok(DownloadedFile {
                        path: cached,
                        sha256: actual_sha256,
                    })
                } else {
                    Err(err)
                }
            }

            Err(err) => Err(err),
        }
    }

    fn cached_path(&self, sha256: &str) -> PathBuf {
        self.root.join(format!("sha256-{sha256}"))
    }

    fn temp_path(&self, attempt: u32) -> PathBuf {
        let pid = std::process::id();
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();

        self.root
            .join(format!(".tmp-download-{pid}-{epoch}-{attempt}"))
    }
}

impl Default for DownloadCache {
    fn default() -> Self {
        Self::new(state_dir().join("downloads"))
    }
}

pub(super) fn read_url(url: &str) -> io::Result<Vec<u8>> {
    let mut reader = url_reader(url)?;
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|e| io::Error::other(format!("failed to read {url}: {e}")))?;
    Ok(bytes)
}

fn copy_url_to_writer(url: &str, writer: &mut impl Write) -> io::Result<String> {
    let mut reader = url_reader(url)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| io::Error::other(format!("failed to read {url}: {e}")))?;
        if n == 0 {
            break;
        }

        hasher.update(&buf[..n]);
        writer.write_all(&buf[..n])?;
    }

    Ok(hex(hasher.finalize().as_slice()))
}

fn url_reader(url: &str) -> io::Result<Box<dyn Read>> {
    if let Some(path) = url.strip_prefix("file://") {
        let file = fs::File::open(path)
            .map_err(|e| io::Error::new(e.kind(), format!("failed to read {url}: {e}")))?;
        return Ok(Box::new(file));
    }

    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported source URL `{url}`"),
        ));
    }

    let (_, body) = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| io::Error::other(format!("failed to fetch {url}: {e}")))?
        .into_parts();

    Ok(Box::new(body.into_reader()))
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
    use std::fs;

    use super::*;
    use crate::store::hash_bytes;

    #[test]
    fn materializes_file_url_into_content_addressed_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source.tar.gz");
        fs::write(&source, b"archive bytes").unwrap();
        let cache = DownloadCache::new(tmp.path().join("downloads"));

        let downloaded = cache
            .materialize(&format!("file://{}", source.display()), None)
            .unwrap();

        let sha256 = hash_bytes(b"archive bytes");
        assert_eq!(downloaded.sha256, sha256);
        assert_eq!(downloaded.path, cache.cached_path(&sha256));
        assert_eq!(fs::read(downloaded.path).unwrap(), b"archive bytes");
    }

    #[test]
    fn returns_valid_cached_file_before_opening_url() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = DownloadCache::new(tmp.path().join("downloads"));
        fs::create_dir_all(&cache.root).unwrap();
        let sha256 = hash_bytes(b"cached bytes");
        fs::write(cache.cached_path(&sha256), b"cached bytes").unwrap();

        let downloaded = cache
            .materialize("unsupported://does-not-matter", Some(&sha256))
            .unwrap();

        assert_eq!(downloaded.path, cache.cached_path(&sha256));
        assert_eq!(downloaded.sha256, sha256);
    }

    #[test]
    fn rejects_expected_hash_mismatch_without_caching() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source.tar.gz");
        fs::write(&source, b"actual bytes").unwrap();
        let cache = DownloadCache::new(tmp.path().join("downloads"));

        let err = cache
            .materialize(
                &format!("file://{}", source.display()),
                Some(&hash_bytes(b"expected bytes")),
            )
            .unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(fs::read_dir(&cache.root).unwrap().next().is_none());
    }

    #[test]
    fn offline_cache_misses_do_not_open_url() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = DownloadCache::offline(tmp.path().join("downloads"));

        let err = cache
            .materialize("unsupported://does-not-matter", Some("missing"))
            .unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(err.to_string().contains("offline mode"));
    }
}
