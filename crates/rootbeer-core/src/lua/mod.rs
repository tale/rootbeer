mod fs;
pub(crate) mod require;
mod secret;
mod sys;
mod vm;
mod writer;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

use crate::plan::Op;
use crate::Runtime;
use mlua::{AppDataRef, Error as LuaError, Lua, Result as LuaResult};
use std::path::Path;
pub(crate) use vm::{create_vm, Run};

/// Extract the shared Runtime and Run from Lua app data.
pub(super) fn ctx(lua: &Lua) -> (AppDataRef<'_, Runtime>, AppDataRef<'_, Run>) {
    (
        lua.app_data_ref::<Runtime>().expect("Runtime not set"),
        lua.app_data_ref::<Run>().expect("Run not set"),
    )
}

/// Read a file, resolving the path against the script dir / `~`.
/// Synchronous: the script needs the value immediately.
pub(super) fn slurp(lua: &Lua, path: &str) -> LuaResult<String> {
    let (runtime, _) = ctx(lua);
    let resolved = fs::resolve_path(&runtime.script_dir, path);
    std::fs::read_to_string(&resolved).map_err(|e| {
        LuaError::RuntimeError(format!(
            "failed to read '{}' (resolved to '{}'): {}",
            path,
            resolved.display(),
            e
        ))
    })
}

/// Defer a file write via the run log. Path is resolved against the script
/// dir / `~`.
pub(super) fn defer_write(lua: &Lua, path: &str, content: String) -> LuaResult<()> {
    let (runtime, run) = ctx(lua);
    let resolved = fs::resolve_path(&runtime.script_dir, path);
    run.lock().push(Op::WriteFile {
        path: resolved,
        content,
    });
    Ok(())
}

/// Defer a chmod via the run log. Caller passes a fully-resolved path.
pub(super) fn defer_chmod(lua: &Lua, path: &Path, mode: u32) -> LuaResult<()> {
    let (_, run) = ctx(lua);
    run.lock().push(Op::Chmod {
        path: path.to_path_buf(),
        mode,
    });
    Ok(())
}
