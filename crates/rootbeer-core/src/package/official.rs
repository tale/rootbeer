use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};

use super::{ArtifactIndex, PackageCatalog, PackageIndexPin, ResolverInput};
use crate::store::hash_bytes;

/// A release-configured endpoint and Ed25519 verification key for the official index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficialIndexSource {
    pub url: String,
    pub public_key: String,
}

/// The selected immutable snapshot and an explanation of any availability fallback.
#[derive(Debug)]
pub struct IndexSelection {
    pub input: ResolverInput,
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    sequence: u64,
    index: PackageIndexPin,
    signature: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedSnapshot {
    manifest: Manifest,
    fetched_unix_seconds: u64,
}

impl OfficialIndexSource {
    /// Reads the public endpoint and trust key embedded by the release build.
    pub fn configured() -> Result<Option<Self>, String> {
        match (
            option_env!("ROOTBEER_INDEX_URL"),
            option_env!("ROOTBEER_INDEX_PUBLIC_KEY"),
        ) {
            (None, None) => Ok(None),
            (Some(url), Some(public_key)) => {
                let source = Self {
                    url: url.into(),
                    public_key: public_key.into(),
                };
                source.validate()?;
                Ok(Some(source))
            }
            _ => Err(
                "release must configure both ROOTBEER_INDEX_URL and ROOTBEER_INDEX_PUBLIC_KEY"
                    .into(),
            ),
        }
    }

    fn validate(&self) -> Result<(), String> {
        super::index::validate_https(&self.url)?;
        decode_hex::<32>(&self.public_key)?;
        Ok(())
    }

    /// Fetches and verifies a snapshot, or falls back on availability errors.
    /// Explicit refreshes never use a stale cache or the embedded catalog.
    pub fn select(&self, state: &Path, should_refresh: bool) -> Result<IndexSelection, String> {
        self.validate()?;
        self.select_with(state, should_refresh, fetch)
    }

    fn select_with(
        &self,
        state: &Path,
        should_refresh: bool,
        mut fetch: impl FnMut(&str, usize) -> Result<Vec<u8>, FetchError>,
    ) -> Result<IndexSelection, String> {
        let identity = hash_bytes(&serde_json::to_vec(self).map_err(|e| e.to_string())?);
        let cache = state.join("indexes").join(identity);
        fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
        let lock_file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(cache.join("lock"))
            .map_err(|e| e.to_string())?;
        use std::os::fd::AsRawFd;
        if unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(io::Error::last_os_error().to_string());
        }
        let cached = self.cached(&cache, state)?;
        let bytes = match fetch(&self.url, 64 * 1024) {
            Ok(bytes) => bytes,
            Err(error) => return fallback(error, cached, should_refresh),
        };
        let manifest: Manifest = serde_json::from_slice(&bytes)
            .map_err(|e| format!("invalid official index manifest: {e}"))?;
        self.verify(&manifest)?;
        if let Some(previous) = &cached {
            if manifest.sequence < previous.manifest.sequence
                || (manifest.sequence == previous.manifest.sequence
                    && manifest.index != previous.manifest.index)
            {
                return Err("official index rollback or conflicting sequence detected".into());
            }
        }
        let bytes = match fetch(&manifest.index.url, 16 * 1024 * 1024) {
            Ok(bytes) => bytes,
            Err(error) => return fallback(error, cached, should_refresh),
        };
        verify_index(&manifest.index, &bytes)?;
        let downloads = state.join("downloads");
        fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;
        atomic_write(
            &downloads.join(format!("sha256-{}", manifest.index.sha256)),
            &bytes,
        )?;
        let pin = manifest.index.clone();
        let snapshot = CachedSnapshot {
            manifest,
            fetched_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs(),
        };
        atomic_write(
            &cache.join("latest.json"),
            &serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?,
        )?;
        Ok(IndexSelection {
            input: ResolverInput::OfficialIndex(pin),
            notice: None,
        })
    }

