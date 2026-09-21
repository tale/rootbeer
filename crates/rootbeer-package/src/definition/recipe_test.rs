use super::PackageDefinition;

const BINARY: &str = r#"return {
    schema = 2, name = "tool", description = "Tool", homepage = "https://example.com",
    default_version = "2", default_versions = { ["x86_64-linux"] = "1" },
    systems = { "aarch64-macos", "x86_64-linux" },
    upstream = { github = "owner/tool", repository_id = 42, tag_prefix = "tool-" },
    inputs = { prebuilt = {
        github = "owner/tool", tag = "tool-{version}",
        assets = {
            ["aarch64-macos"] = "tool-{tag}-arm64.tar.gz",
            ["x86_64-linux"] = "tool-{version}-linux.tar.gz",
        },
    } },
    outputs = { bins = { "tool" }, checks = { { "tool", "--version" } } },
    versions = {
        ["1"] = { revision = 2, inputs = { prebuilt = {
            tag = "legacy-1", assets = { ["x86_64-linux"] = "old.zip" },
        } } },
        ["2"] = { systems = { "aarch64-macos" } },
    },
}"#;

fn source() -> String {
    format!(
        r#"return {{
        schema = 2, name = "tool", description = "Tool", homepage = "https://example.com",
        default_version = "1", systems = {{ "aarch64-macos" }},
        upstream = {{ github = "owner/tool", repository_id = 42, tag_prefix = "v" }},
        inputs = {{ source = {{ url = "https://example.com/tool-{{version}}.tar.gz", archive = "tar.gz", strip_prefix = "tool-{{version}}" }} }},
        build = {{ backend = "autotools", configure = {{ "--disable-shared" }} }},
        outputs = {{ bins = {{ "tool" }}, checks = {{ {{ "tool", "--version" }} }}, libraries = {{ "lib/libtool.a" }} }},
        versions = {{ ["1"] = {{ inputs = {{ source = {{ sha256 = "{}" }} }} }} }},
    }}"#,
        "a".repeat(64)
    )
}

fn roundtrip(definition: &PackageDefinition) -> PackageDefinition {
    let loaded = PackageDefinition::from_lua(&definition.to_lua().unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&loaded.package).unwrap(),
        serde_json::to_value(&definition.package).unwrap()
    );
    loaded
}

#[test]
fn separates_discovery_from_prebuilt_resolution() {
    let mut definition = PackageDefinition::from_lua(BINARY).unwrap();
    let original = serde_json::to_value(&definition.package).unwrap();
    definition.upstream = None;
    let loaded = roundtrip(&definition);
    assert!(loaded.github_upstream().unwrap().is_none());
    assert_eq!(serde_json::to_value(&loaded.package).unwrap(), original);
    assert!(loaded
        .package
        .versions
        .values()
        .all(|recipe| recipe.build.is_none()));
}

#[test]
fn preserves_templates_and_retained_platform_exceptions() {
    let definition = PackageDefinition::from_lua(BINARY).unwrap();
    let old = &definition.package.versions["1"];
    assert_eq!(old.revision, 2);
    assert_eq!(old.source.as_deref(), Some("github:owner/tool@legacy-1"));
    assert_eq!(old.assets["x86_64-linux"], "old.zip");
    assert_eq!(definition.package.default_version_for("x86_64-linux"), "1");
    let loaded = roundtrip(&definition);
    assert!(loaded.to_lua().unwrap().contains("tool-{tag}-arm64.tar.gz"));
}

#[test]
fn discovery_uses_shared_outputs_without_inheriting_version_exceptions() {
    let text = BINARY.replace(
        "[\"2\"] = { systems",
        "[\"2\"] = { outputs = { checks = { { \"tool\", \"--help\" } } }, systems",
    );
    let definition = PackageDefinition::from_lua(&text).unwrap();
    let loaded = roundtrip(&definition);
    assert_eq!(loaded.package.versions["2"].checks[0], ["tool", "--help"]);
    assert_eq!(
        loaded.github_upstream().unwrap().unwrap().checks[0],
        ["tool", "--version"]
    );
}

