use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
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
        let blob = url
            .starts_with("ghcr://")
            .then(|| super::ghcr::GhcrBlob::parse(url))
            .transpose()
            .map_err(io::Error::other)?;
        if let Some(blob) = &blob {
            if expected_sha256.is_some_and(|expected| expected != blob.sha256) {
                return Err(io::Error::other(
                    "GHCR reference digest does not match locked source hash",
                ));
            }
        }
        let expected_sha256 =
            expected_sha256.or_else(|| blob.as_ref().map(|blob| blob.sha256.as_str()));
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
        with_retries(|| self.download_once(url))
    }

    fn download_once(&self, url: &str) -> io::Result<(PathBuf, String)> {
        for attempt in 0..16 {
            let tmp = self.temp_path(attempt);
            let mut file = match OpenOptions::new().write(true).create_new(true).open(&tmp) {
                Ok(file) => file,
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(err),
            };

            match copy_url_to_writer(url, &mut file) {
                Ok(sha256) => {
                    if let Err(error) = file.sync_all() {
                        let _ = fs::remove_file(&tmp);
                        return Err(error);
                    }
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
    with_retries(|| {
        let mut reader = url_reader(url)?;
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|error| body_error(url, error))?;
        Ok(bytes)
    })
}

pub(super) fn read_json_url<T>(url: &str) -> io::Result<T>
where
    T: DeserializeOwned,
{
    let bytes = read_url(url)?;
    serde_json::from_slice(&bytes).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to parse JSON from {url}: {e}"),
        )
    })
}

fn copy_url_to_writer(url: &str, writer: &mut impl Write) -> io::Result<String> {
    let mut reader = url_reader(url)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|error| body_error(url, error))?;
        if n == 0 {
            break;
        }

        hasher.update(&buf[..n]);
        writer.write_all(&buf[..n])?;
    }

    Ok(hex(hasher.finalize().as_slice()))
}

fn url_reader(url: &str) -> io::Result<Box<dyn Read>> {
    if url.starts_with("ghcr://") {
        return super::ghcr::GhcrBlob::parse(url)
            .map_err(io::Error::other)?
            .reader();
    }
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

    let token = std::env::var("GITHUB_TOKEN").ok();
    let (_, body) = http_request(url, token.as_deref())
        .call()
        .map_err(|error| {
            let is_transient = matches!(
                error,
                ureq::Error::StatusCode(408 | 429 | 500 | 502 | 503 | 504)
                    | ureq::Error::Io(_)
                    | ureq::Error::Timeout(_)
                    | ureq::Error::HostNotFound
                    | ureq::Error::ConnectionFailed
                    | ureq::Error::Protocol(_)
                    | ureq::Error::Decompress(_, _)
            );
            let message = format!("failed to fetch {url}: {error}");
            if is_transient {
                return io::Error::other(TransientDownload(message));
            }
            io::Error::other(message)
        })?
        .into_parts();

    Ok(Box::new(body.into_reader()))
}

#[derive(Debug)]
struct TransientDownload(String);

impl std::fmt::Display for TransientDownload {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for TransientDownload {}

fn body_error(url: &str, error: io::Error) -> io::Error {
    let message = format!("failed to read {url}: {error}");
    if url.starts_with("http://") || url.starts_with("https://") {
        return io::Error::other(TransientDownload(message));
    }
    io::Error::new(error.kind(), message)
}

fn with_retries<T>(mut operation: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    for attempt in 0..3 {
        match operation() {
            Err(error)
                if attempt < 2
                    && error
                        .get_ref()
                        .is_some_and(|cause| cause.is::<TransientDownload>()) =>
            {
                let delay = 1 << attempt;
                eprintln!("download retry {}/3 in {delay}s: {error}", attempt + 2);
                std::thread::sleep(std::time::Duration::from_secs(delay));
            }
            result => return result,
        }
    }
    unreachable!()
}

pub(super) fn http_request(
    url: &str,
    token: Option<&str>,
) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .redirect_auth_headers(ureq::config::RedirectAuthHeaders::Never)
        .build()
        .into();
    let request = agent.get(url).header("User-Agent", USER_AGENT);
    let Some(token) = token.map(str::trim).filter(|token| !token.is_empty()) else {
        return request;
    };
    if !url.starts_with("https://api.github.com/") {
        return request;
    }

    request.header("Authorization", format!("Bearer {token}"))
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

