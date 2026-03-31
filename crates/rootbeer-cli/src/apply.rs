use std::path::PathBuf;

use owo_colors::OwoColorize;
use rootbeer_core::{ExecutionHandler, Op, OpResult};

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

struct CliHandler;

impl ExecutionHandler for CliHandler {
    fn on_start(&mut self, op: &Op) {
        if let Op::Exec { cmd, args, .. } = op {
            let display = std::iter::once(cmd.as_str())
                .chain(args.iter().map(|s| s.as_str()))
                .collect::<Vec<_>>()
                .join(" ");

            eprintln!("  {} `{display}`", "exec".cyan());
        }
    }

    fn on_output(&mut self, line: &str) {
        eprintln!("    {}", line.dimmed());
    }

    fn on_result(&mut self, result: &OpResult) {
        match result {
            OpResult::FileWritten { path, bytes } => {
                eprintln!("  {} {} ({bytes} bytes)", "write".green(), path.display());
            }
            OpResult::SymlinkCreated { src, dst } => {
                eprintln!(
                    "  {} {} -> {}",
                    "link".green(),
                    dst.display(),
                    src.display()
                );
            }
            OpResult::SymlinkUnchanged { dst } => {
                eprintln!("  {} {} (unchanged)", "skip".dimmed(), dst.display());
            }
            OpResult::SymlinkOverwritten { src, dst } => {
                eprintln!(
                    "  {} {} -> {}",
                    "force".yellow(),
                    dst.display(),
                    src.display()
                );
            }
            OpResult::CommandRan { cmd, status } => {
                if *status == 0 {
                    eprintln!("  {} `{cmd}`", "done".green());
                } else {
                    eprintln!("  {} `{cmd}` (exit {status})", "fail".red());
                }
            }
        }
    }
}

pub fn run(args: Args, lua_dir: Option<&PathBuf>) {
    let script = args.script.unwrap_or_else(rootbeer_core::script_path);

    if !script.exists() {
        eprintln!("error: script not found: {}", script.display());
        std::process::exit(1);
    }

    let mut opts = rootbeer_core::Options::from_script(&script).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });

    opts.mode = if args.dry_run {
        rootbeer_core::Mode::DryRun
    } else {
        rootbeer_core::Mode::Apply
    };
    opts.force = args.force;
    opts.profile = args.profile;

    if let Some(lua_dir) = lua_dir {
        opts.lua_dir = lua_dir.clone();
    }

    let pipeline = rootbeer_core::Pipeline::new(opts);

    eprintln!(
        "applying ({}){}",
        pipeline.mode(),
        if pipeline.force() { " [force]" } else { "" }
    );

    let planned = pipeline.plan().unwrap_or_else(|e| {
        if let rootbeer_core::Error::ProfileRequired { active, profiles } = &e {
            match active {
                Some(name) => eprintln!(
                    "{} unknown profile '{}', expected one of: {}",
                    "✗".red().bold(),
                    name,
                    profiles.join(", ")
                ),
                None => eprintln!(
                    "{} a profile is required for this configuration",
                    "✗".red().bold(),
                ),
            }
            eprintln!(
                "hint: run {} with a profile name",
                format!("rb apply <{}>", profiles.join("|")).cyan()
            );
        } else {
            eprintln!("{} error: {e}", "✗".red().bold());
        }
        std::process::exit(1);
    });

    let mut handler = CliHandler;

    let result = planned.execute(&mut handler);

    match result {
        Ok(report) => {
            eprintln!(
                "{} done ({} operations)",
                "✓".green().bold(),
                report.results.len()
            );
        }
        Err(e) => {
            eprintln!("{} error: {e}", "✗".red().bold());
            std::process::exit(1);
        }
    }
}
