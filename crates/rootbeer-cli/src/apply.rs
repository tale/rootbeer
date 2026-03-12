use std::path::PathBuf;

/// Apply the rootbeer configuration
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Prints the operations that would be performed without making any changes
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Overwrites any content that would be modified by the script, this
    /// includes symlinks, files, and directories. Use with caution.
    #[arg(short, long)]
    pub force: bool,

    /// Override the script to execute (must still be in the config directory,
    /// default: `init.lua`)
    #[arg(short, long)]
    pub script: Option<PathBuf>,

    /// An optional profile to pass to the script, which can be accessed via
    /// `rb.profile` in Lua.
    pub profile: Option<String>,
}

pub fn run(args: Args, lua_dir: Option<&PathBuf>) {
    let script = match args.script {
        Some(path) => rootbeer_core::config_dir().join(path),
        None => rootbeer_core::config_dir().join("init.lua"),
    };

    if !script.exists() {
        eprintln!("error: script not found: {}", script.display());
        std::process::exit(1);
    }

    let mut opts = rootbeer_core::Options::from_script(&script).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });

    opts.force = args.force;
    opts.profile = args.profile;
    opts.mode = match args.dry_run {
        true => rootbeer_core::Mode::DryRun,
        false => rootbeer_core::Mode::Apply,
    };

    if let Some(lua_dir) = lua_dir {
        opts.lua_dir = lua_dir.clone();
    }

    let pipeline = rootbeer_core::Pipeline::new(opts);
    eprintln!(
        "applying ({}){}",
        pipeline.mode(),
        if pipeline.force() { " [force]" } else { "" }
    );

    match pipeline.run() {
        Ok(report) => {
            eprintln!("done ({} operations)", report.results.len());
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
