use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Contract, PackageDefinition, PackageUpstream};
use crate::package::{CatalogPackage, CatalogRecipe, PackageRequest, SourceBuild};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompactPackage {
    name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    aliases: Vec<String>,
    description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    homepage: Option<String>,
    default_version: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    default_versions: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<GitHubSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    build: Option<BuildTemplate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    systems: Option<Vec<String>>,
    bins: Vec<String>,
    checks: Vec<Vec<String>>,
    versions: BTreeMap<String, Version>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GitHubSource {
    github: String,
    tag: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repository_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tag_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    exclude_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    update_systems: Option<Vec<String>>,
    #[serde(default = "yes", skip_serializing_if = "is_true", rename = "track")]
    should_track: bool,
    assets: BTreeMap<String, String>,
}

fn yes() -> bool {
    true
}
fn is_true(value: &bool) -> bool {
    *value
}
fn one() -> u32 {
    1
}
fn is_one(value: &u32) -> bool {
    *value == 1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildTemplate {
    backend: crate::package::BuildBackend,
    url: String,
    archive: String,
    strip_prefix: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    configure: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dependencies: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Version {
    #[serde(default = "one", skip_serializing_if = "is_one")]
    revision: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    build: Option<SourceBuild>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    assets: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    systems: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bins: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checks: Option<Vec<Vec<String>>>,
}

fn expand(pattern: &str, version: &str, tag: Option<&str>) -> Result<String, String> {
    let mut value = pattern.replace("{version}", version);
    if let Some(tag) = tag {
        value = value.replace("{tag}", tag);
    }
    if value.contains(['{', '}']) {
        return Err(format!("unsupported placeholder in `{pattern}`"));
    }
    Ok(value)
}

impl CompactPackage {
    pub(super) fn expand(self) -> Result<PackageDefinition, String> {
        if self.source.is_some() == self.build.is_some() {
            return Err("package needs exactly one shared source or build definition".into());
        }
        if let Some(source) = &self.source {
            crate::package::github::repository(&source.github)?;
            if source.repository_id == Some(0) {
                return Err("invalid GitHub repository ID".into());
            }
            if source.update_systems.as_ref().is_some_and(Vec::is_empty) {
                return Err(
                    "update_systems must not be empty; use track = false to opt out".into(),
                );
            }
            if source.assets.keys().any(|system| {
                !matches!(
                    system.as_str(),
                    "aarch64-linux" | "x86_64-linux" | "aarch64-macos" | "x86_64-macos"
                )
            }) {
                return Err("source assets reference an unsupported platform".into());
            }
            let tag = expand(&source.tag, "1", None)?;
            if tag.trim().is_empty() {
                return Err("source tag must not be empty".into());
            }
            for pattern in source.assets.values() {
                expand(pattern, "1", Some(&tag))?;
            }
            if source.assets.is_empty() {
                return Err("GitHub source needs platform asset patterns".into());
            }
        }
        if let Some(build) = &self.build {
            expand(&build.url, "1", None)?;
            expand(&build.strip_prefix, "1", None)?;
        }
        let homepage = self
            .homepage
            .clone()
            .or_else(|| {
                self.source
                    .as_ref()
                    .map(|source| format!("https://github.com/{}", source.github))
            })
            .ok_or("source builds require a homepage")?;
        let systems = self
            .systems
            .clone()
            .or_else(|| {
                self.source
                    .as_ref()
                    .map(|source| source.assets.keys().cloned().collect())
            })
            .ok_or("source builds require explicit systems")?;
        let mut versions = BTreeMap::new();
        for (version, entry) in &self.versions {
            let recipe = self
                .expand_version(version, entry, &systems)
                .map_err(|e| format!("{}@{version}: {e}", self.name))?;
            versions.insert(version.clone(), recipe);
        }
        let upstream = self
            .source
            .as_ref()
            .filter(|source| source.should_track)
            .map(|source| {
                let mut update_systems = source
                    .update_systems
                    .clone()
                    .unwrap_or_else(|| systems.clone());
                update_systems.sort();
                let assets = source
                    .assets
                    .iter()
                    .filter(|(system, _)| update_systems.contains(system))
                    .map(|(system, asset)| (system.clone(), asset.clone()))
                    .collect();
                PackageUpstream::Github {
                    repository: source.github.clone(),
                    repository_id: source.repository_id,
                    tag_prefix: source
                        .tag_prefix
                        .clone()
                        .or_else(|| inferred_prefix(&source.tag)),
                    exclude_tags: source.exclude_tags.clone(),
                    systems: update_systems,
                    assets,
                }
            });
        let definition = PackageDefinition {
            contract: Some(Contract {
                bins: self.bins,
                checks: self.checks,
            }),
            package: CatalogPackage {
                name: self.name,
                aliases: self.aliases,
                description: self.description,
                homepage,
                default_version: self.default_version,
                default_versions: self.default_versions,
                versions,
            },
            upstream,
        };
        definition.github_upstream()?;
        Ok(definition)
    }

    fn expand_version(
        &self,
        version: &str,
        entry: &Version,
        systems: &[String],
    ) -> Result<CatalogRecipe, String> {
        let systems = entry.systems.clone().unwrap_or_else(|| systems.to_vec());
        let mut recipe = CatalogRecipe {
            revision: entry.revision,
            source: None,
            build: None,
            assets: BTreeMap::new(),
            systems,
            bins: entry.bins.clone().unwrap_or_else(|| self.bins.clone()),
            checks: entry.checks.clone().unwrap_or_else(|| self.checks.clone()),
        };
        if let Some(source) = &self.source {
            if entry.build.is_some() || entry.sha256.is_some() {
                return Err("GitHub versions cannot contain build inputs".into());
            }
            let tag = expand(entry.tag.as_deref().unwrap_or(&source.tag), version, None)?;
            recipe.source = Some(
                entry
                    .source
                    .clone()
                    .unwrap_or_else(|| format!("github:{}@{tag}", source.github)),
            );
            let mut assets = source.assets.clone();
            if let Some(overrides) = &entry.assets {
                if overrides
                    .keys()
                    .any(|system| !recipe.systems.contains(system))
                {
                    return Err("asset override references an undeclared platform".into());
                }
                assets.extend(overrides.clone());
            }
            for system in &recipe.systems {
                let pattern = assets
                    .get(system)
                    .ok_or_else(|| format!("missing asset for {system}"))?;
                recipe
                    .assets
                    .insert(system.clone(), expand(pattern, version, Some(&tag))?);
            }
        }
        if let Some(build) = &self.build {
            if entry.source.is_some() || entry.tag.is_some() || entry.assets.is_some() {
                return Err("build versions cannot contain GitHub source fields".into());
            }
            if entry.build.is_some() && entry.sha256.is_some() {
                return Err("use either an exact build override or sha256".into());
            }
            recipe.build = Some(match &entry.build {
                Some(build) => build.clone(),
                None => {
                    let mut value = serde_json::to_value(build).map_err(|e| e.to_string())?;
                    value["url"] = expand(&build.url, version, None)?.into();
                    value["strip_prefix"] = expand(&build.strip_prefix, version, None)?.into();
                    value["sha256"] = entry
                        .sha256
                        .clone()
                        .ok_or("each source version requires its own sha256")?
                        .into();
                    serde_json::from_value(value).map_err(|e| e.to_string())?
                }
            });
        }
        recipe.validate()?;
        Ok(recipe)
    }
}

impl CompactPackage {
    pub(super) fn from_definition(definition: &PackageDefinition) -> Result<Option<Self>, String> {
        let package = &definition.package;
        let default = package
            .versions
            .get(&package.default_version)
            .ok_or("default version has no recipe")?;
        let mut compact = Self {
            name: package.name.clone(),
            aliases: package.aliases.clone(),
            description: package.description.clone(),
            homepage: Some(package.homepage.clone()),
            default_version: package.default_version.clone(),
            default_versions: package.default_versions.clone(),
            source: None,
            build: None,
            systems: None,
            bins: definition
                .contract
                .as_ref()
                .map(|contract| contract.bins.clone())
                .unwrap_or_else(|| default.bins.clone()),
            checks: definition
                .contract
                .as_ref()
                .map(|contract| contract.checks.clone())
                .unwrap_or_else(|| default.checks.clone()),
            versions: BTreeMap::new(),
        };
        if let Some(source) = &default.source {
            let request = PackageRequest::parse(source);
            if request.resolver.as_deref() != Some("github")
                || package
                    .versions
                    .values()
                    .any(|recipe| recipe.build.is_some() || recipe.assets.is_empty())
            {
                return Ok(None);
            }
            let tag = request.version.ok_or("GitHub source has no tag")?;
            let tag_pattern = tag
                .strip_suffix(&package.default_version)
                .map(|prefix| format!("{prefix}{{version}}"))
                .unwrap_or(tag);
            let mut source = GitHubSource {
                github: request.name,
                tag: tag_pattern,
                repository_id: None,
                tag_prefix: None,
                exclude_tags: Vec::new(),
                update_systems: None,
                should_track: false,
                assets: BTreeMap::new(),
            };
            if let Some(PackageUpstream::Github {
                repository,
                repository_id,
                tag_prefix,
                exclude_tags,
                systems,
                assets,
            }) = &definition.upstream
            {
                if repository != &source.github {
                    return Ok(None);
                }
                source.repository_id = *repository_id;
                source.tag_prefix = tag_prefix
                    .clone()
                    .filter(|prefix| Some(prefix) != inferred_prefix(&source.tag).as_ref());
                source.exclude_tags = exclude_tags.clone();
                source.update_systems = (!systems.is_empty()).then(|| systems.clone());
                source.assets = assets.clone();
                source.should_track = true;
            }
            for (version, recipe) in &package.versions {
                for (system, asset) in &recipe.assets {
                    source
                        .assets
                        .entry(system.clone())
                        .or_insert_with(|| asset.replace(version, "{version}"));
                }
            }
            if package.homepage == format!("https://github.com/{}", source.github) {
                compact.homepage = None;
            }
            compact.source = Some(source);
        } else if let Some(build) = &default.build {
            if package
                .versions
                .values()
                .any(|recipe| recipe.build.is_none())
            {
                return Ok(None);
            }
            let mut value = serde_json::to_value(build).map_err(|e| e.to_string())?;
            value.as_object_mut().unwrap().remove("sha256");
            value["url"] = build
                .url
                .replace(&package.default_version, "{version}")
                .into();
            value["strip_prefix"] = build
                .strip_prefix
                .to_str()
                .ok_or("non-UTF-8 strip prefix")?
                .replace(&package.default_version, "{version}")
                .into();
            compact.build = Some(serde_json::from_value(value).map_err(|e| e.to_string())?);
        } else {
            return Ok(None);
        }
        let systems = most_common_systems(package);
        let inferred: Vec<_> = compact
            .source
            .as_ref()
            .map(|source| source.assets.keys().cloned().collect())
            .unwrap_or_default();
        if systems != inferred {
            compact.systems = Some(systems.clone());
        }
        if let Some(source) = &mut compact.source {
            let mut ordered_systems = systems.clone();
            ordered_systems.sort();
            if source
                .update_systems
                .as_ref()
                .is_some_and(|update_systems| {
                    let mut selected = update_systems.clone();
                    selected.sort();
                    selected == ordered_systems
                })
            {
                source.update_systems = None;
            }
        }
        for (version, recipe) in &package.versions {
            let mut entry = Version {
                revision: recipe.revision,
                systems: (recipe.systems != systems).then(|| recipe.systems.clone()),
                bins: (recipe.bins != compact.bins).then(|| recipe.bins.clone()),
                checks: (recipe.checks != compact.checks).then(|| recipe.checks.clone()),
                ..Version::default()
            };
            if let Some(source) = &compact.source {
                let exact_source = recipe.source.as_ref().ok_or("missing source")?;
                let request = PackageRequest::parse(exact_source);
                let tag = request.version.ok_or("GitHub source has no tag")?;
                if tag != expand(&source.tag, version, None)? {
                    entry.tag = Some(tag.clone());
                }
                if exact_source != &format!("github:{}@{tag}", source.github) {
                    entry.source = Some(exact_source.clone());
                }
                let mut overrides = BTreeMap::new();
                for (system, asset) in &recipe.assets {
                    if source
                        .assets
                        .get(system)
                        .map(|pattern| expand(pattern, version, Some(&tag)))
                        .transpose()?
                        .as_ref()
                        != Some(asset)
                    {
                        overrides.insert(system.clone(), asset.clone());
                    }
                }
                if !overrides.is_empty() {
                    entry.assets = Some(overrides);
                }
            }
            if let Some(build) = &recipe.build {
                entry.sha256 = Some(build.sha256.clone());
            }
            let expanded = compact.expand_version(version, &entry, &systems)?;
            if serde_json::to_value(&expanded).map_err(|e| e.to_string())?
                != serde_json::to_value(recipe).map_err(|e| e.to_string())?
            {
                if recipe.build.is_none() {
                    return Err(format!(
                        "{}@{version}: compact recipe changed expanded data",
                        package.name
                    ));
                }
                entry.sha256 = None;
                entry.build = recipe.build.clone();
            }
            compact.versions.insert(version.clone(), entry);
        }
        Ok(Some(compact))
    }
}

fn most_common_systems(package: &CatalogPackage) -> Vec<String> {
    let mut counts = BTreeMap::new();
    for recipe in package.versions.values() {
        *counts.entry(recipe.systems.clone()).or_insert(0usize) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(systems, _)| systems)
        .unwrap_or_default()
}

fn inferred_prefix(tag: &str) -> Option<String> {
    tag.strip_suffix("{version}")
        .filter(|prefix| !matches!(*prefix, "" | "v"))
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::PackageCatalog;

    const BINARY: &str = r#"return {
        name = "tool", description = "Tool", homepage = "https://example.com",
        default_version = "2", default_versions = { ["x86_64-macos"] = "1" },
        source = {
            github = "owner/tool", tag = "tool-{version}", repository_id = 42,
            assets = {
                ["aarch64-macos"] = "tool-{tag}-arm64.tar.gz",
                ["x86_64-macos"] = "tool-{version}-intel.tar.gz",
            },
        },
        bins = { "tool" }, checks = { { "tool", "--version" } },
        versions = {
            ["1"] = { revision = 2, tag = "legacy-1", assets = { ["x86_64-macos"] = "old.zip" } },
            ["2"] = { systems = { "aarch64-macos" } },
        },
    }"#;

    #[test]
    fn expands_templates_and_historical_platform_exceptions_without_changing_catalogs() {
        let definition = PackageDefinition::from_lua(BINARY).unwrap();
        let old = &definition.package.versions["1"];
        let new = &definition.package.versions["2"];
        assert_eq!(old.source.as_deref(), Some("github:owner/tool@legacy-1"));
        assert_eq!(old.assets["aarch64-macos"], "tool-legacy-1-arm64.tar.gz");
        assert_eq!(old.assets["x86_64-macos"], "old.zip");
        assert_eq!(old.revision, 2);
        assert_eq!(new.revision, 1);
        assert_eq!(new.assets.len(), 1);
        assert_eq!(definition.package.default_version_for("x86_64-macos"), "1");
        let upstream = definition.github_upstream().unwrap().unwrap();
        assert_eq!(upstream.tag_prefix.as_deref(), Some("tool-"));
        assert_eq!(upstream.systems.len(), 2);
        let repeated = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&definition.package).unwrap(),
            serde_json::to_value(repeated.package).unwrap()
        );
    }

    #[test]
    fn round_trips_every_embedded_recipe_including_source_builds_and_retained_versions() {
        let catalog = PackageCatalog::embedded().unwrap();
        for package in catalog.packages.values() {
            let definition = PackageDefinition::new(package.clone());
            let source = definition.to_lua().unwrap();
            let expanded = PackageDefinition::from_lua(&source).unwrap();
            assert_eq!(
                serde_json::to_value(package).unwrap(),
                serde_json::to_value(expanded.package).unwrap(),
                "{}",
                package.name
            );
            assert!(expanded.upstream.is_none());
        }
    }

    #[test]
    fn discovery_uses_shared_checks_instead_of_the_current_versions_exception() {
        let source = BINARY.replace(
            "[\"2\"] = { systems",
            "[\"2\"] = { checks = { { \"tool\", \"--help\" } }, systems",
        );
        let definition = PackageDefinition::from_lua(&source).unwrap();
        let repeated = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        for definition in [definition, repeated] {
            assert_eq!(
                definition.package.versions["2"].checks[0],
                ["tool", "--help"]
            );
            assert_eq!(
                definition.github_upstream().unwrap().unwrap().checks[0],
                ["tool", "--version"]
            );
        }
    }

    #[test]
    fn discovery_retains_shared_targets_not_yet_available_in_pinned_versions() {
        let source = BINARY.replace(
            "revision = 2, tag = \"legacy-1\", assets = { [\"x86_64-macos\"] = \"old.zip\" }",
            "revision = 2, tag = \"legacy-1\", systems = { \"aarch64-macos\" }",
        );
        let source = source.replace("default_versions = { [\"x86_64-macos\"] = \"1\" },", "");
        let definition = PackageDefinition::from_lua(&source).unwrap();
        let repeated = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        for definition in [definition, repeated] {
            assert_eq!(definition.package.versions["1"].systems, ["aarch64-macos"]);
            assert_eq!(
                definition.github_upstream().unwrap().unwrap().systems,
                ["aarch64-macos", "x86_64-macos"]
            );
        }
    }

    #[test]
    fn source_build_versions_require_independent_checksums_and_allow_exact_overrides() {
        let package = PackageCatalog::embedded().unwrap().packages["xz"].clone();
        let definition = PackageDefinition::new(package);
        let mut compact = CompactPackage::from_definition(&definition)
            .unwrap()
            .unwrap();
        let version = compact.default_version.clone();
        let checksum = compact.versions[&version].sha256.clone().unwrap();
        compact.versions.get_mut(&version).unwrap().sha256 = None;
        assert!(compact.expand().unwrap_err().contains("own sha256"));
        let mut compact = CompactPackage::from_definition(&definition)
            .unwrap()
            .unwrap();
        let mut build = definition.package.versions[&version].build.clone().unwrap();
        build.configure.push("--disable-threads".into());
        compact.versions.insert(
            "99".into(),
            Version {
                revision: 1,
                build: Some(build.clone()),
                ..Version::default()
            },
        );
        let expanded = compact.expand().unwrap();
        assert_eq!(
            expanded.package.versions[&version]
                .build
                .as_ref()
                .unwrap()
                .sha256,
            checksum
        );
        assert_eq!(
            expanded.package.versions["99"]
                .build
                .as_ref()
                .unwrap()
                .configure,
            build.configure
        );
        assert!(expanded.to_lua().unwrap().contains("--disable-threads"));
    }

    #[test]
    fn rejects_unknown_fields_placeholders_empty_contracts_and_undeclared_asset_overrides() {
        for invalid in [
            BINARY.replace("tag = \"tool-{version}\"", "tag = \"tool-{typo}\""),
            BINARY.replace("bins =", "bin_typo ="),
            BINARY.replace("bins = { \"tool\" }", "bins = {}"),
            BINARY.replace("systems = { \"aarch64-macos\" }", "systems = {}"),
            BINARY.replace("revision = 2", "revision = 0"),
            BINARY.replace(
                "[\"x86_64-macos\"] = \"old.zip\"",
                "[\"x86_64-linux\"] = \"old.zip\"",
            ),
            BINARY.replace("tool-{version}-intel.tar.gz", "tool-{unknown}-intel.tar.gz"),
            BINARY.replace(
                "[\"2\"] = { systems",
                "[\"2\"] = { sha256 = \"bad\", systems",
            ),
        ] {
            assert!(PackageDefinition::from_lua(&invalid).is_err(), "{invalid}");
        }
    }
}
