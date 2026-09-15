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
        let graph = DependencyGraph::new(
            catalog,
            &[request.into()],
            &ResolveContext::current().system,
        )?;
        let needs_aqua = graph.order.iter().any(|key| {
            graph::find_recipe(catalog, key).is_ok_and(|(_, _, recipe)| {
                recipe
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
        Self::from_graph(catalog, graph, &inputs)
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
        Self::from_graph(catalog, graph, inputs)
    }

    fn from_graph(
        catalog: &PackageCatalog,
        graph: DependencyGraph,
        inputs: &PackageResolverInputs,
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
        let mut resolver = backend_stack(inputs).with_implicit_resolver("rootbeer");
        resolver.push(CatalogResolver::new(catalog, inputs, backend_stack(inputs)));
        let mut binaries = BTreeMap::new();
        let mut recipes = BTreeMap::new();
        for key in &graph.order {
            let (_, _, recipe) = graph::find_recipe(catalog, key)?;
            if recipe
                .source
                .as_deref()
                .is_some_and(|source| source.starts_with("aqua:"))
                && inputs.aqua_registry().is_none()
            {
                return Err("build dependencies require a pinned Aqua registry".into());
            }
            if recipe.build.is_none() {
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
