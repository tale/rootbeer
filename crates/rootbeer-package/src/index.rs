pub use rootbeer_catalog::{is_sha256, validate_https, PackageIndexPin};

use super::{LockedInstall, LockedSource};

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
    use crate::{CatalogRecipe, PublishedArtifact};

    /// The published `xz` artifact, renamed so no test depends on the catalog's own entry.
    fn fixture() -> (String, PublishedArtifact, CatalogRecipe) {
        let (catalog, mut artifact) = crate::artifact::fixture();
        artifact.package.name = "new-tool".into();
        let package = &catalog.packages["xz"];
        let recipe = package.versions[&artifact.package.version]
            .for_system("aarch64-linux")
            .unwrap()
            .clone();
        (artifact.package.id(), artifact, recipe)
    }

    #[test]
    fn rejects_unsafe_or_inconsistent_artifacts() {
        let (key, original, recipe) = fixture();
        let revision = original.revision;
        original
            .validate(&key, "aarch64-linux", revision, &recipe)
            .unwrap();
        let unsafe_source = LockedSource::File {
            path: "/etc/passwd".into(),
            sha256: "0".repeat(64),
        };
        for field in ["source", "bins", "output", "revision"] {
            let mut artifact = original.clone();
            match field {
                "source" => artifact.package.source = unsafe_source.clone(),
                "bins" => {
                    *artifact.package.provides.bins.values_mut().next().unwrap() =
                        "../escape".into()
                }
                "output" => artifact.package.output_sha256 = None,
                "revision" => artifact.revision = 99,
                _ => unreachable!(),
            }
            assert!(
                artifact
                    .validate(&key, "aarch64-linux", revision, &recipe)
                    .is_err(),
                "{field}"
            );
        }
    }

    #[test]
    fn artifact_validation_enforces_declared_command_paths() {
        let (key, mut artifact, mut recipe) = fixture();
        let key = &key;
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
        let (key, mut artifact, mut recipe) = fixture();
        let key = &key;
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
