use std::process::Command;

pub fn run() {
    let dest = super::source_dir();

    if !dest.exists() {
        eprintln!("error: source directory does not exist: {}", dest.display());
        eprintln!("  run `rb init` first");
        std::process::exit(1);
    }

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    let status = Command::new(&shell)
        .current_dir(&dest)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("error: failed to spawn shell ({shell}): {e}");
            std::process::exit(1);
        });

    std::process::exit(status.code().unwrap_or(1));
}
