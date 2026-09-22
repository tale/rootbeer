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
pub use updates::{discover_definition_updates, discover_updates, UpdateReport};

#[derive(Debug, Deserialize)]
struct Repository {
    id: u64,
    full_name: String,
    description: Option<String>,
    homepage: Option<String>,
}

fn discover_package(
    catalog: &PackageCatalog,
    definition: &GitHubUpstream,
    recipe: &mut rootbeer_package::PackageDefinition,
    max_pages: usize,
    fetch: &mut impl FnMut(&str) -> Result<serde_json::Value, String>,
) -> Result<GitHubUpstream, String> {
    check_identity(
        catalog,
        &definition.name,
        &definition.repository,
        definition.build.is_some(),
    )?;
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
    if upstream.build.is_some() {
        let cache = rootbeer_package::download::DownloadCache::default();
        generate::source_package(&upstream, &releases, recipe, |url| {
            cache
                .materialize(url, None)
                .map(|file| file.sha256)
                .map_err(|error| error.to_string())
        })?;
    } else {
        generate::package(&mut upstream, &repository, &releases, recipe)?;
    }
    Ok(upstream)
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
}
