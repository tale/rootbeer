use std::collections::BTreeMap;

use rootbeer_package::github::Release;
use rootbeer_package::{PackageDefinition, PackageUpstream};

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

/// Stable versions an upstream publishes, ordered oldest to newest.
fn stable_versions(
    upstream: &PackageUpstream,
    releases: &[Release],
) -> Result<BTreeMap<Vec<u64>, String>, String> {
    let mut ordered = BTreeMap::new();
    for release in releases {
        if release.draft || release.prerelease || upstream.exclude_tags.contains(&release.tag_name)
        {
            continue;
        }
        let Some(version) = upstream.version_of(&release.tag_name) else {
            continue;
        };
        let Ok(key) = version_key(&version) else {
            continue;
        };
        if ordered.insert(key, version.clone()).is_some() {
            return Err(format!(
                "multiple release tags normalize to version `{version}`; narrow the upstream tag"
            ));
        }
    }
    Ok(ordered)
}

/// The digest of exactly what `system` downloads for `version`, or None when that release
/// does not publish it.
fn pin(
    definition: &PackageDefinition,
    upstream: &PackageUpstream,
    system: &str,
    version: &str,
    commit: Option<&str>,
    releases: &[Release],
    hash: &mut impl FnMut(&str) -> Result<String, String>,
) -> Result<Option<String>, String> {
    let candidate = definition.candidate(system, version, commit)?;
    if let Some(build) = &candidate.build {
        return hash(&build.url).map(Some);
    }
    let source = candidate
        .source
        .as_deref()
        .ok_or("a candidate downloads nothing")?;
    let Some(name) = &candidate.asset else {
        return hash(source).map(Some);
    };
    let (repository, tag) = source
        .strip_prefix("github:")
        .and_then(|reference| reference.rsplit_once('@'))
        .ok_or_else(|| format!("unsupported release source `{source}`"))?;
    if !repository.eq_ignore_ascii_case(upstream.repository()) {
        return Err(format!(
            "downloads from {repository}, but discovers from {}",
            upstream.repository()
        ));
    }
    let Some(asset) = releases
        .iter()
        .find(|release| release.tag_name == tag)
        .and_then(|release| release.assets.iter().find(|asset| &asset.name == name))
    else {
        return Ok(None);
    };
    // Pinning the digest upstream published closes the window where an asset is replaced
    // between discovery proposing a version and CI qualifying it.
    let digest = asset
        .sha256()
        .ok_or_else(|| format!("release asset `{name}` publishes no sha256 digest"))?;
    Ok(Some(digest.to_string()))
}

