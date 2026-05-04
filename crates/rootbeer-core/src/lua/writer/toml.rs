use super::codec::Codec;
use mlua::{Error as LuaError, Lua, LuaSerdeExt, Result as LuaResult, Value};

pub(super) struct Toml;

impl Codec for Toml {
    const NAME: &'static str = "toml";

    fn encode(value: &Value) -> LuaResult<String> {
        ::toml::to_string(value)
            .map_err(|e| LuaError::RuntimeError(format!("TOML serialization failed: {e}")))
    }

    fn decode(lua: &Lua, s: &str) -> LuaResult<Value> {
        let v: serde_json::Value = ::toml::from_str(s)
            .map_err(|e| LuaError::RuntimeError(format!("TOML parse failed: {e}")))?;
        lua.to_value(&v)
    }
}
