//! Tests for `rb.file`, `rb.exec`, `rb.remote`, `rb.link`, `rb.link_file`
//! and the `~`/relative path resolution in `lua::fs`.

use std::fs;
use std::path::PathBuf;

use crate::lua::test_support::{run, run_in};
use crate::plan::Op;

#[test]
fn rb_file_pushes_write_file_op() {
    let ops = run(r#"rb.file("/tmp/rb-test/out.txt", "hello\n")"#);
    assert_eq!(
        ops,
        vec![Op::WriteFile {
            path: PathBuf::from("/tmp/rb-test/out.txt"),
            content: "hello\n".into(),
        }]
    );
}

#[test]
fn rb_file_resolves_tilde_to_home() {
    let home = std::env::var("HOME").expect("HOME set");
    let ops = run(r#"rb.file("~/note.txt", "x")"#);
    assert_eq!(
        ops,
        vec![Op::WriteFile {
            path: PathBuf::from(home).join("note.txt"),
            content: "x".into(),
        }]
    );
}

#[test]
fn rb_file_resolves_relative_to_script_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let ops = run_in(r#"rb.file("nested/file.txt", "x")"#, tmp.path());
    assert_eq!(
        ops,
        vec![Op::WriteFile {
            path: tmp.path().join("nested/file.txt"),
            content: "x".into(),
        }]
    );
}

#[test]
fn rb_exec_pushes_exec_op_with_script_dir_cwd() {
    let tmp = tempfile::tempdir().unwrap();
    let ops = run_in(r#"rb.exec("echo", { "hello", "world" })"#, tmp.path());
    assert_eq!(
        ops,
        vec![Op::Exec {
            cmd: "echo".into(),
            args: vec!["hello".into(), "world".into()],
            cwd: tmp.path().to_path_buf(),
        }]
    );
}

#[test]
fn rb_exec_with_no_args_defaults_to_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let ops = run_in(r#"rb.exec("date")"#, tmp.path());
    assert_eq!(
        ops,
        vec![Op::Exec {
            cmd: "date".into(),
            args: vec![],
            cwd: tmp.path().to_path_buf(),
        }]
    );
}

#[test]
fn rb_remote_pushes_set_remote_url_op() {
    let tmp = tempfile::tempdir().unwrap();
    let ops = run_in(
        r#"rb.remote("git@github.com:tale/rootbeer.git")"#,
        tmp.path(),
    );
    assert_eq!(
        ops,
        vec![Op::SetRemoteUrl {
            dir: tmp.path().to_path_buf(),
            url: "git@github.com:tale/rootbeer.git".into(),
        }]
    );
}

#[test]
fn rb_link_pushes_symlink_op() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("source.txt"), "x").unwrap();

    let ops = run_in(
        r#"rb.link("source.txt", "/tmp/rb-test/dest.txt")"#,
        tmp.path(),
    );
    assert_eq!(
        ops,
        vec![Op::Symlink {
            src: tmp.path().join("source.txt"),
            dst: PathBuf::from("/tmp/rb-test/dest.txt"),
        }]
    );
}

#[test]
fn rb_link_errors_on_missing_source() {
    let tmp = tempfile::tempdir().unwrap();
    let vm = crate::lua::test_support::vm(tmp.path(), None);
    let err = vm
        .lua
        .load(
            r#"
            local rb = require("rootbeer")
            rb.link("nope.txt", "/tmp/x")
            "#,
        )
        .exec()
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "got: {err}");
}

#[test]
fn rb_link_file_canonicalizes_source() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("real.txt");
    fs::write(&src, "x").unwrap();

    let ops = run_in(
        r#"rb.link_file("real.txt", "/tmp/rb-test/link.txt")"#,
        tmp.path(),
    );

    let canonical = src.canonicalize().unwrap();
    assert_eq!(
        ops,
        vec![Op::Symlink {
            src: canonical,
            dst: PathBuf::from("/tmp/rb-test/link.txt"),
        }]
    );
}

#[test]
fn rb_copy_file_pushes_copy_op_with_canonical_source() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("seed.txt");
    fs::write(&src, "x").unwrap();

    let ops = run_in(
        r#"rb.copy_file("seed.txt", "/tmp/rb-test/seed.txt")"#,
        tmp.path(),
    );

    let canonical = src.canonicalize().unwrap();
    assert_eq!(
        ops,
        vec![Op::CopyFileIfMissing {
            src: canonical,
            dst: PathBuf::from("/tmp/rb-test/seed.txt"),
        }]
    );
}

#[test]
fn rb_copy_file_errors_on_missing_source() {
    let tmp = tempfile::tempdir().unwrap();
    let vm = crate::lua::test_support::vm(tmp.path(), None);
    let err = vm
        .lua
        .load(
            r#"
            local rb = require("rootbeer")
            rb.copy_file("nope.txt", "/tmp/x")
            "#,
        )
        .exec()
        .unwrap_err();
    assert!(err.to_string().contains("copy_file source"), "got: {err}");
}

#[test]
fn path_predicates_return_correct_values() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.txt"), "x").unwrap();
    fs::create_dir(tmp.path().join("d")).unwrap();

    // The predicates don't push ops; this just exercises them.
    let _ = crate::lua::test_support::vm_in(
        r#"
        assert(rb.path_exists("a.txt"))
        assert(rb.is_file("a.txt"))
        assert(not rb.is_dir("a.txt"))
        assert(rb.is_dir("d"))
        assert(not rb.is_file("d"))
        assert(not rb.path_exists("missing"))
        "#,
        tmp.path(),
    );
}
