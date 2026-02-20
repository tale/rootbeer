use std::path::PathBuf;

use clap::{Parser, Subcommand};
use rootbeer_core::OpResult;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable debug output
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    /// Path to the lua/ standard library directory
    #[arg(long)]
    lua_dir: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Apply {
        /// Perform a dry run without making any changes
        #[arg(short, long)]
        dry_run: bool,

        /// Path to a .lua script to execute (default: data_dir/source/rootbeer.lua)
        script: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Apply { dry_run, script } => {
            println!("got mode dry run = true? {dry_run}");
            let mode = if dry_run {
                rootbeer_core::Mode::DryRun
            } else {
                rootbeer_core::Mode::Apply
            };

            let script = script.unwrap_or_else(|| {
                let data_dir = rootbeer_core::Runtime::default_dir();
                data_dir.join("source/rootbeer.lua")
            });

            if !script.exists() {
                eprintln!("error: script not found: {}", script.display());
                std::process::exit(1);
            }

            let mut runtime = rootbeer_core::Runtime::from_script(&script).unwrap_or_else(|e| {
                eprintln!("error: {e}");
                std::process::exit(1);
            });

            if let Some(lua_dir) = &cli.lua_dir {
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
                        }
                    }
                }

                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}
