mod executor;
mod lua;
mod plan;

pub use executor::{ExecutionReport, Mode, OpResult};
pub use plan::Op;

use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::{error, fs, io};

#[derive(Debug)]
pub struct Runtime {
    pub script_dir: PathBuf,
    pub script_name: String,
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
            script_dir,
            script_name,
        })
    }

    pub fn default_dir() -> PathBuf {
        if let Some(xdg_data_home) = std::env::var_os("XDG_DATA_HOME") {
            PathBuf::from(xdg_data_home).join("rootbeer")
        } else if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home).join(".local/share/rootbeer")
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        }
    }
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

pub fn execute(script: &Path, mode: Mode) -> Result<ExecutionReport, Error> {
    let runtime = Runtime::from_script(&script)?;
    let run = Arc::new(Mutex::new(lua::Run::default()));

    let source = fs::read_to_string(&script)?;
    let script_name = runtime.script_name.clone();

    lua::create_vm(runtime)?
        .load(&source)
        .set_name(&script_name)
        .exec()?;

    let ops = std::mem::take(&mut run.lock().unwrap().ops);
    let report = match mode {
        Mode::Apply => executor::apply(&ops)?,
        Mode::DryRun => executor::dry_run(&ops),
    };

    Ok(report)
}
