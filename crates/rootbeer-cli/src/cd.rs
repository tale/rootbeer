use std::process::Command;

pub fn run() {
    let dest = rootbeer_core::config_dir();
    if !dest.exists() {
        eprintln!("error: rootbeer has not been initialized");
        eprintln!("hint: run `rb init` first");
        std::process::exit(1);
    }

    let shell = match std::env::var("SHELL") {
        Ok(s) if !s.is_empty() => s,
        _ => {
            eprintln!("warning: SHELL environment variable is not set, defaulting to /bin/sh");
            "/bin/sh".to_string()
        }
    };

    let status = Command::new(&shell)
        .current_dir(&dest)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("error: failed to spawn shell ({shell}): {e}");
            std::process::exit(1);
        });

    std::process::exit(status.code().unwrap_or(1));
}
