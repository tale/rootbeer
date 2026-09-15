use super::*;
use std::fs;
use std::path::PathBuf;

fn catalog(system: &str) -> PackageCatalog {
    let mut catalog = PackageCatalog {
        schema: 1,
        packages: BTreeMap::new(),
    };
    for name in ["root", "tool", "compiler", "runtime"] {
        let mut package = crate::test_catalog::catalog().packages["xz"].clone();
        package.name = name.into();
        package.aliases.clear();
        package.default_version = "1".into();
        let mut recipe = package.versions.remove("5.8.3").unwrap();
        recipe.systems = vec![system.into()];
        recipe.bins = vec![name.into()];
        recipe.checks = vec![vec![name.into()]];
        if matches!(name, "tool" | "runtime") {
            recipe.source = Some(format!("github:fixture/{name}@1"));
            recipe.assets.insert(system.into(), name.into());
        }
        let build = recipe.build.as_mut().unwrap();
        build.dependencies = match name {
            "root" => vec!["tool@1".into()],
            "tool" => vec![
                "compiler@1".into(),
                BuildDependency::Scoped {
                    package: "runtime@1".into(),
                    kind: DependencyKind::Runtime,
                },
            ],
            _ => vec![],
        };
        if name == "runtime" {
            recipe.build = None;
        }
        package.versions = BTreeMap::from([("1".into(), recipe)]);
        catalog.packages.insert(name.into(), package);
    }
    catalog
}

struct LocalBinaries(PathBuf);

impl PackageResolver for LocalBinaries {
    fn name(&self) -> &str {
        "github"
    }

    fn resolve(
        &self,
        request: &PackageRequest,
        _: &ResolveContext,
    ) -> Result<Option<PackageResolution>, String> {
        let name = request.name.strip_prefix("fixture/").unwrap();
        assert!(matches!(name, "tool" | "runtime"));
        Ok(Some(PackageResolution::new(
            LockedPackage {
                name: name.into(),
                version: "1".into(),
                source: LockedSource::File {
                    path: self.0.clone(),
                    sha256: rootbeer_store::hash_file(&self.0).unwrap(),
                },
                install: LockedInstall::Binary { path: name.into() },
                provides: Provides {
                    bins: BTreeMap::from([(name.into(), name.into())]),
                    apps: BTreeMap::new(),
                },
                runtime_dependencies: BTreeMap::new(),
                output_sha256: None,
            },
            ResolutionProof::GitRelease(GitReleaseProof {
                host: "github.com".into(),
                owner: "fixture".into(),
                repo: name.into(),
                tag: Some("1".into()),
                target_commit: None,
                release_id: None,
                documents: vec![],
            }),
        )))
    }
}

