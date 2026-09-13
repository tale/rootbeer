use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::package::download::http_request;
use crate::store::hash_bytes;

const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
struct Entry {
    url: String,
    etag: Option<String>,
    sha256: String,
    body: String,
}

#[derive(Default, Serialize)]
pub(super) struct Statistics {
    pub fetched: usize,
    pub not_modified: usize,
}

pub(super) struct MetadataCache {
    directory: PathBuf,
    pub statistics: Statistics,
}

impl MetadataCache {
    pub fn new(directory: &Path) -> Result<Self, String> {
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        Ok(Self {
            directory: directory.into(),
            statistics: Statistics::default(),
        })
    }

    pub fn fetch(&mut self, url: &str) -> Result<Value, String> {
        self.fetch_with(url, |etag| {
            let token = std::env::var("GITHUB_TOKEN").ok();
            let mut request =
                http_request(url, token.as_deref()).header("Accept", "application/vnd.github+json");
            if let Some(etag) = etag {
                request = request.header("If-None-Match", etag);
            }
            let mut response = request
                .config()
                .timeout_global(Some(Duration::from_secs(60)))
                .build()
                .call()
                .map_err(|e| format!("cannot fetch {url}: {e}"))?;
            let status = response.status().as_u16();
            let etag = response
                .headers()
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .map(String::from);
            let mut bytes = Vec::new();
            response
                .body_mut()
                .as_reader()
                .take((MAX_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > MAX_BYTES {
                return Err(format!("metadata exceeds {MAX_BYTES} bytes: {url}"));
            }
            let body = String::from_utf8(bytes).map_err(|e| e.to_string())?;
            Ok((status, etag, body))
        })
    }

    fn fetch_with(
        &mut self,
        url: &str,
        request: impl FnOnce(Option<&str>) -> Result<(u16, Option<String>, String), String>,
    ) -> Result<Value, String> {
        let path = self
            .directory
            .join(format!("{}.json", hash_bytes(url.as_bytes())));
        let cached: Option<Entry> = match fs::read(&path) {
            Ok(bytes) => {
                let entry: Entry = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("invalid metadata cache {}: {e}", path.display()))?;
                if entry.url != url || hash_bytes(entry.body.as_bytes()) != entry.sha256 {
                    return Err(format!(
                        "metadata cache hash or URL mismatch: {}",
                        path.display()
                    ));
                }
                Some(entry)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        let etag = cached.as_ref().and_then(|entry| entry.etag.as_deref());
        let (status, new_etag, body) = request(etag)?;
        if status == 304 {
            let entry = cached
                .filter(|entry| entry.etag.is_some())
                .ok_or("received 304 without a cached validator")?;
            let value = serde_json::from_str(&entry.body).map_err(|e| e.to_string())?;
            self.statistics.not_modified += 1;
            return Ok(value);
        }
        if status != 200 {
            return Err(format!("unexpected metadata HTTP status {status}: {url}"));
        }
        let value =
            serde_json::from_str(&body).map_err(|e| format!("invalid metadata JSON: {e}"))?;
        let entry = Entry {
            url: url.into(),
            etag: new_etag,
            sha256: hash_bytes(body.as_bytes()),
            body,
        };
        let mut temporary =
            tempfile::NamedTempFile::new_in(&self.directory).map_err(|e| e.to_string())?;
        temporary
            .write_all(&serde_json::to_vec(&entry).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        temporary.as_file().sync_all().map_err(|e| e.to_string())?;
        temporary.persist(path).map_err(|e| e.to_string())?;
        self.statistics.fetched += 1;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revalidates_with_etags_without_using_stale_data_on_errors() {
        let root = tempfile::tempdir().unwrap();
        let mut cache = MetadataCache::new(root.path()).unwrap();
        let url = "https://api.github.com/repos/owner/tool";
        let value = cache
            .fetch_with(url, |etag| {
                assert!(etag.is_none());
                Ok((200, Some("\"one\"".into()), "{\"id\":42}".into()))
            })
            .unwrap();
        let repeated = cache
            .fetch_with(url, |etag| {
                assert_eq!(etag, Some("\"one\""));
                Ok((304, None, String::new()))
            })
            .unwrap();
        assert_eq!(value, repeated);
        assert_eq!(cache.statistics.fetched, 1);
        assert_eq!(cache.statistics.not_modified, 1);
        assert!(cache
            .fetch_with(url, |_| Err("rate limited".into()))
            .is_err());
        let updated = cache
            .fetch_with(url, |_| {
                Ok((200, Some("\"two\"".into()), "{\"id\":43}".into()))
            })
            .unwrap();
        assert_eq!(updated["id"], 43);
        cache
            .fetch_with(url, |etag| {
                assert_eq!(etag, Some("\"two\""));
                Ok((304, None, String::new()))
            })
            .unwrap();
    }

    #[test]
    fn rejects_corrupt_entries_and_unsolicited_not_modified() {
        let root = tempfile::tempdir().unwrap();
        let mut cache = MetadataCache::new(root.path()).unwrap();
        let url = "https://api.github.com/repos/owner/tool";
        assert!(cache
            .fetch_with(url, |_| Ok((304, None, String::new())))
            .is_err());
        cache
            .fetch_with(url, |_| Ok((200, Some("one".into()), "{}".into())))
            .unwrap();
        let path = root
            .path()
            .join(format!("{}.json", hash_bytes(url.as_bytes())));
        let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["body"] = "{\"changed\":true}".into();
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(cache
            .fetch_with(url, |_| panic!("corrupt data must not reach the network"))
            .unwrap_err()
            .contains("mismatch"));
    }
}
