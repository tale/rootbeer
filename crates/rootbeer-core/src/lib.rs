mod executor;
mod lua;
mod plan;

pub use executor::{ExecutionReport, Mode, OpResult, Options};
#[cfg(feature = "embedded-stdlib")]
pub use lua::require::embedded_modules;
pub use plan::Op;

/// Returns the compile-time Lua standard library directory path.
pub fn lua_dir() -> PathBuf {
    PathBuf::from(env!("ROOTBEER_LUA_DIR"))
}

use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::{error, fs, io};

#[derive(Debug)]
pub struct Runtime {
    pub script_dir: PathBuf,
    pub script_name: String,
    pub lua_dir: PathBuf,
    pub profile: Option<String>,
}

impl Runtime {
    pub fn from_script(script: &Path) -> io::Result<Self> {
        let script = script.canonicalize()?;
        let script_dir = script
            .parent()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "script has no parent directory",
                )
            })?
            .to_path_buf();

        let script_name = script
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "script has no file name"))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            lua_dir: PathBuf::from(env!("ROOTBEER_LUA_DIR")),
            script_dir,
            script_name,
            profile: None,
        })
    }
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

/// Default path to the user's rootbeer script inside the config directory.
pub fn script_path() -> PathBuf {
    config_dir().join("init.lua")
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

pub fn execute(script: &Path, opts: Options) -> Result<ExecutionReport, Error> {
    let runtime = Runtime::from_script(script)?;
    execute_with(runtime, opts)
}

pub fn execute_with(runtime: Runtime, opts: Options) -> Result<ExecutionReport, Error> {
    let script_path = runtime.script_dir.join(&runtime.script_name);
    let source = fs::read_to_string(&script_path)?;
    let chunk_name = format!("@{}", script_path.with_extension("").display());

    let lua = lua::create_vm(runtime)?;
    lua.load(&source).set_name(&chunk_name).exec()?;

    let ops = lua
        .remove_app_data::<lua::Run>()
        .unwrap_or_default()
        .into_ops();

    let report = match opts.mode {
        Mode::Apply => executor::apply(&ops, opts.force)?,
        Mode::DryRun => executor::dry_run(&ops),
    };

    Ok(report)
}
