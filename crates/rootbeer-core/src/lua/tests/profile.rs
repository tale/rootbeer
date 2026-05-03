//! Tests for `lua/rootbeer/profile.lua`.

use std::fs;
use std::path::Path;

use crate::lua::test_support::{drain, vm_in_with_profile};
use crate::plan::Op;

fn setup_fixture(dir: &Path) {
    fs::create_dir_all(dir.join("hosts")).unwrap();
    fs::write(
        dir.join("hosts/work.lua"),
        "local rb = require(\"rootbeer\")\nrb.file(\"/tmp/rb-test/work.txt\", \"work\\n\")\n",
    )
    .unwrap();
    fs::write(
        dir.join("hosts/personal.lua"),
        "local rb = require(\"rootbeer\")\nrb.file(\"/tmp/rb-test/personal.txt\", \"personal\\n\")\n",
    )
    .unwrap();
}

const SNIPPET: &str = r#"
    local profile = require("rootbeer.profile")
    profile.config({
        work = "hosts/work.lua",
        personal = "hosts/personal.lua",
    })
"#;

#[test]
fn profile_config_no_profile_is_noop() {
    let tmp = tempfile::tempdir().unwrap();
    setup_fixture(tmp.path());

    let lua = vm_in_with_profile(SNIPPET, tmp.path(), None);
    let ops = drain(lua);
    assert_eq!(ops, vec![], "expected no ops, got {ops:?}");
}

#[test]
fn profile_config_dispatches_to_selected_profile() {
    let tmp = tempfile::tempdir().unwrap();
    setup_fixture(tmp.path());

    let lua = vm_in_with_profile(SNIPPET, tmp.path(), Some("work"));
    let ops = drain(lua);
    assert_eq!(ops.len(), 1, "got: {ops:?}");
    assert!(matches!(
        &ops[0],
        Op::WriteFile { path, .. } if path.ends_with("work.txt")
    ));
}

#[test]
fn profile_config_unknown_profile_emits_sentinel_error() {
    let tmp = tempfile::tempdir().unwrap();
    setup_fixture(tmp.path());

    // We can't use `vm_in_with_profile` here because it `expect`s exec to
    // succeed. Build the VM manually and inspect the error.
    let script_path = tmp.path().join("test.lua");
    fs::write(
        &script_path,
        format!("local rb = require(\"rootbeer\")\n{SNIPPET}"),
    )
    .unwrap();

    let runtime = crate::Runtime {
        script_dir: tmp.path().to_path_buf(),
        script_name: "test.lua".into(),
        lua_dir: std::path::PathBuf::from(env!("ROOTBEER_LUA_DIR")),
        profile: Some("oops".into()),
    };
    let lua = crate::lua::create_vm(runtime).unwrap();
    let chunk_name = format!("@{}", script_path.display());
    let src = fs::read_to_string(&script_path).unwrap();
    let err = lua
        .load(&src)
        .set_name(&chunk_name)
        .exec()
        .expect_err("expected error");

    let s = err.to_string();
    assert!(s.contains("__rb_profile_required:oops:"), "got: {s}");
    assert!(s.contains("personal,work"), "got: {s}");
}

#[test]
fn profile_config_missing_file_errors_eagerly() {
    let tmp = tempfile::tempdir().unwrap();
    // Don't create the file.

    let script_path = tmp.path().join("test.lua");
    let body = r#"
        local rb = require("rootbeer")
        local profile = require("rootbeer.profile")
        profile.config({ work = "hosts/missing.lua" })
    "#;
    fs::write(&script_path, body).unwrap();

    let runtime = crate::Runtime {
        script_dir: tmp.path().to_path_buf(),
        script_name: "test.lua".into(),
        lua_dir: std::path::PathBuf::from(env!("ROOTBEER_LUA_DIR")),
        profile: Some("work".into()),
    };
    let lua = crate::lua::create_vm(runtime).unwrap();
    let chunk_name = format!("@{}", script_path.display());
    let err = lua
        .load(body)
        .set_name(&chunk_name)
        .exec()
        .expect_err("expected error");

    let s = err.to_string();
    assert!(s.contains("file not found"), "got: {s}");
}