#[test]
fn updates_render_changed_outputs_and_preserve_shared_contracts() {
    let mut definition = PackageDefinition::from_lua(BINARY).unwrap();
    definition.package.versions.get_mut("2").unwrap().checks =
        vec![vec!["tool".into(), "--help".into()]];
    let loaded = roundtrip(&definition);
    assert_eq!(
        loaded.github_upstream().unwrap().unwrap().checks[0],
        ["tool", "--version"]
    );
}

#[test]
fn source_hashes_and_build_overrides_are_independent() {
    let definition = PackageDefinition::from_lua(&source()).unwrap();
    let mut definition = roundtrip(&definition);
    let recipe = definition.package.versions.get_mut("1").unwrap();
    recipe.build.as_mut().unwrap().sha256 = "b".repeat(64);
    recipe
        .build
        .as_mut()
        .unwrap()
        .configure
        .push("--disable-threads".into());
    let loaded = roundtrip(&definition);
    assert_eq!(
        loaded.package.versions["1"].build.as_ref().unwrap().sha256,
        "b".repeat(64)
    );
    assert_eq!(
        loaded
            .github_upstream()
            .unwrap()
            .unwrap()
            .build
            .unwrap()
            .configure,
        ["--disable-shared"]
    );
}

#[test]
fn source_discovery_supports_library_only_outputs_and_tag_templates() {
    let text = source()
        .replace(
            "bins = { \"tool\" }, checks = { { \"tool\", \"--version\" } }",
            "bins = {}, checks = {}",
        )
        .replace("tool-{version}", "tool-{tag}");
    let definition = PackageDefinition::from_lua(&text).unwrap();
    assert!(definition.package.versions["1"].bins.is_empty());
    let upstream = definition.github_upstream().unwrap().unwrap();
    assert!(upstream.assets.is_empty());
    assert!(upstream.build.unwrap().url.contains("{tag}"));
    let mut definition = roundtrip(&definition);
    definition
        .package
        .versions
        .get_mut("1")
        .unwrap()
        .build
        .as_mut()
        .unwrap()
        .sha256 = "b".repeat(64);
    roundtrip(&definition);
}

#[test]
fn source_discovery_preserves_shared_backend_and_patches() {
    let text = source().replace("backend = \"autotools\"", "backend = \"zig\", dependencies = { { package = \"zig@1\", kind = \"build\" } }, args = { \"-Doptimize=ReleaseFast\" }").replace(
        "configure = { \"--disable-shared\" }", "configure = {}",
    ).replace("archive = \"tar.gz\"", "patches = { \"--- a/file\\n+++ b/file\\n@@ -1 +1 @@\\n-old\\n+new\\n\" }, archive = \"tar.gz\"");
    let definition = PackageDefinition::from_lua(&text).unwrap();
    let loaded = roundtrip(&definition);
    let build = loaded.github_upstream().unwrap().unwrap().build.unwrap();
    assert_eq!(build.args, ["-Doptimize=ReleaseFast"]);
    assert!(build.patches[0].contains("\n-old\n+new\n"));
}

#[test]
fn prebuilt_apps_keep_pins_and_empty_output_overrides() {
    let text = BINARY.replace("systems = { \"aarch64-macos\", \"x86_64-linux\" }", "systems = { \"aarch64-macos\" }").replace("default_versions = { [\"x86_64-linux\"] = \"1\" },", "").replace(
        "bins = { \"tool\" }, checks", "bins = { \"tool\" }, bin_paths = { tool = \"Tool.app/Contents/MacOS/tool\" }, apps = { [\"Tool.app\"] = \"Tool.app\" }, checks",
    ).replace("assets = { [\"x86_64-linux\"] = \"old.zip\" }", "mirror = true, checksums = { [\"aarch64-macos\"] = \"HASH\" }").replace("HASH", &"a".repeat(64));
    let mut definition = PackageDefinition::from_lua(&text).unwrap();
    definition
        .package
        .versions
        .get_mut("1")
        .unwrap()
        .bin_paths
        .clear();
    definition
        .package
        .versions
        .get_mut("1")
        .unwrap()
        .apps
        .clear();
    let loaded = roundtrip(&definition);
    assert!(loaded.package.versions["1"].mirror);
    assert_eq!(loaded.package.versions["2"].apps.len(), 1);
    assert!(loaded.package.versions["1"].apps.is_empty());
}

