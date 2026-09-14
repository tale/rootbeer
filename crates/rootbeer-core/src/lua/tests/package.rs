use std::path::PathBuf;

use crate::package::{ArchiveFormat, LockedInstall, LockedSource, PackageIntent, PackageRequest};
use crate::plan::Op;

use super::super::test_support::{run, vm_in};

#[test]
fn catalog_commands_are_available_while_planning_without_a_lock() {
    let root = tempfile::tempdir().unwrap();
    let vm = vm_in(
        r#"
        local rb = require("rootbeer")
        rb.package("rg")
        result = rb.which("rg")
    "#,
        root.path(),
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(
        PathBuf::from(result),
        crate::package::profile::bin_path("rg")
    );
    assert!(!root.path().join("rootbeer.lock").exists());
}

#[test]
fn rb_package_pushes_realize_package_op() {
    let ops = run(r#"
        rb.package({
          name = "demo",
          version = "1.0.0",
          source = {
            file = "demo.tar.gz",
            sha256 = "abc123",
          },
          install = {
            archive = "tar.gz",
            strip_prefix = "demo",
          },
          apps = { ["Demo.app"] = "Demo.app" },
          bins = {
            demo = "bin/demo",
          },
        })
        "#);

    let [Op::Package { intent }] = ops.as_slice() else {
        panic!("expected one Package op, got {ops:?}");
    };
    let PackageIntent::Locked(package) = intent else {
        panic!("expected locked package intent, got {intent:?}");
    };

    assert_eq!(package.name, "demo");
    assert_eq!(package.version, "1.0.0");
    assert_eq!(
        package.provides.apps.get("Demo.app"),
        Some(&PathBuf::from("Demo.app"))
    );
    assert!(matches!(
        &package.source,
        LockedSource::File { path, sha256 }
            if path.ends_with("demo.tar.gz") && sha256 == "abc123"
    ));
    assert!(matches!(
        &package.install,
        LockedInstall::Archive {
            format: ArchiveFormat::TarGz,
            strip_prefix: Some(prefix),
        } if prefix == &PathBuf::from("demo")
    ));
    assert_eq!(
        package.provides.bins.get("demo"),
        Some(&PathBuf::from("bin/demo"))
    );
}

#[test]
fn rb_package_string_pushes_request_intent_without_resolving() {
    let ops = run(r#"
        rb.package("aqua:owner/tool@v1.0.0")
        "#);

    let [Op::Package { intent }] = ops.as_slice() else {
        panic!("expected one Package op, got {ops:?}");
    };

    assert_eq!(
        intent,
        &PackageIntent::Request(
            PackageRequest::new("owner/tool")
                .resolver("aqua")
                .version("v1.0.0")
        )
    );
}

#[test]
fn rb_which_returns_planned_package_bin_path() {
    let tmp = tempfile::tempdir().unwrap();
    let vm = vm_in(
        r#"
        rb.package({
          name = "demo",
          version = "1.0.0",
          source = { file = "demo.tar.gz", sha256 = "abc123" },
          install = { archive = "tar.gz", strip_prefix = "demo" },
          bins = { demo = "bin/demo" },
        })
        result = rb.which("demo")
        missing = rb.which("missing")
        "#,
        tmp.path(),
    );

    let result: Option<String> = vm.lua.globals().get("result").unwrap();
    let missing: Option<String> = vm.lua.globals().get("missing").unwrap();

    assert!(result
        .unwrap()
        .ends_with("profiles/default/current/bin/demo"));
    assert_eq!(missing, None);
}

#[test]
fn rb_env_export_writes_package_env_file_and_returns_path() {
    let tmp = tempfile::tempdir().unwrap();
    let vm = vm_in(
        r#"
        result = rb.env_export("zsh")
        "#,
        tmp.path(),
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    let ops = super::super::test_support::drain(vm);

    let [Op::WriteFile { path, source }] = ops.as_slice() else {
        panic!("expected one WriteFile op, got {ops:?}");
    };

    assert_eq!(result, path.to_string_lossy());
    assert!(path.ends_with("profiles/default/current/env.sh"));
    let content = source.as_str().expect("inline package environment");
    assert!(content.contains("_rootbeer_package_bin="));
    assert!(content.contains("export PATH=\"$_rootbeer_package_bin:$PATH\""));
}

#[test]
fn github_options_remain_declarative() {
    let ops = run(r#"
        rb.package("github:owner/tool@v1", {
            asset = "tool-darwin-arm64.tar.gz",
            bins = { tool = "release/bin/tool" },
        })
    "#);
    let [Op::Package {
        intent: PackageIntent::Request(request),
    }] = ops.as_slice()
    else {
        panic!("expected a package request");
    };
    assert_eq!(request.asset.as_deref(), Some("tool-darwin-arm64.tar.gz"));
    assert_eq!(request.bins["tool"], PathBuf::from("release/bin/tool"));
}

#[test]
fn accepts_raw_and_additional_archive_formats() {
    let ops = run(r#"
        for _, install in ipairs({ { binary = "bin/demo" }, { archive = "zip", directory = false }, { archive = "tar.xz" } }) do
            rb.package({ name = "demo", version = "1", source = { file = "asset", sha256 = "hash" },
                install = install, bins = { demo = "bin/demo" } })
        end
    "#);
    assert_eq!(ops.len(), 3);
}

#[test]
fn batches_and_structured_requests_match_individual_declarations() {
    let single = run(r#"
        rb.package("aqua:owner/first@v1")
        rb.package("github:owner/second@v2", { asset = "second.zip", bins = { second = "bin/second" } })
        rb.package({ name = "raw", version = "1", source = { file = "/raw.zip", sha256 = "hash" },
            install = { archive = "zip" }, bins = { raw = "bin/raw" } })
    "#);
    let batch = run(r#"
        rb.packages({
            "aqua:owner/first@v1",
            { request = "github:owner/second@v2", asset = "second.zip", bins = { second = "bin/second" } },
            { name = "raw", version = "1", source = { file = "/raw.zip", sha256 = "hash" },
                install = { archive = "zip" }, bins = { raw = "bin/raw" } },
        })
    "#);
    let intents = |ops: Vec<Op>| {
        ops.into_iter()
            .map(|op| match op {
                Op::Package { intent } => intent,
                _ => panic!("unexpected op"),
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(intents(single), intents(batch));
    let structured = intents(run(
        r#"rb.package({ request = "github:owner/second@v2", asset = "second.zip", bins = { second = "bin/second" } })"#,
    ));
    assert_eq!(
        structured,
        intents(run(
            r#"rb.package("github:owner/second@v2", { asset = "second.zip", bins = { second = "bin/second" } })"#
        ))
    );
}

#[test]
fn invalid_batches_leave_operations_and_planned_bins_untouched() {
    let root = tempfile::tempdir().unwrap();
    let vm = vm_in(
        r#"
        local first = { request = "github:owner/tool@v1", bins = { batch_atomic_test_command = "bin/tool" } }
        ok, failure = pcall(function() rb.packages({ first, { request = "github:owner/tool@v1", asset = 42 } }) end)
        failure = tostring(failure)
        leaked = rb.which("batch_atomic_test_command")
    "#,
        root.path(),
    );
    assert!(!vm.lua.globals().get::<bool>("ok").unwrap());
    assert!(vm
        .lua
        .globals()
        .get::<String>("failure")
        .unwrap()
        .contains("entry 2"));
    assert!(vm
        .lua
        .globals()
        .get::<Option<String>>("leaked")
        .unwrap()
        .is_none());
    assert!(super::super::test_support::drain(vm).is_empty());
}

#[test]
fn rejects_sparse_mixed_lists_and_ambiguous_or_mistyped_specs() {
    for invalid in [
        r#"rb.packages({ [1] = "rg", [3] = "jq" })"#,
        r#"rb.packages({ "rg", name = "jq" })"#,
        r#"rb.packages({ [0] = "rg" })"#,
        r#"rb.package({ request = "rg", version = "1" })"#,
        r#"rb.package({ request = "rg", name = "ambiguous" })"#,
        r#"rb.package("github:owner/tool@v1", { assset = "typo" })"#,
        r#"rb.package("github:owner/tool@v1", { request = "ignored" })"#,
        r#"rb.package({ request = "rg", bins = {} })"#,
        r#"rb.package({ name = "raw", version = 1, source = { file = "tool", sha256 = "hash" }, install = { binary = "tool" }, bins = { tool = "tool" } })"#,
        r#"rb.package({ name = "raw", version = "1", source = { file = "tool", url = "https://example.com/tool", sha256 = "hash" }, install = { binary = "tool" }, bins = { tool = "tool" } })"#,
        r#"rb.package({ name = "raw", version = "1", source = { path = "tool", file = "tool", sha256 = "hash" }, install = { binary = "tool" }, bins = { tool = "tool" } })"#,
        r#"rb.package({ name = "raw", version = "1", source = { file = "tool", sha256 = "hash" }, install = { directory = true, archive = "zip" }, bins = { tool = "tool" } })"#,
        r#"rb.package({ name = "raw", version = "1", source = { file = "tool", sha256 = "hash" }, install = { binary = "tool", archive = "zip" }, bins = { tool = "tool" } })"#,
        r#"rb.package({ name = "raw", version = "1", source = { file = "tool", sha256 = "hash" }, install = { directory = "true" }, bins = { tool = "tool" } })"#,
        r#"rb.package({ name = "raw", version = "1", source = { file = "tool", sha256 = "hash" }, install = { binary = "tool" }, bins = { tool = 42 } })"#,
    ] {
        let root = tempfile::tempdir().unwrap();
        let vm = vm_in(
            &format!("ok = pcall(function() {invalid} end)"),
            root.path(),
        );
        assert!(!vm.lua.globals().get::<bool>("ok").unwrap(), "{invalid}");
        assert!(super::super::test_support::drain(vm).is_empty());
    }
}

#[test]
fn profile_path_helpers_work_before_any_package_is_declared() {
    let root = tempfile::tempdir().unwrap();
    let vm = vm_in(
        r#"
        directory = rb.bin_dir()
        path = rb.bin_path("fresh_bootstrap_test_command")
        installed = rb.which("fresh_bootstrap_test_command")
        for _, name in ipairs({ "", ".", "..", "/bin/sh", "../sh", "foo/bar", "foo\\bar" }) do
            assert(not pcall(rb.which, name))
            assert(not pcall(rb.bin_path, name))
        end
        assert(not pcall(rb.which, 42))
        assert(not pcall(rb.bin_path, 42))
    "#,
        root.path(),
    );
    assert_eq!(
        PathBuf::from(vm.lua.globals().get::<String>("directory").unwrap()),
        crate::package::profile::bin_dir()
    );
    assert_eq!(
        PathBuf::from(vm.lua.globals().get::<String>("path").unwrap()),
        crate::package::profile::bin_path("fresh_bootstrap_test_command")
    );
    assert!(vm
        .lua
        .globals()
        .get::<Option<String>>("installed")
        .unwrap()
        .is_none());
    assert!(super::super::test_support::drain(vm).is_empty());
}
