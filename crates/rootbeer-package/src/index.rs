use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::download::DownloadCache;
use super::{
    ArtifactIndex, LockedInstall, LockedSource, PackageRequest, PackageResolution, PackageResolver,
    ResolutionProof, ResolveContext,
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

pub fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

pub fn validate_https(url: &str) -> Result<(), String> {
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
    /// Requires an artifact for every declared version and platform before publication.
    pub fn validate_complete(&self) -> Result<(), String> {
        self.validate()?;
        for package in self.catalog.packages.values() {
            for (version, recipe) in &package.versions {
                let key = format!("{}@{version}", package.name);
                for system in &recipe.systems {
                    if !self
                        .artifacts
                        .get(&key)
                        .is_some_and(|systems| systems.contains_key(system))
                    {
                        return Err(format!("incomplete publication: missing {key} on {system}"));
                    }
                }
            }
        }
        Ok(())
    }

    /// Validates the catalog and every advertised artifact without executing recipes.
    pub fn validate(&self) -> Result<(), String> {
        if self.artifacts.is_empty() {
            return Err("empty artifact index".into());
        }
        self.validate_fragment()
    }

    pub fn schema_for(catalog: &super::PackageCatalog) -> u32 {
        if catalog.packages.values().any(|package| {
            package.versions.values().any(|recipe| {
                recipe.build.as_ref().is_some_and(|build| {
                    build
                        .dependencies
                        .iter()
                        .any(|dependency| dependency.kind().is_runtime())
                })
            })
        }) {
            return 6;
        }
        if catalog.packages.values().any(|package| {
            package.versions.values().any(|recipe| {
                recipe.build.as_ref().is_some_and(|build| {
                    build.dependencies.iter().any(|dependency| {
                        matches!(dependency, super::BuildDependency::Scoped { .. })
                    })
                })
            })
        }) {
            return 5;
        }
        if catalog.packages.values().any(|package| {
            package.versions.values().any(|recipe| {
                recipe.build.as_ref().is_some_and(|build| {
                    !build.libraries.is_empty()
                        || matches!(build.backend, super::BuildBackend::Custom)
                })
            })
        }) {
            return 4;
        }
        if catalog.packages.values().any(|package| {
            package
                .versions
                .values()
                .any(|recipe| !recipe.apps.is_empty())
        }) {
            3
        } else {
            2
        }
    }

    pub fn validate_fragment(&self) -> Result<(), String> {
        self.catalog.validate()?;
        if !matches!(self.schema, 1..=6) || self.catalog_sha256 != self.catalog.sha256() {
            return Err("invalid artifact index schema or catalog digest".into());
        }
        if self.schema < 6 && Self::schema_for(&self.catalog) == 6 {
            return Err("runtime dependencies require artifact index schema 6".into());
        }
        if self.schema < 5 && Self::schema_for(&self.catalog) == 5 {
            return Err("scoped build dependencies require artifact index schema 5".into());
        }
        if self.schema < 4 && Self::schema_for(&self.catalog) == 4 {
            return Err(
                "library dependencies and command builds require artifact index schema 4".into(),
            );
        }
        if self.schema < 3 && Self::schema_for(&self.catalog) == 3 {
            return Err("application exports require artifact index schema 3".into());
        }
        if self.schema == 1
            && self.catalog.packages.values().any(|package| {
                package.versions.values().any(|recipe| {
                    !recipe.bin_paths.is_empty()
                        || !recipe.checksums.is_empty()
                        || recipe.mirror
                        || recipe.build.as_ref().is_some_and(|build| {
                            matches!(build.backend, super::BuildBackend::Zig)
                                || !build.args.is_empty()
                                || !build.patches.is_empty()
                        })
                })
            })
        {
            return Err("extended package recipes require artifact index schema 2".into());
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
                artifact.validate(key, system, recipe)?;
                for dependency in super::runtime::closure(&artifact.package)? {
                    if self.schema < 6 {
                        return Err("runtime dependencies require artifact index schema 6".into());
                    }
                    let (_, _, recipe) =
                        super::graph::find_recipe(&self.catalog, &dependency.id())?;
                    super::PublishedArtifact {
                        revision: recipe.revision,
                        receipt_sha256: artifact.receipt_sha256.clone(),
                        package: dependency.clone(),
                    }
                    .validate(&dependency.id(), system, recipe)?;
                }
            }
        }
        Ok(())
    }
}

