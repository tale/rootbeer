use crate::{BuildDependency, DependencyKind};
use crate::{CatalogPackage, CatalogRecipe, PackageCatalog, PackageRequest, ResolveContext};
use std::collections::{BTreeMap, BTreeSet};

pub fn find_recipe<'a>(
    catalog: &'a PackageCatalog,
    request: &str,
) -> Result<(&'a CatalogPackage, &'a str, &'a CatalogRecipe), String> {
    let request = PackageRequest::parse(request);
    if request
        .resolver
        .as_deref()
        .is_some_and(|resolver| resolver != "rootbeer")
    {
        return Err("source builds use canonical catalog names".into());
    }
    let package = catalog
        .find(&request.name)
        .ok_or_else(|| format!("unknown package `{}`", request.name))?;
    let version = request
        .version
        .as_deref()
        .unwrap_or_else(|| package.default_version_for(&ResolveContext::current().system));
    let (version, recipe) = package
        .versions
        .get_key_value(version)
        .ok_or_else(|| format!("{}@{version} is not in the catalog", package.name))?;
    Ok((package, version, recipe))
}

fn visit(
    catalog: &PackageCatalog,
    request: &str,
    active: &mut BTreeSet<String>,
    done: &mut BTreeSet<String>,
    order: &mut Vec<String>,
) -> Result<(), String> {
    let (package, version, recipe) = find_recipe(catalog, request)?;
    recipe.validate()?;
    let key = format!("{}@{version}", package.name);
    if done.contains(&key) {
        return Ok(());
    }
    if !active.insert(key.clone()) {
        return Err(format!("build dependency cycle at `{key}`"));
    }
    if let Some(build) = &recipe.build {
        for dependency in &build.dependencies {
            let dependency = dependency.package();
            if !catalog
                .packages
                .contains_key(&PackageRequest::parse(dependency).name)
            {
                return Err(format!(
                    "build dependency `{dependency}` must use a canonical package name"
                ));
            }
            visit(catalog, dependency, active, done, order)?;
        }
    }
    active.remove(&key);
    done.insert(key.clone());
    order.push(key);
    Ok(())
}

pub(crate) fn validate_dependencies(catalog: &PackageCatalog) -> Result<(), String> {
    let mut done = BTreeSet::new();
    for package in catalog.packages.values() {
        for version in package.versions.keys() {
            visit(
                catalog,
                &format!("{}@{version}", package.name),
                &mut BTreeSet::new(),
                &mut done,
                &mut Vec::new(),
            )?;
        }
    }
    Ok(())
}

/// A validated dependency closure in deterministic execution order.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DependencyGraph {
    pub system: String,
    pub order: Vec<String>,
    pub nodes: std::collections::BTreeMap<String, DependencyNode>,
}

/// Direct edges and the ordered transitive inputs visible to a recipe.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DependencyNode {
    pub dependencies: Vec<BuildDependency>,
    pub closure: Vec<String>,
    pub runtime_closure: Vec<String>,
    pub exports: BTreeMap<String, DependencyExports>,
}

/// Exports visible to a consumer, after propagating dependency roles.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DependencyExports {
    pub has_bins: bool,
    pub has_libraries: bool,
    pub libraries: Vec<std::path::PathBuf>,
}

fn exports(
    package: &str,
    kind: DependencyKind,
    nodes: &BTreeMap<String, DependencyNode>,
    visible: &mut BTreeMap<String, DependencyExports>,
    visited: &mut BTreeSet<(String, DependencyKind)>,
) {
    if kind == DependencyKind::Runtime || !visited.insert((package.into(), kind)) {
        return;
    }
    let entry = visible.entry(package.into()).or_default();
    entry.has_bins |= matches!(kind, DependencyKind::All | DependencyKind::Build);
    entry.has_libraries |= kind != DependencyKind::Build;
    if kind == DependencyKind::Build {
        return;
    }
    for dependency in &nodes[package].dependencies {
        if dependency.kind() == DependencyKind::Runtime
            || (matches!(kind, DependencyKind::Link | DependencyKind::LinkRuntime)
                && dependency.kind() == DependencyKind::Build)
        {
            continue;
        }
        let kind = if matches!(kind, DependencyKind::Link | DependencyKind::LinkRuntime) {
            DependencyKind::Link
        } else {
            dependency.kind()
        };
        exports(dependency.package(), kind, nodes, visible, visited);
    }
}

