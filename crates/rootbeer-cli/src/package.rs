use std::io::{self, Write};
use std::path::PathBuf;

use clap::{Args as ClapArgs, Subcommand};
use rootbeer_core::package::PackageCatalog;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Read recipes from a directory instead of the embedded catalog
    #[arg(long, global = true)]
    catalog: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// List canonical names, approved defaults, and descriptions
    List,
    /// Show a package's identity and version recipes, accepting aliases
    Show { name: String },
    /// Validate the selected catalog and print its digest
    Check,
    /// Write a deterministic JSON catalog snapshot to stdout
    Index,
    /// Assemble verified source artifacts into a directory ready for hosting
    Bundle {
        #[arg(long, required = true)]
        receipt: Vec<PathBuf>,
        #[arg(long)]
        base_url: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Build, check, and export the current platform's package recipes
    Export {
        #[arg(long)]
        registry: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(short, long, default_value_t = 2)]
        jobs: usize,
    },
    /// Merge platform bundles and require complete version/platform coverage
    Assemble {
        #[arg(long)]
        inputs: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Upload GHCR blobs and prepare the signed Pages directory (requires ORAS)
    Publish {
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        site: PathBuf,
        #[arg(long)]
        site_url: String,
        #[arg(long)]
        registry: String,
        #[arg(long)]
        repository_url: String,
        #[arg(long)]
        sequence: u64,
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        public_key: String,
    },
    /// Validate an artifact index, optionally requiring every declared platform
    VerifyIndex {
        index: PathBuf,
        #[arg(long)]
        complete: bool,
    },
    /// Sign a complete artifact index with an Ed25519 PKCS#8 DER key
    SignIndex {
        index: PathBuf,
        #[arg(long)]
        url: String,
        #[arg(long)]
        sequence: u64,
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        public_key: String,
        #[arg(long)]
        previous: Option<PathBuf>,
        #[arg(long)]
        output: PathBuf,
    },
    /// Compile a trusted source recipe into an installable local artifact
    Build {
        name: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(short, long, default_value_t = 2)]
        jobs: usize,
    },
}

pub fn run(args: Args) {
    if let Err(error) = execute(args) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn execute(args: Args) -> Result<(), String> {
    let local_catalog = args
        .catalog
        .as_deref()
        .map(PackageCatalog::from_directory)
        .transpose()?;
    let catalog = match &local_catalog {
        Some(catalog) => catalog,
        None => PackageCatalog::embedded()?,
    };
    let mut output = io::stdout().lock();
    match args.command {
        Command::List => {
            for package in catalog.packages.values() {
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}",
                    package.name,
                    package.default_version,
                    if package.versions[&package.default_version].build.is_some() {
                        "source"
                    } else {
                        "binary"
                    },
                    package.description
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
                    recipe
                        .source
                        .as_deref()
                        .unwrap_or("source build (binary not published)"),
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
        Command::Bundle {
            receipt,
            base_url,
            output: destination,
        } => {
            let digest = rootbeer_core::package::bundle_artifacts(
                catalog,
                &receipt,
                &base_url,
                &destination,
            )?;
            writeln!(
                output,
                "index: {}\nsha256:{digest}",
                destination.join("index.json").display()
            )
            .map_err(|e| e.to_string())?;
        }
        Command::Export {
            registry,
            output,
            jobs,
        } => rootbeer_core::package::export_catalog(catalog, &registry, &output, jobs)?,
        Command::Assemble { inputs, output } => {
            rootbeer_core::package::assemble_indexes(&inputs, &output)?
        }
        Command::Publish {
            bundle,
            site,
            site_url,
            registry,
            repository_url,
            sequence,
            key,
            public_key,
        } => {
            rootbeer_core::package::publish_index(&rootbeer_core::package::PublishOptions {
                bundle: &bundle,
                site: &site,
                site_url: &site_url,
                registry: &registry,
                repository_url: &repository_url,
                sequence,
                key: &key,
                public_key: &public_key,
            })?;
        }
        Command::VerifyIndex { index, complete } => {
            let index: rootbeer_core::package::ArtifactIndex =
                serde_json::from_slice(&std::fs::read(index).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if complete {
                index.validate_complete()?;
            } else {
                index.validate()?;
            }
            writeln!(output, "verified artifact index").map_err(|e| e.to_string())?;
        }
        Command::SignIndex {
            index,
            url,
            sequence,
            key,
            public_key,
            previous,
            output: destination,
        } => {
            let bytes = std::fs::read(index).map_err(|e| e.to_string())?;
            let key = std::fs::read(key).map_err(|e| e.to_string())?;
            let previous = previous
                .map(std::fs::read)
                .transpose()
                .map_err(|e| e.to_string())?;
            let manifest = rootbeer_core::package::sign_index(
                &bytes,
                &url,
                sequence,
                &key,
                &public_key,
                previous.as_deref(),
            )?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)
                .map_err(|e| e.to_string())?;
            file.write_all(&manifest).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
        }
        Command::Build {
            name,
            output: destination,
            jobs,
        } => {
            let artifact =
                rootbeer_core::package::build_package(catalog, &name, &destination, jobs)?;
            writeln!(
                output,
                "built {} for {}\nartifact: {}\ninstall: rb apply --script {}",
                artifact.package.id(),
                artifact.system,
                destination.join("package.tar.gz").display(),
                destination.join("install.lua").display()
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
