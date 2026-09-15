use super::*;

impl RecipeDefinition {
    pub(in crate::definition) fn from_definition(
        definition: &PackageDefinition,
    ) -> Result<Self, String> {
        let package = &definition.package;
        let recipe = package
            .versions
            .get(&package.default_version)
            .ok_or("default version has no recipe")?;
        let (inputs, build, outputs) = split(recipe)?;
        let mut result = Self {
            schema: 2,
            name: package.name.clone(),
            aliases: package.aliases.clone(),
            description: package.description.clone(),
            homepage: package.homepage.clone(),
            default_version: package.default_version.clone(),
            default_versions: package.default_versions.clone(),
            upstream: None,
            inputs,
            build,
            systems: recipe.systems.clone(),
            outputs,
            versions: BTreeMap::new(),
        };
        if let Some(input) = &mut result.inputs.source {
            input.sha256 = None;
        }
        if let Some(input) = &mut result.inputs.prebuilt {
            input.checksums = None;
        }
        if let Some(input) = &mut result.inputs.prebuilt {
            input.tag = input.tag.as_ref().map(|tag| {
                tag.strip_suffix(&package.default_version)
                    .map(|prefix| format!("{prefix}{{version}}"))
                    .unwrap_or_else(|| tag.clone())
            });
        }
        result.apply_upstream(definition)?;
        result.update_versions(package)?;
        Ok(result)
    }

    fn apply_upstream(&mut self, definition: &PackageDefinition) -> Result<(), String> {
        let Some(upstream) = definition.github_upstream()? else {
            self.upstream = None;
            return Ok(());
        };
        self.upstream = Some(Discovery {
            github: upstream.repository,
            repository_id: upstream.repository_id,
            tag_prefix: upstream.tag_prefix,
            exclude_tags: upstream.exclude_tags,
            systems: (upstream.systems != self.systems).then_some(upstream.systems),
        });
        self.outputs.bins = Some(upstream.bins);
        self.outputs.bin_paths = (!upstream.bin_paths.is_empty()).then_some(upstream.bin_paths);
        self.outputs.apps = (!upstream.apps.is_empty()).then_some(upstream.apps);
        self.outputs.checks = Some(upstream.checks);
        if let Some(build) = &upstream.build {
            let input = self
                .inputs
                .source
                .as_mut()
                .ok_or("source discovery requires source inputs")?;
            input.url = Some(build.url.clone());
            input.strip_prefix = Some(build.strip_prefix.to_string_lossy().into_owned());
        }
        if let Some(input) = &mut self.inputs.prebuilt {
            let mut assets = input.assets.clone().unwrap_or_default();
            assets.extend(upstream.assets);
            input.assets = Some(assets);
            input.mirror = upstream.mirror.then_some(true);
        }
        Ok(())
    }

    fn update_versions(&mut self, package: &CatalogPackage) -> Result<(), String> {
        self.versions
            .retain(|version, _| package.versions.contains_key(version));
        for (version, recipe) in &package.versions {
            if let Some(entry) = self.versions.get(version) {
                if same(&self.expand_version(version, entry)?, recipe)? {
                    continue;
                }
            }
            let (inputs, build, outputs) = split(recipe)?;
            let mut entry = Version {
                revision: recipe.revision,
                inputs: Some(inputs),
                build,
                systems: (recipe.systems != self.systems).then(|| recipe.systems.clone()),
                outputs: Some(outputs),
            };
            minimize(self, version, recipe, &mut entry)?;
            self.versions.insert(version.clone(), entry);
        }
        Ok(())
    }

    pub(in crate::definition) fn updated(
        &self,
        definition: &PackageDefinition,
    ) -> Result<Self, String> {
        let mut result = self.clone();
        let package = &definition.package;
        result.name = package.name.clone();
        result.aliases = package.aliases.clone();
        result.description = package.description.clone();
        result.homepage = package.homepage.clone();
        result.default_version = package.default_version.clone();
        result.default_versions = package.default_versions.clone();
        if !same(
            &self.expand()?.github_upstream()?,
            &definition.github_upstream()?,
        )? {
            result.apply_upstream(definition)?;
        }
        result.update_versions(package)?;
        if !same(&result.expand()?.package, package)? {
            return Err("recipe rendering changed resolved package data".into());
        }
        Ok(result)
    }
}

fn same(a: &impl Serialize, b: &impl Serialize) -> Result<bool, String> {
    Ok(serde_json::to_value(a).map_err(|error| error.to_string())?
        == serde_json::to_value(b).map_err(|error| error.to_string())?)
}

fn split(recipe: &CatalogRecipe) -> Result<(Inputs, Option<Build>, Outputs), String> {
    let outputs = Outputs {
        bins: Some(recipe.bins.clone()),
        checks: Some(recipe.checks.clone()),
        bin_paths: (!recipe.bin_paths.is_empty()).then(|| recipe.bin_paths.clone()),
        apps: (!recipe.apps.is_empty()).then(|| recipe.apps.clone()),
        libraries: recipe
            .build
            .as_ref()
            .filter(|build| !build.libraries.is_empty())
            .map(|build| build.libraries.clone()),
    };
    if let Some(build) = &recipe.build {
        let value = serde_json::to_value(build).map_err(|error| error.to_string())?;
        let input = Source {
            git: build.git.clone(),
            url: Some(build.url.clone()),
            sha256: Some(build.sha256.clone()),
            archive: value["archive"].as_str().map(String::from),
            strip_prefix: Some(build.strip_prefix.to_string_lossy().into_owned()),
            patches: (!build.patches.is_empty()).then(|| build.patches.clone()),
        };
        return Ok((
            Inputs {
                source: Some(input),
                prebuilt: prebuilt(recipe)?,
            },
            Some(Build {
                backend: build.backend.clone(),
                configure: build.configure.clone(),
                args: build.args.clone(),
                dependencies: build.dependencies.clone(),
                steps: build.steps.clone(),
            }),
            outputs,
        ));
    }
    Ok((
        Inputs {
            prebuilt: prebuilt(recipe)?,
            source: None,
        },
        None,
        outputs,
    ))
}

