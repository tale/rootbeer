mod cache;
mod environment;
pub use environment::{verify_environment, BuildEnvironment};
mod plan;
pub use cache::BuildCache;
pub use plan::BuildPlan;

mod archive;
pub mod audit;
mod backend;
mod runner;
mod sandbox;
pub use archive::pack;
pub use runner::{run, run_with_sandbox};
pub use sandbox::Sandbox;

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::process::Command;
use std::time::Duration;

use rootbeer_package::download::DownloadCache;
use rootbeer_package::*;

use rootbeer_store::{hash_file, hash_tree, Store};

pub mod dependencies;
#[cfg(test)]
mod libraries_test;

/// Resource limits and storage locations for a build execution.
#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub environment: Option<BuildEnvironmentLock>,
    pub is_isolated: bool,
    pub jobs: usize,
    pub downloads: PathBuf,
    pub cache: Option<BuildCache>,
    pub phase_timeout: Duration,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            environment: None,
            is_isolated: false,
            jobs: 2,
            downloads: rootbeer_store::state_dir().join("downloads"),
            cache: None,
            phase_timeout: Duration::from_secs(1200),
        }
    }
}

impl BuildOptions {
    fn validate(&self) -> Result<(), String> {
        if self.is_isolated && self.environment.is_none() {
            return Err("isolated builds require a pinned environment".into());
        }
        if self.jobs == 0 || self.jobs > 64 {
            return Err("jobs must be between 1 and 64".into());
        }
        if self.phase_timeout.is_zero() {
            return Err("phase timeout must be positive".into());
        }
        if self
            .cache
            .as_ref()
            .is_some_and(|cache| cache.context.trim().is_empty())
        {
            return Err(
                "build cache requires an identity for the host image, SDK, and toolchain".into(),
            );
        }
        Ok(())
    }
}

/// Resolves and builds a source recipe using trusted host tools.
pub fn build_package(
    catalog: &PackageCatalog,
    request: &str,
    output: &Path,
    opts: &BuildOptions,
) -> Result<BuildArtifact, String> {
    opts.validate()?;
    BuildPlan::current(catalog, request)?.execute(output, opts)
}

