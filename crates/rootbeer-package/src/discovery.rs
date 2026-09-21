use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};

use crate::distribution::{verify_record, PackageRecord};
use crate::download::DownloadCache;
use crate::official::decode_hex;
use crate::{
    PackageCatalog, PackageIndexPin, PackageRequest, PackageResolution, PackageResolver,
    ResolutionProof, ResolveContext,
};
use rootbeer_store::hash_bytes;

pub const MANIFEST_LIMIT: usize = 16 * 1024 * 1024;

/// Discovery metadata; package content approval remains in each independently signed record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryManifest {
    pub schema: u32,
    pub sequence: u64,
    pub catalog: PackageCatalog,
    pub records: BTreeMap<String, BTreeMap<String, PackageIndexPin>>,
    pub signature: String,
}

/// Immutable discovery selection and the independently configured publisher key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryPin {
    pub manifest: PackageIndexPin,
    pub public_key: String,
}

/// The signed package record used for an installation, independent of later discovery changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRecordProof {
    pub record: PackageIndexPin,
    pub public_key: String,
    pub system: String,
}

/// Canonical JSON values sort object keys before signing; arrays retain their order.
pub(crate) fn signing_message<C: Serialize, R: Serialize>(
    sequence: u64,
    catalog: &C,
    records: &R,
) -> Result<Vec<u8>, String> {
    let mut value = serde_json::to_value(("rootbeer-discovery-v1", sequence, catalog, records))
        .map_err(|error| error.to_string())?;
    value.sort_all_objects();
    serde_json::to_vec(&value).map_err(|error| error.to_string())
}

fn signed_sequence(value: &serde_json::Value) -> Result<u64, String> {
    let sequence = value["sequence"].as_u64().unwrap_or_default();
    if value["schema"].as_u64() != Some(2) || sequence == 0 || sequence > 9_007_199_254_740_991 {
        return Err("unsupported discovery schema or sequence".into());
    }
    Ok(sequence)
}

/// Verifies the publisher signature over the whole document without decoding any recipe.
pub fn verify_signed(bytes: &[u8], public_key: &str) -> Result<u64, String> {
    verify_value(&parse_manifest(bytes)?, public_key)
}

pub(crate) fn parse_manifest(bytes: &[u8]) -> Result<serde_json::Value, String> {
    if bytes.len() > MANIFEST_LIMIT {
        return Err("discovery manifest exceeds 16 MiB".into());
    }
    serde_json::from_slice(bytes).map_err(|error| error.to_string())
}

fn verify_value(value: &serde_json::Value, public_key: &str) -> Result<u64, String> {
    let sequence = signed_sequence(value)?;
    let signature = value["signature"]
        .as_str()
        .ok_or("discovery manifest has no signature")?;
    UnparsedPublicKey::new(&ED25519, decode_hex::<32>(public_key)?)
        .verify(
            &signing_message(sequence, &value["catalog"], &value["records"])?,
            &decode_hex::<64>(signature)?,
        )
        .map_err(|_| "discovery signature verification failed".to_string())?;
    Ok(sequence)
}

impl DiscoveryManifest {
    pub fn signing_message(&self) -> Result<Vec<u8>, String> {
        signing_message(self.sequence, &self.catalog, &self.records)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 2 || self.sequence == 0 || self.sequence > 9_007_199_254_740_991 {
            return Err("unsupported discovery schema or sequence".into());
        }
        self.catalog.validate()?;
        for (id, platforms) in &self.records {
            let (package, version, recipe) =
                crate::graph::find_recipe_definition(&self.catalog, id)?;
            if id != &format!("{}@{version}", package.name) || platforms.is_empty() {
                return Err("invalid discovery package identity".into());
            }
            for (system, pin) in platforms {
                if !recipe.supported_systems().contains(system) {
                    return Err(format!("{id}: unapproved discovery platform {system}"));
                }
                crate::index::validate_https(&pin.url)?;
                pin.validate()?;
            }
        }
        Ok(())
    }

    pub fn verify(&self, public_key: &str) -> Result<(), String> {
        UnparsedPublicKey::new(&ED25519, decode_hex::<32>(public_key)?)
            .verify(
                &self.signing_message()?,
                &decode_hex::<64>(&self.signature)?,
            )
            .map_err(|_| "discovery signature verification failed".to_string())?;
        self.validate()
    }

