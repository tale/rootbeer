use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use mlua::LuaSerdeExt;
use serde::{Deserialize, Serialize};

pub mod lua;
use super::{CatalogPackage, GitHubUpstream};

mod compact;

/// A package's approved versions and optional update rules, stored in one Lua file.
#[derive(Debug, Clone)]
pub struct PackageDefinition {
    pub package: CatalogPackage,
    pub upstream: Option<PackageUpstream>,
    contract: Option<Contract>,
    authoring: Option<compact::CompactPackage>,
}

#[derive(Debug, Clone)]
struct Contract {
    bins: Vec<String>,
    bin_paths: BTreeMap<String, PathBuf>,
    apps: BTreeMap<String, PathBuf>,
    mirror: bool,
    checks: Vec<Vec<String>>,
}

/// Authoring metadata excluded from published catalogs and qualification fingerprints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "lowercase", deny_unknown_fields)]
pub enum PackageUpstream {
    Github {
        repository: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repository_id: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tag_prefix: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        exclude_tags: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        systems: Vec<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        assets: BTreeMap<String, String>,
    },
}

impl PackageDefinition {
    /// Wraps exact catalog recipes without update rules or shared authoring defaults.
    pub fn new(package: CatalogPackage) -> Self {
        Self {
            package,
            upstream: None,
            contract: None,
            authoring: None,
        }
    }

    /// Evaluates a package in the same bounded, I/O-free sandbox as catalog recipes.
    pub fn from_lua(source: &str) -> Result<Self, String> {
        let (lua, value) = lua::evaluate(source)?;
        let table = value.as_table().ok_or("package must return a table")?;
        if table.contains_key("source").map_err(|e| e.to_string())?
            || table.contains_key("build").map_err(|e| e.to_string())?
        {
            let package: compact::CompactPackage =
                lua.from_value(value).map_err(|e| e.to_string())?;
            return package.expand();
        }
        let upstream = lua
            .from_value(table.raw_get("upstream").map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        table.raw_remove("upstream").map_err(|e| e.to_string())?;
        let package = lua.from_value(value).map_err(|e| e.to_string())?;
        let definition = Self {
            package,
            upstream,
            contract: None,
            authoring: None,
        };
        definition.github_upstream()?;
        Ok(definition)
    }

    /// Reads canonical package files in deterministic name order.
    pub fn from_directory(directory: &Path) -> Result<BTreeMap<String, Self>, String> {
        let mut definitions = BTreeMap::new();
        for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_none_or(|extension| extension != "lua") {
                continue;
            }
            let source = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let definition =
                Self::from_lua(&source).map_err(|e| format!("{}: {e}", path.display()))?;
            if path.file_stem().and_then(|name| name.to_str()) != Some(&definition.package.name) {
                return Err(format!(
                    "{}: canonical name must match the filename",
                    path.display()
                ));
            }
            definitions.insert(definition.package.name.clone(), definition);
        }
        Ok(definitions)
    }

    /// Combines discovered rules with their package without duplicating its identity or checks.
    pub fn with_github_upstream(
        package: CatalogPackage,
        upstream: &GitHubUpstream,
    ) -> Result<Self, String> {
        let definition = Self::from_github_upstream(package, upstream)?;
        match compact::CompactPackage::from_definition(&definition)? {
            Some(authoring) => authoring.expand(),
            None => Ok(definition),
        }
    }

    fn from_github_upstream(
        package: CatalogPackage,
        upstream: &GitHubUpstream,
    ) -> Result<Self, String> {
        if package.name != upstream.name
            || package.aliases != upstream.aliases
            || upstream
                .description
                .as_ref()
                .is_some_and(|value| value != &package.description)
            || upstream
                .homepage
                .as_ref()
                .is_some_and(|value| value != &package.homepage)
        {
            return Err(format!(
                "{}: update rules must inherit the package identity",
                package.name
            ));
        }
        let systems = if package_systems(&package) == upstream.systems.iter().cloned().collect() {
            Vec::new()
        } else {
            upstream.systems.clone()
        };
        let definition = Self {
            package,
            authoring: None,
            contract: Some(Contract {
                bins: upstream.bins.clone(),
                bin_paths: upstream.bin_paths.clone(),
                apps: upstream.apps.clone(),
                mirror: upstream.mirror,
                checks: upstream.checks.clone(),
            }),
            upstream: Some(PackageUpstream::Github {
                repository: upstream.repository.clone(),
                repository_id: upstream.repository_id,
                tag_prefix: upstream.tag_prefix.clone(),
                exclude_tags: upstream.exclude_tags.clone(),
                systems,
                assets: upstream.assets.clone(),
            }),
        };
        definition.github_upstream()?;
        Ok(definition)
    }

