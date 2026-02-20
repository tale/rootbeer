use crate::{
    executor::{ExecutionReport, OpResult},
    Mode, Op,
};

pub fn dry_run(ops: &[Op]) -> ExecutionReport {
    let mut report = ExecutionReport {
        mode: Mode::DryRun,
        ..Default::default()
    };

    for op in ops {
        match op {
            Op::WriteFile { path, content } => {
                report.results.push(OpResult::FileWritten {
                    path: path.clone(),
                    bytes: content.len(),
                });
            }

            Op::Symlink { src, dst } => {
                report.results.push(OpResult::SymlinkCreated {
                    src: src.clone(),
                    dst: dst.clone(),
                });
            }
        }
    }

    report
}
