use std::os::unix::process::CommandExt;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn run(
    args: &[String],
    source: &Path,
    environment: &BTreeMap<&str, String>,
    log: &Path,
    timeout: Duration,
) -> Result<(), String> {
    run_with_sandbox(args, source, environment, log, timeout, None)
}

/// Executes a command with optional OS isolation and a bounded process lifetime.
pub fn run_with_sandbox(
    args: &[String],
    source: &Path,
    environment: &BTreeMap<&str, String>,
    log: &Path,
    timeout: Duration,
    sandbox: Option<&crate::Sandbox>,
) -> Result<(), String> {
    if args.is_empty() || args[0].is_empty() {
        return Err("build command cannot be empty".into());
    }
    eprintln!("building: {}", args.join(" "));
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| e.to_string())?;
    let mut command = match sandbox {
        Some(sandbox) => sandbox.command(args, source, environment)?,
        None => {
            let mut command = Command::new(&args[0]);
            command.args(&args[1..]);
            command
        }
    };
    command.env_clear();
    if sandbox.is_none() {
        command.envs(environment);
    }
    let mut child = command
        .current_dir(source)
        .stdin(Stdio::null())
        .stdout(file.try_clone().map_err(|e| e.to_string())?)
        .stderr(file)
        .process_group(0)
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if sandbox.is_some() {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
            }
            if status.success() {
                return Ok(());
            }
            return Err(format!(
                "build command {} failed ({status}); see {}",
                args[0],
                log.display()
            ));
        }
        if start.elapsed() > timeout {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            return Err(format!(
                "build step exceeded its time limit; see {}",
                log.display()
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
