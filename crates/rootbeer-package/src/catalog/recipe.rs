use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::valid_name;
use crate::PackageRequest;

/// Fields published by a newer engine, kept verbatim rather than rejected.
///
/// Values round trip, but they re-serialize after the known fields, so `sha256` still
/// differs from the publisher's until hashing is canonical.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExtraFields(pub BTreeMap<String, serde_json::Value>);

impl Eq for ExtraFields {}

impl ExtraFields {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A package's identity and the versions a PDR publishes for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogPackage {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub description: String,
    pub homepage: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipe_maintainers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_engine_level: Option<u32>,
    /// The version each platform resolves to when a request names none.
    pub default_versions: BTreeMap<String, String>,
    pub versions: BTreeMap<String, CatalogVersion>,
    #[serde(flatten, default, skip_serializing_if = "ExtraFields::is_empty")]
    pub extra: ExtraFields,
}

/// One version, and the contract each platform builds for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogVersion {
    /// SPDX expression, or `NOASSERTION` when upstream states none.
    pub license: String,
    pub revision: u32,
    pub platforms: BTreeMap<String, CatalogRecipe>,
    #[serde(flatten, default, skip_serializing_if = "ExtraFields::is_empty")]
    pub extra: ExtraFields,
}

/// What one platform acquires and what it installs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogRecipe {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<crate::LockedInstall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<crate::SourceBuild>,
    /// The single release asset this platform downloads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Exported commands, each resolved to its path in the installed tree.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bins: BTreeMap<String, PathBuf>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub apps: BTreeMap<String, PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<Vec<String>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mirror: bool,
    #[serde(flatten, default, skip_serializing_if = "ExtraFields::is_empty")]
    pub extra: ExtraFields,
}

impl CatalogPackage {
    /// The version this system installs when a request names none.
    pub fn default_version_for(&self, system: &str) -> Option<&str> {
        self.default_versions.get(system).map(String::as_str)
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if !valid_name(&self.name) {
            return Err(format!("invalid package name `{}`", self.name));
        }
        if self.default_versions.is_empty() {
            return Err(format!("{}: no platform declares a version", self.name));
        }
        super::validate_systems(&self.default_versions.keys().cloned().collect::<Vec<_>>())?;
        for (system, version) in &self.default_versions {
            let entry = self
                .versions
                .get(version)
                .ok_or_else(|| format!("{}: default version {version} has no recipe", self.name))?;
            if !entry.platforms.contains_key(system) {
                return Err(format!(
                    "{}: default version {version} does not build {system}",
                    self.name
                ));
            }
        }
        for (version, entry) in &self.versions {
            entry
                .validate()
                .map_err(|error| format!("{}@{version}: {error}", self.name))?;
        }
        Ok(())
    }
}

impl CatalogVersion {
    pub fn for_system(&self, system: &str) -> Option<&CatalogRecipe> {
        self.platforms.get(system)
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.revision == 0 {
            return Err("recipe needs a revision".into());
        }
        if self.license.trim().is_empty() {
            return Err("recipe needs an SPDX license or NOASSERTION".into());
        }
        if self.platforms.is_empty() {
            return Err("version builds no platform".into());
        }
        super::validate_systems(&self.platforms.keys().cloned().collect::<Vec<_>>())?;
        for (system, recipe) in &self.platforms {
            recipe
                .validate(system)
                .map_err(|error| format!("{system}: {error}"))?;
        }
        Ok(())
    }
}

impl CatalogRecipe {
    pub fn sha256(&self) -> String {
        crate::store::hash_bytes(
            &serde_json::to_vec(self).expect("recipe serialization cannot fail"),
        )
    }

