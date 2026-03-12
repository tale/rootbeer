mod executor;
mod lua;
mod pipeline;
mod plan;

pub use executor::{ExecutionReport, OpResult};
pub use pipeline::{Mode, Options, Pipeline};
pub use plan::Op;

#[cfg(feature = "embedded-stdlib")]
pub use lua::require::embedded_modules;

/// Returns the compile-time Lua standard library directory path.
pub fn lua_dir() -> PathBuf {
    PathBuf::from(env!("ROOTBEER_LUA_DIR"))
}

use std::fmt::Display;
use std::path::PathBuf;
use std::{error, io};

#[derive(Debug)]
pub(crate) struct Runtime {
    pub script_dir: PathBuf,
    pub script_name: String,
    pub lua_dir: PathBuf,
    pub profile: Option<String>,
}

fn xdg_dir(env_var: &str, fallback: &str) -> PathBuf {
    if let Some(val) = std::env::var_os(env_var) {
        PathBuf::from(val).join("rootbeer")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(fallback).join("rootbeer")
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

/// User configuration directory (`~/.config/rootbeer`).
/// The user's dotfiles/lua scripts live here.
pub fn config_dir() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config")
}

/// State directory (`~/.local/state/rootbeer`).
/// Revisions, operation history, and other persistent runtime state.
pub fn state_dir() -> PathBuf {
    xdg_dir("XDG_STATE_HOME", ".local/state")
}

/// Data directory (`~/.local/share/rootbeer`).
/// Type definitions, extracted stdlib, and other shared data.
pub fn data_dir() -> PathBuf {
    xdg_dir("XDG_DATA_HOME", ".local/share")
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Lua(mlua::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Lua(e) => write!(f, "{e}"),
        }
    }
}

impl error::Error for Error {}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<mlua::Error> for Error {
    fn from(e: mlua::Error) -> Self {
        Error::Lua(e)
    }
}
