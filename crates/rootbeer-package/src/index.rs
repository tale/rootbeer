pub use rootbeer_catalog::{is_sha256, validate_https, PackageIndexPin};

use super::{ArtifactIndex, LockedInstall, LockedSource, PackageRequest};

impl ArtifactIndex {
    /// Requires artifacts for prebuilt-only recipes; source recipes can be built by consumers.
    pub fn validate_complete(&self) -> Result<(), String> {
        self.validate()?;
        for package in self.catalog.packages.values() {
            for (version, entry) in &package.versions {
                let key = format!("{}@{version}", package.name);
                for (system, recipe) in &entry.platforms {
                    if recipe.build.is_some() {
                        continue;
                    }
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
        if self.artifacts.is_empty()
            && !self.catalog.packages.values().any(|package| {
                package
                    .versions
                    .values()
                    .flat_map(|entry| entry.platforms.values())
                    .any(|recipe| recipe.build.is_some())
            })
        {
            return Err("empty artifact index".into());
        }
        self.validate_fragment()
    }

    pub fn validate_fragment(&self) -> Result<(), String> {
        self.catalog.validate()?;
        if self.catalog_sha256 != self.catalog.sha256() {
            return Err("invalid artifact index catalog digest".into());
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
                let platform = recipe
                    .for_system(system)
                    .ok_or_else(|| format!("{key}: {system} has no recipe"))?;
                artifact.validate(key, system, recipe.revision, platform)?;
                for dependency in super::runtime::closure(&artifact.package)? {
                    let (_, _, entry) =
                        super::graph::find_recipe_definition(&self.catalog, &dependency.id())?;
                    let dependency_recipe = entry
                        .for_system(system)
                        .ok_or_else(|| format!("{}: {system} has no recipe", dependency.id()))?;
                    super::PublishedArtifact {
                        revision: entry.revision,
                        receipt_sha256: artifact.receipt_sha256.clone(),
                        package: dependency.clone(),
                    }
                    .validate(
                        &dependency.id(),
                        system,
                        entry.revision,
                        dependency_recipe,
                    )?;
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
        revision: u32,
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
            || self.revision != revision
            || !is_sha256(&self.receipt_sha256)
            || package
                .output_sha256
                .as_deref()
                .is_none_or(|hash| !is_sha256(hash))
            || package.provides.bins.len() != recipe.bins.names().len()
            || recipe
                .bins
                .names()
                .iter()
                .any(|bin| !package.provides.bins.contains_key(*bin))
        {
            return Err(format!("{key}: invalid platform artifact contract"));
        }
        if package.provides.apps != recipe.apps {
            return Err(format!(
                "{key}: artifact application paths differ from the recipe"
            ));
        }
        super::catalog::validate_apps(&package.provides.apps)?;
        if recipe
            .bins
            .paths()
            .is_some_and(|paths| &package.provides.bins != paths)
        {
            return Err(format!(
                "{key}: artifact command paths differ from the recipe"
            ));
        }
        for path in package.provides.bins.values() {
            super::realize::validate_relative_path("index command", path)
                .map_err(|e| e.to_string())?;
        }
        match &package.install {
            LockedInstall::Dmg => {
                if !system.ends_with("-macos") || package.provides.apps.is_empty() {
                    return Err(
                        "DMG artifacts require macOS and declared application bundles".into(),
                    );
                }
            }
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
        if !recipe.mirror
            && recipe
                .source
                .as_deref()
                .is_some_and(|source| source.starts_with("https://"))
            && (recipe.source.as_ref() != Some(url)
                || recipe.install.as_ref() != Some(&package.install))
        {
            return Err(format!(
                "{key}: download differs from the approved URL or install format"
            ));
        }
        if recipe.mirror && !url.starts_with("ghcr://") {
            return Err(format!("{key}: mirrored artifacts must use GHCR"));
        }
        if !recipe.mirror
            && recipe
                .sha256
                .as_deref()
                .is_some_and(|expected| expected != sha256)
        {
            return Err(format!("{key}: artifact checksum differs from the recipe"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::hash_bytes;
    use std::{fs, path::Path};

    fn fixture(root: &Path) -> (ArtifactIndex, PackageIndexPin) {
        let mut index = crate::artifact::fixture();
        let mut entry = index.catalog.packages.remove("xz").unwrap();
        entry.name = "new-tool".into();
        entry.aliases = vec!["new-alias".into()];
        let mut platforms = index
            .artifacts
            .remove(&format!(
                "xz@{}",
                entry.default_version_for("aarch64-linux").unwrap()
            ))
            .unwrap();
        for artifact in platforms.values_mut() {
            artifact.package.name = entry.name.clone();
        }
        index.artifacts.insert(
            format!(
                "new-tool@{}",
                entry.default_version_for("aarch64-linux").unwrap()
            ),
            platforms,
        );
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
        let mut recipe = package.versions[package.default_version_for("aarch64-linux").unwrap()]
            .for_system("aarch64-linux")
            .unwrap()
            .clone();
        recipe.build = None;
        recipe.source = Some("github:owner/tool@v1".into());
        recipe.bins = crate::Bins::Paths(artifact.package.provides.bins.clone());
        recipe.asset = Some("tool-v1-linux.tar.gz".into());
        recipe.validate("aarch64-linux").unwrap();
        artifact
            .validate(key, "aarch64-linux", artifact.revision, &recipe)
            .unwrap();

        *artifact.package.provides.bins.values_mut().next().unwrap() = "bin/other".into();
        assert!(artifact
            .validate(key, "aarch64-linux", artifact.revision, &recipe)
            .unwrap_err()
            .contains("command paths differ"));
    }

    #[test]
    fn artifact_validation_checks_platform_pins_without_comparing_mirror_archive_to_upstream() {
        let root = tempfile::tempdir().unwrap();
        let (index, _) = fixture(root.path());
        let (key, platforms) = index.artifacts.iter().next().unwrap();
        let mut artifact = platforms["aarch64-linux"].clone();
        let package = &index.catalog.packages["new-tool"];
        let mut recipe = package.versions[package.default_version_for("aarch64-linux").unwrap()]
            .for_system("aarch64-linux")
            .unwrap()
            .clone();
        recipe.build = None;
        recipe.source = Some("github:owner/tool@v1".into());
        let LockedSource::Url { sha256, .. } = &artifact.package.source else {
            panic!("expected URL source");
        };
        recipe.sha256 = Some(sha256.clone());
        recipe.asset = Some("tool-v1-linux.tar.gz".into());
        recipe.validate("aarch64-linux").unwrap();
        artifact
            .validate(key, "aarch64-linux", artifact.revision, &recipe)
            .unwrap();

        recipe.sha256 = Some("b".repeat(64));
        assert!(artifact
            .validate(key, "aarch64-linux", artifact.revision, &recipe)
            .unwrap_err()
            .contains("checksum differs"));
        recipe.sha256 = Some(sha256.clone());

        recipe.mirror = true;
        assert!(artifact
            .validate(key, "aarch64-linux", artifact.revision, &recipe)
            .unwrap_err()
            .contains("mirrored artifacts must use GHCR"));
        artifact.package.source = LockedSource::Url {
            url: format!("ghcr://owner/tool@sha256:{}", "c".repeat(64)),
            sha256: "c".repeat(64),
        };
        artifact
            .validate(key, "aarch64-linux", artifact.revision, &recipe)
            .unwrap();
        recipe.mirror = false;
        assert!(artifact
            .validate(key, "aarch64-linux", artifact.revision, &recipe)
            .unwrap_err()
            .contains("checksum differs"));
    }
}
