use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use mlua::LuaSerdeExt;
use serde::{Deserialize, Serialize};

pub mod lua;
use super::CatalogPackage;

mod recipe;

/// A package's approved versions and optional update rules, stored in one Lua file.
#[derive(Debug, Clone)]
pub struct PackageDefinition {
    pub package: CatalogPackage,
    pub upstream: BTreeMap<String, PackageUpstream>,
    authoring: Option<recipe::Recipe>,
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
    },
}

impl PackageDefinition {
    /// Wraps exact catalog recipes without update rules or shared authoring defaults.
    pub fn new(package: CatalogPackage) -> Self {
        Self {
            package,
            upstream: BTreeMap::new(),
            authoring: None,
        }
    }

    /// Evaluates a package in the same bounded, I/O-free sandbox as catalog recipes.
    pub fn from_lua(source: &str) -> Result<Self, String> {
        let (lua, value) = lua::evaluate(source)?;
        let recipe: recipe::Recipe = lua.from_value(value).map_err(|error| error.to_string())?;
        recipe.expand()
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

    /// Renders one complete package file, including its update rules when configured.
    pub fn to_lua(&self) -> Result<String, String> {
        let recipe = self
            .authoring
            .as_ref()
            .ok_or("rendering a package file requires its authored recipe")?;
        lua::write(recipe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELIUM: &str = r#"return {
        name = "helium",
        description = "Browse the web",
        homepage = "https://helium.computer",
        default_license = "GPL-3.0-only",
        platforms = {
            ["aarch64-macos"] = {
                default_version = "0.17.2.1",
                upstream = { github = "imputnet/helium-macos" },
                prebuilt = { github = "imputnet/helium-macos", asset = "helium-{version}-arm64.dmg" },
                outputs = { apps = { ["Helium.app"] = "Helium.app" } },
            },
            ["x86_64-linux"] = {
                default_version = "0.17.2.1",
                upstream = { github = "imputnet/helium-linux" },
                prebuilt = { github = "imputnet/helium-linux", asset = "helium-{version}-x86_64.AppImage" },
                outputs = { bins = { "helium" }, checks = { { "helium", "--version" } } },
            },
        },
        versions = {
            ["0.17.2.1"] = { digests = { ["aarch64-macos"] = "aa", ["x86_64-linux"] = "bb" } },
        },
    }"#;

    #[test]
    fn a_package_keeps_one_upstream_per_platform() {
        let definition = PackageDefinition::from_lua(HELIUM).unwrap();
        let repositories: Vec<_> = definition
            .upstream
            .values()
            .map(|PackageUpstream::Github { repository, .. }| repository.as_str())
            .collect();
        assert_eq!(
            repositories,
            ["imputnet/helium-macos", "imputnet/helium-linux"]
        );
    }

    #[test]
    fn each_platform_resolves_its_own_asset_and_outputs() {
        let definition = PackageDefinition::from_lua(HELIUM).unwrap();
        let entry = &definition.package.versions["0.17.2.1"];

        let macos = entry.for_system("aarch64-macos").unwrap();
        assert_eq!(macos.asset.as_deref(), Some("helium-0.17.2.1-arm64.dmg"));
        assert!(macos.apps.contains_key("Helium.app") && macos.bins.is_empty());

        let linux = entry.for_system("x86_64-linux").unwrap();
        assert_eq!(
            linux.asset.as_deref(),
            Some("helium-0.17.2.1-x86_64.AppImage")
        );
        assert!(linux.bins.names().contains(&"helium".to_string()) && linux.apps.is_empty());
    }

    #[test]
    fn a_version_only_builds_the_platforms_it_carries_a_digest_for() {
        let source = HELIUM.replace(
            r#"digests = { ["aarch64-macos"] = "aa", ["x86_64-linux"] = "bb" }"#,
            r#"digests = { ["aarch64-macos"] = "aa" }"#,
        );
        let definition = PackageDefinition::from_lua(&source).unwrap_err();
        assert!(definition.contains("x86_64-linux"), "{definition}");
    }

    #[test]
    fn rendering_round_trips_through_the_authored_recipe() {
        let definition = PackageDefinition::from_lua(HELIUM).unwrap();
        let rendered = definition.to_lua().unwrap();
        let again = PackageDefinition::from_lua(&rendered).unwrap();
        assert_eq!(again.package, definition.package);
    }
}
