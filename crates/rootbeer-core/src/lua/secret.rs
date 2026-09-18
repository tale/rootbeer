use mlua::{Lua, Result as LuaResult, Table};

use super::ctx::Ctx;
use super::module::Module;
use crate::plan::{AgeIdentity, Op, WriteSource};

fn age_identity(cx: &Ctx<'_>, opts: &Table) -> LuaResult<AgeIdentity> {
    let path = opts.get::<Option<String>>("identity")?;
    let reference = opts.get::<Option<String>>("identity_op")?;
    match (path, reference) {
        (Some(path), None) => Ok(AgeIdentity::File(cx.resolve(&path))),
        (None, Some(reference)) if reference.starts_with("op://") => {
            Ok(AgeIdentity::OnePassword(reference))
        }
        _ => Err(mlua::Error::RuntimeError(
            "provide exactly one of identity (key file) or identity_op (op:// reference)".into(),
        )),
    }
}

pub(crate) struct Secret;

impl Module for Secret {
    const NAME: &'static str = "secret";

    fn build(lua: &Lua, t: &Table) -> LuaResult<()> {
        t.set(
            "age",
            lua.create_function(|lua, (path, opts): (String, Table)| {
                let cx = Ctx::from(lua);
                let identity = age_identity(&cx, &opts)?;
                let bytes = crate::age::decrypt(&cx.resolve(&path), &identity, &cx.runtime.tools)
                    .map_err(mlua::Error::external)?;
                String::from_utf8(bytes).map_err(|_| {
                    mlua::Error::RuntimeError(
                        "age plaintext is not UTF-8; use age_file for binary files".into(),
                    )
                })
            })?,
        )?;

        t.set(
            "age_file",
            lua.create_function(|lua, (path, dest, opts): (String, String, Table)| {
                let cx = Ctx::from(lua);
                let identity = age_identity(&cx, &opts)?;
                let mode = opts.get::<Option<u32>>("mode")?.unwrap_or(0o600);
                if mode > 0o777 {
                    return Err(mlua::Error::RuntimeError(
                        "age file mode must be between 0 and 0777".into(),
                    ));
                }
                cx.push(Op::WriteFile {
                    path: cx.resolve(&dest),
                    source: WriteSource::AgeFile {
                        path: cx.resolve(&path),
                        identity,
                        mode,
                    },
                });
                Ok(())
            })?,
        )?;

        t.set(
            "op",
            lua.create_function(|lua, reference: String| {
                crate::one_password::read(&Ctx::from(lua).runtime.tools, &reference)
                    .map_err(mlua::Error::external)
            })?,
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
