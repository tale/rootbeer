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
