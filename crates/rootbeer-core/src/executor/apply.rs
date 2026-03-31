use std::io::{self, BufRead, BufReader};
use std::{fs, os::unix::fs as unix_fs, process, process::Command, thread};

use crate::{
    executor::{ExecutionHandler, ExecutionReport, OpResult},
    Op,
};

pub fn apply(
    ops: &[Op],
    force: bool,
    handler: &mut impl ExecutionHandler,
) -> io::Result<ExecutionReport> {
    let mut report = ExecutionReport::default();

    for op in ops {
        handler.on_start(op);

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

                handler.on_result(&result);
                report.results.push(result);
            }

            Op::Symlink { src, dst } => {
                let mut overwritten = false;

                if dst.is_symlink() {
                    if let Ok(target) = fs::read_link(dst) {
                        if target == *src {
                            let result = OpResult::SymlinkUnchanged { dst: dst.clone() };
                            handler.on_result(&result);
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
                handler.on_result(&result);
                report.results.push(result);
            }

            Op::Exec { cmd, args, cwd } => {
                let display = std::iter::once(cmd.as_str())
                    .chain(args.iter().map(|s| s.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ");

                let mut child = process::Command::new(cmd)
                    .args(args)
                    .current_dir(cwd)
                    .stdin(process::Stdio::inherit())
                    .stdout(process::Stdio::piped())
                    .stderr(process::Stdio::piped())
                    .spawn()?;

                let stderr = child.stderr.take().unwrap();
                let stderr_lines = thread::spawn(move || {
                    BufReader::new(stderr)
                        .lines()
                        .collect::<Result<Vec<_>, _>>()
                });

                let stdout = child.stdout.take().unwrap();
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    handler.on_output(&line);
                }

                if let Ok(Ok(lines)) = stderr_lines.join() {
                    for line in lines {
                        handler.on_output(&line);
                    }
                }

                let status = child.wait()?;

                let result = OpResult::CommandRan {
                    cmd: display,
                    status: status.code().unwrap_or(1),
                };
                handler.on_result(&result);
                report.results.push(result);
            }

            Op::SetRemoteUrl { dir, url } => {
                let current = Command::new("git")
                    .args(["-C", &dir.to_string_lossy(), "remote", "get-url", "origin"])
                    .output()
                    .map_err(|e| io::Error::other(format!("git: {e}")))?;

                if !current.status.success() {
                    return Err(io::Error::other(
                        "failed to get origin URL; is the source directory a git repo?",
                    ));
                }

                let current_url = String::from_utf8_lossy(&current.stdout).trim().to_string();

                if *url == current_url {
                    let result = OpResult::RemoteUnchanged { url: current_url };
                    handler.on_result(&result);
                    report.results.push(result);
                } else {
                    let status = Command::new("git")
                        .args([
                            "-C",
                            &dir.to_string_lossy(),
                            "remote",
                            "set-url",
                            "origin",
                            url,
                        ])
                        .status()
                        .map_err(|e| io::Error::other(format!("git: {e}")))?;

                    if !status.success() {
                        return Err(io::Error::other("failed to set origin URL"));
                    }

                    let result = OpResult::RemoteUpdated {
                        from: current_url,
                        to: url.clone(),
                    };
                    handler.on_result(&result);
                    report.results.push(result);
                }
            }
        }
    }

    Ok(report)
}
