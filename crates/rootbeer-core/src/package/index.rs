use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::download::DownloadCache;
use super::{
    ArchiveFormat, ArtifactIndex, LockedInstall, LockedSource, PackageRequest, PackageResolution,
    PackageResolver, ResolutionProof, ResolveContext,
};

/// An explicitly trusted index URL and SHA-256 of its exact JSON bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIndexPin {
    pub url: String,
    pub sha256: String,
}

impl PackageIndexPin {
    /// Validates an HTTPS index or an explicitly selected absolute local file URL.
    pub fn validate(&self) -> Result<(), String> {
        if !is_sha256(&self.sha256) {
            return Err("package index requires a lowercase SHA-256".into());
        }
        if self.url.starts_with("file:///") && !self.url.contains(['?', '#', '\0']) {
            return Ok(());
        }
        validate_https(&self.url)
    }
}

/// Records which pinned index authorized the selected platform artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedIndexProof {
    pub index: PackageIndexPin,
    pub catalog_sha256: String,
    pub revision: u32,
    pub system: String,
    pub receipt_sha256: String,
}

pub(super) fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

pub(super) fn validate_https(url: &str) -> Result<(), String> {
    let uri: ureq::http::Uri = url.parse().map_err(|e| format!("invalid index URL: {e}"))?;
    if uri.scheme_str() != Some("https")
        || uri.host().is_none_or(str::is_empty)
        || url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '@' | '#' | '\\'))
    {
        return Err(
            "index and artifact URLs must use HTTPS without credentials or fragments".into(),
        );
    }
    Ok(())
}

impl ArtifactIndex {
    /// Validates the catalog and every advertised artifact without executing recipes.
    pub fn validate(&self) -> Result<(), String> {
        self.catalog.validate()?;
        if self.schema != 1
            || self.catalog_sha256 != self.catalog.sha256()
            || self.artifacts.is_empty()
        {
            return Err("invalid artifact index schema, catalog digest, or empty artifacts".into());
        }
        for (key, systems) in &self.artifacts {
            let request = PackageRequest::parse(key);
            if request.resolver.is_some() {
                return Err(format!("{key}: index keys must use canonical name@version"));
            }
            let recipe = self
                .catalog
                .packages
                .get(&request.name)
                .and_then(|entry| {
                    request
                        .version
                        .as_ref()
                        .and_then(|version| entry.versions.get(version))
                })
                .ok_or_else(|| format!("{key}: missing index recipe"))?;
            if systems.is_empty() {
                return Err(format!("{key}: no platform artifacts"));
            }
            for (system, artifact) in systems {
                let package = &artifact.package;
                if package.id() != *key
                    || artifact.revision != recipe.revision
                    || !recipe.systems.contains(system)
                    || !is_sha256(&artifact.receipt_sha256)
                    || package
                        .output_sha256
                        .as_deref()
                        .is_none_or(|hash| !is_sha256(hash))
                    || package.install
                        != (LockedInstall::Archive {
                            format: ArchiveFormat::TarGz,
                            strip_prefix: None,
                        })
                    || package.provides.bins.len() != recipe.bins.len()
                    || recipe.bins.iter().any(|bin| {
                        package.provides.bins.get(bin)
                            != Some(&std::path::PathBuf::from("bin").join(bin))
                    })
                {
                    return Err(format!("{key}: invalid platform artifact contract"));
                }
                let LockedSource::Url { url, sha256 } = &package.source else {
                    return Err(format!(
                        "{key}: index artifacts must use HTTPS or GHCR URLs"
                    ));
                };
                if url.starts_with("ghcr://") {
                    let blob = super::ghcr::GhcrBlob::parse(url)?;
                    if blob.sha256 != *sha256 {
                        return Err(format!("{key}: GHCR digest does not match archive hash"));
                    }
                } else {
                    validate_https(url)?;
                }
                if !is_sha256(sha256) {
                    return Err(format!("{key}: invalid archive SHA-256"));
                }
            }
        }
        Ok(())
    }
}

pub(super) struct IndexResolver {
    pin: PackageIndexPin,
    downloads: DownloadCache,
    index: OnceLock<Result<ArtifactIndex, String>>,
}

impl IndexResolver {
    pub(super) fn new(pin: &PackageIndexPin) -> Self {
        Self {
            pin: pin.clone(),
            downloads: DownloadCache::new(crate::state_dir().join("downloads")),
            index: OnceLock::new(),
        }
    }

    fn index(&self) -> Result<&ArtifactIndex, String> {
        self.index
            .get_or_init(|| {
                self.pin.validate()?;
                let file = self
                    .downloads
                    .materialize(&self.pin.url, Some(&self.pin.sha256))
                    .map_err(|e| e.to_string())?;
                if std::fs::metadata(&file.path)
                    .map_err(|e| e.to_string())?
                    .len()
                    > 16 * 1024 * 1024
                {
                    return Err("package index exceeds 16 MiB".into());
                }
                let bytes = std::fs::read(file.path).map_err(|e| e.to_string())?;
                let index: ArtifactIndex =
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                index.validate()?;
                Ok(index)
            })
            .as_ref()
            .map_err(Clone::clone)
    }
}

impl PackageResolver for IndexResolver {
    fn name(&self) -> &str {
        "rootbeer"
    }

