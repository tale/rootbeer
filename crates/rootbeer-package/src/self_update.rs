//! Side channel that resolves Rootbeer itself, and nothing else.
//!
//! `rb self-update` is how a client recovers, so it cannot depend on this build understanding
//! every package the root happens to list: one package using something newer would otherwise
//! leave an older client with no upgrade path at all.
//!
//! This channel narrows only what is decoded. It does not relax trust. The root signature is
//! still verified over the entire document, the package document is still checked against its
//! digest, the record is still verified and bound to the recipe that document publishes, and
//! the channel can resolve no package other than [`PACKAGE`].

use std::path::PathBuf;

use crate::pdr::RootPackage;
use crate::{
    PackageRecordProof, PackageRequest, PackageResolution, PackageResolver, RepositoryPin,
    RepositoryResolver, ResolutionProof, ResolveContext,
};

/// The only package this channel will ever resolve.
pub const PACKAGE: &str = "rootbeer";

/// [`PACKAGE`]'s entry in a verified root, decoded without touching any other package.
pub fn entry(root: &serde_json::Value) -> Result<RootPackage, String> {
    let entry = root["packages"]
        .get(PACKAGE)
        .ok_or_else(|| format!("unknown package {PACKAGE}"))?;
    let package: RootPackage = serde_json::from_value(entry.clone())
        .map_err(|error| format!("{PACKAGE} is not readable by this build: {error}"))?;
    package.validate()?;
    Ok(package)
}

/// Resolves [`PACKAGE`] from a pinned root, and refuses every other request.
pub struct Resolver {
    repository: RepositoryResolver,
    public_key: String,
}

impl Resolver {
    pub fn new(pin: &RepositoryPin, cache: impl Into<PathBuf>) -> Self {
        Self {
            repository: RepositoryResolver::with_cache(pin, cache, false),
            public_key: pin.public_key.clone(),
        }
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
        let package = entry(&self.repository.verified_root()?)?;
        let (record, digest) =
            self.repository
                .record_of(PACKAGE, &package, request.version.as_deref(), context)?;
        Ok(Some(PackageResolution::new(
            record.artifact.package,
            ResolutionProof::PackageRecord(PackageRecordProof {
                record: digest,
                public_key: self.public_key.clone(),
                system: context.system.clone(),
            }),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn root() -> serde_json::Value {
        let package = json!({
            "description": "Declarative system configuration",
            "homepage": "https://example.com",
            "license": "MIT",
            "added": 1,
            "updated": 2,
            "platforms": {
                "aarch64-macos": { "version": "1.0.0", "kind": "command", "commands": ["rb"] },
            },
            "document": "a".repeat(64),
        });
        let mut unreadable = package.clone();
        unreadable["platforms"]["aarch64-macos"]["kind"] = json!("font");
        json!({
            "schema": 3,
            "sequence": 1,
            "packages": { PACKAGE: package, "newer": unreadable },
            "signature": "",
        })
    }

    #[test]
    fn reads_rootbeer_from_a_root_this_build_cannot_fully_decode() {
        let root = root();
        assert!(serde_json::from_value::<crate::pdr::Root>(root.clone())
            .unwrap_err()
            .to_string()
            .contains("font"));
        assert_eq!(
            entry(&root).unwrap().platforms["aarch64-macos"].version,
            "1.0.0"
        );
    }

    #[test]
    fn an_unreadable_rootbeer_entry_is_named() {
        let mut root = root();
        root["packages"][PACKAGE]["platforms"]["aarch64-macos"]["kind"] = json!("font");
        let error = entry(&root).unwrap_err();
        assert!(error.contains("not readable by this build"), "{error}");
    }
}
