use crate::plan::Op;
use mlua::{Error as LuaError, Lua, Result as LuaResult, Table};
use std::path::{Path, PathBuf};

/// Resolves a path, while handling '~' for the home directory otherwise
/// falling back to the script directory for relative paths.
fn resolve_path(script_dir: &Path, path: &str) -> PathBuf {
    let expanded = if let Some(rest) = path.strip_prefix('~') {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
        if rest.is_empty() {
            PathBuf::from(home)
        } else {
            PathBuf::from(home).join(rest.strip_prefix('/').unwrap_or(rest))
        }
    } else {
        PathBuf::from(path)
    };

    if expanded.is_relative() {
        script_dir.join(expanded)
    } else {
        expanded
    }
}

pub(crate) fn register(lua: &Lua, table: &Table) -> LuaResult<()> {
    table.set(
        "file",
        lua.create_function(|lua, (path, content): (String, String)| {
            let (runtime, run) = super::ctx(lua);
            let resolved = resolve_path(&runtime.script_dir, &path);
            run.lock().push(Op::WriteFile {
                path: resolved,
                content,
            });

            Ok(())
        })?,
    )?;

    table.set(
        "link_file",
        lua.create_function(|lua, (src, dest): (String, String)| {
            let (runtime, run) = super::ctx(lua);
            let resolved_src = runtime.script_dir.join(&src);
            let resolved_dst = resolve_path(&runtime.script_dir, &dest);

            let canonical_src = resolved_src.canonicalize().map_err(|e| {
                LuaError::RuntimeError(format!(
                    "link_file source '{}' (resolved to '{}'): {}",
                    src,
                    resolved_src.display(),
                    e
                ))
            })?;

            run.lock().push(Op::Symlink {
                src: canonical_src,
                dst: resolved_dst,
            });

            Ok(())
        })?,
    )?;

    Ok(())
}
