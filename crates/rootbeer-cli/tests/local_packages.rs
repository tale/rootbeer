use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use rootbeer_core::package::{download::DownloadCache, lockfile::RootbeerLock, ResolveContext};

fn apply(root: &Path, flags: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rb"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .args(["apply", "--script"])
        .arg(root.join("rootbeer.lua"))
        .args(flags)
        .output()
        .unwrap()
}

fn succeeds(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn local_source_recipes_build_lock_replay_offline_and_reconcile_edits() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir_all(source.join("tool")).unwrap();
    fs::write(
        source.join("tool/local-tool"),
        "#!/bin/sh\nprintf 'local tool works\\n'\n",
    )
    .unwrap();
    let archive = root.path().join("source.tar.gz");
    rootbeer_build::pack(&source, &archive).unwrap();
    let cached = DownloadCache::new(root.path().join("state/rootbeer/downloads"))
        .materialize(&format!("file://{}", archive.display()), None)
        .unwrap();
    let recipes = root.path().join("packages");
    fs::create_dir(&recipes).unwrap();
    let recipe = format!(
        r#"return {{
        schema = 2, name = "local-tool", description = "Local integration tool",
        homepage = "https://example.invalid/local-tool", default_version = "1",
        systems = {{ "{}" }},
        inputs = {{ source = {{
            url = "https://unavailable.invalid/tool.tar.gz",
            archive = "tar.gz", strip_prefix = "tool",
        }} }},
        build = {{ backend = "custom", steps = {{
            configure = {{}}, build = {{ {{ "chmod", "+x", "local-tool" }} }},
            check = {{ {{ "./local-tool" }} }},
            install = {{ {{ "mkdir", "-p", "{{prefix}}/bin" }}, {{ "cp", "local-tool", "{{prefix}}/bin/local-tool" }} }},
        }} }},
        outputs = {{ bins = {{ "local-tool" }}, checks = {{ {{ "local-tool" }} }} }},
        versions = {{ ["1"] = {{ revision = 1, inputs = {{ source = {{ sha256 = "{}" }} }} }} }},
    }}"#,
        ResolveContext::current().system,
        cached.sha256
    );
    let path = recipes.join("local-tool.lua");
    fs::write(&path, &recipe).unwrap();
    fs::write(
        root.path().join("rootbeer.lua"),
        r#"
        local rb = require("rootbeer")
        rb.package_catalog("packages")
        rb.package("local-tool")
        rb.exec(rb.bin_path("local-tool"))
    "#,
    )
    .unwrap();
    succeeds(apply(root.path(), &["--dry-run"]));
    assert!(!root.path().join("rootbeer.lock").exists());
    assert!(!root.path().join("state/rootbeer/source-builds").exists());
    succeeds(apply(root.path(), &[]));
    let lock_path = root.path().join("rootbeer.lock");
    let before = RootbeerLock::read(&lock_path).unwrap();
    assert!(before.inputs.package_index().is_none());
    assert!(before.inputs.local_catalog().is_some());
    let bin = root
        .path()
        .join("state/rootbeer/profiles/default/current/bin/local-tool");
    assert_eq!(
        Command::new(&bin).output().unwrap().stdout,
        b"local tool works\n"
    );
    fs::remove_file(archive).unwrap();
    succeeds(apply(root.path(), &["--offline", "--locked"]));
    assert_eq!(before, RootbeerLock::read(&lock_path).unwrap());
    fs::write(&path, recipe.replace("revision = 1", "revision = 2")).unwrap();
    let stale = apply(root.path(), &["--locked"]);
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("stale"));
    assert!(!apply(root.path(), &["--offline"]).status.success());
    assert_eq!(before, RootbeerLock::read(&lock_path).unwrap());
    succeeds(apply(root.path(), &[]));
    let after = RootbeerLock::read(&lock_path).unwrap();
    assert_ne!(before.input_fingerprint, after.input_fingerprint);
    assert_eq!(
        after.inputs.local_catalog().unwrap().packages["local-tool"].versions["1"].revision,
        2
    );
    succeeds(apply(root.path(), &["--offline", "--locked"]));
}
