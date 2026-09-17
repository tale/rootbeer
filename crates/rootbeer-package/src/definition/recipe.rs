use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Contract, PackageDefinition, PackageUpstream};
use crate::{CatalogPackage, CatalogRecipe, PackageRequest};

mod render;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecipeDefinition {
    schema: u32,
    name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    aliases: Vec<String>,
    description: String,
    homepage: String,
    default_version: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    default_versions: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    upstream: Option<Discovery>,
    #[serde(default)]
    inputs: Inputs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    build: Option<Build>,
    systems: Vec<String>,
    outputs: Outputs,
    versions: BTreeMap<String, Version>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Discovery {
    github: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repository_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tag_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    exclude_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    systems: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Inputs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prebuilt: Option<Prebuilt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<Source>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Prebuilt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    systems: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    aqua: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    github: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    assets: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checksums: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mirror: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    git: Option<crate::GitSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    archive: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    strip_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    patches: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Build {
    backend: crate::BuildBackend,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rust: Option<crate::RustBuild>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    configure: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dependencies: Vec<crate::BuildDependency>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    steps: Option<crate::BuildSteps>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Outputs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bins: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bin_paths: Option<BTreeMap<String, PathBuf>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    apps: Option<BTreeMap<String, PathBuf>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    libraries: Option<Vec<PathBuf>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checks: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Version {
    #[serde(default = "one", skip_serializing_if = "is_one")]
    revision: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    inputs: Option<Inputs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    build: Option<Build>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    systems: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    outputs: Option<Outputs>,
}

fn one() -> u32 {
    1
}
fn is_one(value: &u32) -> bool {
    *value == 1
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

fn field<T: Clone>(shared: &Option<T>, version: &Option<T>) -> Option<T> {
    version.as_ref().or(shared.as_ref()).cloned()
}

impl Outputs {
    fn overlay(&self, version: &Self) -> Self {
        Self {
            bins: field(&self.bins, &version.bins),
            bin_paths: field(&self.bin_paths, &version.bin_paths),
            apps: field(&self.apps, &version.apps),
            libraries: field(&self.libraries, &version.libraries),
            checks: field(&self.checks, &version.checks),
        }
    }
}

impl RecipeDefinition {
    pub(super) fn expand(&self) -> Result<PackageDefinition, String> {
        if self.schema != 2 {
            return Err("package definitions require schema = 2".into());
        }
        crate::catalog::validate_systems(&self.systems)?;
        if !self.versions.contains_key(&self.default_version) {
            return Err("default version has no recipe".into());
        }
        if self.inputs.prebuilt.is_none() && self.inputs.source.is_none() {
            return Err("inputs require source, prebuilt, or both".into());
        }
        if self
            .inputs
            .source
            .as_ref()
            .is_some_and(|source| source.sha256.is_some())
        {
            return Err("source hashes belong to each version's inputs".into());
        }
        if self
            .inputs
            .prebuilt
            .as_ref()
            .is_some_and(|input| input.checksums.is_some())
        {
            return Err("prebuilt checksums belong to each version's inputs".into());
        }
        let versions = self
            .versions
            .iter()
            .map(|(version, entry)| {
                self.expand_version(version, entry)
                    .map(|recipe| (version.clone(), recipe))
                    .map_err(|error| format!("{}@{version}: {error}", self.name))
            })
            .collect::<Result<_, _>>()?;
        let upstream = self.upstream.as_ref().map(|upstream| {
            let assets = self
                .inputs
                .prebuilt
                .as_ref()
                .and_then(|input| input.assets.clone())
                .unwrap_or_default();
            let mut systems = upstream
                .systems
                .clone()
                .unwrap_or_else(|| self.systems.clone());
            systems.sort();
            PackageUpstream::Github {
                repository: upstream.github.clone(),
                repository_id: upstream.repository_id,
                tag_prefix: upstream.tag_prefix.clone(),
                exclude_tags: upstream.exclude_tags.clone(),
                assets: assets
                    .into_iter()
                    .filter(|(system, _)| systems.contains(system))
                    .collect(),
                systems,
            }
        });
        let definition = PackageDefinition {
            package: CatalogPackage {
                name: self.name.clone(),
                aliases: self.aliases.clone(),
                description: self.description.clone(),
                homepage: self.homepage.clone(),
                default_version: self.default_version.clone(),
                default_versions: self.default_versions.clone(),
                versions,
            },
            upstream,
            contract: Some(Contract {
                bins: self.outputs.bins.clone().unwrap_or_default(),
                bin_paths: self.outputs.bin_paths.clone().unwrap_or_default(),
                apps: self.outputs.apps.clone().unwrap_or_default(),
                mirror: self
                    .inputs
                    .prebuilt
                    .as_ref()
                    .and_then(|input| input.mirror)
                    .unwrap_or(false),
                checks: self.outputs.checks.clone().unwrap_or_default(),
            }),
            authoring: Some(self.clone()),
        };
        definition.github_upstream()?;
        Ok(definition)
    }

    fn source_tag(&self, version: &str) -> Result<Option<String>, String> {
        let source = self.inputs.source.as_ref();
        let uses_tag = source.is_some_and(|input| {
            input
                .url
                .iter()
                .chain(input.strip_prefix.iter())
                .any(|value| value.contains("{tag}"))
        });
        let prefix = self
            .upstream
            .as_ref()
            .and_then(|upstream| upstream.tag_prefix.as_ref());
        if uses_tag && prefix.is_none() {
            return Err("source {tag} templates require upstream.tag_prefix".into());
        }
        Ok(prefix.map(|prefix| format!("{prefix}{version}")))
    }

    pub(super) fn source_template(&self) -> Result<Option<crate::SourceBuild>, String> {
        let Some(source) = &self.inputs.source else {
            return Ok(None);
        };
        if self.build.is_none() {
            return Err("source discovery requires a shared build definition".into());
        }
        let recipe = self.expand_version(
            &self.default_version,
            self.versions
                .get(&self.default_version)
                .ok_or("default version has no recipe")?,
        )?;
        let checksum = recipe.build.ok_or("source inputs require a build")?.sha256;
        let entry = Version {
            revision: 1,
            inputs: Some(Inputs {
                source: Some(Source {
                    sha256: Some(checksum),
                    ..Source::default()
                }),
                prebuilt: None,
            }),
            ..Version::default()
        };
        let mut build = self
            .expand_version(&self.default_version, &entry)?
            .build
            .ok_or("source inputs require a build")?;
        build.url = source.url.clone().ok_or("source input requires a URL")?;
        build.strip_prefix = source
            .strip_prefix
            .clone()
            .ok_or("source input requires strip_prefix")?
            .into();
        if !build.url.contains("{version}") && !build.url.contains("{tag}") {
            return Err(
                "source discovery requires a URL template containing {version} or {tag}".into(),
            );
        }
        Ok(Some(build))
    }

    fn expand_version(&self, version: &str, entry: &Version) -> Result<CatalogRecipe, String> {
        let inputs = entry.inputs.clone().unwrap_or_default();
        let outputs = self
            .outputs
            .overlay(&entry.outputs.clone().unwrap_or_default());
        let mut recipe = CatalogRecipe {
            revision: entry.revision,
            source: None,
            build: None,
            assets: BTreeMap::new(),
            systems: entry
                .systems
                .clone()
                .unwrap_or_else(|| self.systems.clone()),
            bins: outputs.bins.unwrap_or_default(),
            bin_paths: outputs.bin_paths.unwrap_or_default(),
            apps: outputs.apps.unwrap_or_default(),
            checks: outputs.checks.unwrap_or_default(),
            checksums: BTreeMap::new(),
            mirror: false,
        };
        if let Some(shared) = self.inputs.prebuilt.as_ref().filter(|shared| {
            inputs
                .prebuilt
                .as_ref()
                .and_then(|input| input.enabled)
                .or(shared.enabled)
                != Some(false)
        }) {
            if self.inputs.source.is_none()
                && (inputs.source.is_some() || self.build.is_some() || entry.build.is_some())
            {
                return Err("prebuilt inputs cannot have a source build".into());
            }
            if self.inputs.source.is_none()
                && outputs
                    .libraries
                    .as_ref()
                    .is_some_and(|libraries| !libraries.is_empty())
            {
                return Err(
                    "prebuilt library exports require a source-defined build contract".into(),
                );
            }
            let input = inputs.prebuilt.clone().unwrap_or_default();
            let github = field(&shared.github, &input.github);
            let aqua = field(&shared.aqua, &input.aqua);
            let (provider, repository) = match (github, aqua) {
                (Some(repository), None) => ("github", repository),
                (None, Some(repository)) => ("aqua", repository),
                _ => {
                    return Err(
                        "prebuilt input requires exactly one provider: github or aqua".into(),
                    )
                }
            };
            crate::github::repository(&repository)?;
            let tag = expand(
                &field(&shared.tag, &input.tag).ok_or("prebuilt input requires a tag")?,
                version,
                None,
            )?;
            recipe.source = Some(format!("{provider}:{repository}@{tag}"));
            let mut assets = shared.assets.clone().unwrap_or_default();
            for (system, pattern) in &assets {
                if !matches!(
                    system.as_str(),
                    "aarch64-linux" | "x86_64-linux" | "aarch64-macos"
                ) {
                    return Err("prebuilt assets reference an unsupported platform".into());
                }
                expand(pattern, version, Some(&tag))?;
            }
            for (system, asset) in input.assets.unwrap_or_default() {
                if !recipe.systems.contains(&system) {
                    return Err("asset override references an undeclared platform".into());
                }
                assets.insert(system, asset);
            }
            let prebuilt_systems =
                field(&shared.systems, &input.systems).unwrap_or_else(|| recipe.systems.clone());
            if prebuilt_systems
                .iter()
                .any(|system| !recipe.systems.contains(system))
            {
                return Err("prebuilt systems must be supported by the recipe".into());
            }
            for system in prebuilt_systems.iter().filter(|_| provider == "github") {
                let Some(asset) = assets.get(system) else {
                    if self.inputs.source.is_some() {
                        continue;
                    }
                    return Err(format!("missing asset for {system}"));
                };
                recipe
                    .assets
                    .insert(system.clone(), expand(asset, version, Some(&tag))?);
            }
            recipe.checksums = input.checksums.unwrap_or_default();
            recipe.mirror = field(&shared.mirror, &input.mirror).unwrap_or(false);
            if provider == "aqua" && !assets.is_empty() {
                return Err("aqua inputs cannot declare GitHub release assets".into());
            }
        }
        if let Some(shared) = &self.inputs.source {
            if self.inputs.prebuilt.is_none() && inputs.prebuilt.is_some() {
                return Err("source inputs cannot contain prebuilt overrides".into());
            }
            let input = inputs.source.unwrap_or_default();
            let build = entry
                .build
                .as_ref()
                .or(self.build.as_ref())
                .ok_or("source inputs require a build")?;
            let tag = self.source_tag(version)?;
            let mut value = serde_json::to_value(build).map_err(|error| error.to_string())?;
            value["git"] = serde_json::to_value(field(&shared.git, &input.git))
                .map_err(|error| error.to_string())?;
            value["url"] = expand(
                &field(&shared.url, &input.url).ok_or("source input requires a URL")?,
                version,
                tag.as_deref(),
            )?
            .into();
            value["archive"] = field(&shared.archive, &input.archive)
                .ok_or("source input requires an archive format")?
                .into();
            value["strip_prefix"] = expand(
                &field(&shared.strip_prefix, &input.strip_prefix)
                    .ok_or("source input requires strip_prefix")?,
                version,
                tag.as_deref(),
            )?
            .into();
            value["sha256"] = input
                .sha256
                .ok_or("each source version requires its own sha256")?
                .into();
            value["patches"] =
                serde_json::to_value(field(&shared.patches, &input.patches).unwrap_or_default())
                    .map_err(|error| error.to_string())?;
            value["libraries"] = serde_json::to_value(outputs.libraries.unwrap_or_default())
                .map_err(|error| error.to_string())?;
            recipe.build = Some(serde_json::from_value(value).map_err(|error| error.to_string())?);
        }
        recipe.validate()?;
        Ok(recipe)
    }
}
