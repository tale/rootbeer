use std::path::PathBuf;

use owo_colors::OwoColorize;
use rootbeer_core::profile::ProfileError;
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

    /// Configuration profile name; overrides the strategy declared in
    /// `rb.profile.define`.
    #[arg(short = 'p', long)]
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
            OpResult::FileCopied { src, dst } => {
                eprintln!(
                    "  {} {} <- {}",
                    "copy".green(),
                    dst.display(),
                    src.display()
                );
            }
            OpResult::FileCopySkipped { dst } => {
                eprintln!("  {} {} (exists)", "skip".dimmed(), dst.display());
            }
            OpResult::CommandRan { cmd, status } => {
                if *status == 0 {
                    eprintln!("  {} `{cmd}`", "done".green());
                } else {
                    eprintln!("  {} `{cmd}` (exit {status})", "fail".red());
                }
            }
            OpResult::Chmodded { path, mode } => {
                eprintln!("  {} {} ({:o})", "chmod".green(), path.display(), mode);
            }
            OpResult::RemoteUpdated { from, to } => {
                eprintln!("  {} {from} -> {to}", "remote".green());
            }
            OpResult::RemoteUnchanged { url } => {
                eprintln!("  {} {url} (unchanged)", "skip".dimmed());
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
        match &e {
            rootbeer_core::Error::Profile(pe) => {
                eprintln!("{} {pe}", "✗".red().bold());
                if let Some(hint) = profile_hint(pe) {
                    eprintln!("{hint}");
                }
            }
            _ => eprintln!("{} error: {e}", "✗".red().bold()),
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

fn profile_hint(err: &ProfileError) -> Option<String> {
    match err {
        ProfileError::Required { active, profiles } if !profiles.is_empty() => {
            let mut out = format!(
                "  {} {}",
                "hint:".dimmed(),
                format!("rb apply --profile <{}>", profiles.join("|")).cyan()
            );
            if let Some(name) = active {
                if let Some(suggestion) = closest_match(name, profiles) {
                    out.push_str(&format!(" (did you mean '{}'?)", suggestion.cyan()));
                }
            }
            Some(out)
        }
        ProfileError::NoMatch { profiles, .. } if !profiles.is_empty() => Some(format!(
            "  {} pass {} or extend {}",
            "hint:".dimmed(),
            format!("--profile <{}>", profiles.join("|")).cyan(),
            "rb.profile.define(...)".cyan()
        )),
        _ => None,
    }
}

fn closest_match<'a>(name: &str, candidates: &'a [String]) -> Option<&'a str> {
    let target = name.to_lowercase();
    candidates
        .iter()
        .map(|c| (levenshtein(&target, &c.to_lowercase()), c.as_str()))
        .filter(|(d, c)| *d <= (c.len() / 2).max(2))
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}
