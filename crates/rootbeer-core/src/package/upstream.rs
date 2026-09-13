use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::catalog::valid_name;
use super::download::read_json_url;
use super::github::{repository, Release};
use super::{CatalogPackage, PackageCatalog, PackageRequest};

mod generate;
mod lua;

/// Authoring rules for a canonical package imported from GitHub releases.
/// Stored separately from installable recipes and signed catalog snapshots.
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
    #[serde(default = "all_systems")]
    pub systems: Vec<String>,
    /// Exact asset names with optional {tag} and {version} substitutions.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<String, String>,
    pub bins: Vec<String>,
    pub checks: Vec<Vec<String>>,
}

fn all_systems() -> Vec<String> {
    [
        "aarch64-linux",
        "aarch64-macos",
        "x86_64-linux",
        "x86_64-macos",
    ]
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
            systems: all_systems(),
            assets: BTreeMap::new(),
            bins,
            checks,
        }
    }

    /// Loads sandboxed Lua rules, requiring canonical filenames and unique identities.
    pub fn from_directory(directory: &Path) -> Result<Vec<Self>, String> {
        let mut definitions = BTreeMap::new();
        for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_none_or(|extension| extension != "lua") {
                continue;
            }
            let source = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let upstream: Self = lua::read(&source)?;
            if path.file_stem().and_then(|name| name.to_str()) != Some(&upstream.name) {
                return Err(format!(
                    "{}: canonical name must match filename",
                    path.display()
                ));
            }
            if definitions
                .insert(upstream.name.clone(), upstream)
                .is_some()
            {
                return Err("duplicate upstream definition".into());
            }
        }
        let definitions: Vec<_> = definitions.into_values().collect();
        validate_definitions(&definitions)?;
        Ok(definitions)
    }

    fn validate(&self) -> Result<(), String> {
        repository(&self.repository)?;
        if !valid_name(&self.name) || self.repository_id == Some(0) {
            return Err("invalid canonical name or repository ID".into());
        }
        let recipe = super::CatalogRecipe {
            revision: 1,
            source: Some(format!("github:{}@v1", self.repository)),
            build: None,
            assets: BTreeMap::new(),
            systems: self.systems.clone(),
            bins: self.bins.clone(),
            checks: self.checks.clone(),
        };
        let package = CatalogPackage {
            name: self.name.clone(),
            aliases: self.aliases.clone(),
            description: self
                .description
                .clone()
                .unwrap_or_else(|| self.name.clone()),
            homepage: self
                .homepage
                .clone()
                .unwrap_or_else(|| format!("https://github.com/{}", self.repository)),
            default_version: "1".into(),
            default_versions: BTreeMap::new(),
            versions: BTreeMap::from([("1".into(), recipe)]),
        };
        PackageCatalog {
            schema: 1,
            packages: BTreeMap::from([(self.name.clone(), package)]),
        }
        .validate()?;
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

#[derive(Debug, Deserialize)]
struct Repository {
    id: u64,
    full_name: String,
    description: Option<String>,
    homepage: Option<String>,
}

fn validate_definitions(definitions: &[GitHubUpstream]) -> Result<(), String> {
    if definitions.is_empty() {
        return Err("no upstream definitions supplied".into());
    }
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

fn check_identity(catalog: &PackageCatalog, upstream: &GitHubUpstream) -> Result<(), String> {
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
        if package.name == upstream.name && !is_same_repository {
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

/// Generates a new candidate directory containing pinned recipes and reusable upstream rules.
/// Existing catalogs and output paths are never overwritten. Candidates still require export checks.
pub fn import_github_packages(
    catalog: &PackageCatalog,
    definitions: &[GitHubUpstream],
    output: &Path,
    max_pages: usize,
) -> Result<PackageCatalog, String> {
    import_with_fetch(catalog, definitions, output, max_pages, |url| {
        read_json_url(url).map_err(|e| e.to_string())
    })
}

fn import_with_fetch(
    catalog: &PackageCatalog,
    definitions: &[GitHubUpstream],
    output: &Path,
    max_pages: usize,
    mut fetch: impl FnMut(&str) -> Result<serde_json::Value, String>,
) -> Result<PackageCatalog, String> {
    validate_definitions(definitions)?;
    if !(1..=100).contains(&max_pages) {
        return Err("max-pages must be between 1 and 100".into());
    }
    for upstream in definitions {
        check_identity(catalog, upstream)?;
    }
    let staging = super::publication::staging(output)?;
    let mut resolved = Vec::new();
    let mut candidates = PackageCatalog {
        schema: 1,
        packages: BTreeMap::new(),
    };
    let mut combined = catalog.clone();
    for definition in definitions {
        eprintln!(
            "Discover {} from {}",
            definition.name, definition.repository
        );
        let mut upstream = definition.clone();
        let url = format!("https://api.github.com/repos/{}", upstream.repository);
        let repository: Repository =
            serde_json::from_value(fetch(&url)?).map_err(|e| e.to_string())?;
        if repository.id == 0 || upstream.repository_id.is_some_and(|id| id != repository.id) {
            return Err(format!(
                "{}: GitHub repository ID changed; review upstream ownership",
                upstream.name
            ));
        }
        if !upstream
            .repository
            .eq_ignore_ascii_case(&repository.full_name)
        {
            return Err(format!(
                "{}: repository moved to {}; review the identity mapping",
                upstream.name, repository.full_name
            ));
        }
        upstream.repository_id = Some(repository.id);
        let mut releases = Vec::new();
        for page in 1..=max_pages {
            let batch: Vec<Release> =
                serde_json::from_value(fetch(&format!("{url}/releases?per_page=100&page={page}"))?)
                    .map_err(|e| e.to_string())?;
            let is_complete = batch.len() < 100;
            releases.extend(batch);
            if is_complete {
                break;
            }
            if page == max_pages {
                return Err(format!("{}: release history exceeds --max-pages {max_pages}; increase it to avoid incomplete version selection", upstream.name));
            }
        }
        let existing = catalog.packages.get(&upstream.name);
        let package = generate::package(&mut upstream, &repository, &releases, existing)?;
        combined
            .packages
            .insert(package.name.clone(), package.clone());
        candidates.packages.insert(package.name.clone(), package);
        resolved.push(upstream);
    }
    validate_definitions(&resolved)?;
    combined.validate()?;
    candidates.validate()?;
    write_candidates(staging.path(), &candidates, &resolved, output)?;
    Ok(candidates)
}

fn write_candidates(
    staging: &Path,
    candidates: &PackageCatalog,
    definitions: &[GitHubUpstream],
    output: &Path,
) -> Result<(), String> {
    let root = staging.join("candidates");
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    for folder in ["recipes", "upstreams"] {
        fs::create_dir(root.join(folder)).map_err(|e| e.to_string())?;
    }
    for package in candidates.packages.values() {
        fs::write(
            root.join("recipes").join(format!("{}.lua", package.name)),
            lua::write(package)?,
        )
        .map_err(|e| e.to_string())?;
    }
    for upstream in definitions {
        fs::write(
            root.join("upstreams")
                .join(format!("{}.lua", upstream.name)),
            lua::write(upstream)?,
        )
        .map_err(|e| e.to_string())?;
    }
    let loaded = PackageCatalog::from_directory(&root.join("recipes"))?;
    if loaded.sha256() != candidates.sha256() {
        return Err("generated Lua differs from candidate catalog".into());
    }
    GitHubUpstream::from_directory(&root.join("upstreams"))?;
    fs::rename(root, output).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: usize) -> serde_json::Value {
        serde_json::json!({
            "id": version, "tag_name": format!("v{version}"),
            "assets": [{"name": format!("tool-v{version}-darwin-arm64.tar.gz"),
                "browser_download_url": "https://example.com/tool"}]
        })
    }

    #[test]
    fn paginates_metadata_and_never_publishes_incomplete_batches() {
        let root = tempfile::tempdir().unwrap();
        let catalog = PackageCatalog::embedded().unwrap();
        let mut upstream =
            GitHubUpstream::new("tool".into(), "owner/tool".into(), vec!["tool".into()]);
        upstream.systems = vec!["aarch64-macos".into()];
        let fetch = |url: &str| {
            if url.ends_with("/repos/owner/tool") {
                return Ok(
                    serde_json::json!({"id": 42, "full_name": "owner/tool", "description": "Tool"}),
                );
            }
            if url.ends_with("page=1") {
                return Ok(serde_json::Value::Array((1..=100).map(release).collect()));
            }
            assert!(url.ends_with("page=2"));
            Ok(serde_json::json!([release(101)]))
        };
        let output = root.path().join("complete");
        let candidates =
            import_with_fetch(catalog, &[upstream.clone()], &output, 2, fetch).unwrap();
        assert_eq!(candidates.packages["tool"].default_version, "101");
        let saved = GitHubUpstream::from_directory(&output.join("upstreams")).unwrap();
        assert_eq!(saved[0].repository_id, Some(42));

        let output = root.path().join("incomplete");
        assert!(
            import_with_fetch(catalog, &[upstream.clone()], &output, 1, fetch)
                .unwrap_err()
                .contains("exceeds --max-pages")
        );
        assert!(!output.exists());
        let mut other = upstream.clone();
        other.name = "other".into();
        other.repository = "owner/other".into();
        assert!(
            import_with_fetch(catalog, &[upstream, other], &output, 2, |url| {
                if url.ends_with("/repos/owner/other") {
                    return Err("upstream unavailable".into());
                }
                fetch(url)
            })
            .unwrap_err()
            .contains("upstream unavailable")
        );
        assert!(!output.exists());
    }

    #[test]
    fn rejects_replaced_or_moved_repositories_before_reading_releases() {
        let root = tempfile::tempdir().unwrap();
        let catalog = PackageCatalog::embedded().unwrap();
        let mut upstream =
            GitHubUpstream::new("tool".into(), "owner/tool".into(), vec!["tool".into()]);
        upstream.repository_id = Some(42);
        for (id, name, expected) in [
            (43, "owner/tool", "ID changed"),
            (42, "other/tool", "repository moved"),
        ] {
            let error = import_with_fetch(
                catalog,
                &[upstream.clone()],
                &root.path().join("output"),
                1,
                |url| {
                    assert!(!url.contains("/releases"));
                    Ok(serde_json::json!({"id": id, "full_name": name}))
                },
            )
            .unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
    }

    #[test]
    fn rejects_duplicate_names_aliases_repositories_and_ids() {
        let first = GitHubUpstream::new("one".into(), "owner/one".into(), vec!["one".into()]);
        let second = GitHubUpstream::new("two".into(), "owner/two".into(), vec!["two".into()]);
        assert!(validate_definitions(&[first.clone(), second.clone()]).is_ok());
        let mut collision = second.clone();
        collision.aliases.push("one".into());
        assert!(validate_definitions(&[first.clone(), collision]).is_err());
        let mut collision = second.clone();
        collision.repository = "OWNER/One".into();
        assert!(validate_definitions(&[first.clone(), collision]).is_err());
        let mut first = first;
        first.repository_id = Some(42);
        let mut second = second;
        second.repository_id = Some(42);
        assert!(validate_definitions(&[first, second]).is_err());
    }

    #[test]
    fn recognizes_existing_upstreams_and_prevents_alias_takeover() {
        let catalog = PackageCatalog::embedded().unwrap();
        let upstream = GitHubUpstream::new(
            "encryption".into(),
            "filosottile/AGE".into(),
            vec!["age".into()],
        );
        assert!(check_identity(catalog, &upstream)
            .unwrap_err()
            .contains("canonicalized as `age`"));
        let upstream = GitHubUpstream::new("age".into(), "other/tool".into(), vec!["age".into()]);
        assert!(check_identity(catalog, &upstream)
            .unwrap_err()
            .contains("different upstream"));
        let mut upstream =
            GitHubUpstream::new("tool".into(), "other/tool".into(), vec!["tool".into()]);
        upstream.aliases.push("rg".into());
        assert!(check_identity(catalog, &upstream)
            .unwrap_err()
            .contains("belongs to"));
    }

    #[test]
    fn writes_loadable_candidates_and_keeps_existing_output_intact() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("output");
        let package = PackageCatalog::embedded()
            .unwrap()
            .find("age")
            .unwrap()
            .clone();
        let catalog = PackageCatalog {
            schema: 1,
            packages: BTreeMap::from([("age".into(), package)]),
        };
        let upstream =
            GitHubUpstream::new("age".into(), "FiloSottile/age".into(), vec!["age".into()]);
        write_candidates(
            root.path(),
            &catalog,
            std::slice::from_ref(&upstream),
            &output,
        )
        .unwrap();
        assert_eq!(
            PackageCatalog::from_directory(&output.join("recipes"))
                .unwrap()
                .sha256(),
            catalog.sha256()
        );
        assert!(import_github_packages(&catalog, &[upstream], &output, 1)
            .unwrap_err()
            .contains("already exists"));
        assert_eq!(
            PackageCatalog::from_directory(&output.join("recipes"))
                .unwrap()
                .sha256(),
            catalog.sha256()
        );
    }
}
