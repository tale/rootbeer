use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::os::unix::{fs::symlink, process::CommandExt};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use flate2::{write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};

use super::catalog::{CatalogPackage, CatalogRecipe};
use super::download::DownloadCache;
use super::{
    ArchiveFormat, LockedInstall, LockedPackage, LockedSource, PackageCatalog, PackageRealizer,
    PackageRequest, PackageResolverInputs, Provides, ResolveContext,
};
use crate::store::{hash_file, hash_tree, Store};

/// Supported source compilation mechanisms.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildBackend {
    Autotools,
}

/// Verified source inputs and exact canonical build dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceBuild {
    pub backend: BuildBackend,
    pub url: String,
    pub sha256: String,
    #[serde(
        serialize_with = "serialize_archive",
        deserialize_with = "deserialize_archive"
    )]
    pub archive: ArchiveFormat,
    pub strip_prefix: PathBuf,
    #[serde(default)]
    pub configure: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

fn serialize_archive<S: serde::Serializer>(
    format: &ArchiveFormat,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(match format {
        ArchiveFormat::TarGz => "tar.gz",
        ArchiveFormat::TarXz => "tar.xz",
        ArchiveFormat::Zip => "zip",
    })
}

fn deserialize_archive<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ArchiveFormat, D::Error> {
    match String::deserialize(deserializer)?.as_str() {
        "tar.gz" => Ok(ArchiveFormat::TarGz),
        "tar.xz" => Ok(ArchiveFormat::TarXz),
        "zip" => Ok(ArchiveFormat::Zip),
        other => Err(serde::de::Error::custom(format!(
            "unsupported source archive `{other}`"
        ))),
    }
}

impl SourceBuild {
    pub(super) fn validate(&self) -> Result<(), String> {
        if !self.url.starts_with("https://")
            || self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err("source builds require an HTTPS URL and lowercase SHA-256".into());
        }
        if self.strip_prefix.as_os_str().is_empty()
            || self
                .strip_prefix
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err("source strip_prefix must remain within the archive".into());
        }
        if self
            .configure
            .iter()
            .any(|arg| arg.starts_with("--prefix") || !arg.starts_with("--"))
        {
            return Err("configure options must be flags; the builder owns --prefix".into());
        }
        let mut dependencies = BTreeSet::new();
        for dependency in &self.dependencies {
            let request = PackageRequest::parse(dependency);
            if request.resolver.is_some()
                || request.version.is_none()
                || !dependencies.insert(dependency)
            {
                return Err("build dependencies require canonical name@version requests".into());
            }
        }
        Ok(())
    }
}

/// The build inputs, dependency outputs, host toolchain, and resulting artifact.
#[derive(Debug, Serialize, Deserialize)]
pub struct BuildArtifact {
    pub schema: u32,
    pub catalog_sha256: String,
    pub revision: u32,
    pub system: String,
    pub build: SourceBuild,
    pub dependencies: BTreeMap<String, LockedPackage>,
    pub resolver_inputs: PackageResolverInputs,
    pub toolchain: BTreeMap<String, String>,
    pub package: LockedPackage,
}

