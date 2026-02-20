use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mlua::{Error as LuaError, Lua, Result as LuaResult, Table};

use crate::plan::Op;
use crate::Runtime;

#[derive(Debug, Default)]
pub(crate) struct Run {
    pub ops: Vec<Op>,
}

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

fn register_lua_module(lua: &Lua, name: &str, source: &str) -> LuaResult<()> {
    let module = lua.load(source).set_name(name).eval::<mlua::Value>()?;
    lua.register_module(name, module)?;
    Ok(())
}

fn get_hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

fn create_module(lua: &Lua) -> LuaResult<Table> {
    let table = lua.create_table()?;

    table.set(
        "file",
        lua.create_function(|lua, (path, content): (String, String)| {
            let runtime = lua.app_data_ref::<Arc<Runtime>>().unwrap();
            let run = lua.app_data_ref::<Arc<Mutex<Run>>>().unwrap();
            let resolved = resolve_path(&runtime.script_dir, &path);
            run.lock().unwrap().ops.push(Op::WriteFile {
                path: resolved,
                content,
            });
            Ok(())
        })?,
    )?;

    table.set(
        "link_file",
        lua.create_function(|lua, (src, dest): (String, String)| {
            let runtime = lua.app_data_ref::<Arc<Runtime>>().unwrap();
            let run = lua.app_data_ref::<Arc<Mutex<Run>>>().unwrap();
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

            run.lock().unwrap().ops.push(Op::Symlink {
                src: canonical_src,
                dst: resolved_dst,
            });
            Ok(())
        })?,
    )?;

    table.set(
        "data",
        lua.create_function(|lua, ()| {
            let t = lua.create_table()?;
            t.set("os", std::env::consts::OS)?;
            t.set("arch", std::env::consts::ARCH)?;
            t.set("home", std::env::var("HOME").unwrap_or_default())?;
            t.set("username", std::env::var("USER").unwrap_or_default())?;
            t.set("hostname", get_hostname())?;
            Ok(t)
        })?,
    )?;

    Ok(table)
}

pub(crate) fn create_vm(runtime: Runtime) -> LuaResult<Lua> {
    let lua = Lua::new();

    lua.set_app_data(runtime);
    lua.set_app_data(Mutex::from(Run::default()));

    let rb = create_module(&lua)?;
    lua.globals().set("rootbeer", rb.clone())?;
    lua.register_module("@rootbeer", rb)?;

    // Embedded Lua standard library
    register_lua_module(
        &lua,
        "@rootbeer/shells/zsh",
        include_str!("../../../lua/rootbeer/shells/zsh.lua"),
    )?;

    Ok(lua)
}