    /// Updates resolved recipes and discovery rules while retaining authoring templates.
    pub fn with_updates(
        &self,
        package: CatalogPackage,
        upstream: &GitHubUpstream,
    ) -> Result<Self, String> {
        let mut definition = Self::from_github_upstream(package, upstream)?;
        definition.authoring = self.authoring.clone();
        Ok(definition)
    }

    /// Expands update rules using the package's canonical identity and default command contract.
    pub fn github_upstream(&self) -> Result<Option<GitHubUpstream>, String> {
        let Some(PackageUpstream::Github {
            repository,
            repository_id,
            tag_prefix,
            exclude_tags,
            systems,
            assets,
        }) = &self.upstream
        else {
            return Ok(None);
        };
        let package = &self.package;
        let recipe = package
            .versions
            .get(&package.default_version)
            .ok_or("default version has no recipe")?;
        let mut upstream = GitHubUpstream::new(
            package.name.clone(),
            repository.clone(),
            recipe.bins.clone(),
        );
        upstream.aliases = package.aliases.clone();
        upstream.description = Some(package.description.clone());
        upstream.homepage = Some(package.homepage.clone());
        upstream.checks = recipe.checks.clone();
        upstream.bin_paths = recipe.bin_paths.clone();
        upstream.apps = recipe.apps.clone();
        upstream.mirror = recipe.mirror;
        if let Some(contract) = &self.contract {
            upstream.bins = contract.bins.clone();
            upstream.bin_paths = contract.bin_paths.clone();
            upstream.apps = contract.apps.clone();
            upstream.mirror = contract.mirror;
            upstream.checks = contract.checks.clone();
        }
        upstream.repository_id = *repository_id;
        upstream.tag_prefix = tag_prefix.clone();
        upstream.exclude_tags = exclude_tags.clone();
        upstream.assets = assets.clone();
        upstream.systems = if systems.is_empty() {
            package_systems(package).into_iter().collect()
        } else {
            systems.clone()
        };
        upstream.validate()?;
        Ok(Some(upstream))
    }

    /// Renders one complete package file, including its update rules when configured.
    pub fn to_lua(&self) -> Result<String, String> {
        self.github_upstream()?;
        if let Some(authoring) = &self.authoring {
            if let Some(compact) = authoring.updated(self)? {
                return lua::write(&compact);
            }
        }
        let mut expanded = self.clone();
        expanded.contract = None;
        if serde_json::to_value(expanded.github_upstream()?).map_err(|e| e.to_string())?
            != serde_json::to_value(self.github_upstream()?).map_err(|e| e.to_string())?
        {
            return Err(
                "expanded definition cannot preserve shared discovery command or mirror contract"
                    .into(),
            );
        }
        let mut value = serde_json::to_value(&self.package).map_err(|e| e.to_string())?;
        if let Some(upstream) = &self.upstream {
            value.as_object_mut().unwrap().insert(
                "upstream".into(),
                serde_json::to_value(upstream).map_err(|e| e.to_string())?,
            );
        }
        lua::write(&value)
    }
}

