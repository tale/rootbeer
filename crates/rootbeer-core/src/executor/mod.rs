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
}

#[derive(Debug, Default)]
pub struct ExecutionReport {
    pub mode: Mode,
    pub results: Vec<OpResult>,
}

pub use apply::apply;
pub use dry_run::dry_run;