    pub fn from_bytes(bytes: &[u8], public_key: &str) -> Result<Self, String> {
        if bytes.len() > MANIFEST_LIMIT {
            return Err("discovery manifest exceeds 16 MiB".into());
        }
        let manifest: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        manifest.verify(public_key)?;
        Ok(manifest)
    }
}

pub(crate) fn cache_path(source: &crate::OfficialIndexSource, state: &Path) -> std::path::PathBuf {
    let identity = hash_bytes(&serde_json::to_vec(source).expect("source serializes"));
    state.join("discovery").join(identity).join("current.json")
}

pub(crate) fn cached(
    source: &crate::OfficialIndexSource,
    state: &Path,
) -> Result<Option<Vec<u8>>, String> {
    match fs::read(cache_path(source, state)) {
        Ok(bytes) => {
            verify_signed(&bytes, &source.public_key)?;
            Ok(Some(bytes))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn select(
    source: &crate::OfficialIndexSource,
    state: &Path,
    bytes: &[u8],
) -> Result<crate::IndexSelection, String> {
    let sequence = verify_signed(bytes, &source.public_key)?;
    let cache = cache_path(source, state);
    fs::create_dir_all(cache.parent().unwrap()).map_err(|error| error.to_string())?;
    let guard = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(cache.with_extension("lock"))
        .map_err(|error| error.to_string())?;
    guard.lock().map_err(|error| error.to_string())?;
    if let Some(previous) = cached(source, state)? {
        let previous_sequence = verify_signed(&previous, &source.public_key)?;
        if sequence < previous_sequence || (sequence == previous_sequence && bytes != previous) {
            return Err("discovery rollback or conflicting sequence detected".into());
        }
    }
    let sha256 = hash_bytes(bytes);
    let downloads = state.join("downloads");
    fs::create_dir_all(&downloads).map_err(|error| error.to_string())?;
    crate::official::atomic_write(&downloads.join(format!("sha256-{sha256}")), bytes)?;
    crate::official::atomic_write(&cache, bytes)?;
    let (base, _) = source.url.rsplit_once('/').ok_or("invalid discovery URL")?;
    Ok(crate::IndexSelection {
        input: crate::ResolverInput::Discovery(DiscoveryPin {
            manifest: PackageIndexPin {
                url: format!("{base}/manifests/{sha256}.json"),
                sha256,
            },
            public_key: source.public_key.clone(),
        }),
        notice: None,
    })
}

/// Loads discovery once and verifies only the package record requested for installation.
pub struct DiscoveryResolver {
    pin: DiscoveryPin,
    downloads: DownloadCache,
    manifest: OnceLock<Result<DiscoveryManifest, String>>,
}

impl DiscoveryResolver {
    pub fn new(pin: &DiscoveryPin) -> Self {
        Self::with_cache(pin, crate::state_dir().join("downloads"), false)
    }

    pub fn with_cache(
        pin: &DiscoveryPin,
        cache: impl Into<std::path::PathBuf>,
        is_offline: bool,
    ) -> Self {
        Self {
            pin: pin.clone(),
            downloads: if is_offline {
                DownloadCache::offline(cache)
            } else {
                DownloadCache::new(cache)
            },
            manifest: OnceLock::new(),
        }
    }

    pub fn manifest(&self) -> Result<&DiscoveryManifest, String> {
        self.manifest
            .get_or_init(|| {
                self.pin.manifest.validate()?;
                let bytes = self.read(&self.pin.manifest, MANIFEST_LIMIT)?;
                DiscoveryManifest::from_bytes(&bytes, &self.pin.public_key)
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    fn read(&self, pin: &PackageIndexPin, limit: usize) -> Result<Vec<u8>, String> {
        let path = self
            .downloads
            .materialize_verified(&pin.url, &pin.sha256)
            .map_err(|error| error.to_string())?;
        if fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len()
            > limit as u64
        {
            return Err("package metadata exceeds size limit".into());
        }
        fs::read(path).map_err(|error| error.to_string())
    }

    pub fn record(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<(PackageRecord, PackageIndexPin), String> {
        let manifest = self.manifest()?;
        let package = manifest
            .catalog
            .find(&request.name)
            .ok_or_else(|| format!("unknown package {}", request.name))?;
        let version = request
            .version
            .as_deref()
            .unwrap_or_else(|| package.default_version_for(&context.system));
        let id = format!("{}@{version}", package.name);
        let pin = manifest
            .records
            .get(&id)
            .and_then(|platforms| platforms.get(&context.system))
            .ok_or_else(|| format!("{id} has no published package for {}", context.system))?;
        let bytes = self.read(pin, crate::distribution::RECORD_LIMIT)?;
        let record = verify_record(&bytes, &self.pin.public_key, &id, &context.system)?;
        if package
            .versions
            .get(version)
            .map(|recipe| recipe.for_system(&context.system))
            .as_ref()
            != Some(&record.recipe)
        {
            return Err("package record differs from the discovered recipe".into());
        }
        Ok((record, pin.clone()))
    }
}

impl PackageResolver for DiscoveryResolver {
    fn name(&self) -> &str {
        "rootbeer"
    }

    fn resolve(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<Option<PackageResolution>, String> {
        if request.source.is_some() || request.asset.is_some() || !request.bins.is_empty() {
            return Err(
                "published packages do not accept source, asset, or command overrides".into(),
            );
        }
        let (record, pin) = self.record(request, context)?;
        Ok(Some(PackageResolution::new(
            record.artifact.package,
            ResolutionProof::PackageRecord(PackageRecordProof {
                record: pin,
                public_key: self.pin.public_key.clone(),
                system: context.system.clone(),
            }),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    #[test]
    fn signing_payload_sorts_keys_even_with_serde_preserve_order_enabled() {
        let manifest = DiscoveryManifest {
            schema: 2,
            sequence: 1,
            catalog: PackageCatalog {
                extra: Default::default(),
                schema: 1,
                packages: BTreeMap::new(),
            },
            records: BTreeMap::new(),
            signature: String::new(),
        };
        assert_eq!(
            manifest.signing_message().unwrap(),
            br#"["rootbeer-discovery-v1",1,{"packages":{},"schema":1},{}]"#
        );
    }

    #[test]
    fn caches_signed_discovery_and_rejects_rollbacks_and_tampering() {
        let root = tempfile::tempdir().unwrap();
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let public_key: String = key
            .public_key()
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let source = crate::OfficialIndexSource {
            url: "https://example.com/current.json".into(),
            public_key: public_key.clone(),
        };
        let mut manifest = DiscoveryManifest {
            schema: 2,
            sequence: 2,
            catalog: crate::test_catalog::catalog().clone(),
            records: BTreeMap::new(),
            signature: String::new(),
        };
        let signed = |manifest: &mut DiscoveryManifest| {
            manifest.signature = key
                .sign(&manifest.signing_message().unwrap())
                .as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            serde_json::to_vec(manifest).unwrap()
        };
        let bytes = signed(&mut manifest);
        let selection = select(&source, root.path(), &bytes).unwrap();
        let crate::ResolverInput::Discovery(pin) = selection.input else {
            panic!()
        };
        let resolver = DiscoveryResolver::with_cache(&pin, root.path().join("downloads"), true);
        assert_eq!(resolver.manifest().unwrap().sequence, 2);
        assert_eq!(
            source.select_offline(root.path()).unwrap().input,
            crate::ResolverInput::Discovery(pin)
        );
        assert!(resolver
            .resolve(&PackageRequest::parse("fd"), &ResolveContext::current())
            .unwrap_err()
            .contains("no published package"));
        manifest.sequence = 1;
        assert!(select(&source, root.path(), &signed(&mut manifest))
            .unwrap_err()
            .contains("rollback"));
        manifest.sequence = 2;
        manifest
            .catalog
            .packages
            .get_mut("fd")
            .unwrap()
            .description
            .push_str(" changed");
        assert!(select(&source, root.path(), &signed(&mut manifest))
            .unwrap_err()
            .contains("conflicting"));
        manifest.sequence = 3;
        assert!(DiscoveryManifest::from_bytes(
            &serde_json::to_vec(&manifest).unwrap(),
            &public_key
        )
        .unwrap_err()
        .contains("signature"));
        assert_eq!(cached(&source, root.path()).unwrap().unwrap(), bytes);
    }
}
