//! Test helpers for driving Lua snippets through the VM and inspecting
//! the resulting plan.
//!
//! The whole point of rootbeer is that side-effects are deferred into a
//! `Vec<Op>`. That means every Lua module — primitives in `lua/*.rs` and
//! high-level modules in `lua/rootbeer/*.lua` — can be unit-tested by
//! running a snippet and asserting on the produced ops. No filesystem,
//! no subprocesses, no fixtures.

use std::fs;
use std::path::{Path, PathBuf};

use mlua::Lua;

use super::{create_vm, Run};
use crate::plan::Op;
use crate::Runtime;

/// Build a `Runtime` pointing at the real stdlib and the given script dir.
fn runtime(script_dir: PathBuf, profile: Option<String>) -> Runtime {
    Runtime {
        script_dir,
        script_name: "test.lua".into(),
        lua_dir: PathBuf::from(env!("ROOTBEER_LUA_DIR")),
        profile,
    }
}

/// Build a fresh VM rooted at `script_dir` without executing any source.
/// The script_dir is materialized with a stub `test.lua` so that the
/// require system can `reset()` to it.
pub(crate) fn vm(script_dir: &Path, profile: Option<&str>) -> Lua {
    let script_path = script_dir.join("test.lua");
    if !script_path.exists() {
        fs::write(&script_path, "").expect("write stub test script");
    }
    create_vm(runtime(
        script_dir.to_path_buf(),
        profile.map(str::to_owned),
    ))
    .expect("create_vm")
}

/// Like [`vm_in`] but with an explicit `profile` value set on the runtime.
pub(crate) fn vm_in_with_profile(source: &str, script_dir: &Path, profile: Option<&str>) -> Lua {
    let script_name = "test.lua";
    let script_path = script_dir.join(script_name);
    let wrapped = format!("local rb = require(\"rootbeer\")\n{source}");
    fs::write(&script_path, &wrapped).expect("write test script");

    let lua = create_vm(runtime(
        script_dir.to_path_buf(),
        profile.map(str::to_owned),
    ))
    .expect("create_vm");
    let chunk_name = format!("@{}", script_path.display());
    lua.load(&wrapped)
        .set_name(&chunk_name)
        .exec()
        .expect("lua exec");
    lua
}

/// Run `source` in a fresh Lua VM rooted at `script_dir` and return the
/// VM. Useful when the test wants to inspect Lua return values or call
/// further functions before draining the run log.
///
/// The snippet is written to `<script_dir>/test.lua` and a `local rb =
/// require("rootbeer")` shim is injected at the top, so test snippets
/// can use `rb.file(...)`, `rb.json.write(...)`, etc. directly — just
/// like real user scripts.
pub(crate) fn vm_in(source: &str, script_dir: &Path) -> Lua {
    vm_in_with_profile(source, script_dir, None)
}

/// Drain the planned ops from a VM previously returned by [`vm_in`].
pub(crate) fn drain(lua: Lua) -> Vec<Op> {
    lua.remove_app_data::<Run>()
        .expect("Run app data")
        .into_ops()
}

/// Run `source` in a fresh VM with `script_dir` set to a tempdir and
/// return the resulting ops. The tempdir is dropped on return — only
/// pass snippets that don't read files relative to the script dir.
pub(crate) fn run(source: &str) -> Vec<Op> {
    let tmp = tempfile::tempdir().expect("tempdir");
    drain(vm_in(source, tmp.path()))
}

/// Run `source` against an explicit script dir (e.g. one populated with
/// fixture files) and return the ops.
pub(crate) fn run_in(source: &str, script_dir: &Path) -> Vec<Op> {
    drain(vm_in(source, script_dir))
}
