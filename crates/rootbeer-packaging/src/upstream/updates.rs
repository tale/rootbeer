use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use super::metadata::{MetadataCache, Statistics};
use super::{discover_package, validate_definitions, GitHubUpstream};
use crate::{CatalogPackage, PackageCatalog, PackageDefinition, PackageRequest};

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

/// Seeds reusable rules from GitHub-backed defaults without requesting network metadata.
/// Source-built and other backend packages are omitted; the destination must be new.
pub fn seed_upstreams(catalog: &PackageCatalog, output: &Path) -> Result<usize, String> {
    catalog.validate()?;
    let staging = crate::staging::staging(output)?;
    let destination = staging.path().join("packages");
    fs::create_dir(&destination).map_err(|e| e.to_string())?;
    let mut definitions = Vec::new();
    for package in catalog.packages.values() {
        let recipe = &package.versions[&package.default_version];
        let Some(source) = recipe.source.as_deref() else {
            continue;
        };
        let request = PackageRequest::parse(source);
        if request.resolver.as_deref() != Some("github") {
            continue;
        }
        let mut definition =
            GitHubUpstream::new(package.name.clone(), request.name, recipe.bins.clone());
        definition.aliases = package.aliases.clone();
        definition.description = Some(package.description.clone());
        definition.homepage = Some(package.homepage.clone());
        definition.checks = recipe.checks.clone();
        definition.bin_paths = recipe.bin_paths.clone();
        definition.apps = recipe.apps.clone();
        definition.mirror = recipe.mirror;
        definition.systems = package
            .versions
            .values()
            .flat_map(|recipe| recipe.systems.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let tag = request.version.ok_or("GitHub recipe has no tag")?;
        let prefix = tag.strip_suffix(&package.default_version).ok_or_else(|| {
            format!(
                "{}: tag cannot be mapped to its canonical version",
                package.name
            )
        })?;
        definition.tag_prefix = Some(prefix.into());
        definitions.push(definition);
    }
    validate_definitions(&definitions)?;
    for definition in &definitions {
        fs::write(
            destination.join(format!("{}.lua", definition.name)),
            crate::PackageDefinition::with_github_upstream(
                catalog.packages[&definition.name].clone(),
                definition,
            )?
            .to_lua()?,
        )
        .map_err(|e| e.to_string())?;
    }
    fs::rename(destination, output).map_err(|e| e.to_string())?;
    Ok(definitions.len())
}

/// Discovers each tracked project independently using conditional cached API requests.
/// Writes only changed candidate recipes, saved rules, and a consolidated JSON/Markdown report.
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
    let upstreams = GitHubUpstream::from_definitions(definitions)?;
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
        let result = discover_package(&combined, definition, max_pages, &mut fetch).and_then(
            |(upstream, package)| {
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
            },
        );
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
            let definition = match templates.get(&package.name) {
                Some(definition) => definition.with_updates(package, &upstream)?,
                None => PackageDefinition::with_github_upstream(package, &upstream)?,
            };
            fs::write(
                destination
                    .join("packages")
                    .join(format!("{}.lua", definition.package.name)),
                definition.to_lua()?,
            )
            .map_err(|e| e.to_string())?;
        }
    }
    if !report.updated.is_empty() || !report.rules_changed.is_empty() {
        PackageCatalog::from_directory(&destination.join("packages"))?;
    }
    write_report(&destination, &report)?;
    fs::rename(destination, output).map_err(|e| e.to_string())?;
    Ok(report)
}

