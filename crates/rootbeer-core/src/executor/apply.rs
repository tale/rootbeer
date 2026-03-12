use std::{fs, io, os::unix::fs as unix_fs, process};

use crate::{
    executor::{log_result, ExecutionReport, OpResult},
    Op,
};

pub fn apply(ops: &[Op], force: bool) -> io::Result<ExecutionReport> {
    let mut report = ExecutionReport::default();

    for op in ops {
        match op {
            Op::WriteFile { path, content } => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }

                fs::write(path, content)?;
                let result = OpResult::FileWritten {
                    path: path.clone(),
                    bytes: content.len(),
                };
                log_result(&result);
                report.results.push(result);
            }

            Op::Symlink { src, dst } => {
                let mut overwritten = false;

                if dst.is_symlink() {
                    if let Ok(target) = fs::read_link(dst) {
                        if target == *src {
                            let result = OpResult::SymlinkUnchanged { dst: dst.clone() };
                            log_result(&result);
                            report.results.push(result);
                            continue;
                        }
                        fs::remove_file(dst)?;
                    }
                } else if dst.exists() {
                    if force {
                        overwritten = true;
                        if dst.is_dir() {
                            fs::remove_dir_all(dst)?;
                        } else {
                            fs::remove_file(dst)?;
                        }
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            format!(
                                "target {} exists and is not a symlink (use --force to overwrite)",
                                dst.display()
                            ),
                        ));
                    }
                }

                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }

                unix_fs::symlink(src, dst)?;

                let result = if overwritten {
                    OpResult::SymlinkOverwritten {
                        src: src.clone(),
                        dst: dst.clone(),
                    }
                } else {
                    OpResult::SymlinkCreated {
                        src: src.clone(),
                        dst: dst.clone(),
                    }
                };
                log_result(&result);
                report.results.push(result);
            }

            Op::Exec { cmd, args, cwd } => {
                let status = process::Command::new(cmd)
                    .args(args)
                    .current_dir(cwd)
                    .stdin(process::Stdio::inherit())
                    .stdout(process::Stdio::inherit())
                    .stderr(process::Stdio::inherit())
                    .status()?;

                let display = std::iter::once(cmd.as_str())
                    .chain(args.iter().map(|s| s.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ");

                let result = OpResult::CommandRan {
                    cmd: display,
                    status: status.code().unwrap_or(1),
                };
                log_result(&result);
                report.results.push(result);
            }
        }
    }

    Ok(report)
}
