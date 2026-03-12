use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::executor::{self, ExecutionReport};
use crate::plan::Op;
use crate::{Error, Runtime};

#[derive(Debug, Default, Clone, Copy)]
pub enum Mode {
    #[default]
    Apply,
    DryRun,
}

impl Display for Mode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Apply => write!(f, "apply"),
            Mode::DryRun => write!(f, "dry run"),
        }
    }
}

#[derive(Debug)]
pub struct Options {
    pub script_name: String,
    pub lua_dir: PathBuf,
    pub profile: Option<String>,
    pub mode: Mode,
    pub force: bool,
}

impl Options {
    pub fn from_script(script: &Path) -> io::Result<Self> {
        let script = script.canonicalize()?;

        let script_name = script
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "script has no file name"))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            lua_dir: PathBuf::from(env!("ROOTBEER_LUA_DIR")),
            script_name,
            profile: None,
            mode: Mode::default(),
            force: false,
        })
    }
}

/// The main entrypoint to interact with the rootbeer core library. A `Pipeline`
/// is configured with a set of options and will correctly handle the entire
/// process end-to-end:
///
/// 1. Evaluating the Lua script and building a list of operations
/// 2. Executing the operations based on the configured mode
/// 3. Returning a report of the execution results
/// 4. (future) Managing revisions, rollbacks, and other stateful features
pub struct Pipeline {
    opts: Options,
}

impl Pipeline {
    pub fn new(opts: Options) -> Self {
        Self { opts }
    }

    pub fn mode(&self) -> Mode {
        self.opts.mode
    }

    pub fn force(&self) -> bool {
        self.opts.force
    }

    pub fn plan(&self) -> Result<Vec<Op>, Error> {
        let runtime = Runtime {
            script_dir: crate::config_dir(),
            script_name: self.opts.script_name.clone(),
            lua_dir: self.opts.lua_dir.clone(),
            profile: self.opts.profile.clone(),
        };

        let script_path = runtime.script_dir.join(&runtime.script_name);
        let source = fs::read_to_string(&script_path)?;
        let chunk_name = format!("@{}", script_path.with_extension("").display());

        let lua = crate::lua::create_vm(runtime)?;
        lua.load(&source).set_name(&chunk_name).exec()?;

        let ops = lua
            .remove_app_data::<crate::lua::Run>()
            .unwrap_or_default()
            .into_ops();

        Ok(ops)
    }

    pub fn execute(&self, ops: &[Op]) -> Result<ExecutionReport, Error> {
        let report = match self.opts.mode {
            Mode::Apply => executor::apply(ops, self.opts.force)?,
            Mode::DryRun => executor::dry_run(ops),
        };
        Ok(report)
    }

    pub fn run(&self) -> Result<ExecutionReport, Error> {
        let ops = self.plan()?;
        self.execute(&ops)
    }
}