/// Advances each platform to the newest release that publishes what it downloads.
///
/// A platform never moves below its current version, and one whose asset is missing from
/// newer releases stays where it is. Errors are returned per platform rather than raised,
/// so one platform's failure cannot hold the others back.
pub(super) fn discover(
    upstream: &PackageUpstream,
    systems: &[String],
    releases: &[Release],
    definition: &mut PackageDefinition,
    mut hash: impl FnMut(&str) -> Result<String, String>,
    mut commit_of: impl FnMut(&str) -> Result<String, String>,
) -> Result<Vec<String>, String> {
    let versions = stable_versions(upstream, releases)?;
    if versions.is_empty() {
        return Err("no matching stable releases".into());
    }

    let mut hashed: BTreeMap<String, String> = BTreeMap::new();
    let mut hash_once = |url: &str| {
        if let Some(digest) = hashed.get(url) {
            return Ok(digest.clone());
        }
        let digest = hash(url)?;
        hashed.insert(url.to_string(), digest.clone());
        Ok(digest)
    };
    let is_commit_needed = definition.uses_commit();
    let mut commits: BTreeMap<String, String> = BTreeMap::new();
    let mut commit_for = |version: &str| -> Result<Option<String>, String> {
        if !is_commit_needed {
            return Ok(None);
        }
        if let Some(commit) = commits.get(version) {
            return Ok(Some(commit.clone()));
        }
        let commit = commit_of(&upstream.tag_for(version))?;
        commits.insert(version.to_string(), commit.clone());
        Ok(Some(commit))
    };
    let mut selected: BTreeMap<&str, BTreeMap<String, String>> = BTreeMap::new();
    let mut errors = Vec::new();
    for system in systems {
        let current = definition
            .package
            .default_version_for(system)
            .map(version_key)
            .transpose();
        let current = match current {
            Ok(current) => current,
            Err(error) => {
                errors.push(format!("{system}: {error}"));
                continue;
            }
        };
        for (key, version) in versions.iter().rev() {
            if current.as_ref().is_some_and(|current| key <= current) {
                break;
            }
            let commit = match commit_for(version) {
                Ok(commit) => commit,
                Err(error) => {
                    errors.push(format!("{system}: {version}: {error}"));
                    break;
                }
            };
            match pin(
                definition,
                upstream,
                system,
                version,
                commit.as_deref(),
                releases,
                &mut hash_once,
            ) {
                Ok(Some(digest)) => {
                    selected
                        .entry(version)
                        .or_default()
                        .insert(system.clone(), digest);
                    break;
                }
                Ok(None) => continue,
                Err(error) => {
                    errors.push(format!("{system}: {error}"));
                    break;
                }
            }
        }
    }

    for (version, digests) in selected {
        let systems: Vec<String> = digests.keys().cloned().collect();
        definition.add_version(version, digests, None, commits.get(version).cloned())?;
        for system in &systems {
            definition.set_default_version(system, version)?;
        }
    }
    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A distinct digest per asset name, so a test can tell which asset was pinned.
    fn digest_of(name: &str) -> String {
        let hex: String = name.bytes().map(|byte| format!("{byte:02x}")).collect();
        format!("{hex:0<64}")[..64].to_string()
    }

    fn release(tag: &str, assets: &[&str]) -> Release {
        serde_json::from_value(serde_json::json!({
            "id": 1, "tag_name": tag,
            "assets": assets.iter().map(|name| serde_json::json!({
                "name": name,
                "browser_download_url": "https://example.com/archive",
                "digest": format!("sha256:{}", digest_of(name)),
            })).collect::<Vec<_>>()
        }))
        .unwrap()
    }

    fn definition(upstream: &str, version: &str, systems: &[&str]) -> PackageDefinition {
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
                upstream = {upstream},
                prebuilt = {{ github = "owner/tool", asset = "tool-{{version}}-{{target}}.tar.gz" }},
                outputs = {{ bins = {{ "tool" }}, checks = {{ {{ "tool", "--version" }} }} }},
                platforms = {{ {platforms} }},
                versions = {{ ["{version}"] = {{ digests = {{ {digests} }} }} }},
            }}"#
        ))
        .unwrap()
    }

    const UPSTREAM: &str = r#"{ github = "owner/tool" }"#;

    fn run(recipe: &mut PackageDefinition, releases: &[Release]) -> Vec<String> {
        let upstream = recipe.upstreams().remove(0);
        discover(
            &upstream.0,
            &upstream.1,
            releases,
            recipe,
            |url| panic!("a prebuilt must not download {url}"),
            |tag| panic!("{tag} needs no commit"),
        )
        .unwrap()
    }

    fn pinned(recipe: &PackageDefinition, version: &str, system: &str) -> Option<String> {
        recipe.package.versions[version].platforms[system]
            .sha256
            .clone()
    }

    #[test]
    fn selects_the_highest_version_each_platform_can_install() {
        let mut recipe = definition(UPSTREAM, "1", &["aarch64-macos", "x86_64-linux"]);
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
        assert!(run(&mut recipe, &releases).is_empty());

        for system in ["aarch64-macos", "x86_64-linux"] {
            assert_eq!(recipe.package.default_version_for(system), Some("2"));
            assert_eq!(
                pinned(&recipe, "2", system),
                Some(digest_of(&format!("tool-2-{system}.tar.gz")))
            );
        }
        assert!(
            recipe.package.versions.contains_key("1"),
            "retains the old version"
        );
    }

    #[test]
    fn pins_the_asset_its_template_names_rather_than_a_lookalike() {
        let mut recipe = definition(UPSTREAM, "1", &["aarch64-macos"]);
        let releases = [release(
            "2",
            &[
                "tool-2-aarch64-macos-debug.tar.gz",
                "tool-2-aarch64-macos.tar.gz",
                "tool-2-aarch64-macos.tar.gz.sha256",
            ],
        )];
        run(&mut recipe, &releases);
        assert_eq!(
            pinned(&recipe, "2", "aarch64-macos"),
            Some(digest_of("tool-2-aarch64-macos.tar.gz"))
        );
    }

    #[test]
    fn a_platform_whose_asset_disappeared_keeps_the_version_it_had() {
        let mut recipe = definition(UPSTREAM, "1", &["aarch64-macos", "x86_64-linux"]);
        let releases = [
            release("2", &["tool-2-aarch64-macos.tar.gz"]),
            release(
                "1",
                &["tool-1-aarch64-macos.tar.gz", "tool-1-x86_64-linux.tar.gz"],
            ),
        ];
        assert!(run(&mut recipe, &releases).is_empty());
        assert_eq!(
            recipe.package.default_version_for("aarch64-macos"),
            Some("2")
        );
        assert_eq!(
            recipe.package.default_version_for("x86_64-linux"),
            Some("1"),
            "a platform without an asset must not be dragged forward"
        );
        assert!(!recipe.package.versions["2"]
            .platforms
            .contains_key("x86_64-linux"));
    }

    #[test]
    fn a_failing_platform_does_not_hold_back_the_others() {
        let mut recipe = definition(UPSTREAM, "1", &["aarch64-macos", "x86_64-linux"]);
        let mut releases = [release(
            "2",
            &["tool-2-aarch64-macos.tar.gz", "tool-2-x86_64-linux.tar.gz"],
        )];
        releases[0].assets[1] = serde_json::from_value(serde_json::json!({
            "name": "tool-2-x86_64-linux.tar.gz",
            "browser_download_url": "https://example.com/archive"
        }))
        .unwrap();
        let errors = run(&mut recipe, &releases);

        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("x86_64-linux: "), "{errors:?}");
        assert!(errors[0].contains("no sha256 digest"), "{errors:?}");
        assert_eq!(
            recipe.package.default_version_for("aarch64-macos"),
            Some("2")
        );
        assert_eq!(
            recipe.package.default_version_for("x86_64-linux"),
            Some("1")
        );
    }

    #[test]
    fn a_tag_template_maps_versions_and_their_separators() {
        let mut recipe = definition(
            r#"{ github = "owner/tool", tag = "tool-{version}", separator = "_" }"#,
            "8.21.0",
            &["aarch64-macos"],
        );
        let releases = [
            release("tool-8_22_0", &["tool-8.22.0-aarch64-macos.tar.gz"]),
            release("other-9_0_0", &["tool-9.0.0-aarch64-macos.tar.gz"]),
            release("tool-8_21_0", &["tool-8.21.0-aarch64-macos.tar.gz"]),
        ];
        run(&mut recipe, &releases);
        assert_eq!(
            recipe.package.default_version_for("aarch64-macos"),
            Some("8.22.0")
        );
        assert_eq!(
            recipe.package.versions["8.22.0"].platforms["aarch64-macos"]
                .source
                .as_deref(),
            Some("github:owner/tool@tool-8_22_0")
        );
    }

    #[test]
    fn tags_that_normalize_to_one_version_are_rejected_rather_than_guessed() {
        let mut recipe = definition(UPSTREAM, "1", &["aarch64-macos"]);
        let releases = [
            release("2", &["tool-2-aarch64-macos.tar.gz"]),
            release("2.0", &["tool-2.0-aarch64-macos.tar.gz"]),
        ];
        let (upstream, systems) = recipe.upstreams().remove(0);
        let error = discover(
            &upstream,
            &systems,
            &releases,
            &mut recipe,
            |_| unreachable!(),
            |tag| panic!("{tag} needs no commit"),
        )
        .unwrap_err();
        assert!(error.contains("narrow the upstream tag"), "{error}");
    }

    #[test]
    fn drafts_prereleases_and_foreign_tags_are_skipped() {
        let mut recipe = definition(
            r#"{ github = "owner/tool", tag = "v{version}" }"#,
            "1",
            &["aarch64-macos"],
        );
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
        run(&mut recipe, &releases);
        assert_eq!(
            recipe.package.default_version_for("aarch64-macos"),
            Some("2")
        );
    }

    #[test]
    fn a_version_records_the_commit_its_templates_embed() {
        let mut recipe = PackageDefinition::from_lua(&format!(
            r#"return {{
                name = "tool", description = "A tool", homepage = "https://example.com",
                default_license = "MIT",
                upstream = {{ github = "owner/tool", tag = "v{{version}}" }},
                source = {{ url = "https://example.com/tool-{{tag}}.tar.gz", archive = "tar.gz",
                            strip_prefix = "tool-{{version}}" }},
                build = {{ backend = "go", go = {{
                    binaries = {{ tool = "./cmd" }},
                    variables = {{ commit = "{{commit}}" }},
                }} }},
                outputs = {{ bins = {{ "tool" }}, checks = {{ {{ "tool", "--version" }} }} }},
                platforms = {{
                    ["aarch64-macos"] = {{ default_version = "98" }},
                    ["x86_64-linux"] = {{ default_version = "98" }},
                }},
                versions = {{ ["98"] = {{ commit = "{old}", digests = {{
                    ["aarch64-macos"] = "{digest}", ["x86_64-linux"] = "{digest}",
                }} }} }},
            }}"#,
            old = "a".repeat(40),
            digest = "b".repeat(64)
        ))
        .unwrap();
        let releases = [release("v99", &[]), release("v98", &[])];
        let (upstream, systems) = recipe.upstreams().remove(0);

        let mut resolved = Vec::new();
        discover(
            &upstream,
            &systems,
            &releases,
            &mut recipe,
            |_| Ok("c".repeat(64)),
            |tag| {
                resolved.push(tag.to_string());
                Ok("d".repeat(40))
            },
        )
        .unwrap();
        assert_eq!(resolved, ["v99"]);
        for system in &systems {
            let build = recipe.package.versions["99"].platforms[system]
                .build
                .as_ref()
                .unwrap();
            assert_eq!(
                build.go.as_ref().unwrap().variables["commit"],
                "d".repeat(40)
            );
        }
        let rendered = recipe.to_lua().unwrap();
        assert!(
            rendered.contains(&format!("commit = \"{}\"", "d".repeat(40))),
            "{rendered}"
        );
    }

    #[test]
    fn source_discovery_hashes_one_archive_for_every_platform() {
        let mut recipe = PackageDefinition::from_lua(&format!(
            r#"return {{
                name = "tool", description = "A tool", homepage = "https://example.com",
                default_license = "MIT",
                upstream = {{ github = "owner/tool", tag = "v{{version}}" }},
                source = {{ url = "https://example.com/tool-{{tag}}.tar.gz", archive = "tar.gz",
                            strip_prefix = "tool-{{version}}" }},
                build = {{ backend = "autotools" }},
                outputs = {{ bins = {{ "tool" }}, checks = {{ {{ "tool", "--version" }} }} }},
                platforms = {{
                    ["aarch64-macos"] = {{ default_version = "98" }},
                    ["x86_64-linux"] = {{ default_version = "98" }},
                }},
                versions = {{ ["98"] = {{ digests = {{
                    ["aarch64-macos"] = "{digest}", ["x86_64-linux"] = "{digest}",
                }} }} }},
            }}"#,
            digest = "b".repeat(64)
        ))
        .unwrap();
        let releases = [release("v99", &[]), release("v98", &[])];
        let (upstream, systems) = recipe.upstreams().remove(0);

        let mut fetched = Vec::new();
        discover(
            &upstream,
            &systems,
            &releases,
            &mut recipe,
            |url| {
                fetched.push(url.to_string());
                Ok("c".repeat(64))
            },
            |tag| panic!("{tag} needs no commit"),
        )
        .unwrap();
        assert_eq!(fetched, ["https://example.com/tool-v99.tar.gz"]);
        for system in &systems {
            assert_eq!(recipe.package.default_version_for(system), Some("99"));
            let build = recipe.package.versions["99"].platforms[system]
                .build
                .as_ref()
                .unwrap();
            assert_eq!(build.sha256, "c".repeat(64));
        }

        discover(
            &upstream,
            &systems,
            &releases,
            &mut recipe,
            |_| panic!("an unchanged source must not be downloaded again"),
            |tag| panic!("{tag} needs no commit"),
        )
        .unwrap();
    }
}
