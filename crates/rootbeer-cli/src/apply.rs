use std::path::PathBuf;

use rootbeer_core::OpResult;

pub fn run(script: PathBuf, mode: rootbeer_core::Mode, lua_dir: Option<&PathBuf>) {
    if !script.exists() {
        eprintln!("error: script not found: {}", script.display());
        std::process::exit(1);
    }

    let mut runtime = rootbeer_core::Runtime::from_script(&script).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });

    if let Some(lua_dir) = lua_dir {
        runtime.lua_dir = lua_dir.clone();
    }

    match rootbeer_core::execute_with(runtime, mode) {
        Ok(report) => {
            println!("ran in {} mode:", report.mode);
            for result in &report.results {
                match result {
                    OpResult::FileWritten { path, bytes } => {
                        println!("  write {} ({bytes} bytes)", path.display());
                    }

                    OpResult::SymlinkCreated { src, dst } => {
                        println!("  link {} -> {}", dst.display(), src.display());
                    }

                    OpResult::SymlinkUnchanged { dst } => {
                        println!("  link {} (unchanged)", dst.display());
                    }

                    OpResult::CommandRan { cmd, status } => {
                        println!("  exec `{cmd}` (exit {status})");
                    }
                }
            }
        }

        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
