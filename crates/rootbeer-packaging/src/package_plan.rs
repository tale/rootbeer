use std::collections::BTreeMap;

use rootbeer_package::distribution::{input_key, DependencyInputs};
use rootbeer_package::graph::{find_recipe, find_recipe_for_system, DependencyGraph};
use rootbeer_package::{PackageCatalog, ResolveContext};

use crate::BuildOptions;

/// One package to qualify on the current platform.
#[derive(Debug, serde::Serialize)]
pub struct PackageTask {
    pub package: String,
    pub name: String,
    pub system: String,
    pub key: String,
}

/// What identifies every package in a build's closure, so a change to any of them is a new build.
pub(crate) fn dependency_inputs(
    catalog: &PackageCatalog,
    id: &str,
    system: &str,
) -> Result<BTreeMap<String, DependencyInputs>, String> {
    let graph = DependencyGraph::new(catalog, &[id.to_string()], system)?;
    graph.nodes[id]
        .closure
        .iter()
        .map(|dependency| {
            let (package, version, recipe) = find_recipe_for_system(catalog, dependency, system)?;
            let inputs = DependencyInputs {
                revision: package.versions[version].revision,
                recipe_sha256: recipe.sha256(),
                engine_sha256: rootbeer_build::engine_identity(
                    recipe.build.as_ref().map(|build| &build.backend),
                ),
            };
            Ok((dependency.clone(), inputs))
        })
        .collect()
}

/// Plans explicitly selected packages, and the closure each builds with, without building them.
pub fn plan_packages(
    catalog: &PackageCatalog,
    requests: &[String],
    options: &BuildOptions,
    context: &str,
) -> Result<Vec<PackageTask>, String> {
    catalog.validate()?;
    if requests.is_empty() || context.trim().is_empty() {
        return Err(
            "package planning requires exact package requests and an environment context".into(),
        );
    }
    let system = ResolveContext::current().system;
    let mut tasks = std::collections::BTreeMap::new();
    let mut environments = std::collections::BTreeMap::new();
    for request in requests {
        let (package, version, recipe) = find_recipe(catalog, request)?;
        let revision = package.versions[version].revision;
        let id = format!("{}@{version}", package.name);
        if *request != id {
            return Err(format!("use the exact canonical request {id}"));
        }
        let engine =
            rootbeer_build::engine_identity(recipe.build.as_ref().map(|build| &build.backend));
        let environment = match environments.get(&engine) {
            Some(environment) => environment,
            None => {
                let identity = options.environment_identity(catalog, request, context)?;
                environments.entry(engine.clone()).or_insert(identity)
            }
        };
        tasks.insert(
            id.clone(),
            PackageTask {
                key: input_key(
                    &id,
                    &system,
                    revision,
                    &recipe,
                    &engine,
                    environment,
                    &dependency_inputs(catalog, &id, &system)?,
                ),
                package: id,
                name: package.name.clone(),
                system: system.clone(),
            },
        );
    }
    Ok(tasks.into_values().collect())
}
