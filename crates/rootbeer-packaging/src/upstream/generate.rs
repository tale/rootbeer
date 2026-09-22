use rootbeer_package::PackageRequest;
use std::collections::BTreeMap;

use super::{GitHubUpstream, Repository};
use crate::ResolveContext;
use rootbeer_package::github::{select_asset, Release};
use rootbeer_package::PackageDefinition;

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
    if release.draft || release.prerelease || upstream.exclude_tags.contains(&release.tag_name) {
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
    Ok(version.filter(|version| version_key(version).is_ok()))
}

fn pattern(asset: &str, tag: &str, version: &str) -> String {
    if asset.contains(tag) {
        return asset.replace(tag, "{tag}");
    }
    asset.replace(version, "{version}")
}

/// Selects the newest release each platform can install, and records its digest.
///
/// Version selection is unchanged: drafts, prereleases and excluded tags are skipped, tags
/// that normalize to the same version are rejected rather than guessed between, and a
/// platform whose asset disappeared keeps the version it already had. What changed is the
/// output — a version entry naming each platform and the digest it published, instead of a
/// synthesized recipe carrying a copy of the package's contract.
pub(super) fn package(
    upstream: &mut GitHubUpstream,
    repository: &Repository,
    releases: &[Release],
    definition: &mut PackageDefinition,
) -> Result<(), String> {
    if upstream.mirror {
        return Err(format!(
            "{}: mirrored updates require manual checksum qualification",
            upstream.name
        ));
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
    if upstream.description.is_none() {
        upstream.description = repository.description.clone();
    }
    if upstream.homepage.is_none() {
        upstream.homepage = repository
            .homepage
            .clone()
            .filter(|url| url.starts_with("https://"));
    }

    let platforms = definition.platforms();
    let mut selected: BTreeMap<String, (String, BTreeMap<String, String>)> = BTreeMap::new();
    for system in &platforms {
        if !upstream.systems.is_empty() && !upstream.systems.contains(system) {
            continue;
        }
        let previous = definition.package.default_version_for(system);
        let minimum = previous.map(version_key).transpose()?;
        let mut choice = None;
        for (key, (version, release)) in ordered.iter().rev() {
            if minimum.as_ref().is_some_and(|minimum| key < minimum) {
                break;
            }
            let expected = upstream.assets.get(system).map(|pattern| {
                pattern
                    .replace("{tag}", &release.tag_name)
                    .replace("{version}", version)
            });
            if expected
                .as_ref()
                .is_some_and(|name| !release.assets.iter().any(|asset| &asset.name == name))
            {
                continue;
            }
            let asset = match select_asset(
                &release.assets,
                expected.as_deref(),
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
            choice = Some((*version, *release, asset));
            break;
        }
        let Some((version, release, asset)) = choice else {
            if previous.is_none() {
                return Err(format!("{}: no supported release for {system}; provide an asset rule or explicitly narrow systems", upstream.name));
            }
            continue;
        };
        // Pinning the digest upstream published closes the window where an asset is
        // replaced between discovery proposing a version and CI qualifying it.
        let digest = asset.sha256().ok_or_else(|| {
            format!(
                "{}@{version} on {system}: release asset publishes no sha256 digest",
                upstream.name
            )
        })?;
        selected
            .entry(version.to_string())
            .or_default()
            .1
            .insert(system.clone(), digest.to_string());
        upstream
            .assets
            .entry(system.clone())
            .or_insert_with(|| pattern(&asset.name, &release.tag_name, version));
    }

    for (version, (_, digests)) in &selected {
        definition.add_version(version, digests.clone(), None)?;
        for system in digests.keys() {
            definition.set_default_version(system, version)?;
        }
    }
    upstream.aliases = definition.package.aliases.clone();
    Ok(())
}

/// Advances a source-built package to the newest release, hashing its archive.
///
/// The build template still drives `{version}`/`{tag}` substitution and a retained release
/// is left alone. The digest now lives on the version entry, one per platform, because a
/// source build produces one archive that every platform compiles.
pub(super) fn source_package(
    upstream: &GitHubUpstream,
    releases: &[Release],
    definition: &mut PackageDefinition,
    mut hash_source: impl FnMut(&str) -> Result<String, String>,
) -> Result<(), String> {
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
    let (key, (version, release)) = ordered
        .last_key_value()
        .ok_or("no matching stable source releases")?;

    let platforms: Vec<String> = definition
        .platforms()
        .into_iter()
        .filter(|system| upstream.systems.is_empty() || upstream.systems.contains(system))
        .collect();
    let current = platforms
        .iter()
        .filter_map(|system| definition.package.default_version_for(system))
        .map(version_key)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max();
    if current.is_some_and(|current| *key <= current) {
        return Ok(());
    }

    let mut build = upstream
        .build
        .clone()
        .ok_or("source discovery requires a build template")?;
    build.url = build
        .url
        .replace("{version}", version)
        .replace("{tag}", &release.tag_name);
    build.strip_prefix = build
        .strip_prefix
        .to_string_lossy()
        .replace("{version}", version)
        .replace("{tag}", &release.tag_name)
        .into();
    if let Some(go) = &mut build.go {
        for value in go.variables.values_mut() {
            *value = value
                .replace("{version}", version)
                .replace("{tag}", &release.tag_name);
            if value.contains(['{', '}']) {
                return Err("unsupported placeholder in Go discovery template".into());
            }
        }
    }
    if build.url.contains(['{', '}']) || build.strip_prefix.to_string_lossy().contains(['{', '}']) {
        return Err("unsupported placeholder in source discovery template".into());
    }

    let digest = hash_source(&build.url)?;
    let digests = platforms
        .iter()
        .map(|system| (system.clone(), digest.clone()))
        .collect::<BTreeMap<_, _>>();
    definition.add_version(version, digests, None)?;
    for system in &platforms {
        definition.set_default_version(system, version)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_discovery_hashes_new_archives_and_preserves_retained_versions() {
        let package = crate::test_catalog::catalog().packages["xz"].clone();
        let previous = package.default_version.clone();
        let mut build = package.versions[&previous].build.clone().unwrap();
        build.url = "https://example.com/xz-{tag}.tar.gz".into();
        build.strip_prefix = "xz-{version}".into();
        let mut upstream = GitHubUpstream::new("xz".into(), "owner/xz".into(), vec!["xz".into()]);
        upstream.tag_prefix = Some("v".into());
        upstream.systems = vec!["aarch64-macos".into()];
        upstream.build = Some(build);
        let releases = [release("v99.0", &[]), release("v98.0", &[])];
        let mut fetched = Vec::new();
        let generated = source_package(&upstream, &releases, &package, |url| {
            fetched.push(url.to_string());
            Ok("b".repeat(64))
        })
        .unwrap();
        assert_eq!(fetched, ["https://example.com/xz-v99.0.tar.gz"]);
        assert_eq!(generated.default_version, "99.0");
        assert_eq!(generated.default_version_for("x86_64-linux"), previous);
        let recipe = &generated.versions["99.0"];
        assert!(recipe.source.is_none());
        assert!(recipe.assets.is_empty());
        assert_eq!(recipe.build.as_ref().unwrap().sha256, "b".repeat(64));
        assert_eq!(
            serde_json::to_value(&generated.versions[&previous]).unwrap(),
            serde_json::to_value(&package.versions[&previous]).unwrap()
        );
        let repeated = source_package(&upstream, &releases, &generated, |_| {
            panic!("unchanged sources must not be downloaded")
        })
        .unwrap();
        assert_eq!(
            serde_json::to_value(repeated).unwrap(),
            serde_json::to_value(generated).unwrap()
        );
        assert!(source_package(&upstream, &releases, &package, |_| Err(
            "download failed".into()
        ))
        .unwrap_err()
        .contains("download failed"));
    }

    #[test]
    fn go_discovery_expands_linker_versions_and_preserves_the_template() {
        let source = format!(
            r#"return {{
            schema = 2, name = "tool", description = "Tool", homepage = "https://example.com", default_version = "1",
            systems = {{ "aarch64-macos" }},
            upstream = {{ github = "owner/tool", repository_id = 42, tag_prefix = "v" }},
            inputs = {{ source = {{ url = "https://example.com/tool-{{tag}}.tar.gz", archive = "tar.gz", strip_prefix = "tool-{{version}}" }} }},
            build = {{ backend = "go", go = {{ binaries = {{ tool = "." }}, variables = {{ ["main.version"] = "{{tag}}" }} }} }},
            outputs = {{ bins = {{ "tool" }}, checks = {{ {{ "tool", "--version" }} }} }},
            versions = {{ ["1"] = {{ inputs = {{ source = {{ sha256 = "{}" }} }} }} }},
        }}"#,
            "a".repeat(64)
        );
        let mut definition = crate::PackageDefinition::from_lua(&source).unwrap();
        let upstream = definition.github_upstream().unwrap().unwrap();
        definition.package = source_package(
            &upstream,
            &[release("v2.0", &[])],
            &definition.package,
            |_| Ok("b".repeat(64)),
        )
        .unwrap();
        let updated = crate::PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
        assert_eq!(
            updated.package.versions["2.0"]
                .build
                .as_ref()
                .unwrap()
                .go
                .as_ref()
                .unwrap()
                .variables["main.version"],
            "v2.0"
        );
        assert_eq!(
            updated.package.versions["1"]
                .build
                .as_ref()
                .unwrap()
                .go
                .as_ref()
                .unwrap()
                .variables["main.version"],
            "v1"
        );
        assert_eq!(
            updated
                .github_upstream()
                .unwrap()
                .unwrap()
                .build
                .unwrap()
                .go
                .unwrap()
                .variables["main.version"],
            "{tag}"
        );
    }

    #[test]
    fn source_candidates_round_trip_shared_inputs_and_library_contracts() {
        let source = format!(
            r#"return {{
            schema = 2, name = "lib", description = "Library", homepage = "https://example.com",
            systems = {{ "aarch64-macos" }}, default_version = "1",
            upstream = {{ github = "owner/lib", repository_id = 42, tag_prefix = "v" }},
            inputs = {{ source = {{ url = "https://example.com/lib-{{tag}}.tar.gz", archive = "tar.gz", strip_prefix = "lib-{{version}}" }} }},
            build = {{ backend = "custom", steps = {{ configure = {{}}, build = {{ {{ "make" }} }}, check = {{ {{ "make", "test" }} }}, install = {{ {{ "make", "install" }} }} }} }},
            outputs = {{ bins = {{}}, checks = {{}}, libraries = {{ "lib/lib.a" }} }},
            versions = {{ ["1"] = {{ inputs = {{ source = {{ sha256 = "{}" }} }} }} }},
        }}"#,
            "a".repeat(64)
        );
        let definition = crate::PackageDefinition::from_lua(&source).unwrap();
        let upstream = definition.github_upstream().unwrap().unwrap();
        let generated = source_package(
            &upstream,
            &[release("v2", &[])],
            &definition.package,
            |_| Ok("b".repeat(64)),
        )
        .unwrap();
        let updated = definition
            .with_updates(generated.clone(), &upstream)
            .unwrap();
        let text = updated.to_lua().unwrap();
        let loaded = crate::PackageDefinition::from_lua(&text).unwrap();
        assert_eq!(
            serde_json::to_value(loaded.package).unwrap(),
            serde_json::to_value(generated).unwrap()
        );
        assert!(text.contains("https://example.com/lib-{tag}.tar.gz"));
        assert!(text.contains("backend = \"custom\""));
    }

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
        upstream.systems = vec!["aarch64-macos".into(), "x86_64-linux".into()];
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
                    "tool-v1.9.0-linux-amd64.tar.gz",
                ],
            ),
        ];
        let mut upstream = upstream();
        let package = package(&mut upstream, &repository(), &releases, None).unwrap();
        assert_eq!(package.default_version, "1.10.0");
        assert_eq!(package.default_version_for("x86_64-linux"), "1.9.0");
        assert_eq!(
            upstream.assets["aarch64-macos"],
            "tool-{tag}-darwin-arm64.tar.gz"
        );
        let serialized = serde_json::to_string(&upstream).unwrap();
        let mut saved = serde_json::from_str(&serialized).unwrap();
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
            &["tool-darwin-arm64.tar.gz", "tool-linux-amd64.tar.gz"],
        )];
        let old = package(&mut upstream, &repository(), &old_releases, None).unwrap();
        let new_releases = vec![release("v2.0.0", &["tool-darwin-arm64.tar.gz"])];
        let updated = package(&mut upstream, &repository(), &new_releases, Some(&old)).unwrap();
        assert_eq!(updated.default_version, "2.0.0");
        assert_eq!(updated.default_version_for("x86_64-linux"), "1.0.0");
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
            &["tool-darwin-arm64.tar.gz", "tool-linux-amd64.tar.gz"],
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
        assert_eq!(updated.default_version_for("x86_64-linux"), "1");
        assert_eq!(updated.description, "Updated description");
    }

    #[test]
    fn rejects_ambiguity_instead_of_falling_back() {
        let releases = vec![
            release("v2", &["tool-darwin-arm64.tar.gz", "tool-darwin-arm64.zip"]),
            release(
                "v1",
                &["tool-darwin-arm64.tar.gz", "tool-linux-amd64.tar.gz"],
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
            release("tool-100-rc.5", &[]),
            release("tool-100-pgo", &[]),
            release("tool-100..1", &[]),
            release("tool-legacy-build", &[]),
            release(
                "tool-2",
                &["tool-darwin-arm64.tar.gz", "tool-linux-amd64.tar.gz"],
            ),
        ];
        let mut upstream = upstream();
        upstream.tag_prefix = Some("tool-".into());
        upstream.exclude_tags = vec!["tool-legacy-build".into()];
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
        .contains("no matching stable releases"));
    }

    #[test]
    fn exact_prefix_disambiguates_tags_but_duplicate_versions_still_fail() {
        let assets = ["tool-darwin-arm64.tar.gz", "tool-linux-amd64.tar.gz"];
        let releases = vec![release("v2", &assets), release("2", &assets)];
        let mut upstream = upstream();
        assert!(package(&mut upstream, &repository(), &releases, None)
            .unwrap_err()
            .contains("multiple release tags"));
        upstream.tag_prefix = Some("v".into());
        let selected = package(&mut upstream, &repository(), &releases, None).unwrap();
        assert_eq!(
            selected.versions["2"].source.as_deref(),
            Some("github:owner/tool@v2")
        );
        let ambiguous = vec![release("v2", &assets), release("v2.0", &assets)];
        assert!(package(&mut upstream, &repository(), &ambiguous, None)
            .unwrap_err()
            .contains("multiple release tags"));
    }

    #[test]
    fn preserves_pinned_contracts_and_rejects_missing_targets() {
        let releases = vec![release(
            "v1",
            &["tool-darwin-arm64.tar.gz", "tool-linux-amd64.tar.gz"],
        )];
        let mut upstream = upstream();
        let previous = package(&mut upstream, &repository(), &releases, None).unwrap();
        upstream.checks = vec![vec!["tool".into(), "--help".into()]];
        let unchanged = package(&mut upstream, &repository(), &releases, Some(&previous)).unwrap();
        assert_eq!(
            serde_json::to_value(&unchanged).unwrap(),
            serde_json::to_value(&previous).unwrap()
        );
        let newer = package(
            &mut upstream,
            &repository(),
            &[release(
                "v2",
                &["tool-darwin-arm64.tar.gz", "tool-linux-amd64.tar.gz"],
            )],
            Some(&previous),
        )
        .unwrap();
        assert_eq!(newer.versions["2"].checks, upstream.checks);
        assert_eq!(newer.versions["1"].checks, previous.versions["1"].checks);
        let releases = vec![release("v1", &["tool-darwin-arm64.tar.gz"])];
        assert!(package(&mut upstream, &repository(), &releases, None)
            .unwrap_err()
            .contains("no supported release for x86_64-linux"));
    }

    #[test]
    fn discovery_keeps_command_paths_and_requires_manual_mirrored_updates() {
        let mut upstream = upstream();
        upstream
            .bin_paths
            .insert("tool".into(), "Tool.app/Contents/MacOS/client".into());
        let releases = vec![release(
            "v1.0.0",
            &["tool-darwin-arm64.tar.gz", "tool-linux-amd64.tar.gz"],
        )];
        let generated = package(&mut upstream, &repository(), &releases, None).unwrap();
        assert_eq!(generated.versions["1.0.0"].bin_paths, upstream.bin_paths);
        let saved =
            crate::PackageDefinition::with_github_upstream(generated.clone(), &upstream).unwrap();
        let repeated = crate::PackageDefinition::from_lua(&saved.to_lua().unwrap()).unwrap();
        assert_eq!(
            repeated.github_upstream().unwrap().unwrap().bin_paths,
            upstream.bin_paths
        );
        assert_eq!(
            repeated.package.versions["1.0.0"].bin_paths,
            upstream.bin_paths
        );

        upstream.mirror = true;
        assert!(
            package(&mut upstream, &repository(), &releases, Some(&generated))
                .unwrap_err()
                .contains("manual checksum qualification")
        );
    }
}
