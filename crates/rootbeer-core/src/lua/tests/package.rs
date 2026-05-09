use std::path::PathBuf;

use crate::package::{ArchiveFormat, LockedInstall, LockedSource};
use crate::plan::Op;

use super::super::test_support::{run, vm_in};

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
          bins = {
            demo = "bin/demo",
          },
        })
        "#);

    let [Op::RealizePackage { package }] = ops.as_slice() else {
        panic!("expected one RealizePackage op, got {ops:?}");
    };

    assert_eq!(package.name, "demo");
    assert_eq!(package.version, "1.0.0");
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

    let [Op::WriteFile { path, content }] = ops.as_slice() else {
        panic!("expected one WriteFile op, got {ops:?}");
    };

    assert_eq!(result, path.to_string_lossy());
    assert!(path.ends_with("profiles/default/current/env.sh"));
    assert!(content.contains("_rootbeer_package_bin="));
    assert!(content.contains("export PATH=\"$_rootbeer_package_bin:$PATH\""));
}
