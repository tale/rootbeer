use std::{
    fmt::{Display, Formatter, Result},
    path::PathBuf,
};

mod apply;
mod dry_run;

#[derive(Debug, Default)]
pub enum Mode {
    #[default]
    Apply,
    DryRun,
}

#[derive(Debug, Default)]
pub struct Options {
    pub mode: Mode,
    pub force: bool,
}

impl Display for Mode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Mode::Apply => write!(f, "apply"),
            Mode::DryRun => write!(f, "dry run"),
        }
    }
}

#[derive(Debug)]
pub enum OpResult {
    FileWritten { path: PathBuf, bytes: usize },
    SymlinkCreated { src: PathBuf, dst: PathBuf },
    SymlinkUnchanged { dst: PathBuf },
    SymlinkOverwritten { src: PathBuf, dst: PathBuf },
    CommandRan { cmd: String, status: i32 },
}

#[derive(Debug, Default)]
pub struct ExecutionReport {
    pub mode: Mode,
    pub results: Vec<OpResult>,
}

pub use apply::apply;
pub use dry_run::dry_run;

pub(crate) fn log_result(result: &OpResult) {
    match result {
        OpResult::FileWritten { path, bytes } => {
            eprintln!("  write {} ({bytes} bytes)", path.display());
        }
        OpResult::SymlinkCreated { src, dst } => {
            eprintln!("  link {} -> {}", dst.display(), src.display());
        }
        OpResult::SymlinkUnchanged { dst } => {
            eprintln!("  skip {} (unchanged)", dst.display());
        }
        OpResult::SymlinkOverwritten { src, dst } => {
            eprintln!("  force {} -> {}", dst.display(), src.display());
        }
        OpResult::CommandRan { cmd, status } => {
            eprintln!("  exec `{cmd}` (exit {status})");
        }
    }
}