fn execute(plan: &BuildPlan, output: &Path, opts: &BuildOptions) -> Result<BuildArtifact, String> {
    opts.validate()?;
    let environment = environment::Environment::resolve(opts.environment.as_ref())?;
    let isolation = if opts.is_isolated {
        Sandbox::identity()?
    } else {
        "host".into()
    };
    let context = opts.cache.as_ref().map_or("", |cache| &cache.context);
    let identity = environment.identity(&format!("{context}\0{isolation}"))?;
    let graph = &plan.graph;
    let cache = opts.cache.as_ref();
    fs::create_dir(output)
        .map_err(|e| format!("cannot create build output {}: {e}", output.display()))?;
    let output = output.canonicalize().map_err(|e| e.to_string())?;
    let workspace = tempfile::tempdir_in(&output).map_err(|e| e.to_string())?;
    let host_tools = workspace.path().join("environment-tools");
    environment.stage(&host_tools)?;
    let probe = tempfile::tempdir_in(&output).map_err(|error| error.to_string())?;
    let probe_sandbox = opts
        .is_isolated
        .then(|| Sandbox::new(&environment.lock, [], probe.path()))
        .transpose()?;
    if let Some(sandbox) = &probe_sandbox {
        run_with_sandbox(
            &[
                environment.lock.tools["sh"]
                    .path
                    .to_string_lossy()
                    .into_owned(),
                "-c".into(),
                "exit 0".into(),
            ],
            probe.path(),
            &BTreeMap::new(),
            &output.join("sandbox.log"),
            Duration::from_secs(10),
            Some(sandbox),
        )
        .map_err(|error| format!("build isolation is unavailable: {error}"))?;
    }
    let realizer = PackageRealizer::with_dirs(
        Store::new(
            cache
                .map(|cache| cache.directory.join("store"))
                .unwrap_or_else(|| workspace.path().join("store")),
        ),
        &opts.downloads,
        workspace.path().join("install"),
    );
    let mut resolved = BTreeMap::<String, LockedPackage>::new();
    let mut dependency_bins = BTreeMap::new();
    let mut dependency_roots = BTreeMap::<String, PathBuf>::new();
    let mut root_artifact = None;
    for (index, key) in graph.order.iter().enumerate() {
        let recipe = &plan.recipes[key];
        let is_root = index + 1 == graph.order.len();
        let mut built = None;
        let mut cache_entry = None;
        let mut is_cached = false;
        let destination = if is_root {
            output.clone()
        } else {
            output.join(format!("dependency-{key}"))
        };
        let locked = if recipe.build.is_some() {
            if !is_root {
                fs::create_dir(&destination).map_err(|e| e.to_string())?;
            }
            let dependencies = graph.nodes[key]
                .closure
                .iter()
                .map(|key| (key.clone(), resolved[key].clone()))
                .collect::<BTreeMap<_, _>>();
            let tools = workspace.path().join(format!("tools-{index}"));
            fs::create_dir(&tools).map_err(|e| e.to_string())?;
            for (key, exports) in &graph.nodes[key].exports {
                if exports.has_bins {
                    let bins: &BTreeMap<String, PathBuf> = &dependency_bins[key];
                    for (name, path) in bins {
                        if fs::read_link(tools.join(name)).ok().as_ref() == Some(path) {
                            continue;
                        }
                        symlink(path, tools.join(name)).map_err(|e| {
                            format!("build dependency command collision for {name}: {e}")
                        })?;
                    }
                }
                if exports.has_libraries {
                    dependencies::stage(
                        &dependency_roots[key],
                        &exports.libraries,
                        &tools.join(".rootbeer-libraries"),
                    )
                    .map_err(|error| format!("{key}: {error}"))?;
                }
            }
            verify_environment(&environment.lock)?;
            cache_entry = cache
                .map(|cache| {
                    let key = cache::key(
                        key,
                        recipe,
                        &graph.system,
                        &dependencies,
                        &identity,
                        opts.jobs,
                        &graph.nodes[key].exports,
                    )?;
                    cache.entry(key)
                })
                .transpose()?;
            let restored = cache_entry
                .as_ref()
                .map(|entry| entry.restore(&destination))
                .transpose()?
                .flatten();
            is_cached = restored.is_some();
            let mut artifact = match restored {
                Some(mut artifact) => {
                    artifact.catalog_sha256 = plan.catalog_sha256.clone();
                    artifact.dependencies = dependencies;
                    artifact
                }
                None => compile(
                    plan,
                    key,
                    dependencies,
                    &tools,
                    &destination,
                    opts,
                    (&environment, &host_tools, &dependency_roots),
                )?,
            };
            artifact.environment = Some(environment.lock.clone());
            artifact.isolation = Some(isolation.clone());
            artifact.resolver_inputs = plan.inputs.clone();
            artifact.build_key = cache_entry.as_ref().map(|entry| entry.key().to_string());
            artifact.build_environment = cache.map(|cache| cache.context.clone());
            let locked = artifact.package.clone();
            built = Some(artifact);
            locked
        } else {
            plan.binaries[key].clone()
        };
        let realized = realizer.realize(&locked).map_err(|e| e.to_string())?;
        if let Some(artifact) = built.as_mut() {
            artifact.runtime_audit_sha256 =
                Some(audit_output(&realized.store_entry.path, &destination)?);
        }
        let check_workspace = tempfile::tempdir_in(&output).map_err(|error| error.to_string())?;
        let check_environment =
            environment.variables(&host_tools, &host_tools, check_workspace.path());
        let check_sandbox = opts
            .is_isolated
            .then(|| {
                Sandbox::new(
                    &environment.lock,
                    [host_tools.clone(), realized.store_entry.path.clone()],
                    check_workspace.path(),
                )
            })
            .transpose()?;
        for check in &recipe.checks {
            let mut args = check.clone();
            args[0] = realized.bins[&check[0]].to_string_lossy().into_owned();
            run_with_sandbox(
                &args,
                check_workspace.path(),
                &check_environment,
                &output.join("checks.log"),
                Duration::from_secs(30),
                check_sandbox.as_ref(),
            )?;
        }
        if let Some(artifact) = built {
            verify_environment(&environment.lock)?;
            write_install_files(&artifact, &destination)?;
            if let Some(entry) = &cache_entry {
                if !is_cached {
                    entry.save(&artifact, &destination)?;
                }
            }
            fs::write(
                destination.join("receipt.json"),
                serde_json::to_vec_pretty(&artifact).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            if is_root {
                root_artifact = Some(artifact);
            }
        }
        let mut locked = locked;
        locked.output_sha256 = Some(realized.store_entry.output_sha256);
        dependency_bins.insert(key.clone(), realized.bins);
        dependency_roots.insert(key.clone(), realized.store_entry.path);
        resolved.insert(key.clone(), locked);
    }
    root_artifact.ok_or_else(|| "source build produced no artifact".into())
}

fn compile(
    plan: &BuildPlan,
    key: &str,
    dependencies: BTreeMap<String, LockedPackage>,
    tools: &Path,
    output: &Path,
    opts: &BuildOptions,
    execution_environment: (&environment::Environment, &Path, &BTreeMap<String, PathBuf>),
) -> Result<BuildArtifact, String> {
    let (build_environment, host_tools, dependency_roots) = execution_environment;
    let recipe = &plan.recipes[key];
    let request = PackageRequest::parse(key);
    let version = request
        .version
        .as_ref()
        .ok_or("build plan must contain exact versions")?;
    let build = recipe.build.as_ref().ok_or("recipe has no source build")?;
    let downloads = DownloadCache::new(&opts.downloads);
    let archive = downloads
        .materialize_verified(&build.url, &build.sha256)
        .map_err(|e| e.to_string())?;
    let workspace = tempfile::tempdir_in(output).map_err(|e| e.to_string())?;
    let workspace_path = workspace.path().canonicalize().map_err(|e| e.to_string())?;
    rootbeer_package::realize::extract_archive(&archive, build.archive, workspace.path())
        .map_err(|e| e.to_string())?;
    let source = workspace
        .path()
        .join(&build.strip_prefix)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !source.starts_with(&workspace_path) {
        return Err("source directory escapes the archive".into());
    }
    let prefix = workspace_path.join("prefix");
    fs::create_dir(&prefix).map_err(|e| e.to_string())?;
    let mut environment = build_environment.variables(tools, host_tools, &workspace_path);
    let dependency_prefix = tools.join(".rootbeer-libraries");
    dependencies::environment(&dependency_prefix, &mut environment)?;
    let mut reads = vec![tools.to_path_buf(), host_tools.to_path_buf()];
    reads.extend(dependencies.keys().map(|key| dependency_roots[key].clone()));
    let sandbox = opts
        .is_isolated
        .then(|| Sandbox::new(&build_environment.lock, reads, &workspace_path))
        .transpose()?;
    let mut toolchain = BTreeMap::from([(
        "rootbeer.host-path".into(),
        if build_environment.is_host {
            "/usr/bin:/bin:/usr/sbin:/sbin"
        } else {
            ""
        }
        .into(),
    )]);
    let zig = tools.join("zig");
    let compiler_tools = match build.backend {
        BuildBackend::Autotools | BuildBackend::Custom => vec![
            (environment["CC"].as_str(), "--version"),
            ("make", "--version"),
        ],
        BuildBackend::Zig => {
            if !zig.is_file() {
                return Err("Zig builds require an exact catalog dependency providing zig".into());
            }
            vec![("zig", "version")]
        }
    };
    for (index, (name, argument)) in compiler_tools.into_iter().enumerate() {
        let log = output.join(format!("tool-{index}.log"));
        run_with_sandbox(
            &[name.into(), argument.into()],
            &source,
            &environment,
            &log,
            opts.phase_timeout,
            sandbox.as_ref(),
        )?;
        toolchain.insert(
            name.into(),
            fs::read_to_string(log)
                .map_err(|error| error.to_string())?
                .trim()
                .into(),
        );
    }
    let log = output.join("build.log");
    for (index, patch) in build.patches.iter().enumerate() {
        let path = workspace_path.join(format!("patch-{index}.diff"));
        fs::write(&path, patch).map_err(|e| e.to_string())?;
        run_with_sandbox(
            &[
                build_environment.lock.tools["patch"]
                    .path
                    .to_string_lossy()
                    .into_owned(),
                "--batch".into(),
                "-p1".into(),
                "-i".into(),
                path.to_string_lossy().into_owned(),
            ],
            &source,
            &environment,
            &log,
            Duration::from_secs(30),
            sandbox.as_ref(),
        )?;
    }
    fs::create_dir_all(workspace_path.join("zig-global-cache/tmp")).map_err(|e| e.to_string())?;
    let phases = backend::plan(
        build,
        &backend::Context {
            prefix: &prefix,
            dependencies: &dependency_prefix,
            tools,
            workspace: &workspace_path,
            jobs: opts.jobs,
        },
    )?;
    for phase in phases {
        for command in phase.commands {
            run_with_sandbox(
                &command,
                &source,
                &environment,
                &log,
                opts.phase_timeout,
                sandbox.as_ref(),
            )
            .map_err(|error| format!("{}: {error}", phase.name))?;
        }
    }
    let bins: BTreeMap<String, PathBuf> = recipe
        .bins
        .iter()
        .map(|bin| (bin.clone(), PathBuf::from("bin").join(bin)))
        .collect();
    for (name, path) in &bins {
        let path = prefix
            .join(path)
            .canonicalize()
            .map_err(|e| format!("missing output command {name}: {e}"))?;
        if !path.starts_with(&prefix) || !path.is_file() {
            return Err(format!("invalid output command {name}"));
        }
    }
    dependencies::validate(&prefix, &build.libraries)?;
    let runtime_audit = audit_output(&prefix, output)?;
    let artifact_path = output.join("package.tar.gz");
    pack(&prefix, &artifact_path).map_err(|e| e.to_string())?;
    let artifact = BuildArtifact {
        schema: 1,
        build_key: None,
        build_environment: None,
        environment: None,
        isolation: None,
        runtime_audit_sha256: Some(runtime_audit),
        catalog_sha256: plan.catalog_sha256.clone(),
        revision: recipe.revision,
        system: plan.graph.system.clone(),
        build: build.clone(),
        dependencies,
        resolver_inputs: PackageResolverInputs::default(),
        toolchain,
        package: LockedPackage {
            name: request.name,
            version: version.clone(),
            source: LockedSource::File {
                path: artifact_path.clone(),
                sha256: hash_file(&artifact_path).map_err(|e| e.to_string())?,
            },
            install: LockedInstall::Archive {
                format: ArchiveFormat::TarGz,
                strip_prefix: None,
            },
            provides: Provides {
                apps: recipe.apps.clone(),
                bins: bins.clone(),
            },
            output_sha256: Some(hash_tree(&prefix).map_err(|e| e.to_string())?),
        },
    };
    Ok(artifact)
}

fn audit_output(root: &Path, output: &Path) -> Result<String, String> {
    let report = audit::audit(root)?;
    let bytes = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    fs::write(output.join("runtime-audit.json"), &bytes).map_err(|error| error.to_string())?;
    report.validate()?;
    Ok(rootbeer_store::hash_bytes(&bytes))
}

fn write_install_files(artifact: &BuildArtifact, output: &Path) -> Result<(), String> {
    let LockedSource::File { sha256, .. } = &artifact.package.source else {
        unreachable!()
    };
    let spec = serde_json::json!({"name": artifact.package.name, "version": artifact.package.version,
        "source": {"file": "package.tar.gz", "sha256": sha256}, "install": {"archive": "tar.gz"}, "bins": artifact.package.provides.bins});
    fs::write(
        output.join("package.json"),
        serde_json::to_vec_pretty(&spec).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        output.join("install.lua"),
        "local rb = require(\"rootbeer\")\nrb.package(rb.json.read(\"package.json\"))\n",
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests;
