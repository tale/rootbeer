use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::valid_name;
use crate::package::PackageRequest;

/// The upstream identity and approved versions of a canonical package.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPackage {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: String,
    pub homepage: String,
    pub default_version: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub default_versions: BTreeMap<String, String>,
    pub versions: BTreeMap<String, CatalogRecipe>,
}

/// An explicit backend request and its platform and command contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogRecipe {
    pub revision: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<crate::package::SourceBuild>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<String, String>,
    pub systems: Vec<String>,
    pub bins: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bin_paths: BTreeMap<String, PathBuf>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub checksums: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub mirror: bool,
    pub checks: Vec<Vec<String>>,
}

fn is_false(value: &bool) -> bool {
    !value
}

impl CatalogPackage {
    /// Returns the platform override or the package-wide default.
    pub fn default_version_for(&self, system: &str) -> &str {
        self.default_versions
            .get(system)
            .map(String::as_str)
            .unwrap_or(&self.default_version)
    }
}

impl CatalogRecipe {
    pub(in crate::package) fn validate(&self) -> Result<(), String> {
        if self.revision == 0 || self.source.is_some() == self.build.is_some() {
            return Err("recipe needs a revision and exactly one of source or build".into());
        }
        if let Some(build) = &self.build {
            build.validate()?;
        }
        if let Some(source) = &self.source {
            let request = PackageRequest::parse(source);
            if !matches!(request.resolver.as_deref(), Some("aqua" | "github"))
                || request
                    .version
                    .as_deref()
                    .is_none_or(|version| version == "latest")
                || !request
                    .name
                    .split_once('/')
                    .is_some_and(|(owner, repo)| !owner.is_empty() && !repo.is_empty())
            {
                return Err("recipe needs a revision and an exact aqua: or github: source".into());
            }
        }
        validate_systems(&self.systems)?;
        if !self.assets.is_empty()
            && (self
                .source
                .as_deref()
                .is_none_or(|source| !source.starts_with("github:"))
                || self.assets.len() != self.systems.len()
                || self.assets.iter().any(|(system, asset)| {
                    !self.systems.contains(system) || asset.trim().is_empty()
                }))
        {
            return Err("assets must name one GitHub release asset per declared system".into());
        }
        validate_commands(&self.bins, &self.checks)?;
        if (!self.bin_paths.is_empty() || !self.checksums.is_empty() || self.mirror)
            && self
                .source
                .as_deref()
                .is_none_or(|source| !source.starts_with("github:"))
        {
            return Err("bin_paths, checksums, and mirror require a GitHub source".into());
        }
        validate_bin_paths(&self.bins, &self.bin_paths)?;
        if !self.checksums.is_empty()
            && (self.checksums.len() != self.systems.len()
                || self.checksums.iter().any(|(system, digest)| {
                    !self.systems.contains(system)
                        || digest.len() != 64
                        || !digest
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                }))
        {
            return Err(
                "checksums must pin each declared platform with a lowercase SHA-256 digest".into(),
            );
        }
        if self.mirror && self.checksums.is_empty() {
            return Err("mirrored recipes require complete platform checksums".into());
        }
        Ok(())
    }
}

pub(in crate::package) fn validate_systems(declared: &[String]) -> Result<(), String> {
    let systems: BTreeSet<_> = declared.iter().collect();
    if systems.is_empty()
        || systems.len() != declared.len()
        || systems.iter().any(|system| {
            !matches!(
                system.as_str(),
                "aarch64-macos" | "x86_64-macos" | "aarch64-linux" | "x86_64-linux"
            )
        })
    {
        return Err("recipe needs unique supported systems".into());
    }
    Ok(())
}

pub(in crate::package) fn validate_commands(
    declared: &[String],
    checks: &[Vec<String>],
) -> Result<(), String> {
    let bins: BTreeSet<_> = declared.iter().collect();
    if bins.is_empty() || bins.len() != declared.len() || bins.iter().any(|bin| !valid_name(bin)) {
        return Err("recipe needs unique exported command names".into());
    }
    if checks.is_empty()
        || checks
            .iter()
            .any(|check| check.first().is_none_or(|bin| !bins.contains(bin)))
    {
        return Err("checks must execute declared commands".into());
    }
    Ok(())
}

