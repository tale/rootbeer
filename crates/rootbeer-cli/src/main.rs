mod apply;
mod cd;
mod edit;
mod init;
mod package;
mod remote;
mod typegen;
mod update;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "rb",
    version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("RB_BUILD_TIMESTAMP"), ")"),
    about,
    long_about = None,
    max_term_width = 80
)]
/// A command-line tool to deterministically manage your system using Lua!
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable additional debug output
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    /// Suppress all output except errors
    #[arg(short, long, default_value_t = false)]
    quiet: bool,

    /// Path to the lua/ standard library directory
    #[arg(long)]
    lua_dir: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create or load a rootbeer configuration in the source directory
    Init(init::Args),

    /// Open a shell in the rootbeer source directory
    Cd,

    /// Open the rootbeer source directory in $VISUAL/$EDITOR
    Edit,

    /// Apply the rootbeer configuration
    Apply(apply::Args),

    /// Package lockfile commands
    Package(package::Args),

    /// View or change the git remote protocol for the source directory
    Remote(remote::Args),

    /// Update rootbeer to the latest nightly build
    Update,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init(args) => init::run(args),
        Commands::Cd => cd::run(),
        Commands::Edit => edit::run(),
        Commands::Apply(args) => apply::run(args, cli.lua_dir.as_ref()),
        Commands::Package(args) => package::run(args, cli.lua_dir.as_ref()),
        Commands::Remote(args) => remote::run(args),
        Commands::Update => update::run(),
    }
}
