use super::*;

#[test]
fn static_library_chain_builds_and_runs_after_dependencies_are_removed() {
    let directory = tempfile::Builder::new()
        .prefix("rootbeer libraries ")
        .tempdir()
        .unwrap();
    let root = directory.path().canonicalize().unwrap();
    let sources = root.join("sources");
    for name in ["base", "middle", "consumer", "rootbeer-test-generator"] {
        fs::create_dir_all(sources.join(name)).unwrap();
    }
    fs::write(sources.join("rootbeer-test-generator/Makefile"), "all:\n\tprintf '#!/bin/sh\\nexit 0\\n' > rootbeer-test-generator\n\tchmod +x rootbeer-test-generator\ncheck: all\n\t./rootbeer-test-generator\n").unwrap();
    let base = sources.join("base");
    fs::write(base.join("base.h"), "int base(void);\n").unwrap();
    fs::write(base.join("base.c"), "int base(void) { return 40; }\n").unwrap();
    fs::write(
        base.join("check.c"),
        "int base(void); int main(void) { return base() != 40; }\n",
    )
    .unwrap();
    fs::write(base.join("base.pc"), "prefix=/\nlibdir=${prefix}/lib\nincludedir=${prefix}/include\nName: base\nDescription: fixture library\nVersion: 1\nLibs: -L${libdir} -lbase\nCflags: -I${includedir}\n").unwrap();
    fs::write(base.join("Makefile"), "all: libbase.a\nlibbase.a: base.c\n\trootbeer-test-generator\n\t$(CC) -c base.c -o base.o\n\t$(AR) cr libbase.a base.o\ncheck: all\n\t$(CC) check.c libbase.a -o check\n\t./check\n").unwrap();
    let middle = sources.join("middle");
    fs::write(middle.join("middle.h"), "int middle(void);\n").unwrap();
    fs::write(
        middle.join("middle.c"),
        "#include <base.h>\nint middle(void) { return base() + 2; }\n",
    )
    .unwrap();
    fs::write(
        middle.join("check.c"),
        "int middle(void); int main(void) { return middle() != 42; }\n",
    )
    .unwrap();
    fs::write(middle.join("Makefile"), "all: libmiddle.a\nlibmiddle.a: middle.c\n\t$(CC) $(CPPFLAGS) -c middle.c -o middle.o\n\t$(AR) cr libmiddle.a middle.o\ncheck: all\n\t$(CC) $(LDFLAGS) check.c libmiddle.a -lbase -o check\n\t./check\n").unwrap();
    let consumer = sources.join("consumer");
    fs::write(consumer.join("main.c"), "#include <base.h>\n#include <middle.h>\nint main(void) { return middle() - base() != 2; }\n").unwrap();
    fs::write(consumer.join("Makefile"), "all: consumer\nconsumer: main.c\n\t! command -v rootbeer-test-generator\n\t$(CC) $(CPPFLAGS) $(LDFLAGS) main.c -lmiddle -lbase -o consumer\ncheck: all\n\t./consumer\n").unwrap();
    let archive = root.join("sources.tar.gz");
    pack(&sources, &archive).unwrap();
    let downloads = root.join("downloads");
    let cached = DownloadCache::new(&downloads)
        .materialize(&format!("file://{}", archive.display()), None)
        .unwrap();
    let mut catalog = PackageCatalog::embedded().unwrap().clone();
    let template = catalog.packages["xz"].clone();
    catalog.packages.clear();
    for (name, dependency) in [
        ("rootbeer-test-generator", None),
        ("base", Some("rootbeer-test-generator@1")),
        ("middle", Some("base@1")),
        ("consumer", Some("middle@1")),
    ] {
        let is_library = matches!(name, "base" | "middle");
        let mut package = template.clone();
        package.name = name.into();
        package.default_version = "1".into();
        package.default_versions.clear();
        let mut recipe = package.versions.values().next().unwrap().clone();
        let install = if is_library {
            format!("mkdir -p \"$1/lib\" \"$1/include\" \"$1/lib/pkgconfig\"; cp lib{name}.a \"$1/lib/\"; cp {name}.h \"$1/include/\"; if test -f {name}.pc; then cp {name}.pc \"$1/lib/pkgconfig/\"; fi")
        } else {
            format!("mkdir -p \"$1/bin\"; cp {name} \"$1/bin/\"")
        };
        recipe.bins = if is_library {
            vec![]
        } else {
            vec![name.into()]
        };
        recipe.checks = if is_library {
            vec![]
        } else {
            vec![vec![name.into()]]
        };
        recipe.build = Some(
            serde_json::from_value(serde_json::json!({
                "backend": "commands", "url": "https://source.invalid/libraries.tar.gz",
                "sha256": cached.sha256, "archive": "tar.gz", "strip_prefix": name,
                "dependencies": dependency.into_iter().map(|package| serde_json::json!({"package": package, "kind": if name == "base" { "build" } else { "link" }})).collect::<Vec<_>>(),
                "libraries": if is_library { vec![format!("lib/lib{name}.a")] } else { vec![] },
                "steps": {
                    "configure": [], "build": [["make", "-j{jobs}"]], "check": [["make", "check"]],
                    "install": [["/bin/sh", "-ec", install, "install", "{prefix}"]]
                }
            }))
            .unwrap(),
        );
        package.versions = BTreeMap::from([("1".into(), recipe)]);
        catalog.packages.insert(name.into(), package);
    }
    catalog.validate().unwrap();
    let inputs = PackageResolverInputs {
        resolvers: BTreeMap::from([(
            "rootbeer".into(),
            ResolverInput::Catalog {
                sha256: catalog.sha256(),
            },
        )]),
    };
    let output = root.join("output");
    let artifact = BuildPlan::resolve(&catalog, "consumer", &inputs)
        .unwrap()
        .execute(
            &output,
            &BuildOptions {
                downloads: downloads.clone(),
                ..BuildOptions::default()
            },
        )
        .unwrap();
    assert_eq!(artifact.dependencies.len(), 3);
    assert_eq!(ArtifactIndex::schema_for(&catalog), 5);
    let mut index = ArtifactIndex {
        schema: 4,
        catalog_sha256: catalog.sha256(),
        catalog: catalog.clone(),
        artifacts: BTreeMap::new(),
    };
    assert!(index.validate_fragment().unwrap_err().contains("schema 5"));
    index.schema = 5;
    index.validate_fragment().unwrap();
    let relocated = root.join("relocated");
    let realizer =
        PackageRealizer::with_dirs(Store::new(&relocated), &downloads, root.join("install"));
    let installed = realizer.realize(&artifact.package).unwrap();
    fs::remove_dir_all(&output).unwrap();
    assert!(Command::new(&installed.bins["consumer"])
        .env_clear()
        .status()
        .unwrap()
        .success());
}

