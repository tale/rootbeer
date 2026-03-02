use mlua::{Lua, Result as LuaResult, Table};
use std::process::Command;

/// Reads a secret from 1Password via the `op` CLI.
/// The reference should be in `op://` format (e.g. `op://vault/item/field`).
fn read_op_secret(reference: &str) -> Result<String, mlua::Error> {
    let output = Command::new("op")
        .args(["read", "--no-newline", reference])
        .output()
        .map_err(|e| mlua::Error::RuntimeError(format!("failed to run `op`: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(mlua::Error::RuntimeError(format!(
            "op read failed ({}): {stderr}",
            output.status
        )));
    }

    String::from_utf8(output.stdout)
        .map_err(|e| mlua::Error::RuntimeError(format!("op returned invalid UTF-8: {e}")))
}

pub(crate) fn register(lua: &Lua, table: &Table) -> LuaResult<()> {
    let secret = lua.create_table()?;

    secret.set(
        "op",
        lua.create_function(|_, reference: String| read_op_secret(&reference))?,
    )?;

    table.set("secret", secret)?;
    Ok(())
}
