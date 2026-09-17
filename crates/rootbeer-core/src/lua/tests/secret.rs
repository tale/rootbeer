//! Tests for `rb.secret.op_document` — the planning side. The actual
//! `op document get` invocation lives in the apply executor and isn't
//! covered here (it requires a logged-in `op` CLI).

use std::path::PathBuf;

use crate::lua::test_support::run;
use crate::plan::{Op, WriteSource};

#[test]
fn op_document_pushes_write_file_with_op_document_source() {
    let ops = run(r#"rb.secret.op_document("op://Private/work-ssh-key", "/tmp/rb-test/key")"#);
    assert_eq!(
        ops,
        vec![Op::WriteFile {
            path: PathBuf::from("/tmp/rb-test/key"),
            source: WriteSource::OpDocument {
                reference: "op://Private/work-ssh-key".into(),
            },
        }]
    );
}

#[test]
fn op_document_resolves_tilde_in_dest() {
    let home = std::env::var("HOME").expect("HOME set");
    let ops = run(r#"rb.secret.op_document("op://Private/key", "~/secret.bin")"#);
    assert_eq!(
        ops,
        vec![Op::WriteFile {
            path: PathBuf::from(home).join("secret.bin"),
            source: WriteSource::OpDocument {
                reference: "op://Private/key".into(),
            },
        }]
    );
}

#[test]
fn op_document_with_mode_emits_chmod_after_write() {
    let ops =
        run(r#"rb.secret.op_document("op://Private/key", "/tmp/rb-test/key", { mode = 0x180 })"#);
    assert_eq!(
        ops,
        vec![
            Op::WriteFile {
                path: PathBuf::from("/tmp/rb-test/key"),
                source: WriteSource::OpDocument {
                    reference: "op://Private/key".into(),
                },
            },
            Op::Chmod {
                path: PathBuf::from("/tmp/rb-test/key"),
                mode: 0o600,
            },
        ]
    );
}

#[test]
fn write_source_helpers_report_deferred_state() {
    // Sanity for the helpers dry-run and the CLI rely on: secret-backed
    // sources don't know their size at planning time, can't be inspected
    // as text, and surface a fetch label for the CLI preamble.
    let s = WriteSource::OpDocument {
        reference: "op://Private/my-key".into(),
    };
    assert_eq!(s.known_size(), None);
    assert_eq!(s.as_str(), None);
    assert_eq!(
        s.fetch_label().as_deref(),
        Some("op-document op://Private/my-key")
    );

    // Inline sources are the inverse: known size, readable as &str if UTF-8,
    // and no fetch announcement (because no fetch happens).
    let inline = WriteSource::text("hello");
    assert_eq!(inline.known_size(), Some(5));
    assert_eq!(inline.as_str(), Some("hello"));
    assert_eq!(inline.fetch_label(), None);
}

#[test]
fn age_snippet_preserves_whitespace_and_redacts_plan_debug() {
    let dir = tempfile::tempdir().unwrap();
    crate::age::tests::fixture(dir.path(), b"private snippet\n\n", true);
    let ops = crate::lua::test_support::run_in(
        r#"rb.file("output", rb.secret.age("secret.age", { identity = "key.txt" }))"#,
        dir.path(),
    );
    let Op::WriteFile { source, .. } = &ops[0] else {
        panic!("expected write")
    };
    assert_eq!(source.as_str(), Some("private snippet\n\n"));
    assert_eq!(format!("{source:?}"), "Bytes { len: 17 }");
}

#[test]
fn age_file_defers_reading_and_resolves_paths() {
    let dir = tempfile::tempdir().unwrap();
    let ops = crate::lua::test_support::run_in(
        r#"rb.secret.age_file("missing.age", "output", { identity = "missing.key" })"#,
        dir.path(),
    );
    assert_eq!(
        ops,
        vec![Op::WriteFile {
            path: dir.path().join("output"),
            source: WriteSource::AgeFile {
                path: dir.path().join("missing.age"),
                identity: crate::AgeIdentity::File(dir.path().join("missing.key")),
                mode: 0o600,
            },
        }]
    );
    let Op::WriteFile { source, .. } = &ops[0] else {
        unreachable!()
    };
    assert_eq!(source.as_str(), None);
    assert_eq!(source.known_size(), None);
    assert!(source.fetch_label().unwrap().starts_with("age "));
}

#[test]
fn age_file_accepts_deferred_op_identity_and_custom_mode() {
    let ops = run(r#"rb.secret.age_file("secret.age", "output", {
        identity_op = "op://Private/age/key", mode = 0x1a0,
    })"#);
    let Op::WriteFile {
        source: WriteSource::AgeFile { identity, mode, .. },
        ..
    } = &ops[0]
    else {
        panic!("expected age file")
    };
    assert_eq!(
        identity,
        &crate::AgeIdentity::OnePassword("op://Private/age/key".into())
    );
    assert_eq!(*mode, 0o640);
}

#[test]
fn age_rejects_invalid_options_and_binary_snippets() {
    let dir = tempfile::tempdir().unwrap();
    crate::age::tests::fixture(dir.path(), b"\xff", false);
    let vm = crate::lua::test_support::vm(dir.path(), None);
    for source in [
        r#"rb.secret.age_file("x", "y", {})"#,
        r#"rb.secret.age_file("x", "y", { identity = "key", identity_op = "op://v/i/f" })"#,
        r#"rb.secret.age_file("x", "y", { identity_op = "bad-reference" })"#,
        r#"rb.secret.age_file("x", "y", { identity = "key", mode = 4095 })"#,
        r#"rb.secret.age_file("x", "y", { identity = "key", mode = "bad" })"#,
        r#"rb.secret.age("secret.age", { identity = "key.txt" })"#,
    ] {
        assert!(vm
            .exec(&format!("local rb = require('rootbeer'); {source}"), "test")
            .is_err());
    }
}
