use super::codec::Codec;
use mlua::{Error as LuaError, Lua, LuaSerdeExt, Result as LuaResult, Value};

pub(super) struct Yaml;

impl Codec for Yaml {
    const NAME: &'static str = "yaml";

    fn encode(value: &Value) -> LuaResult<String> {
        serde_yml::to_string(value)
            .map_err(|e| LuaError::RuntimeError(format!("YAML serialization failed: {e}")))
    }

    fn decode(lua: &Lua, s: &str) -> LuaResult<Value> {
        let v: serde_json::Value = serde_yml::from_str(s)
            .map_err(|e| LuaError::RuntimeError(format!("YAML parse failed: {e}")))?;
        lua.to_value(&v)
    }
}
