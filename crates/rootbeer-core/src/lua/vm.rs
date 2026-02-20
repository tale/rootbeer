use std::sync::{Mutex, MutexGuard};

use mlua::{Lua, Result};

use crate::plan::Op;
use crate::Runtime;

#[derive(Debug, Default)]
pub(crate) struct Run(Mutex<Vec<Op>>);

impl Run {
    pub fn lock(&self) -> MutexGuard<'_, Vec<Op>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn into_ops(self) -> Vec<Op> {
        self.0.into_inner().unwrap_or_else(|e| e.into_inner())
    }
}

fn register_lua_module(lua: &Lua, name: &str, source: &str) -> Result<()> {
    let module = lua.load(source).set_name(name).eval::<mlua::Value>()?;
    lua.register_module(name, module)?;
    Ok(())
}

#[cfg(not(feature = "dynamic-lua"))]
mod stdlib {
    include!(concat!(env!("OUT_DIR"), "/lua_stdlib.rs"));
}

#[cfg(not(feature = "dynamic-lua"))]
fn register_stdlib(lua: &Lua) -> Result<()> {
    for (name, source) in stdlib::LUA_MODULES {
        register_lua_module(lua, name, source)?;
    }
    Ok(())
}

#[cfg(feature = "dynamic-lua")]
fn register_stdlib(lua: &Lua) -> Result<()> {
    use std::path::Path;

    let lua_dir = Path::new(env!("ROOTBEER_LUA_DIR"));
    load_dir(lua, lua_dir, "rootbeer")?;
    Ok(())
}

#[cfg(feature = "dynamic-lua")]
fn load_dir(lua: &Lua, dir: &std::path::Path, prefix: &str) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| mlua::Error::RuntimeError(format!("reading {}: {e}", dir.display())))?
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.file_name());

    // Process subdirectories first so leaf modules are registered
    // before init.lua files that may require them
    let mut init_lua = None;
    for entry in &entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            load_dir(lua, &path, &format!("{prefix}.{name}"))?;
        } else if name == "init.lua" {
            init_lua = Some(path);
        } else if name.ends_with(".lua") {
            let content = std::fs::read_to_string(&path).map_err(|e| {
                mlua::Error::RuntimeError(format!("reading {}: {e}", path.display()))
            })?;
            if content.starts_with("--- @meta") {
                continue;
            }
            let module_name = format!("@{prefix}.{}", name.strip_suffix(".lua").unwrap());
            register_lua_module(lua, &module_name, &content)?;
        }
    }

    // Register init.lua last (skip top-level since @rootbeer is the native module)
    if let Some(path) = init_lua {
        if prefix != "rootbeer" {
            let content = std::fs::read_to_string(&path).map_err(|e| {
                mlua::Error::RuntimeError(format!("reading {}: {e}", path.display()))
            })?;
            register_lua_module(lua, &format!("@{prefix}"), &content)?;
        }
    }

    Ok(())
}

pub(crate) fn create_vm(runtime: Runtime) -> Result<Lua> {
    let lua = Lua::new();

    lua.set_app_data(runtime);
    lua.set_app_data(Run::default());

    let rb = lua.create_table()?;
    super::fs::register(&lua, &rb)?;
    super::sys::register(&lua, &rb)?;

    lua.globals().set("rootbeer", &rb)?;
    lua.register_module("@rootbeer", rb)?;

    register_stdlib(&lua)?;

    Ok(lua)
}
