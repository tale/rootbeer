use std::collections::BTreeMap;
use std::path::Path;

use rootbeer_package::{catalog::CatalogResolver, graph::DependencyGraph, *};

use crate::BuildOptions;

/// Exact source recipes and resolved binary inputs, prepared before any build executes.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BuildPlan {
    pub(crate) catalog_sha256: String,
    pub(crate) graph: DependencyGraph,
    pub(crate) binaries: BTreeMap<String, LockedPackage>,
    pub(crate) recipes: BTreeMap<String, CatalogRecipe>,
    pub(crate) inputs: PackageResolverInputs,
}

impl BuildPlan {
    /// Pins the metadata sources needed by this graph and resolves all binary inputs.
    pub fn current(catalog: &PackageCatalog, request: &str) -> Result<Self, String> {
        catalog.validate()?;
        let graph = dependency_graph(catalog, request, &ResolveContext::current().system)?;
        let needs_aqua = graph.order.iter().any(|key| {
            graph::find_recipe(catalog, key).is_ok_and(|(_, _, recipe)| {
                Some(key) != graph.order.last()
                    && recipe.has_prebuilt(&graph.system)
                    && recipe
                        .source
                        .as_deref()
                        .is_some_and(|source| source.starts_with("aqua:"))
            })
        });
        let mut inputs = if needs_aqua {
            PackageResolverInputs::resolve_current().map_err(|error| error.to_string())?
        } else {
            PackageResolverInputs::default()
        };
        inputs.resolvers.insert(
            "rootbeer".into(),
            ResolverInput::Catalog {
                sha256: catalog.sha256(),
            },
        );
        Self::from_graph(catalog, graph, &inputs, backend_stack(&inputs))
    }

    /// Resolves a build under caller-supplied, pinned metadata inputs.
    pub fn resolve(
        catalog: &PackageCatalog,
        request: &str,
        inputs: &PackageResolverInputs,
    ) -> Result<Self, String> {
        catalog.validate()?;
        let graph = dependency_graph(catalog, request, &ResolveContext::current().system)?;
        Self::from_graph(catalog, graph, inputs, backend_stack(inputs))
    }

    fn from_graph(
        catalog: &PackageCatalog,
        graph: DependencyGraph,
        inputs: &PackageResolverInputs,
        backends: ResolverStack,
    ) -> Result<Self, String> {
        if inputs.catalog_sha256() != Some(catalog.sha256().as_str()) {
            return Err("build inputs must pin the current catalog".into());
        }
        let root = graph
            .order
            .last()
            .ok_or("build plan needs a root package")?;
        if graph::find_recipe(catalog, root)?.2.build.is_none() {
            return Err("this package uses an upstream binary, not a source build".into());
        }
        let context = ResolveContext::new(&graph.system);
        let mut resolver = ResolverStack::new().with_implicit_resolver("rootbeer");
        resolver.push(CatalogResolver::new(catalog, inputs, backends));
        let mut binaries = BTreeMap::new();
        let mut recipes = BTreeMap::new();
        for key in &graph.order {
            let (_, _, recipe) = graph::find_recipe(catalog, key)?;
            let is_prebuilt = key != root && recipe.has_prebuilt(&graph.system);
            if is_prebuilt
                && recipe
                    .source
                    .as_deref()
                    .is_some_and(|source| source.starts_with("aqua:"))
                && inputs.aqua_registry().is_none()
            {
                return Err("build dependencies require a pinned Aqua registry".into());
            }
            if is_prebuilt {
                let package = resolver
                    .resolve(&PackageRequest::parse(key), &context)
                    .map_err(|error| error.to_string())?
                    .package;
                binaries.insert(key.clone(), package);
            }
            recipes.insert(key.clone(), recipe.clone());
        }
        Ok(Self {
            catalog_sha256: catalog.sha256(),
            graph,
            binaries,
            recipes,
            inputs: inputs.clone(),
        })
    }

    pub fn graph(&self) -> &DependencyGraph {
        &self.graph
    }

    /// Executes the frozen recipes and binary facts without consulting mutable metadata.
    pub fn execute(&self, output: &Path, opts: &BuildOptions) -> Result<BuildArtifact, String> {
        crate::execute(self, output, opts)
    }
}

fn dependency_graph(
    catalog: &PackageCatalog,
    request: &str,
    system: &str,
) -> Result<DependencyGraph, String> {
    let parsed = PackageRequest::parse(request);
    let package = catalog
        .find(&parsed.name)
        .ok_or_else(|| format!("unknown package `{}`", parsed.name))?;
    let version = parsed
        .version
        .as_deref()
        .unwrap_or_else(|| package.default_version_for(system));
    let root = format!("{}@{version}", package.name);
    let mut selected = catalog.clone();
    for package in selected.packages.values_mut() {
        for (version, recipe) in &mut package.versions {
            if format!("{}@{version}", package.name) == root || !recipe.has_prebuilt(system) {
                continue;
            }
            if let Some(build) = &mut recipe.build {
                let has_libraries = !build.libraries.is_empty();
                build.dependencies.retain(|dependency| {
                    dependency.kind().is_runtime()
                        || (has_libraries
                            && matches!(
                                dependency.kind(),
                                DependencyKind::All | DependencyKind::Link
                            ))
                });
            }
        }
    }
    DependencyGraph::new(&selected, &[request.into()], system)
}

#[cfg(test)]
#[path = "plan_test.rs"]
mod tests;
