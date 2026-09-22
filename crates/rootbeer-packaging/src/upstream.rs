use rootbeer_package::github::Release;
use rootbeer_package::{PackageDefinition, PackageUpstream};
use serde::Deserialize;

mod generate;
mod metadata;
mod updates;
pub use updates::{discover_updates, UpdateReport};

#[derive(Debug, Deserialize)]
struct Repository {
    id: u64,
    full_name: String,
}

/// What one upstream contributed to a package, beyond the versions it recorded.
struct Discovery {
    /// Failures confined to single platforms; the package's other platforms still advance.
    errors: Vec<String>,
    repository_id: u64,
    is_newly_pinned: bool,
}

fn discover_upstream(
    upstream: &PackageUpstream,
    systems: &[String],
    recipe: &mut PackageDefinition,
    max_pages: usize,
    fetch: &mut impl FnMut(&str) -> Result<serde_json::Value, String>,
    hash: &mut impl FnMut(&str) -> Result<String, String>,
) -> Result<Discovery, String> {
    let url = format!("https://api.github.com/repos/{}", upstream.repository());
    let repository: Repository = serde_json::from_value(fetch(&url)?).map_err(|e| e.to_string())?;
    if repository.id == 0 || upstream.repository_id.is_some_and(|id| id != repository.id) {
        return Err("GitHub repository ID changed; review upstream ownership".into());
    }
    if !upstream
        .repository()
        .eq_ignore_ascii_case(&repository.full_name)
    {
        return Err(format!(
            "repository moved to {}; review the identity mapping",
            repository.full_name
        ));
    }

    let mut releases: Vec<Release> = Vec::new();
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
            return Err(format!("release history exceeds --max-pages {max_pages}; increase it to avoid incomplete version selection"));
        }
    }

    let errors = generate::discover(upstream, systems, &releases, recipe, hash)?;
    let is_newly_pinned = upstream.repository_id.is_none();
    if is_newly_pinned {
        recipe.pin_repository_id(upstream.repository(), repository.id)?;
    }
    Ok(Discovery {
        errors,
        repository_id: repository.id,
        is_newly_pinned,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rootbeer_package::upstream::{check_identity, validate_upstreams};
    use rootbeer_package::PackageDefinition;

    fn tracked(name: &str, repositories: [&str; 2]) -> (String, PackageDefinition) {
        let definition = PackageDefinition::from_lua(&format!(
            r#"return {{
                name = "{name}", description = "Tool", homepage = "https://example.com",
                default_license = "MIT",
                prebuilt = {{ github = "owner/{name}", asset = "{name}-{{version}}.tar.gz" }},
                outputs = {{ bins = {{ "{name}" }}, checks = {{ {{ "{name}", "--version" }} }} }},
                platforms = {{
                    ["aarch64-macos"] = {{ default_version = "1", upstream = {{ github = "{}" }} }},
                    ["x86_64-linux"] = {{ default_version = "1", upstream = {{ github = "{}" }} }},
                }},
                versions = {{ ["1"] = {{ digests = {{ ["aarch64-macos"] = "{digest}", ["x86_64-linux"] = "{digest}" }} }} }},
            }}"#,
            repositories[0],
            repositories[1],
            digest = "a".repeat(64)
        ))
        .unwrap();
        (name.to_string(), definition)
    }

    #[test]
    fn one_package_may_span_repositories_but_two_may_not_share_one() {
        let split = BTreeMap::from([tracked("helium", ["owner/mac", "owner/linux"])]);
        assert!(validate_upstreams(&split).is_ok());

        let shared = BTreeMap::from([
            tracked("one", ["owner/tool", "owner/tool"]),
            tracked("two", ["owner/other", "OWNER/Tool"]),
        ]);
        let error = validate_upstreams(&shared).unwrap_err();
        assert!(error.contains("another package"), "{error}");
    }

    #[test]
    fn recognizes_existing_upstreams_and_prevents_alias_takeover() {
        let catalog = crate::test_catalog::catalog();
        let identity =
            |name: &str, repository: &str| check_identity(catalog, name, repository, false);
        assert!(identity("encryption", "filosottile/AGE")
            .unwrap_err()
            .contains("canonicalized as `age`"));
        assert!(identity("age", "other/tool")
            .unwrap_err()
            .contains("different upstream"));
        assert!(identity("rg", "other/tool")
            .unwrap_err()
            .contains("belongs to"));
    }
}
