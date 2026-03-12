use std::path::PathBuf;

/// Apply the rootbeer configuration
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Perform a dry run without making any changes
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Overwrite existing files/directories when creating symlinks
    #[arg(short, long)]
    pub force: bool,

    /// Path to a .lua script to execute (default: data_dir/source/rootbeer.lua)
    #[arg(short, long)]
    pub script: Option<PathBuf>,

    /// Configuration profile name, exposed as `rb.profile` in Lua
    pub profile: Option<String>,
}

pub fn run(args: Args, lua_dir: Option<&PathBuf>) {
    let opts = rootbeer_core::Options {
        mode: if args.dry_run {
            rootbeer_core::Mode::DryRun
        } else {
            rootbeer_core::Mode::Apply
        },
        force: args.force,
    };

    let script = args.script.unwrap_or_else(rootbeer_core::script_path);

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

    runtime.profile = args.profile;

    eprintln!(
        "applying ({}){}",
        opts.mode,
        if opts.force { " [force]" } else { "" }
    );
    match rootbeer_core::execute_with(runtime, opts) {
        Ok(report) => {
            eprintln!("done ({} operations)", report.results.len());
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
