use std::{fs, io, os::unix::fs as unix_fs};

use crate::{
    executor::{ExecutionReport, OpResult},
    Mode, Op,
};

pub fn apply(ops: &[Op]) -> io::Result<ExecutionReport> {
    let mut report = ExecutionReport::default();
    report.mode = Mode::Apply;

    for op in ops {
        match op {
            Op::WriteFile { path, content } => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }

                fs::write(path, content)?;
                report.results.push(OpResult::FileWritten {
                    path: path.clone(),
                    bytes: content.len(),
                });
            }

            Op::Symlink { src, dst } => {
                if dst.is_symlink() {
                    if let Ok(target) = fs::read_link(dst) {
                        if target == *src {
                            report
                                .results
                                .push(OpResult::SymlinkUnchanged { dst: dst.clone() });
                            continue;
                        }
                        fs::remove_file(dst)?;
                    }
                } else if dst.exists() {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("target {} exists and is not a symlink", dst.display()),
                    ));
                }

                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }

                unix_fs::symlink(src, dst)?;
                report.results.push(OpResult::SymlinkCreated {
                    src: src.clone(),
                    dst: dst.clone(),
                });
            }
        }
    }

    Ok(report)
}
