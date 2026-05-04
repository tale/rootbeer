use super::codec::Codec;
use mlua::{Error as LuaError, Lua, LuaSerdeExt, Result as LuaResult, Value};

pub(super) struct Plist;

impl Codec for Plist {
    const NAME: &'static str = "plist";

    fn encode(value: &Value) -> LuaResult<String> {
        let mut buf = Vec::new();
        ::plist::to_writer_xml(&mut buf, value)
            .map_err(|e| LuaError::RuntimeError(format!("plist serialization failed: {e}")))?;

        String::from_utf8(buf)
            .map_err(|e| LuaError::RuntimeError(format!("plist produced invalid UTF-8: {e}")))
    }

    fn decode(lua: &Lua, s: &str) -> LuaResult<Value> {
        let v: serde_json::Value = ::plist::from_bytes(s.as_bytes())
            .map_err(|e| LuaError::RuntimeError(format!("plist parse failed: {e}")))?;
        lua.to_value(&v)
    }
}
