//! Writer utilities for data formats and scripts.
//!
//! Each format implements a `Codec` trait by providing `encode`/`decode`
//! over a Serde Value (makes it easy to implement new formats).
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

mod codec;
mod json;
mod plist;
mod scripts;
mod toml;
mod yaml;

use mlua::{Lua, Result as LuaResult, Table};

use super::module::Module;

pub(crate) struct Writer;

impl Module for Writer {
    const NAME: &'static str = "";

    fn build(lua: &Lua, t: &Table) -> LuaResult<()> {
        codec::register::<json::Json>(lua, t)?;
        codec::register::<toml::Toml>(lua, t)?;
        codec::register::<yaml::Yaml>(lua, t)?;
        codec::register::<plist::Plist>(lua, t)?;
        scripts::register(lua, t)?;
        Ok(())
    }
}
