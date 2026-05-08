use std::io::{self, BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::{fs, os::unix::fs as unix_fs, process, process::Command, thread};

use crate::{
    executor::{ExecutionHandler, ExecutionReport, OpResult},
    package::PackageRealizer,
    Op,
};

pub fn apply(
    ops: &[Op],
    force: bool,
    handler: &mut impl ExecutionHandler,
) -> io::Result<ExecutionReport> {
    apply_with_package_realizer(ops, force, handler, &PackageRealizer::default())
}

fn apply_with_package_realizer(
    ops: &[Op],
    force: bool,
    handler: &mut impl ExecutionHandler,
    package_realizer: &PackageRealizer,
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

            Op::CopyFileIfMissing { src, dst } => {
                if dst.exists() || dst.is_symlink() {
                    let result = OpResult::FileCopySkipped { dst: dst.clone() };
                    handler.on_result(&result);
                    report.results.push(result);
                    continue;
                }

                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }

                fs::copy(src, dst)?;

                let result = OpResult::FileCopied {
                    src: src.clone(),
                    dst: dst.clone(),
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

            Op::Chmod { path, mode } => {
                let perms = fs::Permissions::from_mode(*mode);
                fs::set_permissions(path, perms)?;
                let result = OpResult::Chmodded {
                    path: path.clone(),
                    mode: *mode,
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

            Op::RealizePackage { package } => {
                let realized = package_realizer.realize(package)?;
                let result = OpResult::PackageRealized {
                    name: package.name.clone(),
                    version: package.version.clone(),
                    store_path: Some(realized.store_entry.path),
                };
                handler.on_result(&result);
                report.results.push(result);
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::{ArchiveFormat, LockedInstall, LockedPackage, LockedSource, Provides};
    use crate::store::{hash_file, Store};
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[derive(Default)]
    struct Recorder {
        results: Vec<OpResult>,
    }

    impl ExecutionHandler for Recorder {
        fn on_start(&mut self, _: &Op) {}
        fn on_output(&mut self, _: &str) {}
        fn on_result(&mut self, r: &OpResult) {
            self.results.push(r.clone());
        }
    }

    #[test]
    fn write_file_creates_parent_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a/b/c.txt");
        let ops = vec![Op::WriteFile {
            path: path.clone(),
            content: "hello\n".into(),
        }];

        let mut h = Recorder::default();
        apply(&ops, false, &mut h).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "hello\n");
        assert!(matches!(
            &h.results[0],
            OpResult::FileWritten { bytes: 6, .. }
        ));
    }

    #[test]
    fn symlink_create_idempotent_skips_when_target_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.txt");
        let dst = tmp.path().join("dst.txt");
        fs::write(&src, "x").unwrap();
        unix_fs::symlink(&src, &dst).unwrap();

        let ops = vec![Op::Symlink {
            src: src.clone(),
            dst: dst.clone(),
        }];

        let mut h = Recorder::default();
        apply(&ops, false, &mut h).unwrap();

        assert!(matches!(&h.results[0], OpResult::SymlinkUnchanged { .. }));
    }

    #[test]
    fn symlink_replaces_stale_link() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real.txt");
        let stale = tmp.path().join("stale.txt");
        let dst = tmp.path().join("dst.txt");
        fs::write(&real, "x").unwrap();
        fs::write(&stale, "y").unwrap();
        unix_fs::symlink(&stale, &dst).unwrap();

        apply(
            &[Op::Symlink {
                src: real.clone(),
                dst: dst.clone(),
            }],
            false,
            &mut Recorder::default(),
        )
        .unwrap();

        assert_eq!(fs::read_link(&dst).unwrap(), real);
    }

    #[test]
    fn symlink_refuses_to_overwrite_real_file_without_force() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.txt");
        let dst = tmp.path().join("dst.txt");
        fs::write(&src, "x").unwrap();
        fs::write(&dst, "real-file").unwrap();

        let err = apply(
            &[Op::Symlink {
                src,
                dst: dst.clone(),
            }],
            false,
            &mut Recorder::default(),
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&dst).unwrap(), "real-file");
    }

    #[test]
    fn symlink_overwrites_real_file_with_force() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.txt");
        let dst = tmp.path().join("dst.txt");
        fs::write(&src, "x").unwrap();
        fs::write(&dst, "real-file").unwrap();

        let mut h = Recorder::default();
        apply(
            &[Op::Symlink {
                src: src.clone(),
                dst: dst.clone(),
            }],
            true,
            &mut h,
        )
        .unwrap();

        assert!(matches!(&h.results[0], OpResult::SymlinkOverwritten { .. }));
        assert_eq!(fs::read_link(&dst).unwrap(), src);
    }

    #[test]
    fn copy_file_creates_dest_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("seed.txt");
        let dst = tmp.path().join("nested/seed.txt");
        fs::write(&src, "hello").unwrap();

        let mut h = Recorder::default();
        apply(
            &[Op::CopyFileIfMissing {
                src: src.clone(),
                dst: dst.clone(),
            }],
            false,
            &mut h,
        )
        .unwrap();

        assert_eq!(fs::read_to_string(&dst).unwrap(), "hello");
        assert!(matches!(&h.results[0], OpResult::FileCopied { .. }));
    }

    #[test]
    fn copy_file_skips_when_dest_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("seed.txt");
        let dst = tmp.path().join("seed.txt.dst");
        fs::write(&src, "fresh").unwrap();
        fs::write(&dst, "user-modified").unwrap();

        let mut h = Recorder::default();
        apply(
            &[Op::CopyFileIfMissing {
                src,
                dst: dst.clone(),
            }],
            false,
            &mut h,
        )
        .unwrap();

        assert_eq!(fs::read_to_string(&dst).unwrap(), "user-modified");
        assert!(matches!(&h.results[0], OpResult::FileCopySkipped { .. }));
    }

    #[test]
    fn exec_captures_stdout_lines() {
        let tmp = tempfile::tempdir().unwrap();
        #[derive(Default)]
        struct OutputSink {
            lines: Vec<String>,
            results: Vec<OpResult>,
        }
        impl ExecutionHandler for OutputSink {
            fn on_start(&mut self, _: &Op) {}
            fn on_output(&mut self, line: &str) {
                self.lines.push(line.into());
            }
            fn on_result(&mut self, r: &OpResult) {
                self.results.push(r.clone());
            }
        }

        let ops = vec![Op::Exec {
            cmd: "sh".into(),
            args: vec!["-c".into(), "echo hello-stdout".into()],
            cwd: tmp.path().to_path_buf(),
        }];

        let mut h = OutputSink::default();
        apply(&ops, false, &mut h).unwrap();

        assert!(
            h.lines.iter().any(|l| l == "hello-stdout"),
            "got lines: {:?}",
            h.lines
        );
        assert!(matches!(
            &h.results[0],
            OpResult::CommandRan { status: 0, .. }
        ));
    }

    #[test]
    fn write_file_op_paths_use_pathbuf() {
        // Sanity check that PathBuf round-trips through the Op variant
        // — guards against future changes that might tempt String paths.
        let op = Op::WriteFile {
            path: PathBuf::from("/tmp/x"),
            content: "y".into(),
        };
        assert!(matches!(op, Op::WriteFile { .. }));
    }

    fn archive_source() -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let archive_path = root.path().join("demo.tar.gz");
        let file = fs::File::create(&archive_path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(encoder);

        let mut header = tar::Header::new_gnu();
        let body = b"#!/bin/sh\n";
        header.set_size(body.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "demo/bin/demo", &body[..])
            .unwrap();
        builder.finish().unwrap();
        builder.into_inner().unwrap().finish().unwrap();

        (root, archive_path)
    }

    fn locked_package(archive: &PathBuf) -> LockedPackage {
        LockedPackage {
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
            source: LockedSource::File {
                path: archive.clone(),
                sha256: hash_file(archive).unwrap(),
            },
            install: LockedInstall::Archive {
                format: ArchiveFormat::TarGz,
                strip_prefix: Some(PathBuf::from("demo")),
            },
            provides: Provides {
                bins: BTreeMap::from([("demo".to_string(), PathBuf::from("bin/demo"))]),
            },
        }
    }

    #[test]
    fn realize_package_op_installs_locked_package_into_store() {
        let root = tempfile::tempdir().unwrap();
        let (_archive_root, archive) = archive_source();
        let package = locked_package(&archive);
        let ops = vec![Op::RealizePackage {
            package: package.clone(),
        }];
        let package_realizer = PackageRealizer::with_dirs(
            Store::new(root.path().join("store")),
            root.path().join("downloads"),
            root.path().join("tmp"),
        );

        let mut h = Recorder::default();
        apply_with_package_realizer(&ops, false, &mut h, &package_realizer).unwrap();

        let OpResult::PackageRealized {
            name,
            version,
            store_path: Some(store_path),
        } = &h.results[0]
        else {
            panic!("expected package realized result");
        };
        assert_eq!(name, "demo");
        assert_eq!(version, "1.0.0");
        assert!(store_path.join("bin/demo").is_file());
    }
}
