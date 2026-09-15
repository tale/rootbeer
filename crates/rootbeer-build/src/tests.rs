use rootbeer_package::graph::DependencyGraph;

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
fn validates_backend_options_and_patch_inputs() {
    let catalog = source_catalog();
    let mut build = catalog.packages["xz"].versions["5.8.3"]
        .build
        .clone()
        .unwrap();
    build.args = vec!["-Doptimize=ReleaseFast".into()];
    assert!(build.validate().unwrap_err().contains("Autotools"));
    build.backend = BuildBackend::Zig;
    build.configure.clear();
    assert!(build.validate().is_ok());
    for argument in ["--prefix=/tmp", "--global-cache-dir=/tmp", "build", "-D"] {
        build.args = vec![argument.into()];
        assert!(build.validate().is_err());
    }
    build.args.clear();
    build.patches = vec![String::new()];
    assert!(build.validate().unwrap_err().contains("patches"));
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
    let order = DependencyGraph::new(&catalog, &["xz".into()], &ResolveContext::current().system)
        .unwrap()
        .order;
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
    assert!(
        build_package(&catalog, "xz", &output, &BuildOptions::default())
            .unwrap_err()
            .contains("strip_prefix")
    );
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
    assert!(build_package(
        &source_catalog(),
        "xz",
        &output,
        &BuildOptions {
            jobs: 0,
            ..BuildOptions::default()
        }
    )
    .is_err());
    assert!(!output.exists());
}

#[test]
fn pinned_build_rejects_missing_catalog_and_dependency_inputs_before_io() {
    let mut catalog = source_catalog();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("output");
    let mut inputs = PackageResolverInputs::default();
    assert!(BuildPlan::resolve(&catalog, "xz", &inputs)
        .and_then(|plan| plan.execute(&output, &BuildOptions::default()))
        .unwrap_err()
        .contains("pin the current catalog"));
    let mut dependency = catalog.packages["xz"].clone();
    dependency.name = "tool".into();
    let recipe = dependency.versions.get_mut("5.8.3").unwrap();
    recipe.build = None;
    recipe.source = Some("aqua:fixture/tool@5.8.3".into());
    catalog.packages.insert("tool".into(), dependency);
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
        .dependencies = vec!["tool@5.8.3".into()];
    inputs.resolvers.insert(
        "rootbeer".into(),
        rootbeer_package::ResolverInput::Catalog {
            sha256: catalog.sha256(),
        },
    );
    assert!(BuildPlan::resolve(&catalog, "xz", &inputs)
        .and_then(|plan| plan.execute(&output, &BuildOptions::default()))
        .unwrap_err()
        .contains("pinned Aqua registry"));
    assert!(!output.exists());
}

