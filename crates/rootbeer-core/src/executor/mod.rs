use std::path::PathBuf;

mod apply;
mod dry_run;

use crate::plan::Op;

#[derive(Debug, Clone)]
pub enum OpResult {
    FileWritten { path: PathBuf, bytes: usize },
    SymlinkCreated { src: PathBuf, dst: PathBuf },
    SymlinkUnchanged { dst: PathBuf },
    SymlinkOverwritten { src: PathBuf, dst: PathBuf },
    CommandRan { cmd: String, status: i32 },
}

/// Receives lifecycle events during pipeline execution.
pub trait ExecutionHandler {
    /// An operation is about to be executed.
    fn on_start(&mut self, op: &Op);
    /// A line of stdout/stderr output from an exec command.
    fn on_output(&mut self, line: &str);
    /// An operation completed.
    fn on_result(&mut self, result: &OpResult);
}

#[derive(Debug, Default)]
pub struct ExecutionReport {
    pub results: Vec<OpResult>,
}

pub use apply::apply;
pub use dry_run::dry_run;
