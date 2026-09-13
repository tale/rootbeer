use std::collections::BTreeMap;

use super::{GitHubUpstream, Repository};
use crate::package::github::{select_asset, Release};
use crate::package::{CatalogPackage, CatalogRecipe, ResolveContext};

fn version_key(version: &str) -> Result<Vec<u64>, String> {
    if version.is_empty()
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        return Err(format!(
            "unsupported version `{version}`; expected a dotted numeric stable version"
        ));
    }
    let mut parts = version
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| {
            format!("unsupported version `{version}`; expected a dotted numeric stable version")
        })?;
    while parts.len() > 1 && parts.last() == Some(&0) {
        parts.pop();
    }
    Ok(parts)
}

fn release_version<'a>(
    upstream: &GitHubUpstream,
    release: &'a Release,
) -> Result<Option<&'a str>, String> {
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = match &upstream.tag_prefix {
        Some(prefix) => release.tag_name.strip_prefix(prefix),
        None => Some(
            release
                .tag_name
                .strip_prefix('v')
                .unwrap_or(&release.tag_name),
        ),
    };
    if let Some(version) = version {
        version_key(version)?;
    }
    Ok(version)
}

fn pattern(asset: &str, tag: &str, version: &str) -> String {
    if asset.contains(tag) {
        return asset.replace(tag, "{tag}");
    }
    asset.replace(version, "{version}")
}