#[test]
fn library_exports_reject_collisions_escapes_and_thin_archives() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let package = root.join("package");
    fs::create_dir_all(package.join("lib")).unwrap();
    let archive = package.join("lib/libtest.a");
    let libraries = vec![PathBuf::from("lib/libtest.a")];
    fs::write(&archive, "!<thin>\n").unwrap();
    assert!(dependencies::validate(&package, &libraries).is_err());
    fs::write(&archive, "!<arch>\n").unwrap();
    dependencies::stage(&package, &libraries, &root.join("merged")).unwrap();
    let other = root.join("other");
    fs::create_dir_all(other.join("lib")).unwrap();
    fs::write(other.join("lib/libtest.a"), "!<arch>\n").unwrap();
    assert!(
        dependencies::stage(&other, &libraries, &root.join("merged"))
            .unwrap_err()
            .contains("collision")
    );
    fs::remove_file(&archive).unwrap();
    symlink(other.join("lib/libtest.a"), &archive).unwrap();
    assert!(dependencies::validate(&package, &libraries)
        .unwrap_err()
        .contains("escapes"));
}

#[test]
fn compact_library_recipes_require_checks_and_new_index_schema() {
    let source = r#"return {
        name = "library", description = "A library", homepage = "https://example.org",
        default_version = "1", systems = { "aarch64-macos" }, bins = {}, checks = {},
        build = {
            backend = "commands", url = "https://example.org/library-{version}.tar.gz",
            archive = "tar.gz", strip_prefix = "library-{version}", libraries = { "lib/libtest.a" },
            steps = { configure = {}, build = {{ "make" }}, check = {{ "make", "test" }},
                      install = {{ "make", "DESTDIR={prefix}", "install" }} },
        },
        versions = { ["1"] = { sha256 = "0000000000000000000000000000000000000000000000000000000000000000" } },
    }"#;
    let directory = tempfile::tempdir().unwrap();
    let load = |source: &str| {
        fs::write(directory.path().join("library.lua"), source).unwrap();
        PackageCatalog::from_directory(directory.path())
    };
    let catalog = load(source).unwrap();
    assert_eq!(ArtifactIndex::schema_for(&catalog), 4);
    let build = catalog.packages["library"].versions["1"]
        .build
        .as_ref()
        .unwrap();
    assert_eq!(build.url, "https://example.org/library-1.tar.gz");
    assert_eq!(
        build.steps.as_ref().unwrap().install[0][1],
        "DESTDIR={prefix}"
    );
    let index = ArtifactIndex {
        schema: 4,
        catalog_sha256: catalog.sha256(),
        catalog,
        artifacts: BTreeMap::new(),
    };
    let mut decoded: ArtifactIndex =
        serde_json::from_slice(&serde_json::to_vec(&index).unwrap()).unwrap();
    decoded.validate_fragment().unwrap();
    for schema in 1..4 {
        decoded.schema = schema;
        assert!(decoded
            .validate_fragment()
            .unwrap_err()
            .contains("schema 4"));
    }
    for invalid in [
        source.replace("check = {{ \"make\", \"test\" }}", "check = {}"),
        source.replace("backend = \"commands\"", "backend = \"autotools\""),
        source.replace("lib/libtest.a", "../libtest.a"),
        source.replace("lib/libtest.a", "lib/libtest.so"),
        source.replace("libraries = { \"lib/libtest.a\" }", "libraries = {}"),
    ] {
        assert!(load(&invalid).is_err());
    }
}
