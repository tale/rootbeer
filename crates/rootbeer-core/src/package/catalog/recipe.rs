use std::collections::{BTreeMap, BTreeSet};

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
    pub checks: Vec<Vec<String>>,
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
    pub(super) fn validate(&self) -> Result<(), String> {
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
        let systems: BTreeSet<_> = self.systems.iter().collect();
        if systems.is_empty()
            || systems.len() != self.systems.len()
            || systems.iter().any(|system| {
                !matches!(
                    system.as_str(),
                    "aarch64-macos" | "x86_64-macos" | "aarch64-linux" | "x86_64-linux"
                )
            })
        {
            return Err("recipe needs unique supported systems".into());
        }
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
        let bins: BTreeSet<_> = self.bins.iter().collect();
        if bins.is_empty()
            || bins.len() != self.bins.len()
            || bins.iter().any(|bin| !valid_name(bin))
        {
            return Err("recipe needs unique exported command names".into());
        }
        if self.checks.is_empty()
            || self
                .checks
                .iter()
                .any(|check| check.first().is_none_or(|bin| !bins.contains(bin)))
        {
            return Err("checks must execute declared commands".into());
        }
        Ok(())
    }
}