    fn resolve(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<Option<PackageResolution>, String> {
        if request.asset.is_some() || !request.bins.is_empty() {
            return Err("index packages do not accept asset or command overrides".into());
        }
        let index = self.index()?;
        let entry = index
            .catalog
            .find(&request.name)
            .ok_or_else(|| format!("unknown index package `{}`", request.name))?;
        let version = request.version.as_ref().unwrap_or(&entry.default_version);
        let key = format!("{}@{version}", entry.name);
        let artifact = index
            .artifacts
            .get(&key)
            .and_then(|systems| systems.get(&context.system))
            .ok_or_else(|| {
                format!(
                    "{key}: no published artifact for {} in the pinned index",
                    context.system
                )
            })?;
        Ok(Some(PackageResolution::new(
            artifact.package.clone(),
            ResolutionProof::PublishedIndex(PublishedIndexProof {
                index: self.pin.clone(),
                catalog_sha256: index.catalog_sha256.clone(),
                revision: artifact.revision,
                system: context.system.clone(),
                receipt_sha256: artifact.receipt_sha256.clone(),
            }),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::{bundle_artifacts, PackageCatalog};
    use crate::store::hash_bytes;
    use std::{fs, path::Path};

    fn fixture(root: &Path) -> (ArtifactIndex, PackageIndexPin) {
        let (catalog, receipt) = super::super::bundle::tests::fixture(root);
        let output = root.join("bundle");
        bundle_artifacts(&catalog, &[receipt], "https://packages.example", &output).unwrap();
        let mut index: ArtifactIndex =
            serde_json::from_slice(&fs::read(output.join("index.json")).unwrap()).unwrap();
        let mut entry = index.catalog.packages.remove("xz").unwrap();
        entry.name = "new-tool".into();
        entry.aliases = vec!["new-alias".into()];
        let mut platforms = index
            .artifacts
            .remove(&format!("xz@{}", entry.default_version))
            .unwrap();
        for artifact in platforms.values_mut() {
            artifact.package.name = entry.name.clone();
        }
        index
            .artifacts
            .insert(format!("new-tool@{}", entry.default_version), platforms);
        index.catalog.packages.insert(entry.name.clone(), entry);
        index.catalog_sha256 = index.catalog.sha256();
        let bytes = serde_json::to_vec(&index).unwrap();
        let path = root.join("index.json");
        fs::write(&path, &bytes).unwrap();
        (
            index,
            PackageIndexPin {
                url: format!("file://{}", path.display()),
                sha256: hash_bytes(&bytes),
            },
        )
    }

    fn resolver(pin: &PackageIndexPin, root: &Path) -> IndexResolver {
        IndexResolver {
            pin: pin.clone(),
            downloads: DownloadCache::new(root.join("downloads")),
            index: OnceLock::new(),
        }
    }

    #[test]
    fn resolves_new_names_and_aliases_from_verified_cached_index() {
        let root = tempfile::tempdir().unwrap();
        let (_, pin) = fixture(root.path());
        assert!(PackageCatalog::embedded()
            .unwrap()
            .find("new-tool")
            .is_none());
        let resolver = resolver(&pin, root.path());
        let context = ResolveContext::new("aarch64-linux");
        let package = resolver
            .resolve(&PackageRequest::parse("new-alias"), &context)
            .unwrap()
            .unwrap();
        assert_eq!(package.package.name, "new-tool");
        let ResolutionProof::PublishedIndex(proof) = package.proof else {
            panic!("index provenance missing")
        };
        assert_eq!(proof.index, pin);
        fs::remove_file(root.path().join("index.json")).unwrap();
        let cached = IndexResolver {
            pin,
            downloads: DownloadCache::offline(root.path().join("downloads")),
            index: OnceLock::new(),
        };
        assert!(cached
            .resolve(&PackageRequest::parse("new-tool"), &context)
            .unwrap()
            .is_some());
        for request in ["unknown", "age", "new-tool@0", "new-tool@latest"] {
            assert!(cached
                .resolve(&PackageRequest::parse(request), &context)
                .is_err());
        }
        assert!(cached
            .resolve(
                &PackageRequest::parse("new-tool"),
                &ResolveContext::new("x86_64-macos")
            )
            .is_err());
    }

    #[test]
    fn rejects_tampered_index_before_resolution() {
        let root = tempfile::tempdir().unwrap();
        let (_, pin) = fixture(root.path());
        fs::write(root.path().join("index.json"), b"tampered").unwrap();
        assert!(resolver(&pin, root.path())
            .resolve(
                &PackageRequest::parse("new-tool"),
                &ResolveContext::new("aarch64-linux")
            )
            .is_err());
    }

    #[test]
    fn rejects_unsafe_or_inconsistent_index_artifacts() {
        let root = tempfile::tempdir().unwrap();
        let (index, _) = fixture(root.path());
        let original = serde_json::to_value(index).unwrap();
        let key = original["artifacts"]
            .as_object()
            .unwrap()
            .keys()
            .next()
            .unwrap()
            .clone();
        for field in ["source", "bins", "output", "revision", "catalog", "system"] {
            let mut value = original.clone();
            let artifact = &mut value["artifacts"][&key]["aarch64-linux"];
            match field {
                "source" => {
                    artifact["package"]["source"] = serde_json::json!({"File": {"path": "/etc/passwd", "sha256": "0".repeat(64)}})
                }
                "bins" => {
                    artifact["package"]["provides"]["bins"]["xz"] = serde_json::json!("../escape")
                }
                "output" => artifact["package"]["output_sha256"] = serde_json::Value::Null,
                "revision" => artifact["revision"] = serde_json::json!(99),
                "catalog" => value["catalog_sha256"] = serde_json::json!("0".repeat(64)),
                "system" => {
                    let artifact = artifact.clone();
                    value["artifacts"][&key]["windows"] = artifact;
                }
                _ => unreachable!(),
            }
            let index: ArtifactIndex = serde_json::from_value(value).unwrap();
            assert!(index.validate().is_err(), "{field}");
        }
    }
}
