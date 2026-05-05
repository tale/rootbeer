//! Script writers (`rb.scripts.*`).
//!
//! This is basically glue over `rb.file()` for executable scripts. Each helper
//! writes a file with the appropriate shebang (`#!/usr/bin/env <interp>`) and
//! queues a `chmod +x` via the execution plan.
//!
//! TODO: I'd like this to have some linting/validation features eventually, but
//! this is after I'm able to create a content addressable packaging environment
//! so we can bring in the dependencies.
//!

use mlua::{Lua, Result as LuaResult, Table};

use crate::lua::ctx::Ctx;

/// Each `(method_name, interpreter)` pair becomes a flat helper on the
/// `scripts` table that produces the end `rb.scripts.<method_name>` API.
const NAMED_SCRIPT_WRITERS: &[(&str, &str)] = &[
    ("bash", "bash"),
    ("sh", "sh"),
    ("zsh", "zsh"),
    ("fish", "fish"),
    ("python", "python3"),
    ("node", "node"),
    ("lua", "lua"),
    ("nu", "nu"),
    ("ruby", "ruby"),
    ("perl", "perl"),
];

fn write_script(lua: &Lua, interpreter: &str, path: &str, body: &str) -> LuaResult<()> {
    let mut content = match interpreter.starts_with('/') {
        true => format!("#!{interpreter}\n\n"),
        false => format!("#!/usr/bin/env {interpreter}\n\n"),
    };

    content.push_str(body);
    if !content.ends_with('\n') {
        content.push('\n');
    }

    let cx = Ctx::from(lua);
    let resolved = cx.resolve(path);
    cx.write(path, content);
    cx.chmod(&resolved, 0o755);
    Ok(())
}

pub(super) fn register(lua: &Lua, parent: &Table) -> LuaResult<()> {
    let scripts = lua.create_table()?;

    // Our generic function if the user wants to specify their own interpreter
    scripts.set(
        "script",
        lua.create_function(|lua, (interpreter, path, body): (String, String, String)| {
            write_script(lua, &interpreter, &path, &body)
        })?,
    )?;

    for (method, interp) in NAMED_SCRIPT_WRITERS {
        let interp = (*interp).to_owned();
        scripts.set(
            *method,
            lua.create_function(move |lua, (path, body): (String, String)| {
                write_script(lua, &interp, &path, &body)
            })?,
        )?;
    }

    parent.set("scripts", scripts)?;
    Ok(())
}
