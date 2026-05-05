//! The `Codec` trait.
//!
//! Codec implementations operate directly on `mlua::Value` via mlua's
//! `serialize` feature — Lua tables are valid serde values, so each format
//! is just a thin wrapper around the format crate's `to_string` / `from_str`
//! with no IR walker in between. There's also a register function which is
//! used to wire up `encode` and `decode` to the Lua API along with their
//! file IO counterparts `read` and `write`.

use mlua::{Lua, Result as LuaResult, Table, Value};

use crate::lua::ctx::Ctx;

pub(super) trait Codec: 'static {
    const NAME: &'static str;

    fn encode(value: &Value) -> LuaResult<String>;
    fn decode(lua: &Lua, s: &str) -> LuaResult<Value>;
}

pub(super) fn register<C: Codec>(lua: &Lua, parent: &Table) -> LuaResult<()> {
    let t = lua.create_table()?;

    t.set(
        "encode",
        lua.create_function(|_, table: Table| C::encode(&Value::Table(table)))?,
    )?;
    t.set(
        "decode",
        lua.create_function(|lua, s: String| C::decode(lua, &s))?,
    )?;
    t.set(
        "read",
        lua.create_function(|lua, path: String| {
            let s = Ctx::from(lua).slurp(&path)?;
            C::decode(lua, &s)
        })?,
    )?;
    t.set(
        "write",
        lua.create_function(|lua, (path, table): (String, Table)| {
            let mut repr = C::encode(&Value::Table(table))?;
            if !repr.ends_with('\n') {
                repr.push('\n');
            }
            Ctx::from(lua).write(&path, repr);
            Ok(())
        })?,
    )?;

    parent.set(C::NAME, t)?;
    Ok(())
}