fn platform_defaults(package: &CatalogPackage) -> BTreeMap<String, String> {
    package
        .versions
        .values()
        .flat_map(|recipe| &recipe.systems)
        .map(|system| (system.clone(), package.default_version_for(system).into()))
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

    #[test]
    fn unchanged_packages_need_no_qualification_and_failures_do_not_stop_other_projects() {
        let root = tempfile::tempdir().unwrap();
        let catalog = PackageCatalog::embedded().unwrap();
        let mut definition =
            GitHubUpstream::new("tool".into(), "owner/tool".into(), vec!["tool".into()]);
        definition.systems = vec!["aarch64-macos".into()];
        let mut broken = definition.clone();
        broken.name = "broken".into();
        broken.repository = "owner/broken".into();
        let fetch = |url: &str| {
            if url.contains("/broken") {
                return Err("rate limited".into());
            }
            if !url.contains("/releases") {
                return Ok(
                    serde_json::json!({"id":42,"full_name":"owner/tool","description":"Tool"}),
                );
            }
            Ok(
                serde_json::json!([{"id":1,"tag_name":"v1","assets":[{"name":"tool-darwin-arm64.tar.gz","browser_download_url":"https://example.com/tool"}]}]),
            )
        };
        let output = root.path().join("first");
        let report = discover_with_fetch(
            catalog,
            &[broken, definition],
            &BTreeMap::new(),
            &output,
            1,
            fetch,
        )
        .unwrap();
        assert_eq!(report.updated, ["tool"]);
        assert_eq!(report.errors["broken"], "rate limited");
        let candidates = PackageCatalog::from_directory(&output.join("packages")).unwrap();
        let definitions = GitHubUpstream::from_directory(&output.join("packages")).unwrap();
        let repeat = root.path().join("repeat");
        let report = discover_with_fetch(
            &candidates,
            &definitions,
            &BTreeMap::new(),
            &repeat,
            1,
            fetch,
        )
        .unwrap();
        assert!(report.updated.is_empty());
        assert!(report.rules_changed.is_empty());
        assert_eq!(report.unchanged, ["tool"]);
        assert_eq!(fs::read_dir(repeat.join("packages")).unwrap().count(), 0);

        let mut definitions = definitions;
        definitions[0].repository_id = None;
        let metadata_only = root.path().join("metadata-only");
        let report = discover_with_fetch(
            &candidates,
            &definitions,
            &BTreeMap::new(),
            &metadata_only,
            1,
            fetch,
        )
        .unwrap();
        assert!(report.updated.is_empty());
        assert_eq!(report.rules_changed, ["tool"]);
        let saved = PackageCatalog::from_directory(&metadata_only.join("packages")).unwrap();
        assert_eq!(saved.sha256(), candidates.sha256());
        assert_eq!(
            GitHubUpstream::from_directory(&metadata_only.join("packages")).unwrap()[0]
                .repository_id,
            Some(42)
        );
    }

    #[test]
    fn discovery_preserves_templates_and_retained_version_overrides() {
        let source = r#"return {
            name = "tool",
            description = "Tool",
            default_version = "1",
            source = {
                github = "owner/tool",
                tag = "v{version}",
                assets = { ["aarch64-macos"] = "tool-{tag}-darwin-arm64.tar.gz" },
            },
            bins = { "tool" },
            checks = { { "tool", "--version" } },
            versions = {
                ["1"] = {
                    revision = 3,
                    assets = { ["aarch64-macos"] = "legacy-tool.tar.gz" },
                    checks = { { "tool", "--help" } },
                },
            },
        }"#;
        let definition = PackageDefinition::from_lua(source).unwrap();
        let original: Value = rootbeer_package::definition::lua::read(source).unwrap();
        let definitions = BTreeMap::from([("tool".into(), definition)]);
        let catalog = PackageCatalog::from_definitions(&definitions).unwrap();
        let upstreams = GitHubUpstream::from_definitions(&definitions).unwrap();
        let root = tempfile::tempdir().unwrap();
        for version in ["1", "2"] {
            let output = root.path().join(version);
            let report = discover_with_fetch(
                &catalog, &upstreams, &definitions, &output, 1, |url| {
                    if !url.contains("/releases") {
                        return Ok(serde_json::json!({"id":42,"full_name":"owner/tool","description":"Tool"}));
                    }
                    Ok(serde_json::json!([{
                        "id":1,
                        "tag_name":format!("v{version}"),
                        "assets":[{
                            "name": if version == "1" { "legacy-tool.tar.gz" } else { "tool-v2-darwin-arm64.tar.gz" },
                            "browser_download_url":"https://example.com/tool"
                        }]
                    }]))
                },
            ).unwrap();
            assert!(report.errors.is_empty());
            assert_eq!(report.updated.is_empty(), version == "1");
            assert_eq!(report.rules_changed, ["tool"]);
            let saved_source = fs::read_to_string(output.join("packages/tool.lua")).unwrap();
            let mut saved: Value = rootbeer_package::definition::lua::read(&saved_source).unwrap();
            assert_eq!(saved["source"]["repository_id"], 42);
            saved["source"]
                .as_object_mut()
                .unwrap()
                .remove("repository_id");
            assert_eq!(saved["source"], original["source"]);
            assert_eq!(saved["versions"]["1"], original["versions"]["1"]);
            assert_eq!(saved["checks"], original["checks"]);
            let expanded = PackageDefinition::from_lua(&saved_source).unwrap();
            assert_eq!(expanded.package.default_version, version);
            assert_eq!(
                serde_json::to_value(&expanded.package.versions["1"]).unwrap(),
                serde_json::to_value(&catalog.packages["tool"].versions["1"]).unwrap(),
            );
        }
    }

    #[test]
    fn seeds_only_github_projects_and_preserves_functional_checks() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("upstreams");
        let catalog = PackageCatalog::embedded().unwrap();
        let count = seed_upstreams(catalog, &output).unwrap();
        let definitions = GitHubUpstream::from_directory(&output).unwrap();
        assert_eq!(count, definitions.len());
        assert!(!definitions.iter().any(|definition| definition.name == "xz"));
        let age = definitions
            .iter()
            .find(|definition| definition.name == "age")
            .unwrap();
        assert_eq!(age.bins, ["age", "age-keygen"]);
        assert_eq!(age.tag_prefix.as_deref(), Some("v"));
    }
}
