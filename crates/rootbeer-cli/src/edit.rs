use std::process::Command;

pub fn run() {
    let dest = rootbeer_core::config_dir();
    if !dest.exists() {
        eprintln!("error: source directory does not exist: {}", dest.display());
        eprintln!("hint: run `rb init` first");
        std::process::exit(1);
    }

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    let status = Command::new(&editor)
        .arg(".")
        .current_dir(&dest)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("error: failed to run editor ({editor}): {e}");
            if editor == "vi" {
                eprintln!(
                    "hint: set the VISUAL or EDITOR environment variable to your preferred editor"
                );
            }
            std::process::exit(1);
        });

    std::process::exit(status.code().unwrap_or(1));
}
