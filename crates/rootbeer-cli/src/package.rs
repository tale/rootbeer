use std::io::{self, Write};

use clap::{Args as ClapArgs, Subcommand};
use rootbeer_core::package::PackageCatalog;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// List canonical names, approved defaults, and descriptions
    List,
    /// Show a package's identity and version recipes, accepting aliases
    Show { name: String },
    /// Validate the embedded catalog and print its digest
    Check,
    /// Write a deterministic JSON catalog snapshot to stdout
    Index,
}

pub fn run(args: Args) {
    if let Err(error) = execute(args) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn execute(args: Args) -> Result<(), String> {
    let catalog = PackageCatalog::embedded()?;
    let mut output = io::stdout().lock();
    match args.command {
        Command::List => {
            for package in catalog.packages.values() {
                writeln!(
                    output,
                    "{}\t{}\t{}",
                    package.name, package.default_version, package.description
                )
                .map_err(|e| e.to_string())?;
            }
        }
        Command::Show { name } => {
            let package = catalog
                .find(&name)
                .ok_or_else(|| format!("unknown catalog package `{name}`"))?;
            writeln!(
                output,
                "{} — {}\n{}\ndefault: {}\naliases: {}",
                package.name,
                package.description,
                package.homepage,
                package.default_version,
                package.aliases.join(", ")
            )
            .map_err(|e| e.to_string())?;
            for (version, recipe) in &package.versions {
                writeln!(
                    output,
                    "\n{version} (revision {})\n  source: {}\n  systems: {}\n  commands: {}",
                    recipe.revision,
                    recipe.source,
                    recipe.systems.join(", "),
                    recipe.bins.join(", ")
                )
                .map_err(|e| e.to_string())?;
            }
        }
        Command::Check => {
            catalog.validate()?;
            writeln!(
                output,
                "{} packages; sha256:{}",
                catalog.packages.len(),
                catalog.sha256()
            )
            .map_err(|e| e.to_string())?;
        }
        Command::Index => writeln!(output, "{}", catalog.to_json()?).map_err(|e| e.to_string())?,
    }
    Ok(())
}