fn package_systems(package: &CatalogPackage) -> BTreeSet<String> {
    package
        .versions
        .values()
        .flat_map(|recipe| recipe.systems.iter().cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PackageCatalog;

    fn definition() -> PackageDefinition {
        let package = PackageCatalog::embedded().unwrap().packages["age"].clone();
        let mut upstream = GitHubUpstream::new(
            package.name.clone(),
            "FiloSottile/age".into(),
            package.versions[&package.default_version].bins.clone(),
        );
        upstream.aliases = package.aliases.clone();
        upstream.checks = package.versions[&package.default_version].checks.clone();
        upstream.repository_id = Some(123);
        PackageDefinition::with_github_upstream(package, &upstream).unwrap()
    }

    #[test]
    fn inherits_contract_and_excludes_authoring_rules_from_catalog_identity() {
        let root = tempfile::tempdir().unwrap();
        let definition = definition();
        let expected = PackageCatalog {
            schema: 1,
            packages: BTreeMap::from([("age".into(), definition.package.clone())]),
        };
        let source = definition.to_lua().unwrap();
        fs::write(root.path().join("age.lua"), &source).unwrap();
        let catalog = PackageCatalog::from_directory(root.path()).unwrap();
        assert_eq!(catalog.sha256(), expected.sha256());
        assert!(!catalog.to_json().unwrap().contains("upstream"));
        let upstream = GitHubUpstream::from_directory(root.path())
            .unwrap()
            .remove(0);
        assert_eq!(upstream.bins, ["age", "age-keygen"]);
        assert_eq!(
            upstream.checks,
            definition.package.versions[&definition.package.default_version].checks
        );
        assert_eq!(upstream.systems.len(), 3);

        fs::write(
            root.path().join("age.lua"),
            source.replace(
                "repository_id = 123",
                "repository_id = 456, exclude_tags = { \"legacy\" }",
            ),
        )
        .unwrap();
        assert_eq!(
            PackageCatalog::from_directory(root.path())
                .unwrap()
                .sha256(),
            expected.sha256()
        );
        let upstream = GitHubUpstream::from_directory(root.path())
            .unwrap()
            .remove(0);
        assert_eq!(upstream.repository_id, Some(456));
        assert_eq!(upstream.exclude_tags, ["legacy"]);
    }

    #[test]
    fn loaded_definitions_supply_catalog_and_discovery_without_source_files() {
        let root = tempfile::tempdir().unwrap();
        let definition = definition();
        let path = root.path().join("age.lua");
        fs::write(&path, definition.to_lua().unwrap()).unwrap();
        let expected = PackageCatalog::from_directory(root.path()).unwrap();
        let definitions = PackageDefinition::from_directory(root.path()).unwrap();
        fs::remove_file(path).unwrap();

        let catalog = PackageCatalog::from_definitions(&definitions).unwrap();
        let upstreams = GitHubUpstream::from_definitions(&definitions).unwrap();
        assert_eq!(catalog.sha256(), expected.sha256());
        assert_eq!(upstreams.len(), 1);
        assert_eq!(upstreams[0].repository_id, Some(123));
        assert_eq!(upstreams[0].bins, ["age", "age-keygen"]);
    }

    #[test]
    fn validates_rules_and_rejects_duplicate_or_mismatched_identities() {
        let root = tempfile::tempdir().unwrap();
        let definition = definition();
        let source = definition.to_lua().unwrap();
        for invalid in [
            source.replace(
                "github = \"FiloSottile/age\"",
                "gitlab = \"FiloSottile/age\"",
            ),
            source.replace("repository_id", "repository_typo"),
            source.replace("homepage", "homepage_typo"),
            source.replace("repository_id = 123", "repository_id = 0"),
        ] {
            assert!(PackageDefinition::from_lua(&invalid).is_err(), "{invalid}");
        }
        let mut legacy = serde_json::to_value(&definition.package).unwrap();
        legacy["upstream"] = serde_json::to_value(&definition.upstream).unwrap();
        fs::write(
            root.path().join("age.lua"),
            lua::write(&legacy).unwrap().replace(
                "repository = \"FiloSottile/age\"",
                "repository = \"other/age\"",
            ),
        )
        .unwrap();
        assert!(PackageCatalog::from_directory(root.path()).is_err());

        fs::write(root.path().join("age.lua"), &source).unwrap();
        let mut duplicate = definition.clone();
        duplicate.package.name = "another-age".into();
        duplicate.package.aliases.clear();
        fs::write(
            root.path().join("another-age.lua"),
            duplicate.to_lua().unwrap(),
        )
        .unwrap();
        assert!(PackageCatalog::from_directory(root.path())
            .unwrap_err()
            .contains("canonical identity"));
    }

    #[test]
    fn untracked_packages_and_restricted_discovery_preserve_installable_platforms() {
        let mut definition = definition();
        let Some(PackageUpstream::Github {
            systems, assets, ..
        }) = &mut definition.upstream
        else {
            unreachable!()
        };
        *systems = vec!["aarch64-macos".into()];
        assets.retain(|system, _| systems.contains(system));
        let loaded = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        assert_eq!(
            loaded.github_upstream().unwrap().unwrap().systems,
            ["aarch64-macos"]
        );
        assert_eq!(package_systems(&loaded.package).len(), 3);
        definition.upstream = None;
        assert!(PackageDefinition::from_lua(&definition.to_lua().unwrap())
            .unwrap()
            .github_upstream()
            .unwrap()
            .is_none());
        assert!(PackageDefinition::from_lua("return os.getenv('HOME')").is_err());
    }
}
