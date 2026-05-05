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

use super::Vm;
use crate::plan::Op;
use crate::Runtime;

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
pub(crate) fn vm(script_dir: &Path, profile: Option<&str>) -> Vm {
    let script_path = script_dir.join("test.lua");
    if !script_path.exists() {
        fs::write(&script_path, "").expect("write stub test script");
    }
    Vm::new(runtime(
        script_dir.to_path_buf(),
        profile.map(str::to_owned),
    ))
    .expect("create vm")
}

/// Like [`vm_in`] but with an explicit `profile` value set on the runtime.
pub(crate) fn vm_in_with_profile(source: &str, script_dir: &Path, profile: Option<&str>) -> Vm {
    let script_path = script_dir.join("test.lua");
    let wrapped = format!("local rb = require(\"rootbeer\")\n{source}");
    fs::write(&script_path, &wrapped).expect("write test script");

    let vm = Vm::new(runtime(
        script_dir.to_path_buf(),
        profile.map(str::to_owned),
    ))
    .expect("create vm");
    let chunk_name = format!("@{}", script_path.display());
    vm.exec(&wrapped, &chunk_name).expect("lua exec");
    vm
}

/// Run `source` in a fresh Lua VM rooted at `script_dir` and return the
/// VM. Useful when the test wants to inspect Lua return values or call
/// further functions before draining the run log.
pub(crate) fn vm_in(source: &str, script_dir: &Path) -> Vm {
    vm_in_with_profile(source, script_dir, None)
}

pub(crate) fn drain(vm: Vm) -> Vec<Op> {
    vm.drain_ops()
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
