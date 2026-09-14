use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Contract, PackageDefinition, PackageUpstream};
use crate::package::{CatalogPackage, CatalogRecipe, PackageRequest, SourceBuild};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    bin_paths: BTreeMap<String, PathBuf>,
    #[serde(default, skip_serializing_if = "is_false")]
    mirror: bool,
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

fn is_false(value: &bool) -> bool {
    !value
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
    args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    patches: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dependencies: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    bin_paths: Option<BTreeMap<String, PathBuf>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    checksums: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mirror: Option<bool>,
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
    pub(super) fn expand(&self) -> Result<PackageDefinition, String> {
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
                    "aarch64-linux" | "x86_64-linux" | "aarch64-macos"
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
            authoring: Some(self.clone()),
            contract: Some(Contract {
                bins: self.bins.clone(),
                bin_paths: self.bin_paths.clone(),
                mirror: self.mirror,
                checks: self.checks.clone(),
            }),
            package: CatalogPackage {
                name: self.name.clone(),
                aliases: self.aliases.clone(),
                description: self.description.clone(),
                homepage,
                default_version: self.default_version.clone(),
                default_versions: self.default_versions.clone(),
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
            bin_paths: entry
                .bin_paths
                .clone()
                .unwrap_or_else(|| self.bin_paths.clone()),
            checksums: entry.checksums.clone(),
            mirror: entry.mirror.unwrap_or(self.mirror),
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
        let Some(upstream) = definition.github_upstream()? else {
            return Ok(None);
        };
        let Some(request) = default.source.as_deref().map(PackageRequest::parse) else {
            return Ok(None);
        };
        if request.resolver.as_deref() != Some("github")
            || request.name != upstream.repository
            || package
                .versions
                .values()
                .any(|recipe| recipe.build.is_some() || recipe.assets.is_empty())
        {
            return Ok(None);
        }
        let tag = request.version.ok_or("GitHub source has no tag")?;
        let tag = tag
            .strip_suffix(&package.default_version)
            .map(|prefix| format!("{prefix}{{version}}"))
            .unwrap_or(tag);
        let mut assets = upstream.assets;
        for (version, recipe) in &package.versions {
            for (system, asset) in &recipe.assets {
                assets
                    .entry(system.clone())
                    .or_insert_with(|| asset.replace(version, "{version}"));
            }
        }
        let systems = default.systems.clone();
        let mut compact = Self {
            name: package.name.clone(),
            aliases: package.aliases.clone(),
            description: package.description.clone(),
            homepage: (package.homepage != format!("https://github.com/{}", upstream.repository))
                .then(|| package.homepage.clone()),
            default_version: package.default_version.clone(),
            default_versions: package.default_versions.clone(),
            source: Some(GitHubSource {
                github: upstream.repository,
                repository_id: upstream.repository_id,
                tag_prefix: upstream
                    .tag_prefix
                    .filter(|prefix| Some(prefix) != inferred_prefix(&tag).as_ref()),
                tag,
                exclude_tags: upstream.exclude_tags,
                update_systems: Some(upstream.systems),
                should_track: true,
                assets,
            }),
            build: None,
            systems: Some(systems.clone()),
            bins: upstream.bins,
            bin_paths: upstream.bin_paths,
            mirror: upstream.mirror,
            checks: upstream.checks,
            versions: BTreeMap::new(),
        };
        compact.update_versions(package, &systems)?;
        Ok(Some(compact))
    }

    fn update_versions(
        &mut self,
        package: &CatalogPackage,
        systems: &[String],
    ) -> Result<(), String> {
        self.versions
            .retain(|version, _| package.versions.contains_key(version));
        for (version, recipe) in &package.versions {
            if let Some(entry) = self.versions.get(version) {
                if serde_json::to_value(self.expand_version(version, entry, systems)?)
                    .map_err(|e| e.to_string())?
                    == serde_json::to_value(recipe).map_err(|e| e.to_string())?
                {
                    continue;
                }
            }
            let mut entry = Version {
                revision: recipe.revision,
                systems: (recipe.systems != systems).then(|| recipe.systems.clone()),
                bins: (recipe.bins != self.bins).then(|| recipe.bins.clone()),
                bin_paths: (recipe.bin_paths != self.bin_paths).then(|| recipe.bin_paths.clone()),
                checksums: recipe.checksums.clone(),
                mirror: (recipe.mirror != self.mirror).then_some(recipe.mirror),
                checks: (recipe.checks != self.checks).then(|| recipe.checks.clone()),
                ..Version::default()
            };
            if let Some(source) = &self.source {
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
            let expanded = self.expand_version(version, &entry, systems)?;
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
            self.versions.insert(version.clone(), entry);
        }
        Ok(())
    }

    pub(super) fn updated(&self, definition: &PackageDefinition) -> Result<Option<Self>, String> {
        let mut compact = self.clone();
        let previous = self.expand()?;
        let package = &definition.package;
        compact.name = package.name.clone();
        compact.aliases = package.aliases.clone();
        compact.description = package.description.clone();
        if package.homepage != previous.package.homepage {
            compact.homepage = Some(package.homepage.clone());
        }
        compact.default_version = package.default_version.clone();
        compact.default_versions = package.default_versions.clone();
        let previous_upstream = previous.github_upstream()?;
        let upstream = definition.github_upstream()?;
        if serde_json::to_value(&previous_upstream).map_err(|e| e.to_string())?
            != serde_json::to_value(&upstream).map_err(|e| e.to_string())?
        {
            let Some(source) = &mut compact.source else {
                return Ok(None);
            };
            source.should_track = definition.upstream.is_some();
            if let Some(upstream) = upstream {
                source.github = upstream.repository;
                source.repository_id = upstream.repository_id;
                if previous_upstream
                    .as_ref()
                    .is_none_or(|previous| previous.tag_prefix != upstream.tag_prefix)
                {
                    source.tag_prefix = upstream.tag_prefix;
                }
                source.exclude_tags = upstream.exclude_tags;
                if previous_upstream
                    .as_ref()
                    .is_none_or(|previous| previous.systems != upstream.systems)
                {
                    source.update_systems = Some(upstream.systems);
                }
                if previous_upstream
                    .as_ref()
                    .is_none_or(|previous| previous.assets != upstream.assets)
                {
                    source.assets.extend(upstream.assets);
                }
                compact.bins = upstream.bins;
                compact.bin_paths = upstream.bin_paths;
                compact.mirror = upstream.mirror;
                compact.checks = upstream.checks;
            }
        }
        let systems = compact.systems.clone().unwrap_or_else(|| {
            compact
                .source
                .as_ref()
                .unwrap()
                .assets
                .keys()
                .cloned()
                .collect()
        });
        if package.versions.values().any(|recipe| {
            recipe.build.is_some() != compact.build.is_some()
                || (compact.source.is_some() && recipe.assets.is_empty())
        }) {
            return Ok(None);
        }
        compact.update_versions(package, &systems)?;
        let expanded = compact.expand()?;
        if serde_json::to_value(&expanded.package).map_err(|e| e.to_string())?
            != serde_json::to_value(package).map_err(|e| e.to_string())?
            || serde_json::to_value(expanded.github_upstream()?).map_err(|e| e.to_string())?
                != serde_json::to_value(definition.github_upstream()?).map_err(|e| e.to_string())?
        {
            return Ok(None);
        }
        Ok(Some(compact))
    }
}

fn inferred_prefix(tag: &str) -> Option<String> {
    tag.strip_suffix("{version}").map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::PackageCatalog;

    const BINARY: &str = r#"return {
        name = "tool", description = "Tool", homepage = "https://example.com",
        default_version = "2", default_versions = { ["x86_64-linux"] = "1" },
        source = {
            github = "owner/tool", tag = "tool-{version}", repository_id = 42,
            assets = {
                ["aarch64-macos"] = "tool-{tag}-arm64.tar.gz",
                ["x86_64-linux"] = "tool-{version}-linux.tar.gz",
            },
        },
        bins = { "tool" }, checks = { { "tool", "--version" } },
        versions = {
            ["1"] = { revision = 2, tag = "legacy-1", assets = { ["x86_64-linux"] = "old.zip" } },
            ["2"] = { systems = { "aarch64-macos" } },
        },
    }"#;

    #[test]
    fn discovery_prefix_matches_the_source_tag_template() {
        assert_eq!(inferred_prefix("v{version}"), Some("v".into()));
        assert_eq!(inferred_prefix("{version}"), Some(String::new()));
        assert_eq!(inferred_prefix("cli-v{version}"), Some("cli-v".into()));
        assert_eq!(inferred_prefix("{version}-release"), None);
    }

    #[test]
    fn expands_templates_and_historical_platform_exceptions_without_changing_catalogs() {
        let definition = PackageDefinition::from_lua(BINARY).unwrap();
        let old = &definition.package.versions["1"];
        let new = &definition.package.versions["2"];
        assert_eq!(old.source.as_deref(), Some("github:owner/tool@legacy-1"));
        assert_eq!(old.assets["aarch64-macos"], "tool-legacy-1-arm64.tar.gz");
        assert_eq!(old.assets["x86_64-linux"], "old.zip");
        assert_eq!(old.revision, 2);
        assert_eq!(new.revision, 1);
        assert_eq!(new.assets.len(), 1);
        assert_eq!(definition.package.default_version_for("x86_64-linux"), "1");
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
    fn preserves_authoring_and_reflects_public_recipe_mutations() {
        let mut definition = PackageDefinition::from_lua(BINARY).unwrap();
        let original = serde_json::to_value(definition.authoring.as_ref().unwrap()).unwrap();
        let rendered = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(rendered.authoring.unwrap()).unwrap(),
            original
        );

        definition.package.description = "Updated tool".into();
        definition.package.default_version = "3".into();
        let mut recipe = definition.package.versions["2"].clone();
        recipe.source = Some("github:owner/tool@tool-3".into());
        recipe
            .assets
            .insert("aarch64-macos".into(), "tool-tool-3-arm64.tar.gz".into());
        definition.package.versions.insert("3".into(), recipe);
        definition.package.versions.get_mut("2").unwrap().revision = 4;
        let repeated = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&repeated.package).unwrap(),
            serde_json::to_value(&definition.package).unwrap()
        );
        let saved = serde_json::to_value(repeated.authoring.unwrap()).unwrap();
        assert_eq!(saved["source"], original["source"]);
        assert_eq!(saved["versions"]["1"], original["versions"]["1"]);
        assert_eq!(
            saved["versions"]["3"],
            serde_json::json!({"systems": ["aarch64-macos"]})
        );

        definition.package.versions.remove("2");
        let repeated = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        assert!(!repeated.package.versions.contains_key("2"));
    }

    #[test]
    fn incompatible_template_edits_preserve_rules_or_fail_explicitly() {
        let mut definition = PackageDefinition::from_lua(BINARY).unwrap();
        let Some(PackageUpstream::Github {
            tag_prefix, assets, ..
        }) = &mut definition.upstream
        else {
            unreachable!()
        };
        *tag_prefix = None;
        assets.clear();
        let repeated = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        assert!(repeated.authoring.is_none());
        assert_eq!(
            serde_json::to_value(repeated.github_upstream().unwrap()).unwrap(),
            serde_json::to_value(definition.github_upstream().unwrap()).unwrap()
        );

        definition.package.versions.get_mut("2").unwrap().checks =
            vec![vec!["tool".into(), "--help".into()]];
        assert!(definition
            .to_lua()
            .unwrap_err()
            .contains("shared discovery"));

        definition.upstream = None;
        let recipe = definition.package.versions.get_mut("2").unwrap();
        recipe.source = Some("aqua:owner/tool@2".into());
        recipe.assets.clear();
        let repeated = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&repeated.package).unwrap(),
            serde_json::to_value(&definition.package).unwrap()
        );
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
            "revision = 2, tag = \"legacy-1\", assets = { [\"x86_64-linux\"] = \"old.zip\" }",
            "revision = 2, tag = \"legacy-1\", systems = { \"aarch64-macos\" }",
        );
        let source = source.replace("default_versions = { [\"x86_64-linux\"] = \"1\" },", "");
        let definition = PackageDefinition::from_lua(&source).unwrap();
        let repeated = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        for definition in [definition, repeated] {
            assert_eq!(definition.package.versions["1"].systems, ["aarch64-macos"]);
            assert_eq!(
                definition.github_upstream().unwrap().unwrap().systems,
                ["aarch64-macos", "x86_64-linux"]
            );
        }
    }

    #[test]
    fn source_build_versions_require_independent_checksums_and_allow_exact_overrides() {
        let package = PackageCatalog::embedded().unwrap().packages["xz"].clone();
        let version = package.default_version.clone();
        let recipe = &package.versions[&version];
        let build = recipe.build.as_ref().unwrap();
        let compact = CompactPackage {
            name: package.name.clone(),
            aliases: package.aliases.clone(),
            description: package.description.clone(),
            homepage: Some(package.homepage.clone()),
            default_version: version.clone(),
            default_versions: BTreeMap::new(),
            source: None,
            build: Some(BuildTemplate {
                backend: build.backend.clone(),
                url: build.url.replace(&version, "{version}"),
                archive: "tar.gz".into(),
                strip_prefix: "xz-{version}".into(),
                configure: build.configure.clone(),
                args: build.args.clone(),
                patches: build.patches.clone(),
                dependencies: build.dependencies.clone(),
            }),
            systems: Some(recipe.systems.clone()),
            bins: recipe.bins.clone(),
            bin_paths: BTreeMap::new(),
            mirror: false,
            checks: recipe.checks.clone(),
            versions: BTreeMap::from([(
                version.clone(),
                Version {
                    revision: recipe.revision,
                    sha256: Some(build.sha256.clone()),
                    ..Version::default()
                },
            )]),
        };
        let definition = compact.expand().unwrap();
        let original = compact.clone();
        let mut compact = compact;
        let version = compact.default_version.clone();
        let checksum = compact.versions[&version].sha256.clone().unwrap();
        compact.versions.get_mut(&version).unwrap().sha256 = None;
        assert!(compact.expand().unwrap_err().contains("own sha256"));
        let mut compact = original;
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
        let source = expanded.to_lua().unwrap();
        assert!(source.contains("--disable-threads"));
        let mut changed = expanded.clone();
        changed
            .package
            .versions
            .get_mut(&version)
            .unwrap()
            .build
            .as_mut()
            .unwrap()
            .sha256 = "a".repeat(64);
        let repeated = PackageDefinition::from_lua(&changed.to_lua().unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&repeated.package).unwrap(),
            serde_json::to_value(&changed.package).unwrap()
        );
        assert_eq!(
            serde_json::to_value(repeated.authoring.unwrap().build).unwrap(),
            serde_json::to_value(expanded.authoring.unwrap().build).unwrap()
        );
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
                "[\"x86_64-linux\"] = \"old.zip\"",
                "[\"aarch64-linux\"] = \"old.zip\"",
            ),
            BINARY.replace("tool-{version}-linux.tar.gz", "tool-{unknown}-linux.tar.gz"),
            BINARY.replace(
                "[\"2\"] = { systems",
                "[\"2\"] = { sha256 = \"bad\", systems",
            ),
        ] {
            assert!(PackageDefinition::from_lua(&invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn command_paths_round_trip_shared_defaults_empty_overrides_and_retained_versions() {
        let source = BINARY.replace(
            "bins = { \"tool\" },",
            "bins = { \"tool\" }, bin_paths = { tool = \"Tool.app/Contents/MacOS/client\" },",
        );
        let source = source.replace("revision = 2,", "revision = 2, bin_paths = {},");
        let mut definition = PackageDefinition::from_lua(&source).unwrap();
        assert!(definition.package.versions["1"].bin_paths.is_empty());
        assert_eq!(
            definition.package.versions["2"].bin_paths["tool"],
            PathBuf::from("Tool.app/Contents/MacOS/client")
        );
        assert_eq!(
            definition.github_upstream().unwrap().unwrap().bin_paths,
            definition.package.versions["2"].bin_paths
        );
        let original = serde_json::to_value(&definition.package).unwrap();
        let repeated = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        assert_eq!(serde_json::to_value(repeated.package).unwrap(), original);

        definition
            .package
            .versions
            .get_mut("2")
            .unwrap()
            .bin_paths
            .insert("tool".into(), "new/client".into());
        let repeated = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&repeated.package).unwrap(),
            serde_json::to_value(&definition.package).unwrap()
        );
        assert_eq!(
            repeated.github_upstream().unwrap().unwrap().bin_paths["tool"],
            PathBuf::from("Tool.app/Contents/MacOS/client")
        );
    }

    #[test]
    fn mirror_defaults_require_per_version_checksums_and_preserve_overrides() {
        let source = BINARY.replace("bins =", "mirror = true, bins =");
        let source = source.replace("revision = 2,", "revision = 2, mirror = false,");
        let source = source.replace(
            "[\"2\"] = { systems",
            &format!(
                "[\"2\"] = {{ checksums = {{ [\"aarch64-macos\"] = \"{}\" }}, systems",
                "a".repeat(64)
            ),
        );
        let mut definition = PackageDefinition::from_lua(&source).unwrap();
        assert!(!definition.package.versions["1"].mirror);
        assert!(definition.package.versions["1"].checksums.is_empty());
        assert!(definition.package.versions["2"].mirror);
        let repeated = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&repeated.package).unwrap(),
            serde_json::to_value(&definition.package).unwrap()
        );
        assert!(repeated.github_upstream().unwrap().unwrap().mirror);
        definition
            .package
            .versions
            .get_mut("2")
            .unwrap()
            .checksums
            .insert("aarch64-macos".into(), "b".repeat(64));
        let repeated = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        assert_eq!(
            repeated.package.versions["2"].checksums["aarch64-macos"],
            "b".repeat(64)
        );
        assert!(PackageDefinition::from_lua(
            &source.replace("[\"2\"] = { checksums", "[\"2\"] = { unknown_checksums")
        )
        .is_err());
        assert!(
            PackageDefinition::from_lua(&BINARY.replace("bins =", "mirror = true, bins ="))
                .unwrap_err()
                .contains("complete platform checksums")
        );
    }

    #[test]
    fn old_recipes_omit_optional_binary_contract_fields() {
        let definition = PackageDefinition::from_lua(BINARY).unwrap();
        let expanded = serde_json::to_string(&definition.package).unwrap();
        let authored = definition.to_lua().unwrap();
        for name in ["bin_paths", "checksums", "mirror"] {
            assert!(!expanded.contains(name));
            assert!(!authored.contains(name));
        }
    }

    #[test]
    fn zig_templates_preserve_build_arguments_and_inline_patches() {
        let source = format!(
            r#"return {{
            name = "tool", description = "Tool", homepage = "https://example.com",
            default_version = "1", systems = {{ "aarch64-macos" }},
            build = {{
                backend = "zig", url = "https://example.com/tool-{{version}}.tar.gz",
                archive = "tar.gz", strip_prefix = "tool-{{version}}",
                args = {{ "-Doptimize=ReleaseFast" }},
                patches = {{ "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n" }},
            }},
            bins = {{ "tool" }}, checks = {{ {{ "tool", "--help" }} }},
            versions = {{ ["1"] = {{ sha256 = "{}" }} }},
        }}"#,
            "a".repeat(64)
        );
        let mut definition = PackageDefinition::from_lua(&source).unwrap();
        let build = definition.package.versions["1"].build.as_ref().unwrap();
        assert_eq!(build.args, ["-Doptimize=ReleaseFast"]);
        assert!(build.patches[0].contains("\n-old\n+new\n"));
        let repeated = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&repeated.package).unwrap(),
            serde_json::to_value(&definition.package).unwrap()
        );
        definition
            .package
            .versions
            .get_mut("1")
            .unwrap()
            .build
            .as_mut()
            .unwrap()
            .args = vec!["-Doptimize=ReleaseSafe".into()];
        let repeated = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        assert_eq!(
            repeated.package.versions["1"].build.as_ref().unwrap().args,
            ["-Doptimize=ReleaseSafe"]
        );
    }
}
