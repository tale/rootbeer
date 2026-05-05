//! Tests for data-format codecs (`rb.json`, `rb.toml`, `rb.yaml`,
//! `rb.plist`) and script writers (`rb.scripts.bash`,
//! `rb.scripts.python`, `rb.scripts.script`, …).

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
    let vm = crate::lua::test_support::vm(tmp.path(), None);
    let err = vm
        .lua
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
        rb.yaml.write("/tmp/rb-test/c.yaml", { c = 3 })
        "#);
    assert_eq!(ops.len(), 3);
    assert!(matches!(&ops[0], Op::WriteFile { path, .. } if path.ends_with("a.json")));
    assert!(matches!(&ops[1], Op::WriteFile { path, .. } if path.ends_with("b.toml")));
    assert!(matches!(&ops[2], Op::WriteFile { path, .. } if path.ends_with("c.yaml")));
}

// ── YAML ──────────────────────────────────────────────────────────

#[test]
fn yaml_round_trip_preserves_scalars_and_arrays() {
    let tmp = tempfile::tempdir().unwrap();
    let lua = vm_in(
        r#"
        local sample = { name = "rootbeer", count = 3, tags = { "a", "b" } }
        local s = rb.yaml.encode(sample)
        local back = rb.yaml.decode(s)
        assert(back.name == "rootbeer", "name")
        assert(back.count == 3, "count")
        assert(#back.tags == 2 and back.tags[1] == "a" and back.tags[2] == "b", "tags")
        "#,
        tmp.path(),
    );
    drop(lua);
}

#[test]
fn yaml_write_pushes_write_file_with_trailing_newline() {
    let ops = run(r#"rb.yaml.write("/tmp/rb-test/x.yaml", { name = "rb", n = 1 })"#);
    assert_eq!(ops.len(), 1);
    let Op::WriteFile { path, content } = &ops[0] else {
        panic!("expected WriteFile, got {:?}", ops[0]);
    };
    assert_eq!(path, &PathBuf::from("/tmp/rb-test/x.yaml"));
    assert!(content.ends_with('\n'), "missing trailing newline");
    // serde_yml may quote scalar keys that resemble YAML reserved words
    // (e.g. `n` → `'n':`); accept either form.
    assert!(content.contains("name: rb"), "got: {content}");
    assert!(
        content.contains("n: 1") || content.contains("'n': 1"),
        "got: {content}"
    );
}

#[test]
fn yaml_read_loads_from_disk() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("data.yaml"),
        "name: rb\ntags:\n  - a\n  - b\n",
    )
    .unwrap();

    let lua = vm_in(
        r#"
        local d = rb.yaml.read("data.yaml")
        assert(d.name == "rb")
        assert(#d.tags == 2)
        "#,
        tmp.path(),
    );
    drop(lua);
}

// ── plist ─────────────────────────────────────────────────────────

#[test]
fn plist_encode_emits_xml_header() {
    let tmp = tempfile::tempdir().unwrap();
    let lua = vm_in(
        r#"
        local s = rb.plist.encode({ Theme = "Dark", Recent = { "a", "b" } })
        assert(s:find("<?xml", 1, true) == 1, "missing xml header")
        assert(s:find("<plist", 1, true) ~= nil, "missing plist root")
        assert(s:find("Theme", 1, true) ~= nil)
        "#,
        tmp.path(),
    );
    drop(lua);
}

#[test]
fn plist_round_trip_preserves_dict_and_array() {
    let tmp = tempfile::tempdir().unwrap();
    let lua = vm_in(
        r#"
        local sample = { Name = "rb", Count = 3, Tags = { "a", "b" } }
        local s = rb.plist.encode(sample)
        local back = rb.plist.decode(s)
        assert(back.Name == "rb")
        assert(back.Count == 3)
        assert(#back.Tags == 2 and back.Tags[1] == "a")
        "#,
        tmp.path(),
    );
    drop(lua);
}

#[test]
fn plist_write_pushes_write_file_with_xml_payload() {
    let ops = run(r#"rb.plist.write("/tmp/rb-test/Prefs.plist", { K = "v" })"#);
    assert_eq!(ops.len(), 1);
    let Op::WriteFile { path, content } = &ops[0] else {
        panic!("expected WriteFile");
    };
    assert_eq!(path, &PathBuf::from("/tmp/rb-test/Prefs.plist"));
    assert!(content.starts_with("<?xml"), "got: {content}");
    assert!(content.ends_with('\n'));
    assert!(content.contains("<key>K</key>"));
    assert!(content.contains("<string>v</string>"));
}

// ── Script writers ────────────────────────────────────────────────

#[test]
fn script_write_emits_writefile_then_chmod() {
    let ops = run(r#"rb.scripts.bash("/tmp/rb-test/hello", "echo hi")"#);
    assert_eq!(ops.len(), 2);

    let Op::WriteFile { path, content } = &ops[0] else {
        panic!("expected WriteFile, got {:?}", ops[0]);
    };
    assert_eq!(path, &PathBuf::from("/tmp/rb-test/hello"));
    assert_eq!(content, "#!/usr/bin/env bash\n\necho hi\n");

    let Op::Chmod { path, mode } = &ops[1] else {
        panic!("expected Chmod, got {:?}", ops[1]);
    };
    assert_eq!(path, &PathBuf::from("/tmp/rb-test/hello"));
    assert_eq!(*mode, 0o755);
}

#[test]
fn script_python_uses_python3_shebang() {
    let ops = run(r#"rb.scripts.python("/tmp/rb-test/x.py", "print('hi')")"#);
    let Op::WriteFile { content, .. } = &ops[0] else {
        panic!("expected WriteFile");
    };
    assert!(
        content.starts_with("#!/usr/bin/env python3\n"),
        "got: {content:?}"
    );
}

#[test]
fn script_generic_with_absolute_interpreter_uses_literal_shebang() {
    let ops = run(r#"rb.scripts.script("/bin/sh", "/tmp/rb-test/x.sh", "echo hi")"#);
    let Op::WriteFile { content, .. } = &ops[0] else {
        panic!("expected WriteFile");
    };
    assert!(content.starts_with("#!/bin/sh\n"), "got: {content:?}");
}

#[test]
fn script_generic_with_bare_interpreter_uses_env_shebang() {
    // rb.scripts.script("awk", ...) → #!/usr/bin/env awk
    let ops = run(r#"rb.scripts.script("awk", "/tmp/rb-test/sum", "{ s += $1 }")"#);
    let Op::WriteFile { content, .. } = &ops[0] else {
        panic!("expected WriteFile");
    };
    assert!(
        content.starts_with("#!/usr/bin/env awk\n"),
        "got: {content:?}"
    );
}

#[test]
fn script_appends_trailing_newline_if_missing() {
    let ops = run(r#"rb.scripts.bash("/tmp/rb-test/x", "echo hi")"#);
    let Op::WriteFile { content, .. } = &ops[0] else {
        panic!("expected WriteFile");
    };
    assert!(content.ends_with('\n'), "missing trailing newline");
}

#[test]
fn script_apply_sets_executable_bit_on_disk() {
    use crate::executor::{ExecutionHandler, ExecutionReport, OpResult};
    use std::os::unix::fs::PermissionsExt;

    struct Sink;
    impl ExecutionHandler for Sink {
        fn on_start(&mut self, _: &Op) {}
        fn on_output(&mut self, _: &str) {}
        fn on_result(&mut self, _: &OpResult) {}
    }

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("hello.sh");
    let path_str = path.to_string_lossy().into_owned();

    let ops = run_in(
        &format!(r#"rb.scripts.bash("{}", "echo hi")"#, path_str),
        tmp.path(),
    );

    let mut sink = Sink;
    let _: ExecutionReport = crate::executor::apply(&ops, false, &mut sink).unwrap();

    let meta = fs::metadata(&path).unwrap();
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(mode, 0o755, "expected 0755, got {mode:o}");

    let on_disk = fs::read_to_string(&path).unwrap();
    assert!(on_disk.starts_with("#!/usr/bin/env bash\n"));
    assert!(on_disk.contains("echo hi"));
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
