use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::catalog::valid_name;
use super::github::repository;
use super::{PackageCatalog, PackageRequest};

/// Authoring rules for a canonical package imported from GitHub releases.
/// Expanded from a package definition for release discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitHubUpstream {
    pub name: String,
    pub repository: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// None accepts numeric tags with an optional v; Some restricts the tag prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_prefix: Option<String>,
    /// Explicitly excluded legacy or unrelated release tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_tags: Vec<String>,
    #[serde(default = "supported_systems")]
    pub systems: Vec<String>,
    /// Exact asset names with optional {tag} and {version} substitutions.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<crate::SourceBuild>,
    pub bins: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bin_paths: BTreeMap<String, PathBuf>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub apps: BTreeMap<String, PathBuf>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub mirror: bool,
    pub checks: Vec<Vec<String>>,
}

fn is_false(value: &bool) -> bool {
    !value
}

fn supported_systems() -> Vec<String> {
    ["aarch64-linux", "aarch64-macos", "x86_64-linux"]
        .map(String::from)
        .to_vec()
}

impl GitHubUpstream {
    /// Creates discovery rules with version checks for explicitly selected commands.
    pub fn new(name: String, repository: String, bins: Vec<String>) -> Self {
        let checks = bins
            .iter()
            .map(|bin| vec![bin.clone(), "--version".into()])
            .collect();
        Self {
            name,
            repository,
            repository_id: None,
            aliases: Vec::new(),
            description: None,
            homepage: None,
            tag_prefix: None,
            exclude_tags: Vec::new(),
            systems: supported_systems(),
            assets: BTreeMap::new(),
            build: None,
            bins,
            bin_paths: BTreeMap::new(),
            apps: BTreeMap::new(),
            mirror: false,
            checks,
        }
    }

    /// Loads update rules from unified package files, inheriting each package's contract.
    pub fn from_directory(directory: &Path) -> Result<Vec<Self>, String> {
        Self::from_definitions(&super::PackageDefinition::from_directory(directory)?)
    }

    /// Extracts validated update rules from already evaluated package definitions.
    pub fn from_definitions(
        definitions: &BTreeMap<String, super::PackageDefinition>,
    ) -> Result<Vec<Self>, String> {
        let definitions = definitions
            .values()
            .filter_map(|definition| definition.github_upstream().transpose())
            .collect::<Result<Vec<_>, _>>()?;
        validate_definitions(&definitions)?;
        Ok(definitions)
    }

    pub fn validate(&self) -> Result<(), String> {
        repository(&self.repository)?;
        if !valid_name(&self.name) || self.repository_id == Some(0) {
            return Err("invalid canonical name or repository ID".into());
        }
        if self
            .description
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
            || self
                .homepage
                .as_ref()
                .is_some_and(|value| !value.starts_with("https://"))
        {
            return Err(format!("{}: invalid package identity", self.name));
        }
        let mut names = BTreeSet::new();
        for name in std::iter::once(&self.name).chain(&self.aliases) {
            if !valid_name(name) || !names.insert(name) {
                return Err(format!("invalid or duplicate package name `{name}`"));
            }
        }
        super::catalog::validate_systems(&self.systems)?;
        if let Some(build) = &self.build {
            build.validate()?;
            if self.mirror {
                return Err(
                    "source discovery with mirrored prebuilts requires pinned release checksums"
                        .into(),
                );
            }
        }
        if !self.bins.is_empty()
            || !self.checks.is_empty()
            || self.apps.is_empty()
                && self
                    .build
                    .as_ref()
                    .is_none_or(|build| build.libraries.is_empty())
        {
            super::catalog::validate_commands(&self.bins, &self.checks)?;
        }
        super::catalog::validate_bin_paths(&self.bins, &self.bin_paths)?;
        super::catalog::validate_apps(&self.apps)?;
        if !self.apps.is_empty()
            && self
                .systems
                .iter()
                .any(|system| !system.ends_with("-macos"))
        {
            return Err("application exports require macOS-only upstream systems".into());
        }
        for (system, pattern) in &self.assets {
            if !self.systems.contains(system) || pattern.trim().is_empty() {
                return Err(
                    "asset rules must reference requested systems and nonempty names".into(),
                );
            }
            let expanded = pattern.replace("{tag}", "1").replace("{version}", "1");
            if expanded.contains(['{', '}']) {
                return Err("asset rules only support {tag} and {version}".into());
            }
        }
        Ok(())
    }
}

pub fn validate_definitions(definitions: &[GitHubUpstream]) -> Result<(), String> {
    let mut names = BTreeSet::new();
    let mut repositories = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for definition in definitions {
        definition
            .validate()
            .map_err(|e| format!("{}: {e}", definition.name))?;
        for name in std::iter::once(&definition.name).chain(&definition.aliases) {
            if !names.insert(name) {
                return Err(format!("duplicate canonical name or alias `{name}`"));
            }
        }
        if !repositories.insert(definition.repository.to_ascii_lowercase())
            || definition.repository_id.is_some_and(|id| !ids.insert(id))
        {
            return Err(format!(
                "{}: upstream already has a canonical identity",
                definition.repository
            ));
        }
    }
    Ok(())
}

pub fn check_identity(catalog: &PackageCatalog, upstream: &GitHubUpstream) -> Result<(), String> {
    for package in catalog.packages.values() {
        let is_same_repository = package
            .versions
            .values()
            .filter_map(|recipe| recipe.source.as_deref())
            .map(PackageRequest::parse)
            .any(|request| {
                request.resolver.as_deref() == Some("github")
                    && request.name.eq_ignore_ascii_case(&upstream.repository)
            });
        if is_same_repository && package.name != upstream.name {
            return Err(format!(
                "{} is already canonicalized as `{}`",
                upstream.repository, package.name
            ));
        }
        if package.name == upstream.name
            && !is_same_repository
            && !(upstream.build.is_some()
                && package
                    .versions
                    .values()
                    .all(|recipe| recipe.build.is_some()))
        {
            return Err(format!(
                "{}: existing package has a different upstream; resolve identity manually",
                upstream.name
            ));
        }
    }
    for name in std::iter::once(&upstream.name).chain(&upstream.aliases) {
        if let Some(existing) = catalog.find(name) {
            if existing.name != upstream.name {
                return Err(format!("name `{name}` belongs to `{}`", existing.name));
            }
        }
    }
    Ok(())
}
