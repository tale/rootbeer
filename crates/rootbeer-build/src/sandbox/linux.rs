use super::*;

pub(super) fn command(
    sandbox: &Sandbox,
    args: &[String],
    source: &Path,
) -> Result<Command, String> {
    let mut command = Command::new(launcher()?);
    command.args([
        "--unshare-user",
        "--unshare-pid",
        "--unshare-ipc",
        "--unshare-net",
        "--unshare-uts",
        "--die-with-parent",
        "--new-session",
        "--cap-drop",
        "ALL",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
    ]);
    for root in [
        "/usr/lib",
        "/usr/lib64",
        "/lib",
        "/lib64",
        "/etc/ld.so.cache",
    ] {
        if Path::new(root).exists() {
            command.args(["--ro-bind", root, root]);
        }
    }
    for path in &sandbox.reads {
        command.arg("--ro-bind").arg(path).arg(path);
    }
    command
        .arg("--bind")
        .arg(&sandbox.workspace)
        .arg(&sandbox.workspace);
    command.args(["--remount-ro", "/"]);
    command.arg("--chdir").arg(source).arg("--").args(args);
    Ok(command)
}
