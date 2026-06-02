use mlua::{Lua, Result as LuaResult, Table};
use std::process::Command;

use super::ctx::Ctx;
use super::module::Module;
use crate::plan::{Op, WriteSource};

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

pub(crate) struct Secret;

impl Module for Secret {
    const NAME: &'static str = "secret";

    fn build(lua: &Lua, t: &Table) -> LuaResult<()> {
        t.set(
            "op",
            lua.create_function(|_, reference: String| read_op_secret(&reference))?,
        )?;

        t.set(
            "op_document",
            lua.create_function(
                |lua, (reference, dest, opts): (String, String, Option<Table>)| {
                    let cx = Ctx::from(lua);
                    let resolved = cx.resolve(&dest);
                    cx.push(Op::WriteFile {
                        path: resolved.clone(),
                        source: WriteSource::OpDocument { reference },
                    });

                    if let Some(opts) = opts {
                        if let Ok(mode) = opts.get::<u32>("mode") {
                            cx.chmod(&resolved, mode);
                        }
                    }

                    Ok(())
                },
            )?,
        )?;

        Ok(())
    }
}
