use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::executor::{self, ExecutionHandler, ExecutionReport};
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
    pub script_dir: PathBuf,
    pub script_name: String,
    pub lua_dir: PathBuf,
    pub profile: Option<String>,
    pub mode: Mode,
    pub force: bool,
}

impl Options {
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
            mode: Mode::default(),
            force: false,
        })
    }
}

/// Entry point: configure the pipeline, then call `.plan()` to evaluate Lua.
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

    /// Evaluate the Lua script and advance to the planned phase.
    pub fn plan(self) -> Result<PlannedPipeline, Error> {
        let runtime = Runtime {
            script_dir: self.opts.script_dir.clone(),
            script_name: self.opts.script_name.clone(),
            lua_dir: self.opts.lua_dir.clone(),
            profile: self.opts.profile.clone(),
        };

        let source = fs::read_to_string(runtime.script_dir.join(&runtime.script_name))?;
        let chunk_name = format!(
            "@{}",
            runtime.script_dir.join(&runtime.script_name).display()
        );
        let lua = crate::lua::create_vm(runtime)?;
        if let Err(e) = lua.load(&source).set_name(&chunk_name).exec() {
            return Err(parse_profile_error(&e).unwrap_or_else(|| e.into()));
        }

        let ops = lua
            .remove_app_data::<crate::lua::Run>()
            .unwrap_or_default()
            .into_ops();

        Ok(PlannedPipeline {
            opts: self.opts,
            ops,
        })
    }
}

const PROFILE_SENTINEL: &str = "__rb_profile_required:";

/// Check if a Lua error contains a structured profile-required sentinel
/// and convert it to `Error::ProfileRequired`.
fn parse_profile_error(e: &mlua::Error) -> Option<Error> {
    // The sentinel may be wrapped in CallbackError, RuntimeError, etc.
    // Search the full error string for the sentinel line.
    let full = e.to_string();
    let sentinel_start = full.find(PROFILE_SENTINEL)?;
    let rest = &full[sentinel_start + PROFILE_SENTINEL.len()..];
    let first_line = rest.split('\n').next()?;
    let (active, profiles_str) = first_line.split_once(':')?;
    let active = if active.is_empty() {
        None
    } else {
        Some(active.to_string())
    };
    let profiles = profiles_str
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    Some(Error::ProfileRequired { active, profiles })
}

/// A pipeline that has been planned — ops are collected, ready to execute.
pub struct PlannedPipeline {
    opts: Options,
    ops: Vec<Op>,
}

impl PlannedPipeline {
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    pub fn mode(&self) -> Mode {
        self.opts.mode
    }

    pub fn force(&self) -> bool {
        self.opts.force
    }

    /// Execute the planned operations, reporting progress to the handler.
    pub fn execute(self, handler: &mut impl ExecutionHandler) -> Result<ExecutionReport, Error> {
        let report = match self.opts.mode {
            Mode::Apply => executor::apply(&self.ops, self.opts.force, handler)?,
            Mode::DryRun => executor::dry_run(&self.ops, handler),
        };

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unknown_profile_with_active_name() {
        let lua_err = mlua::Error::RuntimeError(
            "__rb_profile_required:work:personal,work\nstack traceback".into(),
        );
        let parsed = parse_profile_error(&lua_err).expect("should parse");
        match parsed {
            Error::ProfileRequired { active, profiles } => {
                assert_eq!(active.as_deref(), Some("work"));
                assert_eq!(profiles, vec!["personal".to_string(), "work".into()]);
            }
            _ => panic!("expected ProfileRequired"),
        }
    }

    #[test]
    fn parses_missing_profile_with_no_active_name() {
        let lua_err = mlua::Error::RuntimeError("__rb_profile_required::personal,work".into());
        let parsed = parse_profile_error(&lua_err).expect("should parse");
        match parsed {
            Error::ProfileRequired { active, profiles } => {
                assert_eq!(active, None);
                assert_eq!(profiles, vec!["personal".to_string(), "work".into()]);
            }
            _ => panic!("expected ProfileRequired"),
        }
    }

    #[test]
    fn returns_none_when_sentinel_absent() {
        let lua_err = mlua::Error::RuntimeError("plain error".into());
        assert!(parse_profile_error(&lua_err).is_none());
    }

    #[test]
    fn parses_when_sentinel_is_embedded_in_callback_error() {
        // The real error is wrapped with a "runtime error: ..." prefix
        // and a stack traceback; the parser must still find it.
        let lua_err = mlua::Error::RuntimeError(
            "runtime error: [string \"@\"]:3: __rb_profile_required:foo:a,b\nstack traceback:\n  ..."
                .to_string(),
        );
        let parsed = parse_profile_error(&lua_err).expect("should parse");
        match parsed {
            Error::ProfileRequired { active, profiles } => {
                assert_eq!(active.as_deref(), Some("foo"));
                assert_eq!(profiles, vec!["a".to_string(), "b".into()]);
            }
            _ => panic!("expected ProfileRequired"),
        }
    }
}
