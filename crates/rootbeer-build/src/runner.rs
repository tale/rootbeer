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
    eprintln!("building: {}", args.join(" "));
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| e.to_string())?;
    let mut child = Command::new(&args[0])
        .args(&args[1..])
        .current_dir(source)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::null())
        .stdout(file.try_clone().map_err(|e| e.to_string())?)
        .stderr(file)
        .process_group(0)
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
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
