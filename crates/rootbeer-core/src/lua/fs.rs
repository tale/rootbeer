use mlua::{Error as LuaError, Lua, Result as LuaResult, Table};

use super::ctx::Ctx;
use super::module::Module;
use crate::plan::Op;

pub(crate) struct Fs;

impl Module for Fs {
    const NAME: &'static str = "";

    fn build(lua: &Lua, t: &Table) -> LuaResult<()> {
        t.set(
            "file",
            lua.create_function(|lua, (path, content): (String, String)| {
                Ctx::from(lua).write(&path, content);
                Ok(())
            })?,
        )?;

        t.set(
            "link_file",
            lua.create_function(|lua, (src, dest): (String, String)| {
                let cx = Ctx::from(lua);
                let resolved = cx.source(&src);
                let canonical = cx.canonicalize("link_file source", &src, &resolved)?;
                cx.push(Op::Symlink {
                    src: canonical,
                    dst: cx.resolve(&dest),
                });
                Ok(())
            })?,
        )?;

        t.set(
            "copy_file",
            lua.create_function(|lua, (src, dest): (String, String)| {
                let cx = Ctx::from(lua);
                let resolved = cx.source(&src);
                let canonical = cx.canonicalize("copy_file source", &src, &resolved)?;
                cx.push(Op::CopyFileIfMissing {
                    src: canonical,
                    dst: cx.resolve(&dest),
                });
                Ok(())
            })?,
        )?;

        t.set(
            "link",
            lua.create_function(|lua, (src, dest): (String, String)| {
                let cx = Ctx::from(lua);
                let resolved_src = cx.resolve(&src);
                if !resolved_src.exists() {
                    return Err(LuaError::RuntimeError(format!(
                        "link source '{}' (resolved to '{}'): not found",
                        src,
                        resolved_src.display(),
                    )));
                }
                cx.push(Op::Symlink {
                    src: resolved_src,
                    dst: cx.resolve(&dest),
                });
                Ok(())
            })?,
        )?;

        t.set(
            "path_exists",
            lua.create_function(|lua, path: String| Ok(Ctx::from(lua).resolve(&path).exists()))?,
        )?;

        t.set(
            "is_file",
            lua.create_function(|lua, path: String| Ok(Ctx::from(lua).resolve(&path).is_file()))?,
        )?;

        t.set(
            "is_dir",
            lua.create_function(|lua, path: String| Ok(Ctx::from(lua).resolve(&path).is_dir()))?,
        )?;

        t.set(
            "exec",
            lua.create_function(|lua, (cmd, args): (String, Option<Vec<String>>)| {
                let cx = Ctx::from(lua);
                cx.push(Op::Exec {
                    cmd,
                    args: args.unwrap_or_default(),
                    cwd: cx.runtime.script_dir.clone(),
                });
                Ok(())
            })?,
        )?;

        t.set(
            "remote",
            lua.create_function(|lua, url: String| {
                let cx = Ctx::from(lua);
                cx.push(Op::SetRemoteUrl {
                    dir: cx.runtime.script_dir.clone(),
                    url,
                });
                Ok(())
            })?,
        )?;

        Ok(())
    }
}
