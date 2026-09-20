use rootbeer_package::Execution;
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
    run_with_execution(
        args,
        source,
        environment,
        log,
        timeout,
        sandbox,
        &Execution::default(),
    )
}

/// Runs a command within the caller’s cancellation and wall-clock budget.
pub fn run_with_execution(
    args: &[String],
    source: &Path,
    environment: &BTreeMap<&str, String>,
    log: &Path,
    timeout: Duration,
    sandbox: Option<&crate::Sandbox>,
    execution: &Execution,
) -> Result<(), String> {
    execution.check().map_err(|error| error.to_string())?;
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
    let mut child = Process {
        child: command
            .current_dir(source)
            .stdin(Stdio::null())
            .stdout(file.try_clone().map_err(|e| e.to_string())?)
            .stderr(file)
            .process_group(0)
            .spawn()
            .map_err(|error| {
                format!(
                    "cannot start build command {} in {}: {error}; see {}",
                    args[0],
                    source.display(),
                    log.display()
                )
            })?,
        is_stopped: false,
    };
    let start = Instant::now();
    loop {
        if let Err(error) = execution.check() {
            child.stop().map_err(|error| error.to_string())?;
            return Err(error.to_string());
        }
        if let Some(status) = child.child.try_wait().map_err(|e| e.to_string())? {
            child.stop().map_err(|error| error.to_string())?;
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
            child.stop().map_err(|error| error.to_string())?;
            return Err(format!(
                "build step exceeded its time limit; see {}",
                log.display()
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

struct Process {
    child: std::process::Child,
    is_stopped: bool,
}

impl Process {
    fn stop(&mut self) -> std::io::Result<()> {
        if self.is_stopped {
            return Ok(());
        }
        if unsafe { libc::kill(-(self.child.id() as i32), libc::SIGKILL) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error);
            }
        }
        self.child.wait()?;
        self.is_stopped = true;
        Ok(())
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
