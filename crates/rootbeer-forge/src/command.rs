use std::io::{self, Write};
use std::path::PathBuf;

use clap::{Args as ClapArgs, Subcommand};
use rootbeer_packaging::{PackageCatalog, PackageDefinition};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Read package definitions from an index checkout
    #[arg(long, global = true)]
    catalog: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Plan exact dependency-free packages for this machine
    PackagePlan {
        #[arg(required = true)]
        packages: Vec<String>,
        #[arg(long)]
        context: String,
    },
    /// Verify a signed package result against the requested identity and input key
    VerifyRecord {
        reference: String,
        #[arg(long)]
        package: String,
        #[arg(long)]
        system: String,
        #[arg(long)]
        input_key: String,
        #[arg(long)]
        public_key: String,
        /// Save the verified signed bytes for discovery publication
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Approve one qualified package and prepare its signed package release
    Release {
        /// This package's existing Lua definition
        #[arg(long)]
        recipe: PathBuf,
        #[arg(long)]
        receipt: PathBuf,
        /// GHCR repository, such as owner/packages/tool
        #[arg(long)]
        registry: String,
        #[arg(long)]
        output: PathBuf,
        /// Publisher's Ed25519 PKCS#8 DER key
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        public_key: String,
        /// Require these planned inputs before signing
        #[arg(long)]
        input_key: Option<String>,
    },
    /// Upload a signed package release to GHCR without rebuilding or signing again
    Push {
        release: PathBuf,
        #[arg(long)]
        public_key: String,
    },
    /// Publish a single discovery manifest from independently signed package records
    PublishRecords {
        #[arg(long)]
        record: Vec<String>,
        #[arg(long)]
        records: Option<PathBuf>,
        #[arg(long)]
        site: PathBuf,
        #[arg(long)]
        site_url: String,
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        public_key: String,
    },
    /// Add inferred update rules to copies of the selected catalog's GitHub packages
    /// Check tracked upstreams, caching metadata and reporting independent failures
    Updates {
        #[arg(long)]
        cache: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 20)]
        max_pages: usize,
    },
    /// Discover GitHub releases and generate complete candidate package definitions
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
        /// Maximum wall-clock seconds; completed qualifications remain reusable
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        timeout: Option<u64>,
        /// Write a JSON reuse plan to --output without executing packages
        #[arg(long)]
        plan: bool,
        /// Pinned tools, SDK/sysroot inputs, and build variables
        #[arg(long)]
        environment: Option<PathBuf>,
        /// Deny network access and restrict filesystem access to build inputs and scratch
        #[arg(long, requires = "environment")]
        isolate: bool,
        #[arg(long)]
        registry: String,
        #[arg(long)]
        output: PathBuf,
        /// Total compiler job budget shared by concurrent source builds
        #[arg(short, long, default_value_t = 2)]
        jobs: usize,
        /// Maximum packages to qualify concurrently
        #[arg(long, default_value_t = 2)]
        workers: usize,
        /// Zero-based partition to export
        #[arg(long, requires = "shards")]
        shard: Option<usize>,
        /// Total number of partitions; each retains the full catalog for assembly
        #[arg(long, requires = "shard")]
        shards: Option<usize>,
        /// Reuse verified results from this trusted cache directory
        #[arg(long, requires = "cache_context")]
        cache: Option<PathBuf>,
        /// Identity of the runner image and build toolchain
        #[arg(long, requires = "cache")]
        cache_context: Option<String>,
        /// Rebuild and recheck every package, refreshing cached results
        #[arg(long, requires = "cache")]
        recheck: bool,
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
        /// Manifest channel filename, such as latest-v2.json
        #[arg(long, default_value = "latest.json")]
        manifest: String,
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
    /// Validate a complete bundle's local contents against the selected catalog
    VerifyBundle { bundle: PathBuf },
    /// Validate complete artifacts and qualification evidence against the selected catalog
    VerifyCandidate { bundle: PathBuf },
    /// List required receipt/archive paths from authenticated candidate metadata
    CandidateFiles {
        bundle: PathBuf,
        /// Defaults to the current platform
        #[arg(long)]
        system: Option<String>,
    },
    /// Import qualifications from a candidate whose producer and catalog the caller has approved
    ImportResults {
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        cache: PathBuf,
        /// Import only this platform; other platforms' archive bytes may be absent
        #[arg(long)]
        system: Option<String>,
    },
    /// Retain completed qualifications for retry without compiler caches or build scratch
    CheckpointResults {
        #[arg(long)]
        cache: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, requires = "shards")]
        shard: Option<usize>,
        #[arg(long, requires = "shard")]
        shards: Option<usize>,
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
    /// Hash a build environment specification and write its lock to stdout
    PinEnvironment { specification: PathBuf },
    /// Audit native loader references in an installed package directory
    Audit {
        directory: PathBuf,
        /// Build receipt declaring the installed package's runtime closure
        #[arg(long)]
        receipt: Option<PathBuf>,
    },
    /// Inspect the dependency graph without executing builds
    Plan { name: String },
    /// Prepare a source or upstream binary recipe as an installable local artifact
    #[command(visible_alias = "build")]
    Prepare {
        name: String,
        /// Fail before building if this machine differs from the work plan
        #[arg(long, requires = "cache_context")]
        input_key: Option<String>,
        /// Pinned tools, SDK/sysroot inputs, and build variables
        #[arg(long)]
        environment: Option<PathBuf>,
        /// Deny network access and restrict filesystem access to build inputs and scratch
        #[arg(long, requires = "environment")]
        isolate: bool,
        #[arg(long)]
        output: PathBuf,
        /// Compiler jobs; defaults to all available CPU cores
        #[arg(short, long, default_value_t = std::thread::available_parallelism().map_or(1, usize::from))]
        jobs: usize,
        /// Persistent build result cache
        #[arg(long, requires = "cache_context")]
        cache: Option<PathBuf>,
        /// Identity of the host image, SDK, and toolchain
        #[arg(long, requires = "cache")]
        cache_context: Option<String>,
        /// Rebuild dependencies and refresh cached results
        #[arg(long, requires = "cache")]
        recheck: bool,
        /// Maximum seconds for each configure, build, check, or install command
        #[arg(long, default_value_t = 1200)]
        phase_timeout: u64,
    },
}

