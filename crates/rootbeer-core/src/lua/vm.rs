use std::sync::{Mutex, MutexGuard};

use mlua::{Lua, Result};

#[cfg(all(feature = "embedded-stdlib", not(debug_assertions)))]
use crate::lua::require::EmbeddedRequirer;
use crate::lua::require::FsRequirer;
use crate::lua::{fs, secret, serializer, sys};
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

/// Create a new Lua VM and register the Rootbeer API.
pub(crate) fn create_vm(runtime: Runtime) -> Result<Lua> {
    let lua = Lua::new();

    let lua_dir = runtime.lua_dir.clone();
    let profile = runtime.profile.clone();
    lua.set_app_data(runtime);
    lua.set_app_data(Run::default());

    let rb = lua.create_table()?;
    fs::register(&lua, &rb)?;
    serializer::register(&lua, &rb)?;
    sys::register(&rb)?;
    secret::register(&lua, &rb)?;

    rb.set("profile", profile)?;

    lua.globals().set("rootbeer", &rb)?;
    lua.register_module("@rootbeer", rb)?;

    let inner_require = make_require_fn(&lua, lua_dir)?;

    // Wrap require() to accept standard Lua dot-separated paths
    // (e.g. require("rootbeer.git")) in addition to Luau's native
    // @-prefixed paths (e.g. require("@rootbeer/git")). Luau's C++
    // layer rejects paths that don't start with @, ./, or ../ so the
    // translation must happen before the path reaches it.
    let require_fn = lua.create_function(move |_lua, path: String| {
        let translated =
            if !path.starts_with('@') && !path.starts_with("./") && !path.starts_with("../") {
                if path == "rootbeer" || path.starts_with("rootbeer.") {
                    format!("@{}", path.replace('.', "/"))
                } else {
                    format!("./{}", path.replace('.', "/"))
                }
            } else {
                path
            };

        inner_require.call::<mlua::Value>(translated)
    })?;
    lua.globals().set("require", require_fn)?;

    Ok(lua)
}

fn make_require_fn(lua: &Lua, lua_dir: std::path::PathBuf) -> Result<mlua::Function> {
    // In debug builds, always use the filesystem so Lua changes don't
    // require a Rust recompile. In release builds with embedded-stdlib,
    // serve the stdlib from memory unless --lua-dir was explicitly set.
    #[cfg(all(feature = "embedded-stdlib", not(debug_assertions)))]
    {
        let default_dir = std::path::PathBuf::from(env!("ROOTBEER_LUA_DIR"));
        if lua_dir == default_dir {
            return lua.create_require_function(EmbeddedRequirer::new());
        }
    }

    lua.create_require_function(FsRequirer::new(lua_dir))
}