#[test]
fn explicitly_disabled_mirroring_survives_rendering() {
    let text = BINARY.replace("github = \"owner/tool\", tag", "mirror = true, github = \"owner/tool\", tag")
        .replace("tag = \"legacy-1\"", "mirror = false, tag = \"legacy-1\"")
        .replace("[\"2\"] = { systems", &format!("[\"2\"] = {{ inputs = {{ prebuilt = {{ checksums = {{ [\"aarch64-macos\"] = \"{}\" }} }} }}, systems", "a".repeat(64)));
    let definition = PackageDefinition::from_lua(&text).unwrap();
    let loaded = roundtrip(&definition);
    assert!(!loaded.package.versions["1"].mirror);
    assert!(loaded.package.versions["2"].mirror);
}

#[test]
fn rejects_old_layouts_unknown_fields_and_misplaced_fields() {
    for text in [
        BINARY.replace("schema = 2", "schema = 1"),
        BINARY.replace("schema = 2,", ""),
        BINARY.replace("outputs =", "bins = {}, outputs ="),
        BINARY.replace("inputs = { prebuilt", "source = {}, inputs = { prebuilt"),
        BINARY.replace(
            "[\"2\"] = { systems",
            "[\"2\"] = { sha256 = \"bad\", systems",
        ),
        BINARY.replace(
            "[\"2\"] = { systems",
            "[\"2\"] = { build = { backend = \"custom\" }, systems",
        ),
        source().replace("backend = \"autotools\"", "backend = \"commands\""),
        source().replace(
            "backend = \"autotools\"",
            "backend = \"autotools\", url = \"https://example.com/wrong\"",
        ),
        source().replace("sha256 =", "checksum ="),
    ] {
        assert!(PackageDefinition::from_lua(&text).is_err(), "{text}");
    }
}

#[test]
fn rejects_unpinned_sources_and_invalid_asset_rules() {
    for text in [
        source().replace(&format!("sha256 = \"{}\"", "a".repeat(64)), ""),
        source().replace("url =", &format!("sha256 = \"{}\", url =", "a".repeat(64))),
        BINARY.replace("tool-{tag}-arm64", "tool-{typo}-arm64"),
        BINARY.replace("systems = { \"aarch64-macos\" }", "systems = {}"),
        BINARY.replace("revision = 2", "revision = 0"),
        BINARY.replace(
            "[\"x86_64-linux\"] = \"old.zip\"",
            "[\"aarch64-linux\"] = \"old.zip\"",
        ),
        BINARY.replace("tag = \"legacy-1\"", "mirror = true, tag = \"legacy-1\""),
    ] {
        assert!(PackageDefinition::from_lua(&text).is_err(), "{text}");
    }
}

#[test]
fn source_and_prebuilt_share_a_contract_and_roundtrip_git_inputs() {
    let text = source().replace("inputs = { source = {", "inputs = { prebuilt = { github = 'owner/tool', tag = 'v{version}', assets = { ['aarch64-macos'] = 'tool-{version}.tar.gz' } }, source = { git = { github = 'owner/tool', branch = 'main' },");
    let definition = PackageDefinition::from_lua(&text).unwrap();
    let recipe = &definition.package.versions["1"];
    assert!(recipe.has_prebuilt("aarch64-macos"));
    assert_eq!(
        recipe
            .build
            .as_ref()
            .unwrap()
            .git
            .as_ref()
            .unwrap()
            .branch
            .as_deref(),
        Some("main")
    );
    assert_eq!(recipe.source.as_deref(), Some("github:owner/tool@v1"));
    roundtrip(&definition);
    let mut updated = definition.clone();
    let recipe = updated.package.versions.get_mut("1").unwrap();
    recipe.source = None;
    recipe.assets.clear();
    roundtrip(&updated);
}

