use std::sync::{Mutex, MutexGuard};

use mlua::{Lua, Result};

use crate::lua::require::RootbeerRequirer;
use crate::lua::{fs, serializer, sys};
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
    lua.set_app_data(runtime);
    lua.set_app_data(Run::default());

    let rb = lua.create_table()?;
    fs::register(&lua, &rb)?;
    serializer::register(&lua, &rb)?;
    sys::register(&rb)?;

    lua.globals().set("rootbeer", &rb)?;
    lua.register_module("@rootbeer", rb)?;

    let require_fn = lua.create_require_function(RootbeerRequirer::new(lua_dir))?;
    lua.globals().set("require", require_fn)?;

    Ok(lua)
}
