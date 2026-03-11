use crate::{
    executor::{log_result, ExecutionReport, OpResult},
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
                let result = OpResult::FileWritten {
                    path: path.clone(),
                    bytes: content.len(),
                };
                log_result(&result);
                report.results.push(result);
            }

            Op::Symlink { src, dst } => {
                let result = OpResult::SymlinkCreated {
                    src: src.clone(),
                    dst: dst.clone(),
                };
                log_result(&result);
                report.results.push(result);
            }

            Op::Exec { cmd, args } => {
                let display = std::iter::once(cmd.as_str())
                    .chain(args.iter().map(|s| s.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ");

                let result = OpResult::CommandRan {
                    cmd: display,
                    status: 0,
                };
                log_result(&result);
                report.results.push(result);
            }
        }
    }

    report
}