#[test]
fn optional_prebuilts_can_cover_only_some_source_platforms() {
    let text = source().replace("systems = { \"aarch64-macos\" }", "systems = { 'aarch64-macos', 'x86_64-linux' }")
        .replace("inputs = { source = {", "inputs = { prebuilt = { github = 'owner/tool', tag = 'v{version}', systems = { 'aarch64-macos' }, assets = { ['aarch64-macos'] = 'tool.tar.gz' } }, source = {");
    let definition = PackageDefinition::from_lua(&text).unwrap();
    let recipe = &definition.package.versions["1"];
    assert!(recipe.has_prebuilt("aarch64-macos"));
    assert!(!recipe.has_prebuilt("x86_64-linux"));
    assert!(recipe.build.is_some());
    roundtrip(&definition);
}

#[test]
fn rust_settings_roundtrip_and_reject_builder_overrides() {
    let text = source()
        .replace("backend = \"autotools\", configure = { \"--disable-shared\" }", "backend = \"rust\", rust = { packages = { \"tool-cli\" }, features = { \"tls\" }, environment = { APP_VERSION = \"1\" } }")
        .replace(", libraries = { \"lib/libtool.a\" }", "");
    let definition = roundtrip(&PackageDefinition::from_lua(&text).unwrap());
    let mut build = definition.package.versions["1"].build.clone().unwrap();
    assert_eq!(build.rust.as_ref().unwrap().packages, ["tool-cli"]);
    for name in ["PATH", "CARGO_HOME", "RUSTC", "LD_PRELOAD", "CC"] {
        build
            .rust
            .as_mut()
            .unwrap()
            .environment
            .insert(name.into(), "override".into());
        assert!(build.validate().unwrap_err().contains("environment"));
        build.rust.as_mut().unwrap().environment.remove(name);
    }
    build.rust.as_mut().unwrap().packages.clear();
    assert!(build.validate().is_err());
}

#[test]
fn go_settings_expand_versions_roundtrip_and_reject_escaping_entry_points() {
    let text = source()
        .replace("backend = \"autotools\", configure = { \"--disable-shared\" }", "backend = \"go\", go = { binaries = { tool = \"./cmd/tool\" }, tags = { \"netgo\" }, variables = { [\"main.version\"] = \"v{version}\" } }")
        .replace(", libraries = { \"lib/libtool.a\" }", "");
    let definition = roundtrip(&PackageDefinition::from_lua(&text).unwrap());
    assert_eq!(
        definition.package.versions["1"]
            .build
            .as_ref()
            .unwrap()
            .go
            .as_ref()
            .unwrap()
            .variables["main.version"],
        "v1"
    );
    assert_eq!(
        definition
            .github_upstream()
            .unwrap()
            .unwrap()
            .build
            .unwrap()
            .go
            .unwrap()
            .variables["main.version"],
        "v{version}"
    );
    for path in [
        "../tool",
        "./../tool",
        "./cmd/../../tool",
        "-o",
        "example.com/tool",
        "./cmd/tool@latest",
    ] {
        assert!(PackageDefinition::from_lua(&text.replace("./cmd/tool", path)).is_err());
    }
    assert!(PackageDefinition::from_lua(
        &text.replace("binaries = { tool =", "binaries = { other =")
    )
    .is_err());
    assert!(
        PackageDefinition::from_lua(&text.replace("backend = \"go\"", "backend = \"rust\""))
            .is_err()
    );
}

