use mlua::{Error as LuaError, Lua, Result as LuaResult, Table, Value};

/// Poor man's INI serializer that only supports up to 2 levels of nesting and
/// doesn't really do any escaping. Pretty much only for some git config files.
fn lua_to_ini(table: &Table) -> Result<String, LuaError> {
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
            match &val {
                Value::Table(sub) => {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    writeln!(out, "[{section} \"{key}\"]").unwrap();
                    write_ini_section(sub, &mut out)?;
                }

                _ => {
                    // Collect all scalars under this section first pass
                }
            }
        }

        // Check if section has any scalar keys
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

/// Writes all scalar key-value pairs in the given table as INI entries.
fn write_ini_section(table: &Table, out: &mut String) -> Result<(), LuaError> {
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

        match writeln!(out, "\t{key} = {serialized}") {
            Ok(_) => {}
            Err(e) => {
                return Err(LuaError::RuntimeError(format!(
                    "error writing INI entry '{key}': {e}"
                )))
            }
        }
    }

    Ok(())
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

/// Recursively converts a Lua value into a `serde_json::Value`.
fn lua_to_json_value(value: &Value) -> Result<serde_json::Value, LuaError> {
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

fn lua_to_json(table: &Table) -> Result<String, LuaError> {
    let value = lua_to_json_value(&Value::Table(table.clone()))?;
    serde_json::to_string_pretty(&value)
        .map(|mut s| {
            s.push('\n');
            s
        })
        .map_err(|e| LuaError::RuntimeError(format!("JSON serialization failed: {e}")))
}

/// Recursively converts a Lua value into a `toml::Value`.
fn lua_to_toml_value(value: &Value) -> Result<toml::Value, LuaError> {
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

fn lua_to_toml(table: &Table) -> Result<String, LuaError> {
    let value = lua_to_toml_value(&Value::Table(table.clone()))?;
    let toml::Value::Table(map) = value else {
        return Err(LuaError::RuntimeError(
            "TOML top-level must be a table".into(),
        ));
    };

    toml::to_string(&map)
        .map_err(|e| LuaError::RuntimeError(format!("TOML serialization failed: {e}")))
}

pub(crate) fn register(lua: &Lua, table: &Table) -> LuaResult<()> {
    let encode = lua.create_table()?;

    encode.set(
        "ini",
        lua.create_function(|_, table: Table| lua_to_ini(&table))?,
    )?;

    encode.set(
        "json",
        lua.create_function(|_, table: Table| lua_to_json(&table))?,
    )?;

    encode.set(
        "toml",
        lua.create_function(|_, table: Table| lua_to_toml(&table))?,
    )?;

    table.set("encode", encode)?;
    Ok(())
}
