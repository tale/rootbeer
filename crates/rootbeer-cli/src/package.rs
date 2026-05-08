use std::path::PathBuf;

use clap::Subcommand;
use owo_colors::OwoColorize;
use rootbeer_core::package::lockfile::RootbeerLock;
use rootbeer_core::package::{LockedPackage, PackageRealizer};
use rootbeer_core::Op;

#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Evaluate the config and write rootbeer.lock from locked package specs
    Lock(LockArgs),
}

#[derive(clap::Args, Debug)]
struct LockArgs {
    /// Path to a .lua script to evaluate
    #[arg(short, long)]
    script: Option<PathBuf>,

    /// Configuration profile input
    #[arg(short = 'p', long)]
    profile: Option<String>,

    /// Output lockfile path. Defaults to rootbeer.lock next to the script.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

pub fn run(args: Args, lua_dir: Option<&PathBuf>) {
    match args.command {
        Command::Lock(args) => lock(args, lua_dir),
    }
}

fn lock(args: LockArgs, lua_dir: Option<&PathBuf>) {
    let script = args.script.unwrap_or_else(rootbeer_core::script_path);
    let mut opts = rootbeer_core::Options::from_script(&script).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });

    opts.profile = args.profile;
    if let Some(lua_dir) = lua_dir {
        opts.lua_dir = lua_dir.clone();
    }

    let pipeline = rootbeer_core::Pipeline::new(opts);
    let planned = pipeline.plan().unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });

    let output = args.output.unwrap_or_else(|| {
        script
            .parent()
            .map(|parent| parent.join("rootbeer.lock"))
            .unwrap_or_else(|| PathBuf::from("rootbeer.lock"))
    });

    let packages = realize_packages(planned.ops());
    let lock = RootbeerLock::from_packages(packages).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });

    lock.write(&output).unwrap_or_else(|e| {
        eprintln!("error: failed to write {}: {e}", output.display());
        std::process::exit(1);
    });

    eprintln!(
        "  {} {} ({} packages)",
        "lock".green(),
        output.display(),
        lock.packages.len()
    );
}

fn realize_packages(ops: &[Op]) -> Vec<LockedPackage> {
    let realizer = PackageRealizer::default();
    let mut packages = Vec::new();

    for op in ops {
        let Op::RealizePackage { package } = op else {
            continue;
        };

        let realized = realizer.realize(package).unwrap_or_else(|e| {
            eprintln!("error: failed to realize package {}: {e}", package.id());
            std::process::exit(1);
        });

        let mut locked = package.clone();
        locked.output_sha256 = Some(realized.store_entry.output_sha256);
        packages.push(locked);
    }

    packages
}
