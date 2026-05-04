use super::codec::Codec;
use mlua::{Error as LuaError, Lua, LuaSerdeExt, Result as LuaResult, Value};

pub(super) struct Json;

impl Codec for Json {
    const NAME: &'static str = "json";

    fn encode(value: &Value) -> LuaResult<String> {
        serde_json::to_string_pretty(value)
            .map_err(|e| LuaError::RuntimeError(format!("JSON serialization failed: {e}")))
    }

    fn decode(lua: &Lua, s: &str) -> LuaResult<Value> {
        let v: serde_json::Value = serde_json::from_str(s)
            .map_err(|e| LuaError::RuntimeError(format!("JSON parse failed: {e}")))?;
        lua.to_value(&v)
    }
}
