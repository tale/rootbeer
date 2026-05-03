//! Format writers (JSON, TOML, INI).
//!
//! Each writer registers a sub-table on `rb` with the same shape:
//!
//! ```text
//! rb.<fmt>.encode(t)        — table → string
//! rb.<fmt>.decode(s)        — string → table
//! rb.<fmt>.read(path)       — path  → table   (slurp ∘ decode)
//! rb.<fmt>.write(path, t)   — path, table → ()  (encode ∘ file)
//! ```
//!
//! `read` is synchronous (the script needs the value immediately). `write`
//! defers via the standard `Op::WriteFile` plan op so it participates in
//! dry-run / apply.
//!
//! INI is write-only: the encoder is a "poor man's" gitconfig emitter and
//! we don't ship a parser, so `decode` / `read` are not exposed.

use crate::plan::Op;
use mlua::{Error as LuaError, Lua, Result as LuaResult, Table, Value};
use std::fs;

// ── Shared helpers ────────────────────────────────────────────────

/// Reads a file from the resolved path into a string.
fn slurp(lua: &Lua, path: &str) -> LuaResult<String> {
    let (runtime, _) = super::ctx(lua);
    let resolved = super::fs::resolve_path(&runtime.script_dir, path);
    fs::read_to_string(&resolved).map_err(|e| {
        LuaError::RuntimeError(format!(
            "failed to read '{}' (resolved to '{}'): {}",
            path,
            resolved.display(),
            e
        ))
    })
}

/// Defers a file write via the run log.
fn defer_write(lua: &Lua, path: &str, content: String) -> LuaResult<()> {
    let (runtime, run) = super::ctx(lua);
    let resolved = super::fs::resolve_path(&runtime.script_dir, path);
    run.lock().push(Op::WriteFile {
        path: resolved,
        content,
    });
    Ok(())
}

/// Ensures `s` ends with a newline. Used by `write` so files always have a
/// final newline regardless of what `encode` produces.
fn with_trailing_newline(mut s: String) -> String {
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Checks whether a Lua table has consecutive integer keys starting at 1,
/// i.e. it should be treated as an array rather than an object.
fn is_array(table: &Table) -> LuaResult<bool> {
    let mut max_index: i64 = 0;
    let mut count: i64 = 0;

    for pair in table.clone().pairs::<Value, Value>() {
        let (key, _) = pair?;
        count += 1;
        match key {
            Value::Integer(i) if i >= 1 => {
                if i > max_index {
                    max_index = i;
                }
            }
            _ => return Ok(false),
        }
    }

    Ok(count > 0 && max_index == count)
}

// ── JSON ──────────────────────────────────────────────────────────

fn lua_to_json_value(value: &Value) -> LuaResult<serde_json::Value> {
    match value {
        Value::Nil => Ok(serde_json::Value::Null),
        Value::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Integer(n) => Ok(serde_json::Value::Number((*n).into())),
        Value::Number(n) => {
            let num = serde_json::Number::from_f64(*n).ok_or_else(|| {
                LuaError::RuntimeError(format!("cannot serialize {n} to JSON (NaN/Inf)"))
            })?;
            Ok(serde_json::Value::Number(num))
        }
        Value::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_owned())),
        Value::Table(t) => {
            if is_array(t)? {
                let mut arr = Vec::new();
                for i in 1..=t.raw_len() {
                    let val: Value = t.raw_get(i)?;
                    arr.push(lua_to_json_value(&val)?);
                }
                Ok(serde_json::Value::Array(arr))
            } else {
                let mut map = serde_json::Map::new();
                for pair in t.clone().pairs::<String, Value>() {
                    let (k, v) = pair?;
                    map.insert(k, lua_to_json_value(&v)?);
                }
                Ok(serde_json::Value::Object(map))
            }
        }
        _ => Err(LuaError::RuntimeError(format!(
            "cannot serialize {} to JSON",
            value.type_name()
        ))),
    }
}

