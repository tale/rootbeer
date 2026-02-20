use mlua::{Lua, Result as LuaResult, Table};

/// This is bad, but I'm lazy and it works for now.
fn get_hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

pub(crate) fn register(lua: &Lua, table: &Table) -> LuaResult<()> {
    table.set(
        "data",
        lua.create_function(|lua, ()| {
            let t = lua.create_table()?;
            t.set("os", std::env::consts::OS)?;
            t.set("arch", std::env::consts::ARCH)?;
            t.set("home", std::env::var("HOME").unwrap_or_default())?;
            t.set("username", std::env::var("USER").unwrap_or_default())?;
            t.set("hostname", get_hostname())?;
            Ok(t)
        })?,
    )?;

    Ok(())
}