    fn verify(&self, manifest: &Manifest) -> Result<(), String> {
        if manifest.schema != 1 || manifest.sequence == 0 {
            return Err("unsupported official index manifest schema or sequence".into());
        }
        super::index::validate_https(&manifest.index.url)?;
        manifest.index.validate()?;
        let message = serde_json::to_vec(&(
            "rootbeer-index-v1",
            manifest.sequence,
            &manifest.index.url,
            &manifest.index.sha256,
        ))
        .map_err(|e| e.to_string())?;
        UnparsedPublicKey::new(&ED25519, decode_hex::<32>(&self.public_key)?)
            .verify(&message, &decode_hex::<64>(&manifest.signature)?)
            .map_err(|_| "official index signature verification failed".to_string())
    }

    fn cached(&self, cache: &Path, state: &Path) -> Result<Option<CachedSnapshot>, String> {
        let bytes = match fs::read(cache.join("latest.json")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        let snapshot: CachedSnapshot =
            serde_json::from_slice(&bytes).map_err(|e| format!("invalid cached index: {e}"))?;
        self.verify(&snapshot.manifest)?;
        let bytes = fs::read(
            state
                .join("downloads")
                .join(format!("sha256-{}", snapshot.manifest.index.sha256)),
        )
        .map_err(|e| format!("cannot read cached index snapshot: {e}"))?;
        verify_index(&snapshot.manifest.index, &bytes)?;
        Ok(Some(snapshot))
    }
}

/// Signs a complete index after checking its trust key and previous signed sequence.
pub fn sign_index(
    bytes: &[u8],
    url: &str,
    sequence: u64,
    key_der: &[u8],
    public_key: &str,
    previous: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    use ring::signature::{Ed25519KeyPair, KeyPair};
    let index: ArtifactIndex = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    index.validate_complete()?;
    super::index::validate_https(url)?;
    if sequence == 0 {
        return Err("publication sequence must be positive".into());
    }
    let key =
        Ed25519KeyPair::from_pkcs8(key_der).map_err(|_| "invalid Ed25519 PKCS#8 signing key")?;
    if key.public_key().as_ref() != decode_hex::<32>(public_key)? {
        return Err("signing key does not match the release verification key".into());
    }
    if let Some(bytes) = previous {
        let manifest: Manifest = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        OfficialIndexSource {
            url: url.into(),
            public_key: public_key.into(),
        }
        .verify(&manifest)?;
        if sequence <= manifest.sequence {
            return Err("publication sequence must increase".into());
        }
    }
    let pin = PackageIndexPin {
        url: url.into(),
        sha256: hash_bytes(bytes),
    };
    let message = serde_json::to_vec(&("rootbeer-index-v1", sequence, &pin.url, &pin.sha256))
        .map_err(|e| e.to_string())?;
    let signature: String = key
        .sign(&message)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    serde_json::to_vec(&Manifest {
        schema: 1,
        sequence,
        index: pin,
        signature,
    })
    .map_err(|e| e.to_string())
}

pub(crate) fn select_default(should_refresh: bool) -> Result<IndexSelection, String> {
    match OfficialIndexSource::configured()? {
        Some(source) => source.select(&crate::state_dir(), should_refresh),
        None if should_refresh => Err("this build has no official index endpoint and verification key; cannot refresh the catalog".into()),
        None => embedded("official index is not configured in this build"),
    }
}

fn embedded(reason: &str) -> Result<IndexSelection, String> {
    Ok(IndexSelection {
        input: ResolverInput::Catalog {
            sha256: PackageCatalog::embedded()?.sha256(),
        },
        notice: Some(format!("{reason}; using the embedded package catalog")),
    })
}

#[derive(Debug)]
enum FetchError {
    Unavailable(String),
    Invalid(String),
}

fn fallback(
    error: FetchError,
    cached: Option<CachedSnapshot>,
    should_refresh: bool,
) -> Result<IndexSelection, String> {
    let reason = match error {
        FetchError::Unavailable(reason) => reason,
        FetchError::Invalid(reason) => return Err(reason),
    };
    if should_refresh {
        return Err(format!("cannot refresh official index: {reason}"));
    }
    let Some(cached) = cached else {
        return embedded(&reason);
    };
    Ok(IndexSelection {
        input: ResolverInput::OfficialIndex(cached.manifest.index),
        notice: Some(format!(
            "{reason}; using verified cached catalog (fetched at Unix time {})",
            cached.fetched_unix_seconds
        )),
    })
}

fn fetch(url: &str, limit: usize) -> Result<Vec<u8>, FetchError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .call()
        .map_err(|e| FetchError::Unavailable(format!("official index unavailable: {e}")))?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| FetchError::Unavailable(e.to_string()))?;
    if bytes.len() > limit {
        return Err(FetchError::Invalid(
            "official index response exceeds size limit".into(),
        ));
    }
    Ok(bytes)
}