    pub(crate) fn validate(&self, system: &str) -> Result<(), String> {
        match (self.source.is_some(), self.build.is_some()) {
            (false, false) => return Err("platform needs a prebuilt source or a build".into()),
            (true, true) => {
                return Err("a platform is prebuilt or built from source, not both".into())
            }
            _ => {}
        }
        if let Some(build) = &self.build {
            build.validate()?;
            if let Some(go) = &build.go {
                if go.binaries.keys().collect::<BTreeSet<_>>() != self.bins.keys().collect() {
                    return Err("Go entry points must match exported binaries".into());
                }
            }
        }

        let is_download = self
            .source
            .as_deref()
            .is_some_and(|source| source.starts_with("https://"));
        if is_download {
            crate::index::validate_https(self.source.as_deref().unwrap())?;
            if self.sha256.is_none() || self.asset.is_some() {
                return Err(
                    "direct downloads require an install format, a digest, and no release asset"
                        .into(),
                );
            }
            match self.install.as_ref().ok_or("direct downloads require an install format")? {
                crate::LockedInstall::Dmg if !self.apps.is_empty() => {}
                crate::LockedInstall::Archive { strip_prefix, .. } => {
                    if let Some(path) = strip_prefix {
                        crate::realize::validate_relative_path("archive prefix", path)
                            .map_err(|error| error.to_string())?;
                    }
                }
                crate::LockedInstall::Binary { path } => {
                    crate::realize::validate_relative_path("binary path", path)
                        .map_err(|error| error.to_string())?;
                }
                _ => return Err("unsupported direct download install contract".into()),
            }
        } else if self.install.is_some() {
            return Err("explicit install formats require a direct HTTPS download".into());
        }

        if let Some(source) = self.source.as_ref().filter(|_| !is_download) {
            let request = PackageRequest::parse(source);
            if request.resolver.as_deref() != Some("github")
                || request
                    .version
                    .as_deref()
                    .is_none_or(|version| version == "latest")
                || !request
                    .name
                    .split_once('/')
                    .is_some_and(|(owner, repo)| !owner.is_empty() && !repo.is_empty())
            {
                return Err("prebuilt platforms need an exact github: source".into());
            }
            if self.asset.as_ref().is_none_or(|asset| asset.trim().is_empty()) {
                return Err("a prebuilt platform needs one release asset".into());
            }
        }

        super::validate_apps(&self.apps)?;
        if !self.apps.is_empty() && !system.ends_with("-macos") {
            return Err("application exports are macOS only".into());
        }
        super::validate_commands(self.bins.keys(), &self.checks)?;
        for path in self.bins.values().chain(self.apps.values()) {
            crate::realize::validate_relative_path("output path", path)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

pub(crate) fn validate_systems(declared: &[String]) -> Result<(), String> {
    let systems: BTreeSet<_> = declared.iter().collect();
    if systems.is_empty()
        || systems.len() != declared.len()
        || systems.iter().any(|system| {
            !matches!(
                system.as_str(),
                "aarch64-macos" | "aarch64-linux" | "x86_64-linux"
            )
        })
    {
        return Err("recipe needs unique supported systems".into());
    }
    Ok(())
}

pub(crate) fn validate_commands<'a>(
    bins: impl IntoIterator<Item = &'a String>,
    checks: &[Vec<String>],
) -> Result<(), String> {
    let bins: BTreeSet<_> = bins.into_iter().collect();
    if bins.iter().any(|bin| !valid_name(bin)) {
        return Err("recipe needs unique exported command names".into());
    }
    if !bins.is_empty()
        && (checks.is_empty()
            || checks
                .iter()
                .any(|check| check.first().is_none_or(|bin| !bins.contains(bin))))
    {
        return Err("checks must execute declared commands".into());
    }
    Ok(())
}

pub(crate) fn validate_apps(apps: &BTreeMap<String, PathBuf>) -> Result<(), String> {
    let mut names = BTreeSet::new();
    for (name, path) in apps {
        if !name.ends_with(".app")
            || name.len() <= 4
            || name.contains(['/', '\\'])
            || name.chars().any(char::is_control)
            || !names.insert(name.to_lowercase())
        {
            return Err("application names must be unique .app filenames".into());
        }
        crate::realize::validate_relative_path("application bundle", path)
            .map_err(|error| error.to_string())?;
        if path.extension().is_none_or(|extension| extension != "app") {
            return Err("application paths must name relative .app bundles".into());
        }
    }
    Ok(())
}
