use crate::{
    executor::{ExecutionHandler, ExecutionReport, OpResult},
    Op,
};

pub fn dry_run(ops: &[Op], handler: &mut impl ExecutionHandler) -> ExecutionReport {
    let mut report = ExecutionReport::default();

    for op in ops {
        handler.on_start(op);

        match op {
            Op::WriteFile { path, content } => {
                let result = OpResult::FileWritten {
                    path: path.clone(),
                    bytes: content.len(),
                };
                handler.on_result(&result);
                report.results.push(result);
            }

            Op::Symlink { src, dst } => {
                let result = OpResult::SymlinkCreated {
                    src: src.clone(),
                    dst: dst.clone(),
                };
                handler.on_result(&result);
                report.results.push(result);
            }

            Op::Exec { cmd, args, .. } => {
                let display = std::iter::once(cmd.as_str())
                    .chain(args.iter().map(|s| s.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ");

                let result = OpResult::CommandRan {
                    cmd: display,
                    status: 0,
                };
                handler.on_result(&result);
                report.results.push(result);
            }
        }
    }

    report
}