fn verify_index(pin: &PackageIndexPin, bytes: &[u8]) -> Result<(), String> {
    if hash_bytes(bytes) != pin.sha256 {
        return Err("official index snapshot hash mismatch".into());
    }
    let index: ArtifactIndex = serde_json::from_slice(bytes)
        .map_err(|e| format!("invalid official index snapshot: {e}"))?;
    index.validate()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("cache path has no parent")?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], String> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(format!(
            "expected {} lowercase hexadecimal characters",
            N * 2
        ));
    }
    let mut bytes = [0; N];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte =
            u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn fixture(root: &Path) -> (OfficialIndexSource, Ed25519KeyPair, Vec<u8>) {
        let (catalog, receipt) = super::super::bundle::tests::fixture(root);
        let bundle = root.join("bundle");
        super::super::bundle_artifacts(&catalog, &[receipt], "https://example.org", &bundle)
            .unwrap();
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let source = OfficialIndexSource {
            url: "https://example.org/latest.json".into(),
            public_key: hex(key.public_key().as_ref()),
        };
        (source, key, fs::read(bundle.join("index.json")).unwrap())
    }

    fn manifest(key: &Ed25519KeyPair, bytes: &[u8], sequence: u64) -> Vec<u8> {
        let index = PackageIndexPin {
            url: "https://example.org/index.json".into(),
            sha256: hash_bytes(bytes),
        };
        let message =
            serde_json::to_vec(&("rootbeer-index-v1", sequence, &index.url, &index.sha256))
                .unwrap();
        serde_json::to_vec(&Manifest {
            schema: 1,
            sequence,
            index,
            signature: hex(key.sign(&message).as_ref()),
        })
        .unwrap()
    }

    fn unavailable(_: &str, _: usize) -> Result<Vec<u8>, FetchError> {
        Err(FetchError::Unavailable("network down".into()))
    }

    #[test]
    fn persists_verified_snapshot_and_uses_cache_only_for_availability_failures() {
        let root = tempfile::tempdir().unwrap();
        let (source, key, bytes) = fixture(root.path());
        let state = root.path().join("state");
        let signed = manifest(&key, &bytes, 2);
        let selected = source
            .select_with(&state, false, |url, _| {
                Ok(if url.ends_with("latest.json") {
                    signed.clone()
                } else {
                    bytes.clone()
                })
            })
            .unwrap();
        assert!(matches!(selected.input, ResolverInput::OfficialIndex(_)));
        assert!(selected.notice.is_none());
        let cached = source.select_with(&state, false, unavailable).unwrap();
        assert_eq!(selected.input, cached.input);
        assert!(cached.notice.unwrap().contains("verified cached"));
        assert!(source
            .select_with(&state, true, unavailable)
            .unwrap_err()
            .contains("cannot refresh"));
        let mut invalid: Manifest = serde_json::from_slice(&signed).unwrap();
        invalid.signature = "00".repeat(64);
        assert!(source
            .select_with(&state, false, |_, _| Ok(
                serde_json::to_vec(&invalid).unwrap()
            ))
            .unwrap_err()
            .contains("signature"));
        assert_eq!(
            source
                .select_with(&state, false, unavailable)
                .unwrap()
                .input,
            selected.input
        );
        assert!(source
            .select_with(&state, false, |url, _| Ok(
                if url.ends_with("latest.json") {
                    signed.clone()
                } else {
                    b"tampered".to_vec()
                }
            ))
            .unwrap_err()
            .contains("hash mismatch"));
        assert_eq!(
            source
                .select_with(&state, false, unavailable)
                .unwrap()
                .input,
            selected.input
        );
    }

    #[test]
    fn rejects_rollbacks_conflicts_and_cache_tampering() {
        let root = tempfile::tempdir().unwrap();
        let (source, key, bytes) = fixture(root.path());
        let state = root.path().join("state");
        source
            .select_with(&state, true, |url, _| {
                Ok(if url.ends_with("latest.json") {
                    manifest(&key, &bytes, 3)
                } else {
                    bytes.clone()
                })
            })
            .unwrap();
        for signed in [manifest(&key, &bytes, 2), manifest(&key, b"changed", 3)] {
            assert!(source
                .select_with(&state, false, |_, _| Ok(signed.clone()))
                .unwrap_err()
                .contains("rollback"));
        }
        fs::write(
            state
                .join("downloads")
                .join(format!("sha256-{}", hash_bytes(&bytes))),
            "corrupt",
        )
        .unwrap();
        assert!(source
            .select_with(&state, false, unavailable)
            .unwrap_err()
            .contains("hash mismatch"));
    }

    #[test]
    fn only_unavailability_allows_embedded_fallback() {
        let root = tempfile::tempdir().unwrap();
        let (source, _, _) = fixture(root.path());
        let state = root.path().join("state");
        let result = source.select_with(&state, false, unavailable).unwrap();
        assert!(matches!(result.input, ResolverInput::Catalog { .. }));
        assert!(result.notice.unwrap().contains("embedded"));
        assert!(source.select_with(&state, true, unavailable).is_err());
        assert!(source
            .select_with(&state, false, |_, _| Ok(b"invalid JSON".to_vec()))
            .is_err());
        assert!(source
            .select_with(&state, false, |_, _| Err(FetchError::Invalid(
                "oversized".into()
            )))
            .is_err());
    }
    #[test]
    fn signer_requires_complete_coverage_matching_key_and_increasing_sequence() {
        let root = tempfile::tempdir().unwrap();
        let (_, _, incomplete) = fixture(root.path());
        let der = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(der.as_ref()).unwrap();
        let public_key = hex(key.public_key().as_ref());
        let url = "https://example.org/snapshot.json";
        assert!(
            sign_index(&incomplete, url, 1, der.as_ref(), &public_key, None)
                .unwrap_err()
                .contains("incomplete")
        );
        let mut index: ArtifactIndex = serde_json::from_slice(&incomplete).unwrap();
        index.catalog.packages.retain(|name, _| name == "xz");
        for recipe in index
            .catalog
            .packages
            .get_mut("xz")
            .unwrap()
            .versions
            .values_mut()
        {
            recipe.systems = vec!["aarch64-linux".into()];
        }
        index.catalog_sha256 = index.catalog.sha256();
        let bytes = serde_json::to_vec(&index).unwrap();
        let signed = sign_index(&bytes, url, 10, der.as_ref(), &public_key, None).unwrap();
        let source = OfficialIndexSource {
            url: url.into(),
            public_key: public_key.clone(),
        };
        source
            .verify(&serde_json::from_slice(&signed).unwrap())
            .unwrap();
        assert!(
            sign_index(&bytes, url, 10, der.as_ref(), &public_key, Some(&signed))
                .unwrap_err()
                .contains("increase")
        );
        assert!(sign_index(&bytes, url, 11, der.as_ref(), &public_key, Some(&signed)).is_ok());
        assert!(
            sign_index(&bytes, url, 11, der.as_ref(), &"0".repeat(64), None)
                .unwrap_err()
                .contains("does not match")
        );
    }
}
