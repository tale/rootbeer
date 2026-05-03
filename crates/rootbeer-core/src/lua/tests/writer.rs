//! Tests for `rb.json`, `rb.toml`, and `rb.ini` writers.

use std::fs;
use std::path::PathBuf;

use crate::lua::test_support::{run, run_in, vm_in};
use crate::plan::Op;

// ── encode / decode round-trip ────────────────────────────────────

#[test]
fn json_round_trip_preserves_scalars_and_arrays() {
    let tmp = tempfile::tempdir().unwrap();
    let lua = vm_in(
        r#"
        local sample = { name = "rootbeer", count = 3, tags = { "a", "b" } }
        local s = rb.json.encode(sample)
        local back = rb.json.decode(s)
        assert(back.name == "rootbeer", "name")
        assert(back.count == 3, "count")
        assert(#back.tags == 2 and back.tags[1] == "a" and back.tags[2] == "b", "tags")
        "#,
        tmp.path(),
    );
    drop(lua);
}

#[test]
fn toml_round_trip_preserves_scalars_and_arrays() {
    let tmp = tempfile::tempdir().unwrap();
    let lua = vm_in(
        r#"
        local sample = { name = "rootbeer", count = 3, tags = { "a", "b" } }
        local s = rb.toml.encode(sample)
        local back = rb.toml.decode(s)
        assert(back.name == "rootbeer", "name")
        assert(back.count == 3, "count")
        assert(#back.tags == 2, "tags")
        "#,
        tmp.path(),
    );
    drop(lua);
}

// ── write defers via Op::WriteFile ────────────────────────────────

#[test]
fn json_write_pushes_write_file_with_trailing_newline() {
    let ops = run(r#"rb.json.write("/tmp/rb-test/x.json", { name = "rb", n = 1 })"#);
    assert_eq!(ops.len(), 1);
    let Op::WriteFile { path, content } = &ops[0] else {
        panic!("expected WriteFile, got {:?}", ops[0]);
    };
    assert_eq!(path, &PathBuf::from("/tmp/rb-test/x.json"));
    assert!(content.ends_with('\n'), "missing trailing newline");
    let v: serde_json::Value = serde_json::from_str(content.trim_end()).unwrap();
    assert_eq!(v["name"], "rb");
    assert_eq!(v["n"], 1);
}

#[test]
fn toml_write_pushes_write_file_with_trailing_newline() {
    let ops = run(r#"rb.toml.write("/tmp/rb-test/x.toml", { name = "rb", n = 1 })"#);
    assert_eq!(ops.len(), 1);
    let Op::WriteFile { path, content } = &ops[0] else {
        panic!("expected WriteFile");
    };
    assert_eq!(path, &PathBuf::from("/tmp/rb-test/x.toml"));
    assert!(content.ends_with('\n'));
    assert!(content.contains("name = \"rb\""));
    assert!(content.contains("n = 1"));
}

// ── INI ───────────────────────────────────────────────────────────

#[test]
fn ini_emits_gitconfig_style_sections() {
    let ops = run(r#"
        rb.ini.write("/tmp/rb-test/cfg.ini", {
            user = { name = "alice", email = "a@b" },
            ['filter "lfs"'] = { clean = "git-lfs clean" },
        })
        "#);
    assert_eq!(ops.len(), 1);
    let Op::WriteFile { content, .. } = &ops[0] else {
        panic!("expected WriteFile");
    };
    assert!(content.contains("[user]"));
    assert!(content.contains("name = alice"));
    assert!(content.contains("email = a@b"));
    assert!(content.ends_with('\n'));
}

#[test]
fn ini_supports_subsection_nesting() {
    // Two-level: top key is the section, inner table key whose value is
    // a table becomes a `[section "subkey"]` block.
    let ops = run(r#"
        rb.ini.write("/tmp/rb-test/git.ini", {
            url = {
                ["git@github.com:"] = { insteadOf = "https://github.com/" },
            },
        })
        "#);
    let Op::WriteFile { content, .. } = &ops[0] else {
        panic!("expected WriteFile");
    };
    assert!(
        content.contains(r#"[url "git@github.com:"]"#),
        "got: {content}"
    );
    assert!(content.contains("insteadOf = https://github.com/"));
}

// ── read ──────────────────────────────────────────────────────────

#[test]
fn json_read_loads_from_disk() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("data.json"),
        r#"{"name":"rb","tags":["a","b"]}"#,
    )
    .unwrap();

    let lua = vm_in(
        r#"
        local d = rb.json.read("data.json")
        assert(d.name == "rb")
        assert(#d.tags == 2)
        "#,
        tmp.path(),
    );
    drop(lua);
}

// ── error paths ───────────────────────────────────────────────────

#[test]
fn json_decode_propagates_parse_error() {
    let tmp = tempfile::tempdir().unwrap();
    let lua = crate::lua::test_support::vm(tmp.path(), None);
    let err = lua
        .load(
            r#"
            local rb = require("rootbeer")
            return rb.json.decode("not json")
            "#,
        )
        .eval::<mlua::Value>()
        .unwrap_err();
    assert!(err.to_string().contains("JSON parse failed"), "got: {err}");
}

#[test]
fn write_paths_keep_original_arg_order_in_op_log() {
    // rb.json.write and rb.toml.write should each produce one op in
    // call order (no reordering in the run log).
    let ops = run(r#"
        rb.json.write("/tmp/rb-test/a.json", { a = 1 })
        rb.toml.write("/tmp/rb-test/b.toml", { b = 2 })
        rb.ini.write("/tmp/rb-test/c.ini", { s = { k = "v" } })
        "#);
    assert_eq!(ops.len(), 3);
    assert!(matches!(&ops[0], Op::WriteFile { path, .. } if path.ends_with("a.json")));
    assert!(matches!(&ops[1], Op::WriteFile { path, .. } if path.ends_with("b.toml")));
    assert!(matches!(&ops[2], Op::WriteFile { path, .. } if path.ends_with("c.ini")));
}

#[test]
fn json_write_then_read_round_trips_through_apply() {
    use crate::executor::{ExecutionHandler, ExecutionReport, OpResult};

    struct Sink;
    impl ExecutionHandler for Sink {
        fn on_start(&mut self, _: &Op) {}
        fn on_output(&mut self, _: &str) {}
        fn on_result(&mut self, _: &OpResult) {}
    }

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("out.json");
    let path_str = path.to_string_lossy().into_owned();

    let ops = run_in(
        &format!(r#"rb.json.write("{}", {{ name = "rb", n = 7 }})"#, path_str),
        tmp.path(),
    );

    let mut sink = Sink;
    let _: ExecutionReport = crate::executor::apply(&ops, false, &mut sink).unwrap();

    let on_disk = fs::read_to_string(&path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&on_disk).unwrap();
    assert_eq!(parsed["name"], "rb");
    assert_eq!(parsed["n"], 7);
}
