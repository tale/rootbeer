use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use super::metadata::{MetadataCache, Statistics};
use super::{discover_package, validate_definitions, GitHubUpstream};
use crate::{CatalogPackage, PackageCatalog, PackageDefinition};

/// A discovery report; candidate recipes are unqualified until package export succeeds.
#[derive(Serialize)]
pub struct UpdateReport {
    pub schema: u32,
    pub catalog_sha256: String,
    pub updated: Vec<String>,
    pub unchanged: Vec<String>,
    pub rules_changed: Vec<String>,
    pub errors: BTreeMap<String, String>,
    pub untracked: Vec<String>,
    pub defaults: BTreeMap<String, BTreeMap<String, String>>,
    metadata: Statistics,
}

pub fn discover_updates(
    catalog: &PackageCatalog,
    definitions: &[GitHubUpstream],
    cache: &Path,
    output: &Path,
    max_pages: usize,
) -> Result<UpdateReport, String> {
    discover_cached(
        catalog,
        definitions,
        &BTreeMap::new(),
        cache,
        output,
        max_pages,
    )
}

/// Discovers updates while preserving shared authoring templates and version overrides.
pub fn discover_definition_updates(
    catalog: &PackageCatalog,
    definitions: &BTreeMap<String, PackageDefinition>,
    cache: &Path,
    output: &Path,
    max_pages: usize,
) -> Result<UpdateReport, String> {
    let upstreams: Vec<GitHubUpstream> = definitions
        .values()
        .filter_map(|definition| definition.github_rules())
        .collect();
    discover_cached(catalog, &upstreams, definitions, cache, output, max_pages)
}

fn discover_cached(
    catalog: &PackageCatalog,
    upstreams: &[GitHubUpstream],
    definitions: &BTreeMap<String, PackageDefinition>,
    cache: &Path,
    output: &Path,
    max_pages: usize,
) -> Result<UpdateReport, String> {
    let mut cache = MetadataCache::new(cache)?;
    let mut report =
        discover_with_fetch(catalog, upstreams, definitions, output, max_pages, |url| {
            cache.fetch(url)
        })?;
    report.metadata = cache.statistics;
    write_report(output, &report)?;
    Ok(report)
}

fn discover_with_fetch(
    catalog: &PackageCatalog,
    definitions: &[GitHubUpstream],
    templates: &BTreeMap<String, PackageDefinition>,
    output: &Path,
    max_pages: usize,
    mut fetch: impl FnMut(&str) -> Result<Value, String>,
) -> Result<UpdateReport, String> {
    catalog.validate()?;
    validate_definitions(definitions)?;
    if !(1..=100).contains(&max_pages) {
        return Err("max-pages must be between 1 and 100".into());
    }
    let staging = crate::staging::staging(output)?;
    let destination = staging.path().join("updates");
    fs::create_dir(&destination).map_err(|e| e.to_string())?;
    fs::create_dir(destination.join("packages")).map_err(|e| e.to_string())?;
    let mut report = UpdateReport {
        schema: 1,
        catalog_sha256: catalog.sha256(),
        updated: Vec::new(),
        unchanged: Vec::new(),
        rules_changed: Vec::new(),
        errors: BTreeMap::new(),
        defaults: BTreeMap::new(),
        metadata: Statistics::default(),
        untracked: catalog
            .packages
            .keys()
            .filter(|name| {
                !definitions
                    .iter()
                    .any(|definition| &definition.name == *name)
            })
            .cloned()
            .collect(),
    };
    let mut combined = catalog.clone();
    let mut identities = BTreeMap::new();
    for definition in definitions {
        eprintln!(
            "Discover {} from {}",
            definition.name, definition.repository
        );
        let mut recipe = match templates.get(&definition.name) {
            Some(recipe) => (*recipe).clone(),
            None => {
                report.errors.insert(
                    definition.name.clone(),
                    "discovery requires an authored recipe".into(),
                );
                continue;
            }
        };
        let result = discover_package(&combined, definition, &mut recipe, max_pages, &mut fetch)
            .and_then(|upstream| {
                let package = recipe.package.clone();
                let id = upstream.repository_id.unwrap();
                if let Some(name) = identities.get(&id) {
                    return Err(format!("repository is already tracked as `{name}`"));
                }
                let mut candidate = combined.clone();
                candidate
                    .packages
                    .insert(package.name.clone(), package.clone());
                candidate.validate()?;
                identities.insert(id, package.name.clone());
                combined = candidate;
                Ok((upstream, package))
            });
        let (upstream, package) = match result {
            Ok(result) => result,
            Err(error) => {
                report.errors.insert(definition.name.clone(), error);
                continue;
            }
        };
        let is_changed = catalog
            .packages
            .get(&package.name)
            .map(serde_json::to_value)
            .transpose()
            .map_err(|e| e.to_string())?
            != Some(serde_json::to_value(&package).map_err(|e| e.to_string())?);
        if is_changed {
            report
                .defaults
                .insert(package.name.clone(), platform_defaults(&package));
            report.updated.push(package.name.clone());
        } else {
            report.unchanged.push(package.name.clone());
        }
        let has_rule_changes = serde_json::to_value(definition).map_err(|e| e.to_string())?
            != serde_json::to_value(&upstream).map_err(|e| e.to_string())?;
        if has_rule_changes {
            report.rules_changed.push(upstream.name.clone());
        }
        if is_changed || has_rule_changes {
            fs::write(
                destination
                    .join("packages")
                    .join(format!("{}.lua", package.name)),
                recipe.to_lua()?,
            )
            .map_err(|e| e.to_string())?;
        }
    }
    if !report.updated.is_empty() || !report.rules_changed.is_empty() {
        for package in combined.packages.values() {
            let path = destination
                .join("packages")
                .join(format!("{}.lua", package.name));
            if path.exists() {
                continue;
            }
            let definition = templates
                .get(&package.name)
                .cloned()
                .unwrap_or_else(|| PackageDefinition::new(package.clone()));
            fs::write(path, definition.to_lua()?).map_err(|error| error.to_string())?;
        }
        let candidates = PackageCatalog::from_directory(&destination.join("packages"))?;
        if candidates.sha256() != combined.sha256() {
            return Err("candidate catalog differs from resolved updates".into());
        }
    }
    write_report(&destination, &report)?;
    fs::rename(destination, output).map_err(|e| e.to_string())?;
    Ok(report)
}