#[test]
fn prebuilt_dependencies_prune_source_inputs_and_preserve_runtime_inputs() {
    let root = tempfile::tempdir().unwrap();
    let system = ResolveContext::current().system;
    let mut catalog = catalog(&system);
    let source = root.path().join("source");
    fs::create_dir_all(source.join("fixture")).unwrap();
    fs::write(source.join("fixture/input"), "source").unwrap();
    let archive = root.path().join("source.tar.gz");
    crate::pack(&source, &archive).unwrap();
    let downloads = root.path().join("downloads");
    let cached = download::DownloadCache::new(&downloads)
        .materialize(&format!("file://{}", archive.display()), None)
        .unwrap();
    let build = catalog
        .packages
        .get_mut("root")
        .unwrap()
        .versions
        .get_mut("1")
        .unwrap()
        .build
        .as_mut()
        .unwrap();
    build.backend = BuildBackend::Custom;
    build.configure.clear();
    build.url = "https://source.invalid/archive.tar.gz".into();
    build.sha256 = cached.sha256;
    build.strip_prefix = "fixture".into();
    build.steps = Some(BuildSteps {
        configure: vec![], build: vec![vec!["tool".into()]], check: vec![vec!["sh".into(), "-c".into(), "exit 0".into()]],
        install: vec![vec!["sh".into(), "-c".into(), "mkdir -p \"$1/bin\"; printf '#!/bin/sh\\nexit 0\\n' > \"$1/bin/root\"; chmod +x \"$1/bin/root\"".into(), "install".into(), "{prefix}".into()]],
    });
    // This source-only compiler is unavailable on the host and must not enter the selected graph.
    catalog
        .packages
        .get_mut("compiler")
        .unwrap()
        .versions
        .get_mut("1")
        .unwrap()
        .systems = vec![if system == "x86_64-linux" {
        "aarch64-linux"
    } else {
        "x86_64-linux"
    }
    .into()];
    catalog.validate().unwrap();
    let graph = dependency_graph(&catalog, "root", &system).unwrap();
    assert_eq!(graph.order, ["runtime@1", "tool@1", "root@1"]);
    assert_eq!(graph.nodes["tool@1"].runtime_closure, ["runtime@1"]);
    assert!(!graph.nodes["root@1"].exports.contains_key("compiler@1"));
    let binary = root.path().join("binary");
    fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
    let mut backends = ResolverStack::new();
    backends.push(LocalBinaries(binary));
    let inputs = PackageResolverInputs {
        resolvers: BTreeMap::from([(
            "rootbeer".into(),
            ResolverInput::Catalog {
                sha256: catalog.sha256(),
            },
        )]),
    };
    let plan = BuildPlan::from_graph(&catalog, graph, &inputs, backends).unwrap();
    assert_eq!(
        plan.binaries.keys().map(String::as_str).collect::<Vec<_>>(),
        ["runtime@1", "tool@1"]
    );
    let artifact = plan
        .execute(
            &root.path().join("output"),
            &BuildOptions {
                downloads,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(artifact.dependencies["tool@1"]
        .runtime_dependencies
        .contains_key("runtime@1"));
    assert!(!root
        .path()
        .join("output/dependency-tool@1/build.log")
        .exists());
    assert!(!artifact.dependencies.contains_key("compiler@1"));
}

#[test]
fn source_roots_and_dependencies_without_matching_prebuilts_keep_source_edges() {
    let system = ResolveContext::current().system;
    let mut catalog = catalog(&system);
    let graph = dependency_graph(&catalog, "tool", &system).unwrap();
    assert_eq!(graph.order, ["compiler@1", "runtime@1", "tool@1"]);
    catalog
        .packages
        .get_mut("tool")
        .unwrap()
        .versions
        .get_mut("1")
        .unwrap()
        .assets
        .clear();
    let graph = dependency_graph(&catalog, "root", &system).unwrap();
    assert_eq!(graph.order, ["compiler@1", "runtime@1", "tool@1", "root@1"]);
    let inputs = PackageResolverInputs {
        resolvers: BTreeMap::from([(
            "rootbeer".into(),
            ResolverInput::Catalog {
                sha256: catalog.sha256(),
            },
        )]),
    };
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("binary");
    fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
    let mut backends = ResolverStack::new();
    backends.push(LocalBinaries(binary));
    let plan = BuildPlan::from_graph(&catalog, graph, &inputs, backends).unwrap();
    assert_eq!(
        plan.binaries.keys().map(String::as_str).collect::<Vec<_>>(),
        ["runtime@1"]
    );
}

#[test]
fn prebuilt_libraries_preserve_transitive_link_exports() {
    let system = ResolveContext::current().system;
    let mut catalog = catalog(&system);
    let build = catalog
        .packages
        .get_mut("tool")
        .unwrap()
        .versions
        .get_mut("1")
        .unwrap()
        .build
        .as_mut()
        .unwrap();
    build.libraries = vec!["lib/libtool.a".into()];
    build.dependencies[0] = BuildDependency::Scoped {
        package: "compiler@1".into(),
        kind: DependencyKind::Link,
    };
    catalog
        .packages
        .get_mut("compiler")
        .unwrap()
        .versions
        .get_mut("1")
        .unwrap()
        .build
        .as_mut()
        .unwrap()
        .libraries = vec!["lib/libbase.a".into()];
    let graph = dependency_graph(&catalog, "root", &system).unwrap();
    assert_eq!(graph.order, ["compiler@1", "runtime@1", "tool@1", "root@1"]);
    assert_eq!(
        graph.nodes["root@1"].exports["compiler@1"].libraries,
        vec![PathBuf::from("lib/libbase.a")]
    );
}