#[test]
fn source_packages_do_not_implicitly_compile_during_resolution() {
    let error = rootbeer_package::default_resolver_stack()
        .resolve(&PackageRequest::parse("xz"), &ResolveContext::current())
        .unwrap_err();
    assert!(error.to_string().contains("rootbeer-forge build"));
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
    rootbeer_package::realize::extract_archive(&first, ArchiveFormat::TarGz, &extracted).unwrap();
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
    build.patches = vec![r#"--- a/configure
+++ b/configure
@@ -1,4 +1,5 @@
 #!/bin/sh
 set -eu
 fixture-tool
+printf 'source-patch-applied\n'
 cat > Makefile <<'EOF'
"#
    .into()];
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
    let inputs = PackageResolverInputs {
        resolvers: BTreeMap::from([(
            "rootbeer".into(),
            ResolverInput::Catalog {
                sha256: catalog.sha256(),
            },
        )]),
    };
    let plan = BuildPlan::resolve(&catalog, "fixture", &inputs).unwrap();
    let opts = BuildOptions {
        downloads: directory.path().join("downloads"),
        jobs: 1,
        ..BuildOptions::default()
    };
    let artifact = compile(
        &plan,
        "fixture@5.8.3",
        BTreeMap::new(),
        &tools,
        &output,
        &opts,
    )
    .unwrap();
    write_install_files(&artifact, &output).unwrap();
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
    assert_eq!(artifact.build.patches.len(), 1);
    assert!(fs::read_to_string(output.join("build.log"))
        .unwrap()
        .contains("source-patch-applied\n"));

    fs::create_dir(source.join("tool")).unwrap();
    let configure = fs::read_to_string(source.join("fixture/configure"))
        .unwrap()
        .replace("fixture-tool\n", "")
        .replace("fixture", "fixture-tool");
    fs::write(source.join("tool/configure"), configure).unwrap();
    pack(&source, &archive).unwrap();
    let cached = downloads
        .materialize(&format!("file://{}", archive.display()), None)
        .unwrap();
    let mut dependency = catalog.packages["fixture"].clone();
    dependency.name = "tool".into();
    let recipe = dependency.versions.get_mut("5.8.3").unwrap();
    recipe.bins = vec!["fixture-tool".into()];
    recipe.checks = vec![vec!["fixture-tool".into()]];
    let build = recipe.build.as_mut().unwrap();
    build.sha256 = cached.sha256;
    build.strip_prefix = "tool".into();
    build.patches.clear();
    catalog.packages.insert("tool".into(), dependency);
    catalog
        .packages
        .get_mut("fixture")
        .unwrap()
        .versions
        .get_mut("5.8.3")
        .unwrap()
        .build
        .as_mut()
        .unwrap()
        .dependencies = vec!["tool@5.8.3".into()];
    let inputs = PackageResolverInputs {
        resolvers: BTreeMap::from([
            (
                "rootbeer".into(),
                rootbeer_package::ResolverInput::Catalog {
                    sha256: catalog.sha256(),
                },
            ),
            (
                "aqua".into(),
                rootbeer_package::ResolverInput::AquaRegistry(
                    rootbeer_package::GitHubRepositoryPin {
                        owner: "fixture".into(),
                        repo: "pinned-registry".into(),
                        rev: "a".repeat(40),
                    },
                ),
            ),
        ]),
    };
    let output = directory.path().join("build-with-inputs");
    let artifact = BuildPlan::resolve(&catalog, "fixture", &inputs)
        .unwrap()
        .execute(&output, &opts)
        .unwrap();
    assert_eq!(artifact.resolver_inputs, inputs);
    let dependency: BuildArtifact = serde_json::from_slice(
        &fs::read(output.join("dependency-tool@5.8.3/receipt.json")).unwrap(),
    )
    .unwrap();
    let receipt: BuildArtifact =
        serde_json::from_slice(&fs::read(output.join("receipt.json")).unwrap()).unwrap();
    assert_eq!(receipt.resolver_inputs, inputs);
    assert_eq!(dependency.resolver_inputs, inputs);
    assert_eq!(receipt.dependencies["tool@5.8.3"], dependency.package);

    let cache = BuildCache {
        directory: directory.path().join("build-cache"),
        context: "test-host-v1".into(),
        recheck: false,
    };
    let first = directory.path().join("cached-first");
    let second = directory.path().join("cached-second");
    let first_artifact = BuildPlan::resolve(&catalog, "fixture", &inputs)
        .unwrap()
        .execute(
            &first,
            &BuildOptions {
                cache: Some(cache.clone()),
                ..opts.clone()
            },
        )
        .unwrap();
    assert!(first.join("build.log").is_file());
    assert!(first.join("dependency-tool@5.8.3/build.log").is_file());
    let second_artifact = BuildPlan::resolve(&catalog, "fixture", &inputs)
        .unwrap()
        .execute(
            &second,
            &BuildOptions {
                cache: Some(cache.clone()),
                ..opts.clone()
            },
        )
        .unwrap();
    assert_eq!(
        first_artifact.package.output_sha256,
        second_artifact.package.output_sha256
    );
    assert!(!second.join("build.log").exists());
    assert!(!second.join("dependency-tool@5.8.3/build.log").exists());
    assert!(second.join("install.lua").is_file());
    assert!(second.join("receipt.json").is_file());
    let tool = directory.path().join("cached-tool");
    BuildPlan::resolve(&catalog, "tool", &inputs)
        .unwrap()
        .execute(
            &tool,
            &BuildOptions {
                cache: Some(cache.clone()),
                ..opts.clone()
            },
        )
        .unwrap();
    assert!(!tool.join("build.log").exists());

    let changed = BuildCache {
        context: "test-host-v2".into(),
        ..cache.clone()
    };
    let rebuilt = directory.path().join("new-environment");
    BuildPlan::resolve(&catalog, "tool", &inputs)
        .unwrap()
        .execute(
            &rebuilt,
            &BuildOptions {
                cache: Some(changed),
                ..opts.clone()
            },
        )
        .unwrap();
    assert!(rebuilt.join("build.log").exists());
    let refreshed = directory.path().join("recheck");
    BuildPlan::resolve(&catalog, "tool", &inputs)
        .unwrap()
        .execute(
            &refreshed,
            &BuildOptions {
                cache: Some(BuildCache {
                    recheck: true,
                    ..cache.clone()
                }),
                ..opts.clone()
            },
        )
        .unwrap();
    assert!(refreshed.join("build.log").exists());

    let mut failing_catalog = catalog.clone();
    failing_catalog
        .packages
        .get_mut("fixture")
        .unwrap()
        .versions
        .get_mut("5.8.3")
        .unwrap()
        .build
        .as_mut()
        .unwrap()
        .patches = vec!["invalid patch".into()];
    let failing_inputs = PackageResolverInputs {
        resolvers: BTreeMap::from([(
            "rootbeer".into(),
            ResolverInput::Catalog {
                sha256: failing_catalog.sha256(),
            },
        )]),
    };
    let entries = fs::read_dir(cache.directory.join("results"))
        .unwrap()
        .count();
    let failed = directory.path().join("failed-build");
    assert!(
        BuildPlan::resolve(&failing_catalog, "fixture", &failing_inputs)
            .unwrap()
            .execute(
                &failed,
                &BuildOptions {
                    cache: Some(cache.clone()),
                    ..opts.clone()
                }
            )
            .is_err()
    );
    assert_eq!(
        fs::read_dir(cache.directory.join("results"))
            .unwrap()
            .count(),
        entries
    );
    assert!(!failed.join("dependency-tool@5.8.3/build.log").exists());

    let plan = BuildPlan::resolve(&catalog, "tool", &inputs).unwrap();
    let frozen = serde_json::to_value(&plan).unwrap();
    catalog
        .packages
        .get_mut("tool")
        .unwrap()
        .description
        .push_str(" changed after planning");
    assert_eq!(serde_json::to_value(&plan).unwrap(), frozen);
    let snapshot = directory.path().join("frozen-plan");
    let snapshot_artifact = plan
        .execute(
            &snapshot,
            &BuildOptions {
                cache: Some(cache.clone()),
                ..opts.clone()
            },
        )
        .unwrap();
    assert_eq!(
        snapshot_artifact.catalog_sha256,
        inputs.catalog_sha256().unwrap()
    );
    assert_eq!(
        snapshot_artifact.build_environment.as_deref(),
        Some("test-host-v1")
    );
    assert!(snapshot_artifact.build_key.is_some());
    assert!(!snapshot.join("build.log").exists());
    let description = &mut catalog.packages.get_mut("tool").unwrap().description;
    description.truncate(description.len() - " changed after planning".len());
    let key = cache::key(
        "tool@5.8.3",
        &catalog.packages["tool"].versions["5.8.3"],
        &ResolveContext::current().system,
        &BTreeMap::new(),
        &cache.context,
        1,
        &BTreeMap::new(),
    )
    .unwrap();
    fs::write(
        cache
            .directory
            .join("results")
            .join(key)
            .join("package.tar.gz"),
        "corrupted",
    )
    .unwrap();
    let error = BuildPlan::resolve(&catalog, "tool", &inputs)
        .unwrap()
        .execute(
            &directory.path().join("corrupted-cache"),
            &BuildOptions {
                cache: Some(cache),
                ..opts
            },
        )
        .unwrap_err();
    assert!(error.contains("hash mismatch"));
}