impl super::PublishedArtifact {
    pub fn validate(
        &self,
        key: &str,
        system: &str,
        recipe: &super::CatalogRecipe,
    ) -> Result<(), String> {
        let package = &self.package;
        let runtime: std::collections::BTreeSet<_> = recipe
            .build
            .iter()
            .flat_map(|build| &build.dependencies)
            .filter(|dependency| dependency.kind().is_runtime())
            .map(|dependency| dependency.package())
            .collect();
        if runtime
            != package
                .runtime_dependencies
                .keys()
                .map(String::as_str)
                .collect()
        {
            return Err(format!("{key}: runtime dependencies differ from recipe"));
        }
        if package.id() != key
            || self.revision != recipe.revision
            || !recipe.systems.iter().any(|target| target == system)
            || !is_sha256(&self.receipt_sha256)
            || package
                .output_sha256
                .as_deref()
                .is_none_or(|hash| !is_sha256(hash))
            || package.provides.bins.len() != recipe.bins.len()
            || recipe
                .bins
                .iter()
                .any(|bin| !package.provides.bins.contains_key(bin))
        {
            return Err(format!("{key}: invalid platform artifact contract"));
        }
        if package.provides.apps != recipe.apps {
            return Err(format!(
                "{key}: artifact application paths differ from the recipe"
            ));
        }
        super::catalog::validate_apps(&package.provides.apps)?;
        if !recipe.bin_paths.is_empty() && package.provides.bins != recipe.bin_paths {
            return Err(format!(
                "{key}: artifact command paths differ from the recipe"
            ));
        }
        for path in package.provides.bins.values() {
            super::realize::validate_relative_path("index command", path)
                .map_err(|e| e.to_string())?;
        }
        match &package.install {
            LockedInstall::Archive { strip_prefix, .. } => {
                if let Some(path) = strip_prefix {
                    super::realize::validate_relative_path("index archive prefix", path)
                        .map_err(|e| e.to_string())?;
                }
            }
            LockedInstall::Binary { path } => {
                super::realize::validate_relative_path("index binary", path)
                    .map_err(|e| e.to_string())?;
            }
            LockedInstall::Directory { .. } => {
                return Err("index artifacts cannot install local directories".into())
            }
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
        if recipe.mirror && !url.starts_with("ghcr://") {
            return Err(format!("{key}: mirrored artifacts must use GHCR"));
        }
        if !recipe.mirror
            && recipe
                .checksums
                .get(system)
                .is_some_and(|expected| expected != sha256)
        {
            return Err(format!("{key}: artifact checksum differs from the recipe"));
        }
        Ok(())
    }
}

pub struct IndexResolver {
    pin: PackageIndexPin,
    downloads: DownloadCache,
    index: OnceLock<Result<ArtifactIndex, String>>,
}

impl IndexResolver {
    pub fn new(pin: &PackageIndexPin) -> Self {
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
        let version = request
            .version
            .as_deref()
            .unwrap_or_else(|| entry.default_version_for(&context.system));
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
    use crate::store::hash_bytes;
    use crate::PackageCatalog;
    use std::{fs, path::Path};

    fn fixture(root: &Path) -> (ArtifactIndex, PackageIndexPin) {
        let mut index = crate::artifact::fixture();
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
    fn application_contracts_require_schema_three_and_exact_artifact_paths() {
        let root = tempfile::tempdir().unwrap();
        let (mut index, _) = fixture(root.path());
        let package = index.catalog.packages.get_mut("new-tool").unwrap();
        let recipe = package.versions.get_mut(&package.default_version).unwrap();
        recipe.systems = vec!["aarch64-macos".into()];
        recipe.apps.insert("Tool.app".into(), "Tool.app".into());
        let key = format!("new-tool@{}", package.default_version);
        let mut artifact = index.artifacts[&key]["aarch64-linux"].clone();
        artifact.package.provides.apps = recipe.apps.clone();
        index.artifacts = std::collections::BTreeMap::from([(
            key.clone(),
            std::collections::BTreeMap::from([("aarch64-macos".into(), artifact)]),
        )]);
        index.catalog_sha256 = index.catalog.sha256();
        for schema in [1, 2] {
            index.schema = schema;
            assert!(index.validate().unwrap_err().contains("schema 3"));
        }
        index.schema = 3;
        index.validate().unwrap();
        let mut decoded: ArtifactIndex =
            serde_json::from_slice(&serde_json::to_vec(&index).unwrap()).unwrap();
        decoded.validate().unwrap();
        decoded
            .artifacts
            .get_mut(&key)
            .unwrap()
            .get_mut("aarch64-macos")
            .unwrap()
            .package
            .provides
            .apps
            .clear();
        assert!(decoded
            .validate()
            .unwrap_err()
            .contains("application paths"));
    }

    #[test]
    fn published_index_uses_platform_default_without_changing_exact_pins() {
        let root = tempfile::tempdir().unwrap();
        let (mut index, _) = fixture(root.path());
        let entry = index.catalog.packages.get_mut("new-tool").unwrap();
        let original = entry.default_version.clone();
        entry
            .versions
            .insert("99.0.0".into(), entry.versions[&original].clone());
        entry.default_version = "99.0.0".into();
        entry
            .default_versions
            .insert("aarch64-linux".into(), original.clone());
        index.catalog_sha256 = index.catalog.sha256();
        let bytes = serde_json::to_vec(&index).unwrap();
        let path = root.path().join("platform-index.json");
        fs::write(&path, &bytes).unwrap();
        let pin = PackageIndexPin {
            url: format!("file://{}", path.display()),
            sha256: hash_bytes(&bytes),
        };
        let resolver = resolver(&pin, root.path());
        let context = ResolveContext::new("aarch64-linux");
        let result = resolver
            .resolve(&PackageRequest::parse("new-alias"), &context)
            .unwrap()
            .unwrap();
        assert_eq!(result.package.version, original);
        assert!(resolver
            .resolve(&PackageRequest::parse("new-alias@99.0.0"), &context)
            .unwrap_err()
            .contains("no published artifact"));
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

    #[test]
    fn artifact_validation_enforces_declared_command_paths() {
        let root = tempfile::tempdir().unwrap();
        let (index, _) = fixture(root.path());
        let (key, platforms) = index.artifacts.iter().next().unwrap();
        let mut artifact = platforms["aarch64-linux"].clone();
        let package = &index.catalog.packages["new-tool"];
        let mut recipe = package.versions[&package.default_version].clone();
        recipe.build = None;
        recipe.source = Some("github:owner/tool@v1".into());
        recipe.bin_paths = artifact.package.provides.bins.clone();
        recipe.validate().unwrap();
        artifact.validate(key, "aarch64-linux", &recipe).unwrap();

        *artifact.package.provides.bins.values_mut().next().unwrap() = "bin/other".into();
        assert!(artifact
            .validate(key, "aarch64-linux", &recipe)
            .unwrap_err()
            .contains("command paths differ"));

        recipe.bin_paths.clear();
        artifact.validate(key, "aarch64-linux", &recipe).unwrap();
    }

    #[test]
    fn artifact_validation_checks_platform_pins_without_comparing_mirror_archive_to_upstream() {
        let root = tempfile::tempdir().unwrap();
        let (index, _) = fixture(root.path());
        let (key, platforms) = index.artifacts.iter().next().unwrap();
        let mut artifact = platforms["aarch64-linux"].clone();
        let package = &index.catalog.packages["new-tool"];
        let mut recipe = package.versions[&package.default_version].clone();
        recipe.build = None;
        recipe.source = Some("github:owner/tool@v1".into());
        let LockedSource::Url { sha256, .. } = &artifact.package.source else {
            panic!("expected URL source");
        };
        recipe.checksums = recipe
            .systems
            .iter()
            .map(|system| (system.clone(), "b".repeat(64)))
            .collect();
        recipe
            .checksums
            .insert("aarch64-linux".into(), sha256.clone());
        recipe.validate().unwrap();
        artifact.validate(key, "aarch64-linux", &recipe).unwrap();
        assert!(artifact
            .validate(key, "x86_64-linux", &recipe)
            .unwrap_err()
            .contains("checksum differs"));

        recipe.mirror = true;
        assert!(artifact
            .validate(key, "aarch64-linux", &recipe)
            .unwrap_err()
            .contains("mirrored artifacts must use GHCR"));
        artifact.package.source = LockedSource::Url {
            url: format!("ghcr://owner/tool@sha256:{}", "c".repeat(64)),
            sha256: "c".repeat(64),
        };
        artifact.validate(key, "aarch64-linux", &recipe).unwrap();
        recipe.mirror = false;
        assert!(artifact
            .validate(key, "aarch64-linux", &recipe)
            .unwrap_err()
            .contains("checksum differs"));
    }

    #[test]
    fn reads_legacy_indexes_but_rejects_extended_recipes_under_schema_one() {
        let root = tempfile::tempdir().unwrap();
        let (mut index, _) = fixture(root.path());
        assert_eq!(index.schema, 2);
        index.schema = 1;
        index.validate().unwrap();
        let original = serde_json::to_value(&index).unwrap();

        for feature in ["bin_paths", "checksums", "mirror", "zig", "args", "patches"] {
            let mut index: ArtifactIndex = serde_json::from_value(original.clone()).unwrap();
            let package = index.catalog.packages.get_mut("new-tool").unwrap();
            let recipe = package.versions.get_mut(&package.default_version).unwrap();
            if matches!(feature, "bin_paths" | "checksums" | "mirror") {
                recipe.build = None;
                recipe.source = Some("github:owner/tool@v1".into());
            }
            match feature {
                "bin_paths" => {
                    recipe.bin_paths = recipe
                        .bins
                        .iter()
                        .map(|bin| (bin.clone(), "bin/tool".into()))
                        .collect()
                }
                "checksums" | "mirror" => {
                    recipe.checksums = recipe
                        .systems
                        .iter()
                        .map(|system| (system.clone(), "a".repeat(64)))
                        .collect();
                    recipe.mirror = feature == "mirror";
                }
                "zig" | "args" => {
                    let build = recipe.build.as_mut().unwrap();
                    build.backend = super::super::BuildBackend::Zig;
                    build.configure.clear();
                    if feature == "args" {
                        build.args.push("-Doptimize=ReleaseFast".into());
                    }
                }
                "patches" => recipe
                    .build
                    .as_mut()
                    .unwrap()
                    .patches
                    .push("patch contents".into()),
                _ => unreachable!(),
            }
            index.catalog_sha256 = index.catalog.sha256();
            index.artifacts.clear();
            assert!(
                index.validate_fragment().unwrap_err().contains("schema 2"),
                "{feature}"
            );
            index.schema = 2;
            index.validate_fragment().unwrap();
        }

        for schema in [0, 7] {
            index.schema = schema;
            assert!(index.validate().unwrap_err().contains("schema"));
        }
    }
}
