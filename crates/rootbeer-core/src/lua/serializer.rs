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

pub(crate) fn register(lua: &Lua, table: &Table) -> LuaResult<()> {
    let encode = lua.create_table()?;

    encode.set(
        "ini",
        lua.create_function(|_, table: Table| lua_to_ini(&table))?,
    )?;

    table.set("encode", encode)?;
    Ok(())
}
