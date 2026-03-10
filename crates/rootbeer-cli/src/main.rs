mod apply;
mod cd;
mod edit;
mod init;
mod lsp;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

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
    /// Initialize a rootbeer source directory
    Init {
        /// Source to initialize from: GitHub shorthand (user/repo), git URL, or local path.
        /// When omitted, creates a fresh source directory with a starter rootbeer.lua.
        source: Option<String>,

        /// Remove existing source directory before initializing
        #[arg(short, long)]
        force: bool,

        /// Run apply after initialization
        #[arg(short, long)]
        apply: bool,

        /// Perform a dry run when --apply is used
        #[arg(short = 'n', long)]
        dry_run: bool,
    },

    /// Open a shell in the rootbeer source directory
    Cd,

    /// Open the rootbeer source directory in $VISUAL/$EDITOR
    Edit,

    /// Extract type definitions and write .luaurc for editor autocomplete
    Lsp,

    /// Apply the rootbeer configuration
    Apply {
        /// Perform a dry run without making any changes
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Path to a .lua script to execute (default: data_dir/source/rootbeer.lua)
        #[arg(short, long)]
        script: Option<PathBuf>,

        /// Configuration profile name, exposed as `rb.profile` in Lua
        profile: Option<String>,
    },
}

fn source_dir() -> PathBuf {
    rootbeer_core::Runtime::default_dir().join("source")
}

fn script_path() -> PathBuf {
    source_dir().join("rootbeer.lua")
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init {
            source,
            force,
            apply,
            dry_run,
        } => {
            init::run(source, force);

            if !apply {
                println!("  run `rb apply` to apply your configuration");
            }

            if apply {
                let mode = if dry_run {
                    rootbeer_core::Mode::DryRun
                } else {
                    rootbeer_core::Mode::Apply
                };

                println!();
                apply::run(script_path(), mode, cli.lua_dir.as_ref(), None);
            }
        }

        Commands::Lsp => lsp::run(&source_dir()),
        Commands::Cd => cd::run(),
        Commands::Edit => edit::run(),

        Commands::Apply {
            dry_run,
            script,
            profile,
        } => {
            let mode = if dry_run {
                rootbeer_core::Mode::DryRun
            } else {
                rootbeer_core::Mode::Apply
            };

            let script = script.unwrap_or_else(script_path);
            apply::run(script, mode, cli.lua_dir.as_ref(), profile);
        }
    }
}
