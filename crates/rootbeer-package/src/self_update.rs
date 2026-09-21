//! Side channel that resolves Rootbeer itself, and nothing else.
//!
//! `rb self-update` is how a client recovers, so it cannot depend on this build understanding
//! every recipe the catalog happens to carry: one unknown field in an unrelated package would
//! otherwise leave an older client with no upgrade path at all.
//!
//! This channel narrows only what is decoded. It does not relax trust. The publisher signature
//! is still verified over the entire signed document, the package record is still verified and
//! still bound to the recipe the catalog publishes, and the channel can resolve no package
//! other than [`PACKAGE`].

use std::path::PathBuf;

use crate::discovery::{verify_signed, DiscoveryPin};
use crate::distribution::{verify_record, PackageRecord};
use crate::download::DownloadCache;
use crate::{
    CatalogRecipe, PackageIndexPin, PackageRequest, PackageResolution, PackageResolver,
    ResolutionProof, ResolveContext,
};

/// The only package this channel will ever resolve.
pub const PACKAGE: &str = "rootbeer";

/// One signed discovery entry for [`PACKAGE`], read without decoding the rest of the catalog.
#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub recipe: CatalogRecipe,
    pub pin: PackageIndexPin,
}

impl Entry {
    pub fn read(
        bytes: &[u8],
        public_key: &str,
        version: Option<&str>,
        system: &str,
    ) -> Result<Self, String> {
        let value = crate::discovery::parse_manifest(bytes)?;
        verify_signed(bytes, public_key)?;
        let package = &value["catalog"]["packages"][PACKAGE];
        if package["name"].as_str() != Some(PACKAGE) {
            return Err(format!("unknown package {PACKAGE}"));
        }

        let version = match version {
            Some(version) => version.to_string(),
            None => package["default_versions"][system]
                .as_str()
                .or_else(|| package["default_version"].as_str())
                .ok_or_else(|| format!("{PACKAGE} has no default version"))?
                .to_string(),
        };
        let id = format!("{PACKAGE}@{version}");
        let recipe: CatalogRecipe =
            serde_json::from_value(package["versions"][&version].clone())
                .map_err(|error| format!("{id} is not readable by this build: {error}"))?;
        recipe.validate()?;
        if !recipe
            .supported_systems()
            .iter()
            .any(|value| *value == system)
        {
            return Err(format!("{id}: unapproved discovery platform {system}"));
        }

        let pin: PackageIndexPin = serde_json::from_value(value["records"][&id][system].clone())
            .map_err(|_| format!("{id} has no published package for {system}"))?;
        crate::index::validate_https(&pin.url)?;
        pin.validate()?;
        Ok(Self { id, recipe, pin })
    }
}

/// Resolves [`PACKAGE`] from signed discovery, and refuses every other request.
pub struct Resolver {
    pin: DiscoveryPin,
    downloads: DownloadCache,
}

impl Resolver {
    pub fn new(pin: &DiscoveryPin, cache: impl Into<PathBuf>) -> Self {
        Self {
            pin: pin.clone(),
            downloads: DownloadCache::new(cache),
        }
    }

    fn read(&self, pin: &PackageIndexPin, limit: usize) -> Result<Vec<u8>, String> {
        let path = self
            .downloads
            .materialize_verified(&pin.url, &pin.sha256)
            .map_err(|error| error.to_string())?;
        if std::fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len()
            > limit as u64
        {
            return Err("package metadata exceeds size limit".into());
        }
        std::fs::read(path).map_err(|error| error.to_string())
    }

    pub fn record(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<(PackageRecord, PackageIndexPin), String> {
        self.pin.manifest.validate()?;
        let manifest = self.read(&self.pin.manifest, crate::discovery::MANIFEST_LIMIT)?;
        let entry = Entry::read(
            &manifest,
            &self.pin.public_key,
            request.version.as_deref(),
            &context.system,
        )?;
        let bytes = self.read(&entry.pin, crate::distribution::RECORD_LIMIT)?;
        let record = verify_record(&bytes, &self.pin.public_key, &entry.id, &context.system)?;
        if entry.recipe.for_system(&context.system) != record.recipe {
            return Err("package record differs from the discovered recipe".into());
        }
        Ok((record, entry.pin))
    }
}

impl PackageResolver for Resolver {
    fn name(&self) -> &str {
        "rootbeer"
    }

    fn resolve(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<Option<PackageResolution>, String> {
        if request.name != PACKAGE {
            return Err(format!("self-update resolves {PACKAGE} only"));
        }
        if request.source.is_some() || request.asset.is_some() || !request.bins.is_empty() {
            return Err("self-update does not accept source, asset, or command overrides".into());
        }
        let (record, pin) = self.record(request, context)?;
        Ok(Some(PackageResolution::new(
            record.artifact.package,
            ResolutionProof::PackageRecord(crate::discovery::PackageRecordProof {
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
    use serde_json::json;

    fn signed_manifest(key: &Ed25519KeyPair, system: &str) -> Vec<u8> {
        let mut catalog = serde_json::to_value(crate::test_catalog::catalog()).unwrap();
        let mut entry = catalog["packages"]["age"].clone();
        entry["name"] = json!(PACKAGE);
        catalog["packages"][PACKAGE] = entry;
        catalog["packages"]["fd"]["versions"]["10.5.0"]["install"] = json!("Pkg");

        let records = json!({
            format!("{PACKAGE}@1.3.1"): {
                system: { "url": "https://example.com/records/r.json", "sha256": "a".repeat(64) },
            },
        });
        let mut value = json!({
            "schema": 2,
            "sequence": 9,
            "catalog": catalog,
            "records": records,
            "signature": "",
        });
        value["signature"] = json!(key
            .sign(
                &crate::discovery::signing_message(9, &value["catalog"], &value["records"])
                    .unwrap()
            )
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>());
        serde_json::to_vec(&value).unwrap()
    }

    #[test]
    fn reads_rootbeer_from_a_manifest_this_build_cannot_fully_decode() {
        let key = Ed25519KeyPair::from_seed_unchecked(&[11; 32]).unwrap();
        let public_key: String = key
            .public_key()
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let system = ResolveContext::current().system;
        let bytes = signed_manifest(&key, &system);

        assert!(
            crate::discovery::DiscoveryManifest::from_bytes(&bytes, &public_key)
                .unwrap_err()
                .contains("unknown variant `Pkg`")
        );

        let entry = Entry::read(&bytes, &public_key, None, &system).unwrap();
        assert_eq!(entry.id, format!("{PACKAGE}@1.3.1"));
        assert_eq!(entry.pin.url, "https://example.com/records/r.json");
    }

    #[test]
    fn rejects_a_tampered_record_pin() {
        let key = Ed25519KeyPair::from_seed_unchecked(&[11; 32]).unwrap();
        let public_key: String = key
            .public_key()
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let system = ResolveContext::current().system;
        let mut value: serde_json::Value =
            serde_json::from_slice(&signed_manifest(&key, &system)).unwrap();
        value["records"][format!("{PACKAGE}@1.3.1")][&system]["sha256"] = json!("b".repeat(64));

        assert!(Entry::read(
            &serde_json::to_vec(&value).unwrap(),
            &public_key,
            None,
            &system
        )
        .unwrap_err()
        .contains("signature verification failed"));
    }
}