pub fn run(args: Args) {
    if let Err(error) = execute(args) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn execute(args: Args) -> Result<(), String> {
    let definitions = args
        .catalog
        .as_deref()
        .map(PackageDefinition::from_directory)
        .transpose()?;
    let local_catalog = definitions
        .as_ref()
        .map(PackageCatalog::from_definitions)
        .transpose()?;
    let catalog = || {
        local_catalog.as_ref().ok_or_else(|| {
            "this command requires --catalog pointing to an index recipe directory".to_string()
        })
    };
    let mut output = io::stdout().lock();
    match args.command {
        Command::PackagePlan { packages, context } => {
            let tasks = rootbeer_packaging::plan_packages(
                catalog()?,
                &packages,
                &rootbeer_packaging::BuildOptions::default(),
                &context,
            )?;
            writeln!(
                output,
                "{}",
                serde_json::to_string(&tasks).map_err(|error| error.to_string())?
            )
            .map_err(|error| error.to_string())?;
        }
        Command::VerifyRecord {
            reference,
            package,
            system,
            input_key,
            public_key,
            output: destination,
        } => {
            let bytes = rootbeer_packaging::distribution::read_record(&reference)?;
            let record = rootbeer_packaging::distribution::verify_record(
                &bytes,
                &public_key,
                &package,
                &system,
            )?;
            if record.input_key() != input_key {
                return Err("signed package inputs differ from the requested work".into());
            }
            if let Some(destination) = destination {
                std::fs::write(destination, bytes).map_err(|error| error.to_string())?;
            }
            writeln!(output, "{reference}").map_err(|error| error.to_string())?;
        }
        Command::PublishRecords {
            mut record,
            records,
            site,
            site_url,
            key,
            public_key,
        } => {
            if let Some(directory) = records {
                for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
                    let path = entry.map_err(|error| error.to_string())?.path();
                    if path.extension().is_some_and(|value| value == "json") {
                        record.push(path.to_string_lossy().into_owned());
                    }
                }
            }
            let key = std::fs::read(key).map_err(|error| error.to_string())?;
            let count = rootbeer_packaging::publish_records(
                catalog()?,
                &record,
                &site,
                &site_url,
                &key,
                &public_key,
            )?;
            writeln!(
                output,
                "published discovery for {count} package/platform records"
            )
            .map_err(|error| error.to_string())?;
        }
        Command::Release {
            recipe,
            receipt,
            registry,
            output: destination,
            key,
            public_key,
            input_key,
        } => {
            let definition = PackageDefinition::from_lua(
                &std::fs::read_to_string(recipe).map_err(|error| error.to_string())?,
            )?;
            let reference = rootbeer_packaging::release_package(
                &definition.package,
                &receipt,
                &registry,
                &destination,
                &std::fs::read(key).map_err(|error| error.to_string())?,
                &public_key,
                input_key.as_deref(),
            )?;
            writeln!(
                output,
                "prepared package release\nrecord: {}\nreference: {reference}",
                destination.join("package.json").display(),
            )
            .map_err(|error| error.to_string())?;
        }
        Command::Push {
            release,
            public_key,
        } => {
            let reference = rootbeer_packaging::push_package(&release, &public_key)?;
            writeln!(output, "published {reference}").map_err(|error| error.to_string())?;
        }
        Command::Updates {
            cache,
            output,
            max_pages,
        } => {
            let definitions = definitions
                .as_ref()
                .ok_or("updates requires --catalog pointing to package definitions")?;
            let report =
                rootbeer_packaging::discover_updates(definitions, &cache, &output, max_pages)?;
            eprintln!(
                "{} updates, {} unchanged, {} errors; report: {}",
                report.updated.len(),
                report.unchanged.len(),
                report.errors.len(),
                output.join("summary.md").display()
            );
            if !report.errors.is_empty() {
                return Err("some upstreams failed; see the discovery report".into());
            }
        }
        Command::List => {
            let system = rootbeer_packaging::ResolveContext::current().system;
            for package in catalog()?.packages.values() {
                let Some(version) = package.default_version_for(&system) else {
                    continue;
                };
                writeln!(
                    output,
                    "{}\t{}\t{}\t{}",
                    package.name,
                    version,
                    match package.versions[version].for_system(&system) {
                        Some(recipe) if recipe.build.is_some() => "source",
                        _ => "binary",
                    },
                    package.description
                )
                .map_err(|e| e.to_string())?;
            }
        }
        Command::Show { name } => {
            let package = catalog()?
                .find(&name)
                .ok_or_else(|| format!("unknown catalog package `{name}`"))?;
            writeln!(
                output,
                "{} — {}\n{}\naliases: {}",
                package.name,
                package.description,
                package.homepage,
                package.aliases.join(", ")
            )
            .map_err(|e| e.to_string())?;
            for (system, version) in &package.default_versions {
                writeln!(output, "default for {system}: {version}").map_err(|e| e.to_string())?;
            }
            for (version, entry) in &package.versions {
                writeln!(output, "\n{version} (revision {})", entry.revision)
                    .map_err(|e| e.to_string())?;
                for (system, recipe) in &entry.platforms {
                    writeln!(
                        output,
                        "  {system}: {}\n    commands: {}",
                        recipe
                            .source
                            .as_deref()
                            .unwrap_or("source build (binary not published)"),
                        recipe
                            .bins
                            .names()
                            .into_iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
        }
        Command::Check => {
            let catalog = catalog()?;
            catalog.validate()?;
            writeln!(
                output,
                "{} packages; sha256:{}",
                catalog.packages.len(),
                catalog.sha256()
            )
            .map_err(|e| e.to_string())?;
        }
        Command::Index => {
            writeln!(output, "{}", catalog()?.to_json()?).map_err(|e| e.to_string())?
        }
        Command::Bundle {
            receipt,
            base_url,
            output: destination,
        } => {
            let digest = rootbeer_packaging::bundle_artifacts(
                catalog()?,
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
            timeout,
            plan,
            environment,
            isolate,
            registry,
            output,
            jobs,
            workers,
            shard,
            shards,
            cache,
            cache_context,
            recheck,
        } => {
            let cache = cache.map(|directory| rootbeer_packaging::ExportCache {
                directory,
                context: cache_context.unwrap(),
                recheck,
            });
            let execution = timeout
                .map(|seconds| {
                    rootbeer_packaging::Execution::with_timeout(std::time::Duration::from_secs(
                        seconds,
                    ))
                })
                .transpose()
                .map_err(|error| error.to_string())?
                .unwrap_or_default();
            if !plan {
                for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
                    signal_hook::flag::register(signal, execution.cancellation_flag())
                        .map_err(|error| error.to_string())?;
                }
            }
            let options = rootbeer_packaging::BuildOptions {
                execution,
                jobs,
                environment: read_environment(environment)?,
                is_isolated: isolate,
                ..Default::default()
            };
            let shard = shard.map(|index| rootbeer_packaging::ExportShard {
                index,
                count: shards.unwrap(),
            });
            if plan {
                let result = rootbeer_packaging::plan_export(
                    catalog()?,
                    &registry,
                    &options,
                    cache.as_ref(),
                    shard,
                )?;
                std::fs::write(
                    output,
                    serde_json::to_vec_pretty(&result).map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
                return Ok(());
            }
            rootbeer_packaging::export_catalog_with_workers(
                catalog()?,
                &registry,
                &output,
                &options,
                workers,
                cache.as_ref(),
                shard,
            )?;
        }
        Command::Assemble { inputs, output } => {
            rootbeer_packaging::assemble_indexes(&inputs, &output)?
        }
        Command::Publish {
            bundle,
            site,
            site_url,
            manifest,
            registry,
            repository_url,
            sequence,
            key,
            public_key,
        } => {
            rootbeer_packaging::publish_index(&rootbeer_packaging::PublishOptions {
                bundle: &bundle,
                site: &site,
                site_url: &site_url,
                manifest_name: &manifest,
                registry: &registry,
                repository_url: &repository_url,
                sequence,
                key: &key,
                public_key: &public_key,
            })?;
        }
        Command::VerifyIndex { index, complete } => {
            let index: rootbeer_packaging::ArtifactIndex =
                serde_json::from_slice(&std::fs::read(index).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if complete {
                index.validate_complete()?;
            } else {
                index.validate()?;
            }
            writeln!(output, "verified artifact index").map_err(|e| e.to_string())?;
        }
        Command::VerifyBundle { bundle } => {
            rootbeer_packaging::verify_bundle(&bundle, catalog()?)?;
            writeln!(output, "verified bundle against selected catalog")
                .map_err(|e| e.to_string())?;
        }
        Command::VerifyCandidate { bundle } => {
            rootbeer_packaging::verify_candidate(&bundle, catalog()?)?;
            writeln!(output, "verified candidate against selected catalog")
                .map_err(|error| error.to_string())?;
        }
        Command::CandidateFiles { bundle, system } => {
            let system =
                system.unwrap_or_else(|| rootbeer_packaging::ResolveContext::current().system);
            let files = rootbeer_packaging::candidate_files(&bundle, &system)?;
            serde_json::to_writer(&mut output, &files).map_err(|error| error.to_string())?;
            writeln!(output).map_err(|error| error.to_string())?;
        }
        Command::ImportResults {
            bundle,
            cache,
            system,
        } => {
            let count = match system {
                Some(system) => {
                    rootbeer_packaging::import_results_for_system(&bundle, &cache, &system)?
                }
                None => rootbeer_packaging::import_results(&bundle, &cache)?,
            };
            writeln!(output, "imported {count} admitted qualifications")
                .map_err(|error| error.to_string())?;
        }
        Command::CheckpointResults {
            cache,
            output: destination,
            shard,
            shards,
        } => {
            let catalog = catalog()?;
            let shard = shard.map(|index| rootbeer_packaging::ExportShard {
                index,
                count: shards.unwrap(),
            });
            let count =
                rootbeer_packaging::checkpoint_results(catalog, &cache, &destination, shard)?;
            writeln!(output, "retained {count} completed qualifications")
                .map_err(|error| error.to_string())?;
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
            let manifest = rootbeer_packaging::sign_index(
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
        Command::PinEnvironment { specification } => {
            let specification: rootbeer_packaging::BuildEnvironment = serde_json::from_slice(
                &std::fs::read(specification).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            writeln!(
                output,
                "{}",
                serde_json::to_string_pretty(&specification.pin()?)
                    .map_err(|error| error.to_string())?
            )
            .map_err(|error| error.to_string())?;
        }
        Command::Audit { directory, receipt } => {
            let report = if let Some(receipt) = receipt {
                let artifact: rootbeer_packaging::BuildArtifact =
                    serde_json::from_slice(&std::fs::read(receipt).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                rootbeer_packaging::audit::audit_installed(&directory, &artifact.package)?
            } else {
                rootbeer_packaging::audit::audit(&directory)?
            };
            writeln!(
                output,
                "{}",
                serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
            )
            .map_err(|error| error.to_string())?;
            report.validate()?;
        }
        Command::Plan { name } => {
            catalog()?.validate()?;
            let graph = rootbeer_packaging::graph::DependencyGraph::new(
                catalog()?,
                &[name],
                &rootbeer_packaging::ResolveContext::current().system,
            )?;
            writeln!(
                output,
                "{}",
                serde_json::to_string_pretty(&graph).map_err(|error| error.to_string())?
            )
            .map_err(|error| error.to_string())?;
        }
        Command::Prepare {
            name,
            input_key,
            environment,
            isolate,
            output: destination,
            jobs,
            cache,
            cache_context,
            recheck,
            phase_timeout,
        } => {
            let environment = read_environment(environment)?;
            if let Some(expected) = input_key {
                if isolate || environment.is_some() {
                    return Err("package-plan currently requires the host environment".into());
                }
                let tasks = rootbeer_packaging::plan_packages(
                    catalog()?,
                    std::slice::from_ref(&name),
                    &rootbeer_packaging::BuildOptions::default(),
                    cache_context.as_deref().unwrap(),
                )?;
                if tasks.len() != 1 || tasks[0].key != expected {
                    return Err("package inputs or build environment changed since planning".into());
                }
            }
            let cache = cache.map(|directory| rootbeer_packaging::BuildCache {
                directory,
                context: cache_context.unwrap(),
                recheck,
            });
            let artifact = rootbeer_packaging::prepare_package(
                catalog()?,
                &name,
                &destination,
                &rootbeer_packaging::BuildOptions {
                    environment,
                    is_isolated: isolate,
                    jobs,
                    cache,
                    phase_timeout: std::time::Duration::from_secs(phase_timeout),
                    ..Default::default()
                },
            )?;
            writeln!(
                output,
                "prepared {} for {}\nartifact: {}",
                artifact.id(),
                rootbeer_packaging::ResolveContext::current().system,
                destination.join("package.tar.gz").display()
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn read_environment(
    path: Option<PathBuf>,
) -> Result<Option<rootbeer_packaging::BuildEnvironmentLock>, String> {
    path.map(|path| {
        serde_json::from_slice(&std::fs::read(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn publish_manifest_defaults_to_legacy_and_accepts_an_explicit_channel() {
        let argv = [
            "rootbeer-forge",
            "publish",
            "--bundle",
            "bundle",
            "--site",
            "site",
            "--site-url",
            "https://example.org",
            "--registry",
            "owner/index",
            "--repository-url",
            "https://github.com/owner/index",
            "--sequence",
            "1",
            "--key",
            "key.der",
            "--public-key",
            "public-key",
        ];
        for channel in [None, Some("latest-v2.json")] {
            let mut args = argv.to_vec();
            if let Some(channel) = channel {
                args.extend(["--manifest", channel]);
            }
            let cli = crate::Cli::try_parse_from(args).unwrap();
            let Args {
                command: Command::Publish { manifest, .. },
                ..
            } = cli.args
            else {
                panic!("expected package publish");
            };
            assert_eq!(manifest, channel.unwrap_or("latest.json"));
        }
    }
}
