use rootbeer_package::{
    distribution::input_key, graph::find_recipe, PackageCatalog, ResolveContext,
};

use crate::BuildOptions;

/// One source package to qualify on the current platform.
#[derive(Debug, serde::Serialize)]
pub struct PackageTask {
    pub package: String,
    pub name: String,
    pub system: String,
    pub key: String,
}

/// Plans explicitly selected, dependency-free source packages without building them.
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
        let id = format!("{}@{version}", package.name);
        if *request != id {
            return Err(format!("use the exact canonical request {id}"));
        }
        let build = recipe
            .build
            .as_ref()
            .ok_or_else(|| format!("{id}: requires a source recipe"))?;
        if !build.dependencies.is_empty() {
            return Err(format!(
                "{id}: separate dependency results are not supported yet"
            ));
        }
        if !recipe.systems.contains(&system) {
            continue;
        }
        let engine = rootbeer_build::engine_identity(Some(&build.backend));
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
                key: input_key(&id, &system, recipe, &engine, environment),
                package: id,
                name: package.name.clone(),
                system: system.clone(),
            },
        );
    }
    Ok(tasks.into_values().collect())
}
