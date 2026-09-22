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
    use rootbeer_package::PackageDefinition;

    /// Digests are mandatory, so every release asset must publish one.
    fn release(tag: &str, assets: &[&str]) -> Release {
        serde_json::from_value(serde_json::json!({
            "id": 1, "tag_name": tag,
            "assets": assets.iter().map(|name| serde_json::json!({
                "name": name,
                "browser_download_url": "https://example.com/archive",
                "digest": format!("sha256:{}", "a".repeat(64)),
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
        let mut upstream = GitHubUpstream::new("tool".into(), "owner/tool".into());
        upstream.systems = vec!["aarch64-macos".into(), "x86_64-linux".into()];
        upstream
    }

    fn definition(version: &str, systems: &[&str]) -> PackageDefinition {
        let platforms = systems
            .iter()
            .map(|system| {
                format!(
                    r#"["{system}"] = {{ target = "{system}", default_version = "{version}" }},"#
                )
            })
            .collect::<String>();
        let digests = systems
            .iter()
            .map(|system| format!(r#"["{system}"] = "{}","#, "a".repeat(64)))
            .collect::<String>();
        PackageDefinition::from_lua(&format!(
            r#"return {{
                name = "tool", description = "A tool", homepage = "https://example.com",
                default_license = "MIT",
                prebuilt = {{ github = "owner/tool", asset = "tool-{{version}}-{{target}}.tar.gz" }},
                outputs = {{ bins = {{ "tool" }}, checks = {{ {{ "tool", "--version" }} }} }},
                platforms = {{ {platforms} }},
                versions = {{ ["{version}"] = {{ digests = {{ {digests} }} }} }},
            }}"#
        ))
        .unwrap()
    }

    #[test]
    fn selects_the_highest_version_each_platform_can_install() {
        let mut recipe = definition("1", &["aarch64-macos", "x86_64-linux"]);
        let releases = [
            release(
                "2",
                &["tool-2-aarch64-macos.tar.gz", "tool-2-x86_64-linux.tar.gz"],
            ),
            release(
                "1",
                &["tool-1-aarch64-macos.tar.gz", "tool-1-x86_64-linux.tar.gz"],
            ),
        ];
        package(&mut upstream(), &repository(), &releases, &mut recipe).unwrap();

        assert_eq!(
            recipe.package.default_version_for("aarch64-macos"),
            Some("2")
        );
        assert_eq!(
            recipe.package.default_version_for("x86_64-linux"),
            Some("2")
        );
        assert!(
            recipe.package.versions.contains_key("1"),
            "retains the old version"
        );
    }

    #[test]
    fn a_platform_whose_asset_disappeared_keeps_the_version_it_had() {
        let mut recipe = definition("1", &["aarch64-macos", "x86_64-linux"]);
        let releases = [
            release("2", &["tool-2-aarch64-macos.tar.gz"]),
            release(
                "1",
                &["tool-1-aarch64-macos.tar.gz", "tool-1-x86_64-linux.tar.gz"],
            ),
        ];
        package(&mut upstream(), &repository(), &releases, &mut recipe).unwrap();

        assert_eq!(
            recipe.package.default_version_for("aarch64-macos"),
            Some("2")
        );
        assert_eq!(
            recipe.package.default_version_for("x86_64-linux"),
            Some("1"),
            "a platform without an asset must not be dragged forward"
        );
    }

    #[test]
    fn updating_one_platform_leaves_the_others_alone() {
        let mut recipe = definition("1", &["aarch64-macos", "x86_64-linux"]);
        let mut rules = upstream();
        rules.systems = vec!["aarch64-macos".into()];
        let releases = [
            release(
                "2",
                &["tool-2-aarch64-macos.tar.gz", "tool-2-x86_64-linux.tar.gz"],
            ),
            release(
                "1",
                &["tool-1-aarch64-macos.tar.gz", "tool-1-x86_64-linux.tar.gz"],
            ),
        ];
        package(&mut rules, &repository(), &releases, &mut recipe).unwrap();

        assert_eq!(
            recipe.package.default_version_for("aarch64-macos"),
            Some("2")
        );
        assert_eq!(
            recipe.package.default_version_for("x86_64-linux"),
            Some("1"),
            "an untargeted platform must not move"
        );
    }

    #[test]
    fn tags_that_normalize_to_one_version_are_rejected_rather_than_guessed() {
        let mut recipe = definition("1", &["aarch64-macos"]);
        let mut rules = upstream();
        rules.systems = vec!["aarch64-macos".into()];
        let releases = [
            release("2", &["tool-2-aarch64-macos.tar.gz"]),
            release("v2", &["tool-2-aarch64-macos.tar.gz"]),
        ];
        let error = package(&mut rules, &repository(), &releases, &mut recipe).unwrap_err();
        assert!(error.contains("restrict tag_prefix"), "{error}");
    }

    #[test]
    fn drafts_prereleases_and_foreign_prefixes_are_skipped() {
        let mut recipe = definition("1", &["aarch64-macos"]);
        let mut rules = upstream();
        rules.systems = vec!["aarch64-macos".into()];
        rules.tag_prefix = Some("v".into());
        let mut draft = release("v3", &["tool-3-aarch64-macos.tar.gz"]);
        draft.draft = true;
        let mut prerelease = release("v4", &["tool-4-aarch64-macos.tar.gz"]);
        prerelease.prerelease = true;
        let releases = [
            draft,
            prerelease,
            release("nightly-9", &["tool-9-aarch64-macos.tar.gz"]),
            release("v2", &["tool-2-aarch64-macos.tar.gz"]),
        ];
        package(&mut rules, &repository(), &releases, &mut recipe).unwrap();
        assert_eq!(
            recipe.package.default_version_for("aarch64-macos"),
            Some("2")
        );
    }

    #[test]
    fn an_asset_without_a_published_digest_is_refused() {
        let mut recipe = definition("1", &["aarch64-macos"]);
        let mut rules = upstream();
        rules.systems = vec!["aarch64-macos".into()];
        let mut releases = [release("2", &["tool-2-aarch64-macos.tar.gz"])];
        releases[0].assets[0] = serde_json::from_value(serde_json::json!({
            "name": "tool-2-aarch64-macos.tar.gz",
            "browser_download_url": "https://example.com/archive"
        }))
        .unwrap();
        let error = package(&mut rules, &repository(), &releases, &mut recipe).unwrap_err();
        assert!(error.contains("no sha256 digest"), "{error}");
    }

    #[test]
    fn source_discovery_hashes_the_new_archive_once_and_skips_retained_releases() {
        let mut recipe = definition("1", &["aarch64-macos"]);
        let mut rules = upstream();
        rules.systems = vec!["aarch64-macos".into()];
        rules.tag_prefix = Some("v".into());
        rules.build = Some(
            serde_json::from_value(serde_json::json!({
                "backend": "autotools",
                "url": "https://example.com/tool-{tag}.tar.gz",
                "sha256": "b".repeat(64),
                "archive": "tar.gz", "strip_prefix": "tool-{version}"
            }))
            .unwrap(),
        );
        let releases = [release("v99", &[]), release("v98", &[])];

        let mut fetched = Vec::new();
        source_package(&rules, &releases, &mut recipe, |url| {
            fetched.push(url.to_string());
            Ok("c".repeat(64))
        })
        .unwrap();
        assert_eq!(fetched, ["https://example.com/tool-v99.tar.gz"]);
        assert_eq!(
            recipe.package.default_version_for("aarch64-macos"),
            Some("99")
        );
        assert!(
            recipe.package.versions.contains_key("1"),
            "retains the old version"
        );

        source_package(&rules, &releases, &mut recipe, |_| {
            panic!("an unchanged source must not be downloaded again")
        })
        .unwrap();
    }
}