#[test]
fn platform_contracts_preserve_other_platforms_and_roundtrip() {
    let original = PackageDefinition::from_lua(BINARY).unwrap();
    let source = BINARY.replace(
        "[\"1\"] = { revision = 2,",
        r#"["1"] = { platforms = { ["aarch64-macos"] = {
            inputs = { prebuilt = { github = "owner/macos", tag = "v1", assets = { ["aarch64-macos"] = "App.dmg" } } },
            outputs = { bins = {}, checks = {}, apps = { ["App.app"] = "App.app" } },
        } }, revision = 2,"#,
    );
    let definition = PackageDefinition::from_lua(&source).unwrap();
    let recipe = &definition.package.versions["1"];
    recipe.validate().unwrap();
    let linux = recipe.for_system("x86_64-linux");
    assert_eq!(linux, original.package.versions["1"]);
    assert_eq!(linux.sha256(), original.package.versions["1"].sha256());
    let mac = recipe.for_system("aarch64-macos");
    assert_eq!(mac.source.as_deref(), Some("github:owner/macos@v1"));
    assert!(mac.bins.is_empty());
    assert!(mac.checks.is_empty());
    assert_eq!(mac.systems, ["aarch64-macos"]);
    assert_eq!(mac.apps["App.app"], std::path::PathBuf::from("App.app"));
    roundtrip(&definition);
    let mut changed = definition.clone();
    changed
        .package
        .versions
        .get_mut("1")
        .unwrap()
        .platforms
        .get_mut("aarch64-macos")
        .unwrap()
        .revision = 2;
    roundtrip(&changed);
    let mut invalid = recipe.clone();
    invalid
        .platforms
        .get_mut("aarch64-macos")
        .unwrap()
        .systems
        .push("x86_64-linux".into());
    assert!(invalid.validate().is_err());
}

#[test]
fn direct_downloads_require_pinned_safe_inputs_and_resolve_without_metadata() {
    use crate::{PackageCatalog, PackageRequest, PackageResolver, ResolveContext};
    let source = format!(
        r#"return {{
        schema = 2, name = "app", description = "Application", homepage = "https://example.com",
        default_version = "1", systems = {{ "aarch64-macos" }},
        inputs = {{ prebuilt = {{ url = "https://example.com/App-{{version}}.dmg", install = "Dmg" }} }},
        outputs = {{ apps = {{ ["App.app"] = "App.app" }} }},
        versions = {{ ["1"] = {{ inputs = {{ prebuilt = {{ checksums = {{ ["aarch64-macos"] = "{}" }} }} }} }} }},
    }}"#,
        "a".repeat(64)
    );
    let definition = PackageDefinition::from_lua(&source).unwrap();
    roundtrip(&definition);
    let catalog = PackageCatalog::from_definitions(&std::collections::BTreeMap::from([(
        "app".into(),
        definition,
    )]))
    .unwrap();
    let resolver = crate::catalog::CatalogResolver::new(
        &catalog,
        &Default::default(),
        crate::ResolverStack::new(),
    );
    let resolution = resolver
        .resolve(
            &PackageRequest::parse("app"),
            &ResolveContext::new("aarch64-macos"),
        )
        .unwrap()
        .unwrap();
    assert_eq!(resolution.package.install, crate::LockedInstall::Dmg);
    assert_eq!(
        resolution.package.source,
        crate::LockedSource::Url {
            url: "https://example.com/App-1.dmg".into(),
            sha256: "a".repeat(64)
        }
    );
    assert!(matches!(
        resolution.proof,
        crate::ResolutionProof::Download { .. }
    ));
    for invalid in [
        source.replace(&"a".repeat(64), "bad"),
        source.replace("https://example.com/App-", "http://example.com/App-"),
        source.replace("install = \"Dmg\"", "install = \"Directory\""),
    ] {
        assert!(PackageDefinition::from_lua(&invalid)
            .and_then(|definition| PackageCatalog::from_definitions(
                &std::collections::BTreeMap::from([("app".into(), definition)])
            ))
            .is_err());
    }
}