pub(super) fn package(
    upstream: &mut GitHubUpstream,
    repository: &Repository,
    releases: &[Release],
    existing: Option<&CatalogPackage>,
) -> Result<CatalogPackage, String> {
    if let Some(existing) = existing {
        for system in &upstream.systems {
            let version = existing.default_version_for(system);
            let recipe = &existing.versions[version];
            let Some(asset) = recipe.assets.get(system) else {
                continue;
            };
            let Some(source) = recipe.source.as_deref() else {
                continue;
            };
            let request = crate::package::PackageRequest::parse(source);
            let Some(tag) = request.version else { continue };
            upstream
                .assets
                .entry(system.clone())
                .or_insert_with(|| pattern(asset, &tag, version));
        }
    }
    let mut ordered = BTreeMap::new();
    for release in releases {
        let Some(version) = release_version(upstream, release)? else {
            continue;
        };
        if ordered
            .insert(version_key(version)?, (version, release))
            .is_some()
        {
            return Err(format!(
                "multiple release tags normalize to version `{version}`; restrict tag_prefix"
            ));
        }
    }
    if ordered.is_empty() {
        return Err(format!("{}: no matching stable releases", upstream.name));
    }
    let mut package = match existing {
        Some(package) => package.clone(),
        None => CatalogPackage {
            name: upstream.name.clone(),
            aliases: upstream.aliases.clone(),
            description: upstream
                .description
                .clone()
                .or_else(|| repository.description.clone())
                .unwrap_or_default(),
            homepage: upstream
                .homepage
                .clone()
                .or_else(|| {
                    repository
                        .homepage
                        .clone()
                        .filter(|url| url.starts_with("https://"))
                })
                .unwrap_or_else(|| format!("https://github.com/{}", upstream.repository)),
            default_version: String::new(),
            default_versions: BTreeMap::new(),
            versions: BTreeMap::new(),
        },
    };
    if let Some(description) = &upstream.description {
        package.description = description.clone();
    }
    if let Some(homepage) = &upstream.homepage {
        package.homepage = homepage.clone();
    }
    for alias in &upstream.aliases {
        if !package.aliases.contains(alias) {
            package.aliases.push(alias.clone());
        }
    }
    package.aliases.sort();
    let mut defaults = BTreeMap::new();
    let mut generated: BTreeMap<String, CatalogRecipe> = BTreeMap::new();
    for system in &upstream.systems {
        let previous = existing.and_then(|package| {
            let version = package.default_version_for(system);
            package
                .versions
                .get(version)
                .filter(|recipe| recipe.systems.contains(system))
                .map(|_| version)
        });
        let minimum = previous.map(version_key).transpose()?;
        let mut selection = None;
        for (key, (version, release)) in ordered.iter().rev() {
            if minimum.as_ref().is_some_and(|minimum| key < minimum) {
                break;
            }
            let selected = upstream.assets.get(system).map(|pattern| {
                pattern
                    .replace("{tag}", &release.tag_name)
                    .replace("{version}", version)
            });
            if selected
                .as_ref()
                .is_some_and(|name| !release.assets.iter().any(|asset| &asset.name == name))
            {
                continue;
            }
            let asset = match select_asset(
                &release.assets,
                selected.as_deref(),
                &ResolveContext::new(system),
            ) {
                Ok(asset) => asset,
                Err(error) if error.starts_with("no GitHub release asset matches") => continue,
                Err(error) => {
                    return Err(format!(
                        "{}@{} on {system}: {error}",
                        upstream.name, release.tag_name
                    ))
                }
            };
            selection = Some((*version, *release, asset));
            break;
        }
        let Some((version, release, asset)) = selection else {
            let Some(previous) = previous else {
                return Err(format!("{}: no supported release for {system}; provide an asset rule or explicitly narrow systems", upstream.name));
            };
            defaults.insert(system.clone(), previous.to_string());
            continue;
        };
        let recipe = generated
            .entry(version.to_string())
            .or_insert_with(|| CatalogRecipe {
                revision: 1,
                source: Some(format!(
                    "github:{}@{}",
                    upstream.repository, release.tag_name
                )),
                build: None,
                assets: BTreeMap::new(),
                systems: Vec::new(),
                bins: upstream.bins.clone(),
                checks: upstream.checks.clone(),
            });
        recipe.systems.push(system.clone());
        recipe.assets.insert(system.clone(), asset.name.clone());
        defaults.insert(system.clone(), version.to_string());
        upstream
            .assets
            .entry(system.clone())
            .or_insert_with(|| pattern(&asset.name, &release.tag_name, version));
    }
    for (version, mut recipe) in generated {
        recipe.systems.sort();
        if let Some(previous) = package.versions.get(&version) {
            if previous.source != recipe.source
                || previous.bins != recipe.bins
                || previous.checks != recipe.checks
                || recipe.assets.iter().any(|(system, asset)| {
                    previous.assets.get(system) != Some(asset) || !previous.systems.contains(system)
                })
            {
                return Err(format!(
                    "{}@{version}: existing recipe would change; review and revise it manually",
                    upstream.name
                ));
            }
            continue;
        }
        package.versions.insert(version, recipe);
    }
    let newest = defaults
        .values()
        .map(|version| Ok((version_key(version)?, version.clone())))
        .collect::<Result<Vec<_>, String>>()?
        .into_iter()
        .max()
        .unwrap()
        .1;
    if package.default_version.is_empty()
        || version_key(&newest)? > version_key(&package.default_version)?
    {
        package.default_version = newest;
    }
    for (system, version) in defaults {
        if version == package.default_version {
            package.default_versions.remove(&system);
        } else {
            package.default_versions.insert(system, version);
        }
    }
    // Preserve defaults for existing targets excluded from this discovery pass.
    if let Some(existing) = existing {
        for system in existing
            .versions
            .values()
            .flat_map(|recipe| &recipe.systems)
        {
            if !upstream.systems.contains(system) {
                package
                    .default_versions
                    .insert(system.clone(), existing.default_version_for(system).into());
            }
        }
    }
    upstream.description = Some(package.description.clone());
    upstream.homepage = Some(package.homepage.clone());
    upstream.aliases = package.aliases.clone();
    Ok(package)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, assets: &[&str]) -> Release {
        serde_json::from_value(serde_json::json!({
            "id": 1, "tag_name": tag,
            "assets": assets.iter().map(|name| serde_json::json!({
                "name": name, "browser_download_url": "https://example.com/archive"
            })).collect::<Vec<_>>()
        }))
        .unwrap()
    }

    fn repository() -> Repository {
        Repository {
            id: 42,
            full_name: "owner/tool".into(),
            description: Some("A tool".into()),
            homepage: None,
        }
    }

    fn upstream() -> GitHubUpstream {
        let mut upstream =
            GitHubUpstream::new("tool".into(), "owner/tool".into(), vec!["tool".into()]);
        upstream.systems = vec!["aarch64-macos".into(), "x86_64-macos".into()];
        upstream
    }

    #[test]
    fn selects_highest_version_per_platform_and_round_trips_saved_rules() {
        let releases = vec![
            release("v1.10.0", &["tool-v1.10.0-darwin-arm64.tar.gz"]),
            release(
                "v1.9.0",
                &[
                    "tool-v1.9.0-darwin-arm64.tar.gz",
                    "tool-v1.9.0-darwin-amd64.tar.gz",
                ],
            ),
        ];
        let mut upstream = upstream();
        let package = package(&mut upstream, &repository(), &releases, None).unwrap();
        assert_eq!(package.default_version, "1.10.0");
        assert_eq!(package.default_version_for("x86_64-macos"), "1.9.0");
        assert_eq!(
            upstream.assets["aarch64-macos"],
            "tool-{tag}-darwin-arm64.tar.gz"
        );
        let serialized = crate::package::upstream::lua::write(&upstream).unwrap();
        let mut saved = crate::package::upstream::lua::read(&serialized).unwrap();
        let repeated =
            super::package(&mut saved, &repository(), &releases, Some(&package)).unwrap();
        assert_eq!(
            serde_json::to_value(package).unwrap(),
            serde_json::to_value(repeated).unwrap()
        );
    }

    #[test]
    fn retains_old_recipes_and_defaults_when_assets_disappear() {
        let mut upstream = upstream();
        let old_releases = vec![release(
            "v1.0.0",
            &["tool-darwin-arm64.tar.gz", "tool-darwin-amd64.tar.gz"],
        )];
        let old = package(&mut upstream, &repository(), &old_releases, None).unwrap();
        let new_releases = vec![release("v2.0.0", &["tool-darwin-arm64.tar.gz"])];
        let updated = package(&mut upstream, &repository(), &new_releases, Some(&old)).unwrap();
        assert_eq!(updated.default_version, "2.0.0");
        assert_eq!(updated.default_version_for("x86_64-macos"), "1.0.0");
        assert_eq!(
            serde_json::to_value(&updated.versions["1.0.0"]).unwrap(),
            serde_json::to_value(&old.versions["1.0.0"]).unwrap()
        );
        let repeated =
            package(&mut upstream, &repository(), &old_releases, Some(&updated)).unwrap();
        assert_eq!(repeated.default_version_for("aarch64-macos"), "2.0.0");
    }

    #[test]
    fn preserves_unselected_platforms_when_updating_one_target() {
        let mut upstream = upstream();
        let old_releases = vec![release(
            "v1",
            &["tool-darwin-arm64.tar.gz", "tool-darwin-amd64.tar.gz"],
        )];
        let old = package(&mut upstream, &repository(), &old_releases, None).unwrap();
        upstream.systems = vec!["aarch64-macos".into()];
        upstream
            .assets
            .retain(|system, _| system == "aarch64-macos");
        upstream.description = Some("Updated description".into());
        let updated = package(
            &mut upstream,
            &repository(),
            &[release("v2", &["tool-darwin-arm64.tar.gz"])],
            Some(&old),
        )
        .unwrap();
        assert_eq!(updated.default_version_for("aarch64-macos"), "2");
        assert_eq!(updated.default_version_for("x86_64-macos"), "1");
        assert_eq!(updated.description, "Updated description");
    }

    #[test]
    fn rejects_ambiguity_instead_of_falling_back() {
        let releases = vec![
            release("v2", &["tool-darwin-arm64.tar.gz", "tool-darwin-arm64.zip"]),
            release(
                "v1",
                &["tool-darwin-arm64.tar.gz", "tool-darwin-amd64.tar.gz"],
            ),
        ];
        assert!(package(&mut upstream(), &repository(), &releases, None)
            .unwrap_err()
            .contains("ambiguous"));
        let mut upstream = upstream();
        upstream
            .assets
            .insert("aarch64-macos".into(), "tool-darwin-arm64.tar.gz".into());
        assert!(package(&mut upstream, &repository(), &releases, None).is_ok());
    }

    #[test]
    fn ignores_drafts_prereleases_and_unrelated_tag_prefixes() {
        let mut draft = release("tool-99", &[]);
        draft.draft = true;
        let mut prerelease = release("tool-100-rc1", &[]);
        prerelease.prerelease = true;
        let releases = vec![
            draft,
            prerelease,
            release("other-999", &[]),
            release(
                "tool-2",
                &["tool-darwin-arm64.tar.gz", "tool-darwin-amd64.tar.gz"],
            ),
        ];
        let mut upstream = upstream();
        upstream.tag_prefix = Some("tool-".into());
        assert_eq!(
            package(&mut upstream, &repository(), &releases, None)
                .unwrap()
                .default_version,
            "2"
        );
        assert!(package(
            &mut upstream,
            &repository(),
            &[release("tool-stable", &[])],
            None
        )
        .unwrap_err()
        .contains("unsupported version"));
    }

    #[test]
    fn rejects_rewriting_pinned_versions_and_missing_targets() {
        let releases = vec![release(
            "v1",
            &["tool-darwin-arm64.tar.gz", "tool-darwin-amd64.tar.gz"],
        )];
        let mut upstream = upstream();
        let previous = package(&mut upstream, &repository(), &releases, None).unwrap();
        upstream.checks = vec![vec!["tool".into(), "--help".into()]];
        assert!(
            package(&mut upstream, &repository(), &releases, Some(&previous))
                .unwrap_err()
                .contains("revise it manually")
        );
        let releases = vec![release("v1", &["tool-darwin-arm64.tar.gz"])];
        assert!(package(&mut upstream, &repository(), &releases, None)
            .unwrap_err()
            .contains("no supported release for x86_64-macos"));
    }
}
