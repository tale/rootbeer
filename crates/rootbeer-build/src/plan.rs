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
    /// Catalog revision per key; a recipe is one platform and no longer carries it.
    pub(crate) revisions: BTreeMap<String, u32>,
    pub(crate) inputs: PackageResolverInputs,
}

impl BuildPlan {
    /// Pins the metadata sources needed by this graph and resolves all binary inputs.
    pub fn current(catalog: &PackageCatalog, request: &str) -> Result<Self, String> {
        catalog.validate()?;
        let graph = DependencyGraph::new(
            catalog,
            &[request.into()],
            &ResolveContext::current().system,
        )?;
        let mut inputs = PackageResolverInputs::default();
        inputs.resolvers.insert(
            "rootbeer".into(),
            ResolverInput::Catalog {
                sha256: catalog.sha256(),
            },
        );
        Self::from_graph(catalog, graph, &inputs, backend_stack())
    }

    /// Resolves a build under caller-supplied, pinned metadata inputs.
    pub fn resolve(
        catalog: &PackageCatalog,
        request: &str,
        inputs: &PackageResolverInputs,
    ) -> Result<Self, String> {
        catalog.validate()?;
        let graph = DependencyGraph::new(
            catalog,
            &[request.into()],
            &ResolveContext::current().system,
        )?;
        Self::from_graph(catalog, graph, inputs, backend_stack())
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
        let mut revisions = BTreeMap::new();
        for key in &graph.order {
            let (_, _, recipe) = graph::find_recipe(catalog, key)?;
            let (_, _, entry) = graph::find_recipe_definition(catalog, key)?;
            revisions.insert(key.clone(), entry.revision);
            let is_prebuilt = key != root && recipe.build.is_none();
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
            revisions,
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

#[cfg(test)]
#[path = "plan_test.rs"]
mod tests;