fn prebuilt(recipe: &CatalogRecipe) -> Result<Option<Prebuilt>, String> {
    let Some(source) = &recipe.source else {
        return Ok(None);
    };
    let request = PackageRequest::parse(source);
    if !matches!(request.resolver.as_deref(), Some("github" | "aqua")) {
        return Err("unsupported prebuilt input provider".into());
    }
    Ok(Some(Prebuilt {
        enabled: None,
        systems: recipe
            .build
            .as_ref()
            .filter(|_| request.resolver.as_deref() == Some("github"))
            .map(|_| recipe.assets.keys().cloned().collect()),
        github: (request.resolver.as_deref() == Some("github")).then(|| request.name.clone()),
        aqua: (request.resolver.as_deref() == Some("aqua")).then_some(request.name),
        tag: request.version,
        assets: (!recipe.assets.is_empty()).then(|| recipe.assets.clone()),
        checksums: (!recipe.checksums.is_empty()).then(|| recipe.checksums.clone()),
        mirror: recipe.mirror.then_some(true),
    }))
}

fn minimize(
    shared: &RecipeDefinition,
    version: &str,
    recipe: &CatalogRecipe,
    entry: &mut Version,
) -> Result<(), String> {
    if same(&entry.build, &shared.build)? {
        entry.build = None;
    }
    if let Some(outputs) = &mut entry.outputs {
        if outputs.bins.clone().unwrap_or_default()
            == shared.outputs.bins.clone().unwrap_or_default()
        {
            outputs.bins = None;
        }
        if outputs.checks.clone().unwrap_or_default()
            == shared.outputs.checks.clone().unwrap_or_default()
        {
            outputs.checks = None;
        }
        for (value, base) in [
            (&mut outputs.bin_paths, &shared.outputs.bin_paths),
            (&mut outputs.apps, &shared.outputs.apps),
        ] {
            if value.clone().unwrap_or_default() == base.clone().unwrap_or_default() {
                *value = None;
            } else if value.is_none() {
                *value = Some(BTreeMap::new());
            }
        }
        if outputs.libraries.clone().unwrap_or_default()
            == shared.outputs.libraries.clone().unwrap_or_default()
        {
            outputs.libraries = None;
        } else if outputs.libraries.is_none() {
            outputs.libraries = Some(Vec::new());
        }
        if same(outputs, &Outputs::default())? {
            entry.outputs = None;
        }
    }
    if recipe.source.is_none() && shared.inputs.prebuilt.is_some() {
        entry.inputs.get_or_insert_with(Inputs::default).prebuilt = Some(Prebuilt {
            enabled: Some(false),
            ..Prebuilt::default()
        });
    }
    if let Some(inputs) = &mut entry.inputs {
        if let (Some(input), Some(base)) = (&mut inputs.prebuilt, &shared.inputs.prebuilt) {
            if input.github == base.github {
                input.github = None;
            }
            if input.aqua == base.aqua {
                input.aqua = None;
            }
            let tag = input.tag.clone().unwrap_or_default();
            if base
                .tag
                .as_ref()
                .map(|pattern| expand(pattern, version, None))
                .transpose()?
                .as_ref()
                == Some(&tag)
            {
                input.tag = None;
            }
            if let Some(assets) = &mut input.assets {
                let base = base.assets.clone().unwrap_or_default();
                let mut overrides = BTreeMap::new();
                for (system, asset) in assets.iter() {
                    if base
                        .get(system)
                        .map(|pattern| expand(pattern, version, Some(&tag)))
                        .transpose()?
                        .as_ref()
                        != Some(asset)
                    {
                        overrides.insert(system.clone(), asset.clone());
                    }
                }
                input.assets = (!overrides.is_empty()).then_some(overrides);
            }
            if input.mirror.unwrap_or(false) == base.mirror.unwrap_or(false) {
                input.mirror = None;
            } else {
                input.mirror = Some(input.mirror.unwrap_or(false));
            }
            if same(input, &Prebuilt::default())? {
                inputs.prebuilt = None;
            }
        }
        if let (Some(input), Some(base)) = (&mut inputs.source, &shared.inputs.source) {
            for (value, pattern) in [
                (&mut input.url, &base.url),
                (&mut input.strip_prefix, &base.strip_prefix),
            ] {
                let tag = shared.source_tag(version)?;
                if pattern
                    .as_ref()
                    .map(|pattern| expand(pattern, version, tag.as_deref()))
                    .transpose()?
                    == *value
                {
                    *value = None;
                }
            }
            if input.git == base.git {
                input.git = None;
            }
            if input.archive == base.archive {
                input.archive = None;
            }
            if input.patches.clone().unwrap_or_default() == base.patches.clone().unwrap_or_default()
            {
                input.patches = None;
            } else if input.patches.is_none() {
                input.patches = Some(Vec::new());
            }
        }
        if inputs.prebuilt.is_none() && inputs.source.is_none() {
            entry.inputs = None;
        }
    }
    if !same(&shared.expand_version(version, entry)?, recipe)? {
        return Err("recipe overrides changed resolved data".into());
    }
    Ok(())
}