fn json_to_lua_value(lua: &Lua, value: &serde_json::Value) -> LuaResult<Value> {
    match value {
        serde_json::Value::Null => Ok(Value::Nil),
        serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Err(LuaError::RuntimeError(format!(
                    "cannot represent JSON number {n} in Lua"
                )))
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(lua.create_string(s)?)),
        serde_json::Value::Array(arr) => {
            let t = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                t.set(i + 1, json_to_lua_value(lua, v)?)?;
            }
            Ok(Value::Table(t))
        }
        serde_json::Value::Object(map) => {
            let t = lua.create_table()?;
            for (k, v) in map {
                t.set(k.as_str(), json_to_lua_value(lua, v)?)?;
            }
            Ok(Value::Table(t))
        }
    }
}

fn json_encode(table: &Table) -> LuaResult<String> {
    let value = lua_to_json_value(&Value::Table(table.clone()))?;
    serde_json::to_string_pretty(&value)
        .map_err(|e| LuaError::RuntimeError(format!("JSON serialization failed: {e}")))
}

fn json_decode(lua: &Lua, s: &str) -> LuaResult<Value> {
    let value: serde_json::Value = serde_json::from_str(s)
        .map_err(|e| LuaError::RuntimeError(format!("JSON parse failed: {e}")))?;
    json_to_lua_value(lua, &value)
}

fn register_json(lua: &Lua, parent: &Table) -> LuaResult<()> {
    let t = lua.create_table()?;

    t.set(
        "encode",
        lua.create_function(|_, table: Table| json_encode(&table))?,
    )?;
    t.set(
        "decode",
        lua.create_function(|lua, s: String| json_decode(lua, &s))?,
    )?;
    t.set(
        "read",
        lua.create_function(|lua, path: String| {
            let s = slurp(lua, &path)?;
            json_decode(lua, &s)
        })?,
    )?;
    t.set(
        "write",
        lua.create_function(|lua, (path, table): (String, Table)| {
            let encoded = with_trailing_newline(json_encode(&table)?);
            defer_write(lua, &path, encoded)
        })?,
    )?;

    parent.set("json", t)?;
    Ok(())
}

// ── TOML ──────────────────────────────────────────────────────────

fn lua_to_toml_value(value: &Value) -> LuaResult<toml::Value> {
    match value {
        Value::Boolean(b) => Ok(toml::Value::Boolean(*b)),
        Value::Integer(n) => Ok(toml::Value::Integer(*n)),
        Value::Number(n) => Ok(toml::Value::Float(*n)),
        Value::String(s) => Ok(toml::Value::String(s.to_str()?.to_owned())),
        Value::Table(t) => {
            if is_array(t)? {
                let mut arr = Vec::new();
                for i in 1..=t.raw_len() {
                    let val: Value = t.raw_get(i)?;
                    arr.push(lua_to_toml_value(&val)?);
                }
                Ok(toml::Value::Array(arr))
            } else {
                let mut map = toml::map::Map::new();
                for pair in t.clone().pairs::<String, Value>() {
                    let (k, v) = pair?;
                    map.insert(k, lua_to_toml_value(&v)?);
                }
                Ok(toml::Value::Table(map))
            }
        }
        _ => Err(LuaError::RuntimeError(format!(
            "cannot serialize {} to TOML",
            value.type_name()
        ))),
    }
}

fn toml_to_lua_value(lua: &Lua, value: &toml::Value) -> LuaResult<Value> {
    match value {
        toml::Value::Boolean(b) => Ok(Value::Boolean(*b)),
        toml::Value::Integer(n) => Ok(Value::Integer(*n)),
        toml::Value::Float(n) => Ok(Value::Number(*n)),
        toml::Value::String(s) => Ok(Value::String(lua.create_string(s)?)),
        toml::Value::Datetime(d) => Ok(Value::String(lua.create_string(d.to_string())?)),
        toml::Value::Array(arr) => {
            let t = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                t.set(i + 1, toml_to_lua_value(lua, v)?)?;
            }
            Ok(Value::Table(t))
        }
        toml::Value::Table(map) => {
            let t = lua.create_table()?;
            for (k, v) in map {
                t.set(k.as_str(), toml_to_lua_value(lua, v)?)?;
            }
            Ok(Value::Table(t))
        }
    }
}

