//! An abstraction over the Lua module system to make it easier to register
//! Rust code as Lua modules under the `rb` table. Any code that needs to be
//! exposed to the runtime should be implemented as a `Module`.
//!
//! When creating the Lua runtime, we call `install` for each module. This is
//! intentionally not a macro to keep the build simple.

use mlua::{Lua, Result as LuaResult, Table};

/// A Lua module implemented in Rust.
pub(crate) trait Module {
    /// The name of the module.
    /// Will register as `rb.<NAME>` and importable as `@rootbeer/<NAME>`.
    const NAME: &'static str;

    /// Build the module by populating the provided table with functions and
    /// values. This is called by `install` and should not be called directly.
    fn build(lua: &Lua, t: &Table) -> LuaResult<()>;
}

/// Install a module into the Lua runtime by calling its `build` method and
/// registering it under the `rb` table and as a Lua module. This is called by
/// the Lua runtime initialization code and should not be called directly.
pub(crate) fn install<M: Module>(lua: &Lua, rb: &Table) -> LuaResult<()> {
    if M::NAME.is_empty() {
        return M::build(lua, rb);
    }

    let t = lua.create_table()?;
    M::build(lua, &t)?;
    rb.set(M::NAME, &t)?;
    lua.register_module(&format!("@rootbeer/{}", M::NAME), t)?;
    Ok(())
}