fn platform_defaults(package: &CatalogPackage) -> BTreeMap<String, String> {
    package
        .default_versions
        .iter()
        .map(|(system, version)| (system.clone(), version.clone()))
        .collect()
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('`', "&#96;")
        .replace(['\n', '\r'], " ")
}

fn write_report(output: &Path, report: &UpdateReport) -> Result<(), String> {
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut summary = format!("# Upstream discovery\n\n{} changed; {} unchanged; {} errors; {} untracked.\n\nMetadata: {} fetched, {} not modified.\n", report.updated.len(), report.unchanged.len(), report.errors.len(), report.untracked.len(), report.metadata.fetched, report.metadata.not_modified);
    for (name, defaults) in &report.defaults {
        summary.push_str(&format!(
            "\n- **{}**: {}\n",
            escape(name),
            defaults
                .iter()
                .map(|(system, version)| format!("{} {}", escape(system), escape(version)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for (name, error) in &report.errors {
        summary.push_str(&format!(
            "\n- **{} failed**: {}\n",
            escape(name),
            escape(error)
        ));
    }
    if !report.untracked.is_empty() {
        summary.push_str(&format!(
            "\nNot tracked by GitHub discovery: {}.\n",
            report
                .untracked
                .iter()
                .map(|name| escape(name))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    summary.push_str("\nCandidates require platform qualification and review before promotion.\n");
    fs::write(output.join("summary.md"), summary).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(seed: &str) -> String {
        seed.repeat(64)[..64].to_string()
    }

    fn authored(name: &str, versions: &str) -> String {
        format!(
            r#"return {{
                name = "{name}",
                description = "Tool",
                homepage = "https://example.com",
                default_license = "MIT",
                platforms = {{
                    ["aarch64-macos"] = {{
                        default_version = "1",
                        upstream = {{ github = "owner/{name}", tag_prefix = "v" }},
                        prebuilt = {{ github = "owner/{name}", asset = "{name}-{{tag}}-darwin-arm64.tar.gz" }},
                        outputs = {{ bins = {{ "{name}" }}, checks = {{ {{ "{name}", "--version" }} }} }},
                    }},
                }},
                versions = {versions},
            }}"#
        )
    }

    fn definitions(sources: &[String]) -> BTreeMap<String, PackageDefinition> {
        sources
            .iter()
            .map(|source| {
                let definition = PackageDefinition::from_lua(source).unwrap();
                (definition.package.name.clone(), definition)
            })
            .collect()
    }

    fn rules(definitions: &BTreeMap<String, PackageDefinition>) -> Vec<GitHubUpstream> {
        definitions
            .values()
            .filter_map(|definition| definition.github_rules())
            .collect()
    }

    fn releases(name: &str, tags: &[&str]) -> Value {
        Value::Array(
            tags.iter()
                .enumerate()
                .map(|(index, tag)| {
                    serde_json::json!({
                        "id": index + 1,
                        "tag_name": tag,
                        "assets": [{
                            "name": format!("{name}-{tag}-darwin-arm64.tar.gz"),
                            "browser_download_url": format!("https://example.com/{name}"),
                            "digest": format!("sha256:{}", digest(tag.trim_start_matches('v'))),
                        }],
                    })
                })
                .collect(),
        )
    }

    #[test]
    fn a_failing_upstream_does_not_stop_the_other_packages() {
        let root = tempfile::tempdir().unwrap();
        let authored = [
            authored(
                "tool",
                r#"{ ["1"] = { digests = { ["aarch64-macos"] = "aa" } } }"#,
            )
            .replace("\"aa\"", &format!("\"{}\"", digest("1"))),
            authored(
                "broken",
                r#"{ ["1"] = { digests = { ["aarch64-macos"] = "aa" } } }"#,
            )
            .replace("\"aa\"", &format!("\"{}\"", digest("1"))),
        ];
        let definitions = definitions(&authored);
        let catalog = PackageCatalog::from_definitions(&definitions).unwrap();
        let fetch = |url: &str| {
            if url.contains("/broken") {
                return Err("rate limited".into());
            }
            if !url.contains("/releases") {
                return Ok(
                    serde_json::json!({"id": 42, "full_name": "owner/tool", "description": "Tool"}),
                );
            }
            Ok(releases("tool", &["v1", "v2"]))
        };
        let output = root.path().join("first");
        let report = discover_with_fetch(
            &catalog,
            &rules(&definitions),
            &definitions,
            &output,
            1,
            fetch,
        )
        .unwrap();
        assert_eq!(report.updated, ["tool"]);
        assert_eq!(report.errors["broken"], "rate limited");
        assert_eq!(report.defaults["tool"]["aarch64-macos"], "2");

        let candidates = PackageDefinition::from_directory(&output.join("packages")).unwrap();
        let discovered = PackageCatalog::from_definitions(&candidates).unwrap();
        let tool = &discovered.packages["tool"];
        assert_eq!(tool.default_version_for("aarch64-macos"), Some("2"));
        assert_eq!(
            tool.versions["2"].platforms["aarch64-macos"].sha256,
            Some(digest("2"))
        );
        assert_eq!(
            discovered.packages["broken"].default_version_for("aarch64-macos"),
            Some("1")
        );

        let repeat = root.path().join("repeat");
        let report = discover_with_fetch(
            &discovered,
            &rules(&candidates),
            &candidates,
            &repeat,
            1,
            fetch,
        )
        .unwrap();
        assert!(report.updated.is_empty());
        assert_eq!(report.unchanged, ["tool"]);
    }

    #[test]
    fn discovery_preserves_authored_templates_and_version_overrides() {
        let root = tempfile::tempdir().unwrap();
        let source = authored(
            "tool",
            r#"{ ["1"] = {
                digests = { ["aarch64-macos"] = "DIGEST" },
                revision = 3,
                outputs = { checks = { { "tool", "--help" } } },
            } }"#,
        )
        .replace("DIGEST", &digest("1"));
        let definitions = definitions(std::slice::from_ref(&source));
        let catalog = PackageCatalog::from_definitions(&definitions).unwrap();
        let output = root.path().join("output");
        let report = discover_with_fetch(
            &catalog,
            &rules(&definitions),
            &definitions,
            &output,
            1,
            |url| {
                if !url.contains("/releases") {
                    return Ok(
                        serde_json::json!({"id": 42, "full_name": "owner/tool", "description": "Tool"}),
                    );
                }
                Ok(releases("tool", &["v1", "v2"]))
            },
        )
        .unwrap();
        assert_eq!(report.updated, ["tool"]);

        let original: Value = rootbeer_package::definition::lua::read(&source).unwrap();
        let saved_source = fs::read_to_string(output.join("packages/tool.lua")).unwrap();
        let saved: Value = rootbeer_package::definition::lua::read(&saved_source).unwrap();
        assert_eq!(saved["versions"]["1"], original["versions"]["1"]);
        assert_eq!(
            saved["platforms"]["aarch64-macos"]["prebuilt"],
            original["platforms"]["aarch64-macos"]["prebuilt"]
        );
        assert_eq!(
            saved["platforms"]["aarch64-macos"]["outputs"],
            original["platforms"]["aarch64-macos"]["outputs"]
        );
        assert_eq!(saved["platforms"]["aarch64-macos"]["default_version"], "2");

        let expanded = PackageDefinition::from_lua(&saved_source).unwrap();
        let retained = &expanded.package.versions["1"];
        assert_eq!(retained.revision, 3);
        assert_eq!(
            retained.platforms["aarch64-macos"].checks,
            vec![vec!["tool".to_string(), "--help".into()]]
        );
        assert_eq!(
            serde_json::to_value(retained).unwrap(),
            serde_json::to_value(&catalog.packages["tool"].versions["1"]).unwrap()
        );
    }
}