    struct HttpServer {
        url: String,
        is_done: std::sync::Arc<std::sync::atomic::AtomicBool>,
        requests: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl HttpServer {
        fn new(responses: Vec<&'static str>) -> Self {
            use std::io::{BufRead, BufReader};
            use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
            use std::sync::Arc;

            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let is_done = Arc::new(AtomicBool::new(false));
            let requests = Arc::new(AtomicUsize::new(0));
            let server_done = is_done.clone();
            let server_requests = requests.clone();
            let thread = std::thread::spawn(move || {
                while !server_done.load(Ordering::SeqCst) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(connection) => connection,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                            continue;
                        }
                        Err(error) => panic!("{error}"),
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                        .unwrap();
                    let mut reader = BufReader::new(&stream);
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                            break;
                        }
                    }
                    let request = server_requests.fetch_add(1, Ordering::SeqCst);
                    let response = responses[request.min(responses.len() - 1)];
                    stream.write_all(response.as_bytes()).unwrap();
                }
            });
            Self {
                url,
                is_done,
                requests,
                thread: Some(thread),
            }
        }

        fn request_count(&self) -> usize {
            self.requests.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl Drop for HttpServer {
        fn drop(&mut self) {
            self.is_done
                .store(true, std::sync::atomic::Ordering::SeqCst);
            let result = self.thread.take().unwrap().join();
            if !std::thread::panicking() {
                result.unwrap();
            }
        }
    }

    const SUCCESS: &str =
        "HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\narchive";
    const UNAVAILABLE: &str =
        "HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    const PARTIAL: &str =
        "HTTP/1.1 200 OK\r\nContent-Length: 50\r\nConnection: close\r\n\r\npartial";

    #[test]
    fn retries_transient_status_and_partial_body_with_clean_output() {
        let server = HttpServer::new(vec![UNAVAILABLE, PARTIAL, SUCCESS]);
        let tmp = tempfile::tempdir().unwrap();
        let cache = DownloadCache::new(tmp.path());
        let file = cache
            .materialize(&server.url, Some(&hash_bytes(b"archive")))
            .unwrap();
        assert_eq!(fs::read(file.path).unwrap(), b"archive");
        assert_eq!(server.request_count(), 3);
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
    }

    #[test]
    fn retries_connection_closed_before_response() {
        let server = HttpServer::new(vec!["", SUCCESS]);
        assert_eq!(read_url(&server.url).unwrap(), b"archive");
        assert_eq!(server.request_count(), 2);
    }

    #[test]
    fn exhausted_body_retries_leave_no_partial_downloads() {
        let server = HttpServer::new(vec![PARTIAL]);
        let tmp = tempfile::tempdir().unwrap();
        let error = DownloadCache::new(tmp.path())
            .materialize(&server.url, None)
            .unwrap_err();
        assert!(error.to_string().contains(&server.url));
        assert_eq!(server.request_count(), 3);
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);
    }

    #[test]
    fn exhausted_status_retries_are_bounded() {
        let server = HttpServer::new(vec![UNAVAILABLE]);
        let error = read_url(&server.url).unwrap_err();
        assert!(error.to_string().contains("503"));
        assert_eq!(server.request_count(), 3);
    }

    #[test]
    fn permanent_status_and_checksum_failures_are_not_retried() {
        for response in [
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            SUCCESS,
        ] {
            let server = HttpServer::new(vec![response]);
            let tmp = tempfile::tempdir().unwrap();
            let error = DownloadCache::new(tmp.path())
                .materialize(&server.url, Some(&hash_bytes(b"other")))
                .unwrap_err();
            assert!(error.to_string().contains(if response == SUCCESS {
                "hash mismatch"
            } else {
                "404"
            }));
            assert_eq!(server.request_count(), 1);
            assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn invalid_json_is_not_retried() {
        let server = HttpServer::new(vec![SUCCESS]);
        assert!(read_json_url::<serde_json::Value>(&server.url).is_err());
        assert_eq!(server.request_count(), 1);
    }

    #[test]
    fn authenticates_only_https_github_api_requests() {
        let url = "https://api.github.com/repos/owner/repo/releases/latest";
        let request = http_request(url, Some("test-token"));
        assert_eq!(
            request.headers_ref().unwrap()["authorization"],
            "Bearer test-token"
        );

        for url in [
            "http://api.github.com/repos/owner/repo",
            "https://api.github.com.evil.example/repos/owner/repo",
            "https://api.github.com@evil.example/repos/owner/repo",
            "https://github.com/owner/repo/releases/download/v1/tool",
            "https://example.com/tool",
        ] {
            assert!(!http_request(url, Some("test-token"))
                .headers_ref()
                .unwrap()
                .contains_key("authorization"));
        }
        for token in [None, Some(""), Some("  ")] {
            assert!(!http_request(url, token)
                .headers_ref()
                .unwrap()
                .contains_key("authorization"));
        }
    }

    #[test]
    fn strips_authorization_on_redirects() {
        use std::io::{BufRead, BufReader};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let redirect = format!(
            "HTTP/1.1 302 Found\r\nLocation: {url}/redirected\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        let server = std::thread::spawn(move || {
            for is_redirected in [false, true] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut headers = String::new();
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    headers.push_str(&line.to_ascii_lowercase());
                }
                assert_eq!(
                    headers.contains("authorization: bearer test-token"),
                    !is_redirected
                );
                let response = if is_redirected {
                    "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                } else {
                    &redirect
                };
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        http_request(&url, None)
            .header("Authorization", "Bearer test-token")
            .call()
            .unwrap();
        server.join().unwrap();
    }

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
