use crate::{CatalogPackage, GitHubUpstream, PackageCatalog};
use rootbeer_package::download::read_json_url;
use rootbeer_package::github::Release;
use rootbeer_package::upstream::{check_identity, validate_definitions};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

mod generate;
mod metadata;
mod updates;
pub use updates::{discover_definition_updates, discover_updates, seed_upstreams, UpdateReport};

#[derive(Debug, Deserialize)]
struct Repository {
    id: u64,
    full_name: String,
    description: Option<String>,
    homepage: Option<String>,
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
    if definitions.is_empty() {
        return Err("no upstream definitions supplied".into());
    }
    if !(1..=100).contains(&max_pages) {
        return Err("max-pages must be between 1 and 100".into());
    }
    for upstream in definitions {
        check_identity(catalog, upstream)?;
    }
    let staging = crate::staging::staging(output)?;
    let mut resolved = Vec::new();
    let mut candidates = PackageCatalog {
        extra: Default::default(),
        schema: 1,
        packages: BTreeMap::new(),
    };
    let mut combined = catalog.clone();
    for definition in definitions {
        eprintln!(
            "Discover {} from {}",
            definition.name, definition.repository
        );
        let (upstream, package) = discover_package(catalog, definition, max_pages, &mut fetch)?;
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

fn discover_package(
    catalog: &PackageCatalog,
    definition: &GitHubUpstream,
    max_pages: usize,
    fetch: &mut impl FnMut(&str) -> Result<serde_json::Value, String>,
) -> Result<(GitHubUpstream, CatalogPackage), String> {
    check_identity(catalog, definition)?;
    let mut upstream = definition.clone();
    let url = format!("https://api.github.com/repos/{}", upstream.repository);
    let repository: Repository = serde_json::from_value(fetch(&url)?).map_err(|e| e.to_string())?;
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
    let package = if upstream.build.is_some() {
        let cache = rootbeer_package::download::DownloadCache::default();
        generate::source_package(
            &upstream,
            &releases,
            existing.ok_or("source discovery requires an existing source recipe")?,
            |url| {
                cache
                    .materialize(url, None)
                    .map(|file| file.sha256)
                    .map_err(|error| error.to_string())
            },
        )?
    } else {
        generate::package(&mut upstream, &repository, &releases, existing)?
    };
    Ok((upstream, package))
}

fn write_candidates(
    staging: &Path,
    candidates: &PackageCatalog,
    definitions: &[GitHubUpstream],
    output: &Path,
) -> Result<(), String> {
    let root = staging.join("candidates");
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    fs::create_dir(root.join("packages")).map_err(|e| e.to_string())?;
    for package in candidates.packages.values() {
        let upstream = definitions
            .iter()
            .find(|upstream| upstream.name == package.name)
            .ok_or("candidate has no update rules")?;
        let definition = super::PackageDefinition::with_github_upstream(package.clone(), upstream)?;
        fs::write(
            root.join("packages").join(format!("{}.lua", package.name)),
            definition.to_lua()?,
        )
        .map_err(|e| e.to_string())?;
    }
    let loaded = PackageCatalog::from_directory(&root.join("packages"))?;
    if loaded.sha256() != candidates.sha256() {
        return Err("generated Lua differs from candidate catalog".into());
    }
    GitHubUpstream::from_directory(&root.join("packages"))?;
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
        let catalog = crate::test_catalog::catalog();
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
        let saved = GitHubUpstream::from_directory(&output.join("packages")).unwrap();
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
        let catalog = crate::test_catalog::catalog();
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
    fn rejects_invalid_discovery_contracts_without_catalog_recipes() {
        let upstream = GitHubUpstream::new("tool".into(), "owner/tool".into(), vec!["tool".into()]);
        for invalid in [
            serde_json::json!({"description": " "}),
            serde_json::json!({"homepage": "http://example.com"}),
            serde_json::json!({"aliases": ["tool"]}),
            serde_json::json!({"aliases": ["../tool"]}),
            serde_json::json!({"systems": []}),
            serde_json::json!({"systems": ["aarch64-macos", "aarch64-macos"]}),
            serde_json::json!({"systems": ["unknown"]}),
            serde_json::json!({"bins": ["tool", "tool"]}),
            serde_json::json!({"checks": [["undeclared", "--version"]]}),
        ] {
            let mut value = serde_json::to_value(&upstream).unwrap();
            value
                .as_object_mut()
                .unwrap()
                .extend(invalid.as_object().unwrap().clone());
            let invalid: GitHubUpstream = serde_json::from_value(value).unwrap();
            assert!(invalid.validate().is_err());
        }
    }

    #[test]
    fn recognizes_existing_upstreams_and_prevents_alias_takeover() {
        let catalog = crate::test_catalog::catalog();
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
        let package = crate::test_catalog::catalog().find("age").unwrap().clone();
        let catalog = PackageCatalog {
            extra: Default::default(),
            schema: 1,
            packages: BTreeMap::from([("age".into(), package)]),
        };
        let package = &catalog.packages["age"];
        let recipe = &package.versions[&package.default_version];
        let mut upstream =
            GitHubUpstream::new("age".into(), "FiloSottile/age".into(), recipe.bins.clone());
        upstream.checks = recipe.checks.clone();
        upstream.aliases = package.aliases.clone();
        write_candidates(
            root.path(),
            &catalog,
            std::slice::from_ref(&upstream),
            &output,
        )
        .unwrap();
        assert_eq!(
            PackageCatalog::from_directory(&output.join("packages"))
                .unwrap()
                .sha256(),
            catalog.sha256()
        );
        assert!(import_github_packages(&catalog, &[upstream], &output, 1)
            .unwrap_err()
            .contains("already exists"));
        assert_eq!(
            PackageCatalog::from_directory(&output.join("packages"))
                .unwrap()
                .sha256(),
            catalog.sha256()
        );
    }
}
