# Testing

Rootbeer's plan/execute split makes testing simple: every Lua function
that wants to do something side-effecting pushes an `Op` onto a shared
log instead. Tests run a Lua snippet and assert on the resulting
`Vec<Op>` — no filesystem, no subprocesses, no fixtures.

## Running

```sh
cargo test --workspace
```

That's the whole interface. CI runs the same command.

## Writing a test

Tests live next to the code they cover. Tests for the Rust bindings
sit in `#[cfg(test)] mod tests` blocks at the bottom of each source
file. Tests for the high-level Lua modules live under
`crates/rootbeer-core/src/lua/tests/<module>.rs`.

The helper module `lua::test_support` provides everything you need:

```rust
use crate::lua::test_support::run;
use crate::plan::Op;

#[test]
fn brew_writes_brewfile_and_runs_bundle() {
    let ops = run(
        r#"
        local brew = require("rootbeer.brew")
        brew.config({
            path = "/tmp/Brewfile",
            formulae = { "ripgrep" },
        })
        "#,
    );

    assert_eq!(ops.len(), 2);
    assert!(matches!(&ops[0], Op::WriteFile { .. }));
    assert!(matches!(&ops[1], Op::Exec { cmd, .. } if cmd == "brew"));
}
```

`run` builds a fresh Lua VM rooted at a tempdir, executes the snippet
(with `local rb = require("rootbeer")` injected automatically), drains
the run log, and returns it. `run_in` is the same but takes an explicit
`script_dir` for fixtures. `vm_in` and `vm_in_with_profile` return the
VM itself if a test wants to do more before draining via `drain(lua)`.

## What to assert

`Op` derives `Debug, Clone, PartialEq`, so tests can compare ops
directly:

```rust
assert_eq!(ops, vec![Op::WriteFile {
    path: PathBuf::from("/tmp/x"),
    content: "hello\n".into(),
}]);
```

For high-level modules that write structured config, prefer asserting
on `content` substrings — exact-match assertions tend to be brittle as
the formatter evolves.

## Testing the executors

When you do need to verify side-effects (file writes, symlink
overwrites), drive the executor against a tempdir with a recording
`ExecutionHandler`. Examples in
[`executor/apply.rs`](https://github.com/tale/rootbeer/blob/main/crates/rootbeer-core/src/executor/apply.rs)
and
[`executor/dry_run.rs`](https://github.com/tale/rootbeer/blob/main/crates/rootbeer-core/src/executor/dry_run.rs).