fn toml_encode(table: &Table) -> LuaResult<String> {
    let value = lua_to_toml_value(&Value::Table(table.clone()))?;
    let toml::Value::Table(map) = value else {
        return Err(LuaError::RuntimeError(
            "TOML top-level must be a table".into(),
        ));
    };
    toml::to_string(&map)
        .map_err(|e| LuaError::RuntimeError(format!("TOML serialization failed: {e}")))
}

fn toml_decode(lua: &Lua, s: &str) -> LuaResult<Value> {
    let value: toml::Value =
        toml::from_str(s).map_err(|e| LuaError::RuntimeError(format!("TOML parse failed: {e}")))?;
    toml_to_lua_value(lua, &value)
}

fn register_toml(lua: &Lua, parent: &Table) -> LuaResult<()> {
    let t = lua.create_table()?;

    t.set(
        "encode",
        lua.create_function(|_, table: Table| toml_encode(&table))?,
    )?;
    t.set(
        "decode",
        lua.create_function(|lua, s: String| toml_decode(lua, &s))?,
    )?;
    t.set(
        "read",
        lua.create_function(|lua, path: String| {
            let s = slurp(lua, &path)?;
            toml_decode(lua, &s)
        })?,
    )?;
    t.set(
        "write",
        lua.create_function(|lua, (path, table): (String, Table)| {
            let encoded = with_trailing_newline(toml_encode(&table)?);
            defer_write(lua, &path, encoded)
        })?,
    )?;

    parent.set("toml", t)?;
    Ok(())
}

// ── INI (write-only) ──────────────────────────────────────────────
//
// The encoder is a minimal gitconfig emitter: two levels of nesting,
// no escaping. We don't ship a parser, so `decode` / `read` are not
// exposed.

fn ini_encode(table: &Table) -> LuaResult<String> {
    use std::fmt::Write;
    let mut out = String::new();

    for pair in table.clone().pairs::<String, Value>() {
        let (section, value) = pair?;
        let Value::Table(inner) = &value else {
            return Err(LuaError::RuntimeError(format!(
                "INI top-level key '{section}' must be a table"
            )));
        };

        for inner_pair in inner.clone().pairs::<String, Value>() {
            let (key, val) = inner_pair?;
            if let Value::Table(sub) = &val {
                if !out.is_empty() {
                    out.push('\n');
                }
                writeln!(out, "[{section} \"{key}\"]").unwrap();
                write_ini_section(sub, &mut out)?;
            }
        }

        let has_scalars = inner
            .clone()
            .pairs::<String, Value>()
            .any(|p| p.is_ok_and(|(_, v)| !matches!(v, Value::Table(_))));

        if has_scalars {
            if !out.is_empty() {
                out.push('\n');
            }
            writeln!(out, "[{section}]").unwrap();
            write_ini_section(inner, &mut out)?;
        }
    }

    Ok(out)
}

fn write_ini_section(table: &Table, out: &mut String) -> LuaResult<()> {
    use std::fmt::Write;
    for pair in table.clone().pairs::<String, Value>() {
        let (key, value) = pair?;
        let serialized = match &value {
            Value::Table(_) => continue,
            Value::Boolean(b) => b.to_string(),
            Value::Integer(n) => n.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.to_str()?.to_owned(),
            _ => {
                return Err(LuaError::RuntimeError(format!(
                    "cannot serialize {} to INI value",
                    value.type_name()
                )))
            }
        };

        writeln!(out, "\t{key} = {serialized}")
            .map_err(|e| LuaError::RuntimeError(format!("error writing INI entry '{key}': {e}")))?;
    }
    Ok(())
}

fn register_ini(lua: &Lua, parent: &Table) -> LuaResult<()> {
    let t = lua.create_table()?;

    t.set(
        "encode",
        lua.create_function(|_, table: Table| ini_encode(&table))?,
    )?;
    t.set(
        "write",
        lua.create_function(|lua, (path, table): (String, Table)| {
            let encoded = with_trailing_newline(ini_encode(&table)?);
            defer_write(lua, &path, encoded)
        })?,
    )?;

    parent.set("ini", t)?;
    Ok(())
}

// ── Entry point ───────────────────────────────────────────────────

pub(crate) fn register(lua: &Lua, table: &Table) -> LuaResult<()> {
    register_json(lua, table)?;
    register_toml(lua, table)?;
    register_ini(lua, table)?;
    Ok(())
}