impl DependencyGraph {
    /// Resolves aliases and defaults before constructing one graph for all roots.
    pub fn new(
        catalog: &PackageCatalog,
        requests: &[String],
        system: &str,
    ) -> Result<Self, String> {
        let mut order = Vec::new();
        let mut done = BTreeSet::new();
        for request in requests {
            let parsed = PackageRequest::parse(request);
            let package = catalog
                .find(&parsed.name)
                .ok_or_else(|| format!("unknown package `{}`", parsed.name))?;
            if parsed
                .resolver
                .as_deref()
                .is_some_and(|resolver| resolver != "rootbeer")
            {
                return Err("build graphs require canonical catalog requests".into());
            }
            let version = parsed
                .version
                .as_deref()
                .unwrap_or_else(|| package.default_version_for(system));
            visit(
                catalog,
                &format!("{}@{version}", package.name),
                &mut BTreeSet::new(),
                &mut done,
                &mut order,
            )?;
        }
        let mut nodes = std::collections::BTreeMap::<String, DependencyNode>::new();
        for key in &order {
            let (_, _, recipe) = find_recipe(catalog, key)?;
            if !recipe.systems.iter().any(|value| value == system) {
                return Err(format!("{key} has no recipe for {system}"));
            }
            let dependencies = recipe
                .build
                .as_ref()
                .map(|build| build.dependencies.clone())
                .unwrap_or_default();
            let mut closure = BTreeSet::new();
            let mut runtime_closure = BTreeSet::new();
            let mut visible = BTreeMap::new();
            let mut visited = BTreeSet::new();
            for dependency in &dependencies {
                let package = dependency.package();
                if dependency.kind().is_runtime() {
                    runtime_closure.insert(package.to_string());
                    runtime_closure.extend(nodes[package].runtime_closure.iter().cloned());
                }
                closure.insert(package.to_string());
                closure.extend(nodes[package].closure.iter().cloned());
                exports(
                    package,
                    dependency.kind(),
                    &nodes,
                    &mut visible,
                    &mut visited,
                );
            }
            for (name, exports) in &mut visible {
                if exports.has_libraries {
                    exports.libraries = find_recipe(catalog, name)?
                        .2
                        .build
                        .as_ref()
                        .map(|build| build.libraries.clone())
                        .unwrap_or_default();
                }
            }
            nodes.insert(
                key.clone(),
                DependencyNode {
                    dependencies,
                    runtime_closure: order
                        .iter()
                        .filter(|key| runtime_closure.contains(*key))
                        .cloned()
                        .collect(),
                    exports: visible,
                    closure: order
                        .iter()
                        .filter(|dependency| closure.contains(*dependency))
                        .cloned()
                        .collect(),
                },
            );
        }
        Ok(Self {
            system: system.into(),
            order,
            nodes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_edges_are_separate_from_build_and_link_exports() {
        let template = PackageCatalog::embedded().unwrap().packages["xz"].clone();
        let mut catalog = PackageCatalog {
            schema: 1,
            packages: BTreeMap::new(),
        };
        for (name, edges) in [
            ("data", vec![]),
            ("base", vec![]),
            ("tool", vec![("data", DependencyKind::Runtime)]),
            ("static", vec![]),
            ("shared", vec![("base", DependencyKind::LinkRuntime)]),
            (
                "root",
                vec![
                    ("tool", DependencyKind::Build),
                    ("static", DependencyKind::Link),
                    ("shared", DependencyKind::LinkRuntime),
                    ("data", DependencyKind::Runtime),
                ],
            ),
        ] {
            let mut package = template.clone();
            package.name = name.into();
            package.aliases.clear();
            package.default_version = "1".into();
            package.default_versions.clear();
            let mut recipe = package.versions.values().next().unwrap().clone();
            recipe.build.as_mut().unwrap().dependencies = edges
                .into_iter()
                .map(|(name, kind)| BuildDependency::Scoped {
                    package: format!("{name}@1"),
                    kind,
                })
                .collect();
            package.versions = BTreeMap::from([("1".into(), recipe)]);
            catalog.packages.insert(name.into(), package);
        }
        let graph = DependencyGraph::new(
            &catalog,
            &["root".into()],
            &ResolveContext::current().system,
        )
        .unwrap();
        let root = &graph.nodes["root@1"];
        assert_eq!(
            root.runtime_closure
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["base@1".into(), "shared@1".into(), "data@1".into()])
        );
        assert!(!root.exports.contains_key("data@1"));
        assert!(root.exports["tool@1"].has_bins);
        assert!(!root.exports["tool@1"].has_libraries);
        for name in ["static@1", "shared@1", "base@1"] {
            assert!(root.exports[name].has_libraries);
            assert!(!root.exports[name].has_bins);
        }
    }

    #[test]
    fn diamond_dependencies_share_nodes_and_keep_direct_edges() {
        let template = PackageCatalog::embedded().unwrap().packages["xz"].clone();
        let version = template.default_version.clone();
        let mut catalog = PackageCatalog {
            schema: 1,
            packages: Default::default(),
        };
        for (name, dependencies) in [
            ("base", vec![]),
            ("left", vec!["base"]),
            ("right", vec!["base"]),
            ("root", vec!["left", "right"]),
        ] {
            let mut package = template.clone();
            package.name = name.into();
            package.aliases.clear();
            for recipe in package.versions.values_mut() {
                recipe.build.as_mut().unwrap().dependencies = dependencies
                    .iter()
                    .map(|name| format!("{name}@{version}").into())
                    .collect();
            }
            catalog.packages.insert(name.into(), package);
        }
        catalog
            .packages
            .get_mut("root")
            .unwrap()
            .aliases
            .push("alias".into());
        let graph = DependencyGraph::new(
            &catalog,
            &["alias".into(), "right".into()],
            &ResolveContext::current().system,
        )
        .unwrap();
        let keys = ["base", "left", "right", "root"].map(|name| format!("{name}@{version}"));
        assert_eq!(graph.order, keys);
        assert_eq!(
            graph.nodes[&keys[3]]
                .dependencies
                .iter()
                .map(BuildDependency::package)
                .collect::<Vec<_>>(),
            keys[1..3]
        );
        assert_eq!(graph.nodes[&keys[3]].closure, keys[..3]);
        assert!(graph.nodes[&keys[0]].closure.is_empty());
        catalog
            .packages
            .get_mut("base")
            .unwrap()
            .versions
            .get_mut(&version)
            .unwrap()
            .build
            .as_mut()
            .unwrap()
            .dependencies
            .push(keys[3].clone().into());
        assert!(DependencyGraph::new(
            &catalog,
            &["root".into()],
            &ResolveContext::current().system
        )
        .unwrap_err()
        .contains("cycle"));
    }
}