fn find_recipe<'a>(
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
    let version = request.version.as_ref().unwrap_or(&package.default_version);
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
    let key = format!("{}@{version}", package.name);
    if done.contains(&key) {
        return Ok(());
    }
    if !active.insert(key.clone()) {
        return Err(format!("build dependency cycle at `{key}`"));
    }
    if let Some(build) = &recipe.build {
        for dependency in &build.dependencies {
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

pub(super) fn validate_dependencies(catalog: &PackageCatalog) -> Result<(), String> {
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

/// Builds a catalog source recipe and its build dependencies into a new directory.
/// This executes trusted recipe code with the host compiler, not an OS sandbox.
pub fn build_package(
    catalog: &PackageCatalog,
    request: &str,
    output: &Path,
    jobs: usize,
) -> Result<BuildArtifact, String> {
    catalog.validate()?;
    if jobs == 0 || jobs > 64 {
        return Err("jobs must be between 1 and 64".into());
    }
    let (_, _, root_recipe) = find_recipe(catalog, request)?;
    if root_recipe.build.is_none() {
        return Err("this package uses an upstream binary, not a source build".into());
    }
    let mut order = Vec::new();
    visit(
        catalog,
        request,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut order,
    )?;
    let system = ResolveContext::current();
    for key in &order {
        let (_, _, recipe) = find_recipe(catalog, key)?;
        if !recipe.systems.contains(&system.system) {
            return Err(format!("{key} has no recipe for {}", system.system));
        }
    }
    fs::create_dir(output)
        .map_err(|e| format!("cannot create build output {}: {e}", output.display()))?;
    let output = output.canonicalize().map_err(|e| e.to_string())?;
    let workspace = tempfile::tempdir_in(&output).map_err(|e| e.to_string())?;
    let realizer = PackageRealizer::with_dirs(
        Store::new(workspace.path().join("store")),
        crate::state_dir().join("downloads"),
        workspace.path().join("install"),
    );
    let downloads = DownloadCache::new(crate::state_dir().join("downloads"));
    let inputs = if order
        .iter()
        .any(|key| find_recipe(catalog, key).is_ok_and(|(_, _, recipe)| recipe.source.is_some()))
    {
        PackageResolverInputs::resolve_current().map_err(|e| e.to_string())?
    } else {
        PackageResolverInputs::default()
    };
    let backends = super::resolver_stack_for_inputs(&inputs);
    let mut resolved = BTreeMap::<String, LockedPackage>::new();
    let mut dependency_bins = BTreeMap::new();
    let mut root_artifact = None;
    for (index, key) in order.iter().enumerate() {
        let (_, _, recipe) = find_recipe(catalog, key)?;
        let is_root = index + 1 == order.len();
        let mut built = None;
        let destination = if is_root {
            output.clone()
        } else {
            output.join(format!("dependency-{key}"))
        };
        let locked = if let Some(build) = &recipe.build {
            if !is_root {
                fs::create_dir(&destination).map_err(|e| e.to_string())?;
            }
            let dependencies = build
                .dependencies
                .iter()
                .map(|request| {
                    let (package, version, _) = find_recipe(catalog, request)?;
                    let key = format!("{}@{version}", package.name);
                    Ok((key.clone(), resolved[&key].clone()))
                })
                .collect::<Result<BTreeMap<_, _>, String>>()?;
            let tools = workspace.path().join(format!("tools-{index}"));
            fs::create_dir(&tools).map_err(|e| e.to_string())?;
            for key in dependencies.keys() {
                let bins: &BTreeMap<String, PathBuf> = &dependency_bins[key];
                for (name, path) in bins {
                    symlink(path, tools.join(name)).map_err(|e| {
                        format!("build dependency command collision for {name}: {e}")
                    })?;
                }
            }
            let mut artifact = compile(
                catalog,
                key,
                dependencies,
                &tools,
                &destination,
                &downloads,
                jobs,
            )?;
            artifact.resolver_inputs = inputs.clone();
            let locked = artifact.package.clone();
            built = Some(artifact);
            locked
        } else {
            backends
                .resolve(&PackageRequest::parse(key), &system)
                .map_err(|e| e.to_string())?
                .package
        };
        let realized = realizer.realize(&locked).map_err(|e| e.to_string())?;
        let environment = BTreeMap::from([
            ("PATH", "/usr/bin:/bin".into()),
            ("HOME", workspace.path().to_string_lossy().into_owned()),
            ("LC_ALL", "C".into()),
        ]);
        for check in &recipe.checks {
            let mut args = check.clone();
            args[0] = realized.bins[&check[0]].to_string_lossy().into_owned();
            run(
                &args,
                workspace.path(),
                &environment,
                &output.join("checks.log"),
                Duration::from_secs(30),
            )?;
        }
        if let Some(artifact) = built {
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
        resolved.insert(key.clone(), locked);
    }
    root_artifact.ok_or_else(|| "source build produced no artifact".into())
}

fn compile(
    catalog: &PackageCatalog,
    request: &str,
    dependencies: BTreeMap<String, LockedPackage>,
    tools: &Path,
    output: &Path,
    downloads: &DownloadCache,
    jobs: usize,
) -> Result<BuildArtifact, String> {
    let (package, version, recipe) = find_recipe(catalog, request)?;
    let build = recipe.build.as_ref().ok_or("recipe has no source build")?;
    let archive = downloads
        .materialize_verified(&build.url, &build.sha256)
        .map_err(|e| e.to_string())?;
    let workspace = tempfile::tempdir_in(output).map_err(|e| e.to_string())?;
    let workspace_path = workspace.path().canonicalize().map_err(|e| e.to_string())?;
    super::realize::extract_archive(&archive, build.archive, workspace.path())
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
    let mut environment = BTreeMap::from([
        (
            "PATH",
            format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", tools.display()),
        ),
        ("HOME", workspace.path().to_string_lossy().into_owned()),
        ("TMPDIR", workspace.path().to_string_lossy().into_owned()),
        ("LC_ALL", "C".into()),
        ("TZ", "UTC".into()),
        ("SOURCE_DATE_EPOCH", "1".into()),
        ("CONFIG_SHELL", "/bin/sh".into()),
    ]);
    environment.insert("CC", "/usr/bin/cc".into());
    let mut toolchain = BTreeMap::new();
    for (name, args) in [
        ("/usr/bin/cc", vec!["--version"]),
        ("make", vec!["--version"]),
        ("/usr/bin/uname", vec!["-a"]),
    ] {
        let result = Command::new(name)
            .args(args)
            .env_clear()
            .envs(&environment)
            .output()
            .map_err(|e| e.to_string())?;
        if !result.status.success() {
            return Err(format!("cannot inspect build tool {name}"));
        }
        toolchain.insert(
            name.into(),
            String::from_utf8_lossy(&result.stdout).trim().into(),
        );
    }
    let log = output.join("build.log");
    match build.backend {
        BuildBackend::Autotools => {
            let mut configure = vec!["/bin/sh".into(), "./configure".into(), "--prefix=/".into()];
            configure.extend(build.configure.clone());
            run(
                &configure,
                &source,
                &environment,
                &log,
                Duration::from_secs(1200),
            )?;
            run(
                &["make".into(), format!("-j{jobs}")],
                &source,
                &environment,
                &log,
                Duration::from_secs(1200),
            )?;
            run(
                &["make".into(), "check".into()],
                &source,
                &environment,
                &log,
                Duration::from_secs(1200),
            )?;
            run(
                &[
                    "make".into(),
                    format!("DESTDIR={}", prefix.display()),
                    "install".into(),
                ],
                &source,
                &environment,
                &log,
                Duration::from_secs(1200),
            )?;
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
    let artifact_path = output.join("package.tar.gz");
    pack(&prefix, &artifact_path).map_err(|e| e.to_string())?;
    let artifact = BuildArtifact {
        schema: 1,
        catalog_sha256: catalog.sha256(),
        revision: recipe.revision,
        system: ResolveContext::current().system,
        build: build.clone(),
        dependencies,
        resolver_inputs: PackageResolverInputs::default(),
        toolchain,
        package: LockedPackage {
            name: package.name.clone(),
            version: version.into(),
            source: LockedSource::File {
                path: artifact_path.clone(),
                sha256: hash_file(&artifact_path).map_err(|e| e.to_string())?,
            },
            install: LockedInstall::Archive {
                format: ArchiveFormat::TarGz,
                strip_prefix: None,
            },
            provides: Provides { bins: bins.clone() },
            output_sha256: Some(hash_tree(&prefix).map_err(|e| e.to_string())?),
        },
    };
    let LockedSource::File { sha256, .. } = &artifact.package.source else {
        unreachable!()
    };
    let spec = serde_json::json!({"name": package.name, "version": version,
        "source": {"file": "package.tar.gz", "sha256": sha256}, "install": {"archive": "tar.gz"}, "bins": bins});
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
    Ok(artifact)
}

fn run(
    args: &[String],
    source: &Path,
    environment: &BTreeMap<&str, String>,
    log: &Path,
    timeout: Duration,
) -> Result<(), String> {
    eprintln!("building: {}", args.join(" "));
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| e.to_string())?;
    let mut child = Command::new(&args[0])
        .args(&args[1..])
        .current_dir(source)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::null())
        .stdout(file.try_clone().map_err(|e| e.to_string())?)
        .stderr(file)
        .process_group(0)
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if status.success() {
                return Ok(());
            }
            return Err(format!(
                "build command {} failed ({status}); see {}",
                args[0],
                log.display()
            ));
        }
        if start.elapsed() > timeout {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            return Err(format!(
                "build step exceeded its time limit; see {}",
                log.display()
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn pack(root: &Path, output: &Path) -> io::Result<()> {
    fn paths(root: &Path, directory: &Path, entries: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            entries.push(entry.path().strip_prefix(root).unwrap().to_path_buf());
            if entry.file_type()?.is_dir() {
                paths(root, &entry.path(), entries)?;
            }
        }
        Ok(())
    }
    let mut entries = Vec::new();
    paths(root, root, &mut entries)?;
    entries.sort();
    let mut archive = tar::Builder::new(GzEncoder::new(
        fs::File::create(output)?,
        Compression::default(),
    ));
    archive.mode(tar::HeaderMode::Deterministic);
    archive.follow_symlinks(false);
    for path in entries {
        archive.append_path_with_name(root.join(&path), &path)?;
    }
    archive.into_inner()?.finish()?.sync_all()
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn source_catalog() -> PackageCatalog {
        let catalog = PackageCatalog::embedded().unwrap();
        PackageCatalog {
            schema: 1,
            packages: BTreeMap::from([("xz".into(), catalog.packages["xz"].clone())]),
        }
    }

    #[test]
    fn build_graph_orders_dependencies_once_and_rejects_cycles() {
        let mut catalog = source_catalog();
        let mut dependency = catalog.packages["xz"].clone();
        dependency.name = "build-tool".into();
        catalog.packages.insert("build-tool".into(), dependency);
        catalog
            .packages
            .get_mut("xz")
            .unwrap()
            .versions
            .get_mut("5.8.3")
            .unwrap()
            .build
            .as_mut()
            .unwrap()
            .dependencies = vec!["build-tool@5.8.3".into()];
        catalog.validate().unwrap();
        let mut order = Vec::new();
        visit(
            &catalog,
            "xz",
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
            &mut order,
        )
        .unwrap();
        assert_eq!(order, ["build-tool@5.8.3", "xz@5.8.3"]);
        catalog
            .packages
            .get_mut("build-tool")
            .unwrap()
            .versions
            .get_mut("5.8.3")
            .unwrap()
            .build
            .as_mut()
            .unwrap()
            .dependencies = vec!["xz@5.8.3".into()];
        assert!(catalog.validate().unwrap_err().contains("cycle"));
    }

    #[test]
    fn invalid_build_inputs_fail_before_creating_output() {
        let mut catalog = source_catalog();
        let build = catalog
            .packages
            .get_mut("xz")
            .unwrap()
            .versions
            .get_mut("5.8.3")
            .unwrap()
            .build
            .as_mut()
            .unwrap();
        build.strip_prefix = "../outside".into();
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("output");
        assert!(build_package(&catalog, "xz", &output, 2)
            .unwrap_err()
            .contains("strip_prefix"));
        assert!(!output.exists());
        let mut catalog = source_catalog();
        catalog
            .packages
            .get_mut("xz")
            .unwrap()
            .versions
            .get_mut("5.8.3")
            .unwrap()
            .build
            .as_mut()
            .unwrap()
            .dependencies = vec!["missing@1".into()];
        assert!(catalog
            .validate()
            .unwrap_err()
            .contains("canonical package name"));
        assert!(build_package(&source_catalog(), "xz", &output, 0).is_err());
        assert!(!output.exists());
    }

    #[test]
    fn source_packages_do_not_implicitly_compile_during_resolution() {
        let error = super::super::default_resolver_stack()
            .resolve(&PackageRequest::parse("xz"), &ResolveContext::current())
            .unwrap_err();
        assert!(error.to_string().contains("rb package build"));
    }

    #[test]
    fn archive_roundtrip_preserves_contents_and_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("prefix");
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join("bin/tool"), "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(root.join("bin/tool"), fs::Permissions::from_mode(0o755)).unwrap();
        symlink("tool", root.join("bin/alias")).unwrap();
        let first = directory.path().join("one.tar.gz");
        let second = directory.path().join("two.tar.gz");
        pack(&root, &first).unwrap();
        pack(&root, &second).unwrap();
        assert_eq!(hash_file(&first).unwrap(), hash_file(&second).unwrap());
        let extracted = directory.path().join("extracted");
        fs::create_dir(&extracted).unwrap();
        super::super::realize::extract_archive(&first, ArchiveFormat::TarGz, &extracted).unwrap();
        assert_eq!(hash_tree(&root).unwrap(), hash_tree(&extracted).unwrap());
    }

    #[test]
    fn steps_use_only_the_declared_environment_and_fail_on_errors_or_timeouts() {
        let directory = tempfile::tempdir().unwrap();
        let log = directory.path().join("env.log");
        let environment = BTreeMap::from([("ROOTBEER_BUILD_TEST", "set".into())]);
        run(
            &["/usr/bin/env".into()],
            directory.path(),
            &environment,
            &log,
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(&log).unwrap().trim(),
            "ROOTBEER_BUILD_TEST=set"
        );
        assert!(run(
            &["/bin/sh".into(), "-c".into(), "exit 7".into()],
            directory.path(),
            &environment,
            &log,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .contains("failed"));
        assert!(run(
            &["/bin/sleep".into(), "10".into()],
            directory.path(),
            &environment,
            &log,
            Duration::from_millis(10)
        )
        .unwrap_err()
        .contains("time limit"));
    }

    #[test]
    fn compiles_cached_sources_and_produces_an_installable_artifact() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        fs::create_dir_all(source.join("fixture")).unwrap();
        fs::write(
            source.join("fixture/configure"),
            r#"#!/bin/sh
set -eu
fixture-tool
cat > Makefile <<'EOF'
all:
	printf '#!/bin/sh\nexit 0\n' > fixture
	chmod +x fixture
check: all
	./fixture
install:
	mkdir -p "$(DESTDIR)/bin"
	cp fixture "$(DESTDIR)/bin/fixture"
EOF
"#,
        )
        .unwrap();
        let archive = directory.path().join("source.tar.gz");
        pack(&source, &archive).unwrap();
        let downloads = DownloadCache::new(directory.path().join("downloads"));
        let cached = downloads
            .materialize(&format!("file://{}", archive.display()), None)
            .unwrap();
        let mut catalog = source_catalog();
        let package = catalog.packages.remove("xz").unwrap();
        let mut package = CatalogPackage {
            name: "fixture".into(),
            ..package
        };
        let recipe = package.versions.get_mut("5.8.3").unwrap();
        recipe.bins = vec!["fixture".into()];
        recipe.checks = vec![vec!["fixture".into()]];
        let build = recipe.build.as_mut().unwrap();
        build.url = "https://source.invalid/fixture.tar.gz".into();
        build.sha256 = cached.sha256;
        build.strip_prefix = "fixture".into();
        build.configure.clear();
        catalog.packages.insert("fixture".into(), package);
        catalog.validate().unwrap();
        let tools = directory.path().join("tools");
        fs::create_dir(&tools).unwrap();
        fs::write(tools.join("fixture-tool"), "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(
            tools.join("fixture-tool"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        let output = directory.path().join("output");
        fs::create_dir(&output).unwrap();
        let offline = DownloadCache::offline(directory.path().join("downloads"));
        let artifact = compile(
            &catalog,
            "fixture",
            BTreeMap::new(),
            &tools,
            &output,
            &offline,
            1,
        )
        .unwrap();
        let realizer = PackageRealizer::with_dirs(
            Store::new(directory.path().join("store")),
            directory.path().join("downloads"),
            directory.path().join("tmp"),
        );
        let realized = realizer.realize(&artifact.package).unwrap();
        assert!(Command::new(&realized.bins["fixture"])
            .status()
            .unwrap()
            .success());
        assert!(output.join("install.lua").is_file());
        assert!(artifact.toolchain.contains_key("/usr/bin/cc"));
    }
}
