//! Platform-first recipe authoring.
//!
//! A recipe is an identity plus one build definition per platform. Map keys are the
//! systems, so nothing declares `systems`; a version is a digest per platform, so
//! nothing declares which platforms a version covers.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Recipe {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub description: String,
    pub homepage: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipe_maintainers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_engine_level: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_license: Option<String>,

    #[serde(flatten)]
    pub shared: Spec,

    pub platforms: BTreeMap<String, Platform>,
    pub versions: BTreeMap<String, Version>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Spec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prebuilt: Option<Prebuilt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<crate::SourceBuild>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Outputs>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Platform {
    /// Substituted for `{target}` in this platform's templates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub default_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<Upstream>,
    #[serde(flatten)]
    pub overrides: Spec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Provider {
    Github(String),
    Url(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Prebuilt {
    #[serde(flatten)]
    pub provider: Provider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<crate::LockedInstall>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mirror: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Source {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<crate::GitSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strip_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Upstream {
    pub github: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Outputs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bins: Option<crate::Bins>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apps: Option<BTreeMap<String, PathBuf>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Version {
    pub digests: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub revision: u32,
    #[serde(flatten)]
    pub overrides: Spec,
}

fn one() -> u32 {
    1
}

fn is_one(value: &u32) -> bool {
    *value == 1
}

fn substitute(
    pattern: &str,
    version: &str,
    tag: &str,
    target: Option<&str>,
) -> Result<String, String> {
    let mut value = pattern.replace("{version}", version).replace("{tag}", tag);
    if let Some(target) = target {
        value = value.replace("{target}", target);
    }
    if value.contains(['{', '}']) {
        return Err(format!("unsupported placeholder in `{pattern}`"));
    }
    Ok(value)
}

impl Spec {
    /// Platform and version overrides replace a shared field outright.
    fn overlay(&self, over: &Self) -> Self {
        Self {
            prebuilt: over.prebuilt.clone().or_else(|| self.prebuilt.clone()),
            source: over.source.clone().or_else(|| self.source.clone()),
            build: over.build.clone().or_else(|| self.build.clone()),
            outputs: match (&self.outputs, &over.outputs) {
                (Some(base), Some(over)) => Some(base.overlay(over)),
                (base, over) => over.clone().or_else(|| base.clone()),
            },
        }
    }
}

impl Outputs {
    fn overlay(&self, over: &Self) -> Self {
        Self {
            bins: over.bins.clone().or_else(|| self.bins.clone()),
            apps: over.apps.clone().or_else(|| self.apps.clone()),
            checks: over.checks.clone().or_else(|| self.checks.clone()),
        }
    }
}

impl Recipe {
    pub(super) fn expand(&self) -> Result<super::PackageDefinition, String> {
        if self.platforms.is_empty() {
            return Err("a recipe declares at least one platform".into());
        }
        let mut versions: BTreeMap<String, crate::CatalogVersion> = BTreeMap::new();
        for (version, entry) in &self.versions {
            let mut platforms = BTreeMap::new();
            for (system, digest) in &entry.digests {
                let platform = self.platforms.get(system).ok_or_else(|| {
                    format!(
                        "{}@{version}: {system} has a digest but no platform",
                        self.name
                    )
                })?;
                let spec = self
                    .shared
                    .overlay(&platform.overrides)
                    .overlay(&entry.overrides);
                platforms.insert(
                    system.clone(),
                    self.resolve(version, digest, platform, &spec)
                        .map_err(|error| format!("{}@{version} {system}: {error}", self.name))?,
                );
            }
            versions.insert(
                version.clone(),
                crate::CatalogVersion {
                    license: entry
                        .license
                        .clone()
                        .or_else(|| self.default_license.clone())
                        .ok_or_else(|| format!("{}@{version}: no license", self.name))?,
                    revision: entry.revision,
                    platforms,
                    extra: Default::default(),
                },
            );
        }

        let default_versions = self
            .platforms
            .iter()
            .map(|(system, platform)| (system.clone(), platform.default_version.clone()))
            .collect();
        let package = crate::CatalogPackage {
            name: self.name.clone(),
            aliases: self.aliases.clone(),
            description: self.description.clone(),
            homepage: self.homepage.clone(),
            recipe_maintainers: self.recipe_maintainers.clone(),
            min_engine_level: self.min_engine_level,
            default_versions,
            versions,
            extra: Default::default(),
        };
        package.validate()?;
        Ok(super::PackageDefinition {
            package,
            upstream: self.upstream(),
            authoring: Some(self.clone()),
        })
    }

    fn resolve(
        &self,
        version: &str,
        digest: &str,
        platform: &Platform,
        spec: &Spec,
    ) -> Result<crate::CatalogRecipe, String> {
        if spec.prebuilt.is_some() && spec.source.is_some() {
            return Err("a platform is prebuilt or built from source, not both".into());
        }
        let prefix = platform
            .upstream
            .as_ref()
            .and_then(|upstream| upstream.tag_prefix.as_deref())
            .unwrap_or_default();
        let tag = format!("{prefix}{version}");
        let target = platform.target.as_deref();
        let outputs = spec.outputs.clone().unwrap_or_default();

        let mut recipe = crate::CatalogRecipe {
            source: None,
            install: None,
            build: None,
            asset: None,
            sha256: Some(digest.to_string()),
            bins: outputs.bins.clone().unwrap_or_default(),
            apps: outputs.apps.clone().unwrap_or_default(),
            checks: outputs.checks.clone().unwrap_or_default(),
            mirror: false,
            extra: Default::default(),
        };

        if let Some(prebuilt) = &spec.prebuilt {
            recipe.mirror = prebuilt.mirror;
            recipe.install = prebuilt.install.clone();
            match &prebuilt.provider {
                Provider::Github(repository) => {
                    crate::github::repository(repository)?;
                    let tag = match &prebuilt.tag {
                        Some(pattern) => substitute(pattern, version, &tag, target)?,
                        None => tag.clone(),
                    };
                    recipe.source = Some(format!("github:{repository}@{tag}"));
                    let asset = prebuilt
                        .asset
                        .as_deref()
                        .ok_or("a github prebuilt needs a release asset")?;
                    recipe.asset = Some(substitute(asset, version, &tag, target)?);
                }
                Provider::Url(url) => {
                    recipe.source = Some(substitute(url, version, &tag, target)?);
                }
            }
        } else if let Some(source) = &spec.source {
            let build = spec
                .build
                .as_ref()
                .ok_or("a source platform needs a build")?;
            let mut build = build.clone();
            build.sha256 = digest.to_string();
            build.url = substitute(
                source
                    .url
                    .as_deref()
                    .ok_or("a source platform needs a URL")?,
                version,
                &tag,
                target,
            )?;
            build.strip_prefix = source
                .strip_prefix
                .as_deref()
                .map(|prefix| substitute(prefix, version, &tag, target))
                .transpose()?
                .map(PathBuf::from)
                .unwrap_or_default();
            build.git = source.git.clone();
            build.patches = source.patches.clone();
            recipe.build = Some(build);
        } else {
            return Err("a platform needs a prebuilt or a source".into());
        }
        Ok(recipe)
    }

    fn upstream(&self) -> BTreeMap<String, super::PackageUpstream> {
        self.platforms
            .iter()
            .filter_map(|(system, platform)| {
                let rules = platform.upstream.as_ref()?;
                Some((
                    system.clone(),
                    super::PackageUpstream::Github {
                        repository: rules.github.clone(),
                        repository_id: rules.repository_id,
                        tag_prefix: rules.tag_prefix.clone(),
                        exclude_tags: rules.exclude_tags.clone(),
                    },
                ))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Recipe {
        serde_json::from_str(json).unwrap_or_else(|error| panic!("{error}\n{json}"))
    }

    #[test]
    fn a_shared_upstream_varies_only_by_target() {
        let recipe = parse(
            r#"{
              "name": "fd", "description": "Find entries", "homepage": "https://x",
              "recipe_maintainers": ["tale"],
              "prebuilt": { "github": "sharkdp/fd", "tag": "v{version}",
                            "asset": "fd-v{version}-{target}.tar.gz" },
              "outputs": { "bins": ["fd"], "checks": [["fd", "--version"]] },
              "platforms": {
                "aarch64-macos": { "target": "aarch64-apple-darwin", "default_version": "10.5.0" },
                "x86_64-linux": { "target": "x86_64-unknown-linux-musl", "default_version": "10.5.0" }
              },
              "versions": {
                "10.5.0": { "license": "MIT OR Apache-2.0",
                            "digests": { "aarch64-macos": "aa", "x86_64-linux": "bb" } }
              }
            }"#,
        );
        assert_eq!(recipe.platforms.len(), 2);
        assert!(matches!(
            recipe.shared.prebuilt.as_ref().unwrap().provider,
            Provider::Github(ref repo) if repo == "sharkdp/fd"
        ));
        assert_eq!(recipe.versions["10.5.0"].digests.len(), 2);
    }

    #[test]
    fn platforms_may_carry_their_own_upstream_and_outputs() {
        let recipe = parse(
            r#"{
              "name": "helium", "description": "Browse", "homepage": "https://x",
              "platforms": {
                "aarch64-macos": {
                  "default_version": "0.17.2.1",
                  "upstream": { "github": "imputnet/helium-macos" },
                  "prebuilt": { "github": "imputnet/helium-macos",
                                "asset": "helium_{version}_arm64-macos.dmg", "install": "Dmg" },
                  "outputs": { "apps": { "Helium.app": "Helium.app" } }
                },
                "x86_64-linux": {
                  "default_version": "0.17.2.1",
                  "upstream": { "github": "imputnet/helium-linux" },
                  "prebuilt": { "github": "imputnet/helium-linux",
                                "asset": "helium-{tag}-x86_64.AppImage" },
                  "outputs": { "bins": ["helium"] }
                }
              },
              "versions": {
                "0.17.2.1": { "license": "GPL-3.0-only",
                              "digests": { "aarch64-macos": "aa", "x86_64-linux": "bb" } }
              }
            }"#,
        );
        let mac = &recipe.platforms["aarch64-macos"];
        assert_eq!(
            mac.upstream.as_ref().unwrap().github,
            "imputnet/helium-macos"
        );
        assert!(mac.overrides.outputs.as_ref().unwrap().apps.is_some());
    }

    #[test]
    fn a_version_covering_one_platform_needs_no_systems_field() {
        let recipe = parse(
            r#"{
              "name": "ghostty", "description": "Terminal", "homepage": "https://x",
              "prebuilt": { "url": "https://r/{version}/Ghostty.dmg", "install": "Dmg" },
              "outputs": { "apps": { "Ghostty.app": "Ghostty.app" } },
              "platforms": { "aarch64-macos": { "default_version": "1.3.1" } },
              "versions": { "1.3.1": { "license": "MIT", "digests": { "aarch64-macos": "aa" } } }
            }"#,
        );
        assert_eq!(
            recipe.versions["1.3.1"].digests.keys().collect::<Vec<_>>(),
            vec!["aarch64-macos"]
        );
    }

    #[test]
    fn bins_accept_a_list_or_a_map_of_paths() {
        let named: Outputs = serde_json::from_str(r#"{"bins":["fd"]}"#).unwrap();
        assert!(matches!(named.bins, Some(crate::Bins::Names(ref v)) if v == &["fd"]));

        let located: Outputs =
            serde_json::from_str(r#"{"bins":{"kitty":"kitty.app/Contents/MacOS/kitty"}}"#).unwrap();
        assert!(matches!(located.bins, Some(crate::Bins::Paths(ref m)) if m.contains_key("kitty")));
    }

    #[test]
    fn a_provider_cannot_be_both_github_and_url() {
        let both = serde_json::from_str::<Prebuilt>(r#"{"github":"a/b","url":"https://x"}"#);
        assert!(both.is_err(), "two providers must not deserialize");
    }
}