pub(in crate::package) fn validate_bin_paths(
    declared: &[String],
    paths: &BTreeMap<String, PathBuf>,
) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    if paths.len() != declared.len() || paths.keys().any(|name| !declared.contains(name)) {
        return Err("bin_paths must map exactly the declared commands".into());
    }
    for path in paths.values() {
        if path.as_os_str().as_encoded_bytes().contains(&0) {
            return Err("binary path cannot contain a null byte".into());
        }
        crate::package::realize::validate_relative_path("binary path", path)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe() -> CatalogRecipe {
        CatalogRecipe {
            revision: 1,
            source: Some("github:owner/tool@v1".into()),
            build: None,
            assets: BTreeMap::new(),
            systems: vec!["aarch64-macos".into(), "x86_64-macos".into()],
            bins: vec!["tool".into()],
            bin_paths: BTreeMap::new(),
            checksums: BTreeMap::new(),
            mirror: false,
            checks: vec![vec!["tool".into(), "--help".into()]],
        }
    }

    #[test]
    fn command_paths_require_complete_safe_github_mappings() {
        let mut recipe = recipe();
        recipe
            .bin_paths
            .insert("tool".into(), "Tool.app/Contents/MacOS/client".into());
        recipe.validate().unwrap();
        for invalid in [
            "",
            ".",
            "../tool",
            "bin/../../tool",
            "/bin/tool",
            "bin/\0tool",
        ] {
            recipe.bin_paths.insert("tool".into(), invalid.into());
            assert!(recipe.validate().is_err(), "{invalid:?}");
        }
        recipe.bin_paths = BTreeMap::from([("other".into(), "bin/tool".into())]);
        assert!(recipe
            .validate()
            .unwrap_err()
            .contains("exactly the declared commands"));
        recipe.bin_paths.insert("tool".into(), "bin/tool".into());
        assert!(recipe.validate().is_err());
        recipe.bin_paths.remove("other");
        recipe.source = Some("aqua:owner/tool@1".into());
        assert!(recipe
            .validate()
            .unwrap_err()
            .contains("require a GitHub source"));
    }

    #[test]
    fn mirrored_recipes_require_complete_lowercase_checksums() {
        let mut recipe = recipe();
        recipe.mirror = true;
        assert!(recipe
            .validate()
            .unwrap_err()
            .contains("complete platform checksums"));
        recipe
            .checksums
            .insert("aarch64-macos".into(), "a".repeat(64));
        assert!(recipe
            .validate()
            .unwrap_err()
            .contains("each declared platform"));
        recipe
            .checksums
            .insert("x86_64-macos".into(), "b".repeat(64));
        recipe.validate().unwrap();
        for digest in [
            "A".repeat(64),
            "g".repeat(64),
            "a".repeat(63),
            "a".repeat(65),
        ] {
            recipe.checksums.insert("x86_64-macos".into(), digest);
            assert!(recipe.validate().is_err());
        }
        recipe.checksums.remove("x86_64-macos");
        recipe
            .checksums
            .insert("aarch64-linux".into(), "b".repeat(64));
        assert!(recipe.validate().is_err());
        recipe.checksums.remove("aarch64-linux");
        recipe
            .checksums
            .insert("x86_64-macos".into(), "b".repeat(64));
        recipe.mirror = false;
        recipe.validate().unwrap();
        recipe.source = Some("aqua:owner/tool@1".into());
        assert!(recipe
            .validate()
            .unwrap_err()
            .contains("require a GitHub source"));
    }

    #[test]
    fn build_recipes_reject_binary_paths_checksums_and_mirroring() {
        let catalog = crate::package::PackageCatalog::embedded().unwrap();
        let package = &catalog.packages["xz"];
        let original = &package.versions[&package.default_version];
        for field in ["bin_paths", "checksums", "mirror"] {
            let mut recipe = original.clone();
            match field {
                "bin_paths" => {
                    recipe.bin_paths = recipe
                        .bins
                        .iter()
                        .map(|bin| (bin.clone(), PathBuf::from(bin)))
                        .collect()
                }
                "checksums" => {
                    recipe.checksums = recipe
                        .systems
                        .iter()
                        .map(|system| (system.clone(), "a".repeat(64)))
                        .collect()
                }
                _ => recipe.mirror = true,
            }
            assert!(recipe
                .validate()
                .unwrap_err()
                .contains("require a GitHub source"));
        }
    }
}
