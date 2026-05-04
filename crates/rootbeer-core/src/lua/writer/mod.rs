//! Writer utilities for data formats and scripts.
//!
//! Each format implements a `Codec` trait by providing `encode`/`decode`
//! over a Serde Value (makes it easy to implement new formats). The codec
//! exposes a register method to easily wire it up to the `rb` table.
//!
//! ```text
//! rb.<fmt>.encode(t)        — table -> string
//! rb.<fmt>.decode(s)        — string -> table
//! rb.<fmt>.read(path)       — path  -> table (consume and decode)
//! rb.<fmt>.write(path, t)   — path, table -> (encode and write)
//! ```
//!
//! `read` is synchronous (the script needs the value immediately). `write`
//! defers via to the planning architecture of Rootbeer and only runs during
//! the execution phase.
//!

mod codec;
mod json;
mod plist;
mod scripts;
mod toml;
mod yaml;

use mlua::{Lua, Result as LuaResult, Table};

pub(crate) fn register(lua: &Lua, rb: &Table) -> LuaResult<()> {
    codec::register::<json::Json>(lua, rb)?;
    codec::register::<toml::Toml>(lua, rb)?;
    codec::register::<yaml::Yaml>(lua, rb)?;
    codec::register::<plist::Plist>(lua, rb)?;
    scripts::register(lua, rb)?;
    Ok(())
}
