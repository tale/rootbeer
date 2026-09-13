use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::Args;
use rootbeer_core::package::{import_github_packages, GitHubUpstream, PackageCatalog};

#[derive(Args, Debug)]
pub(super) struct ImportArgs {
    /// GitHub project to discover, e.g. github:owner/repository
    #[arg(required_unless_present = "upstreams", conflicts_with = "upstreams")]
    source: Option<String>,
    /// Batch import saved upstream Lua definitions
    #[arg(long)]
    upstreams: Option<PathBuf>,
    /// Canonical Rootbeer name; defaults to the lowercase repository name
    #[arg(long, requires = "source")]
    name: Option<String>,
    /// Exported command; repeat for multiple commands
    #[arg(long = "bin", requires = "source")]
    bins: Vec<String>,
    #[arg(long = "alias", requires = "source")]
    aliases: Vec<String>,
    /// Check as a JSON argument array; defaults to --version for each command
    #[arg(long = "check", requires = "source")]
    checks: Vec<String>,
    /// Target system; repeat to limit discovery (default: all four supported targets)
    #[arg(long = "system", requires = "source")]
    systems: Vec<String>,
    /// SYSTEM=NAME with optional {tag} and {version} placeholders
    #[arg(long = "asset", requires = "source")]
    assets: Vec<String>,
    /// Only consider tags with this prefix, stripping it from canonical versions
    #[arg(long, requires = "source")]
    tag_prefix: Option<String>,
    #[arg(long, requires = "source")]
    description: Option<String>,
    #[arg(long, requires = "source")]
    homepage: Option<String>,
    /// New directory for candidate recipes/ and reusable upstreams/
    #[arg(long)]
    output: PathBuf,
    /// Maximum release-history pages per project, 100 releases per page
    #[arg(long, default_value_t = 20)]
    max_pages: usize,
}

pub(super) fn run(args: ImportArgs, catalog: &PackageCatalog) -> Result<PackageCatalog, String> {
    let definitions = match args.upstreams {
        Some(directory) => GitHubUpstream::from_directory(&directory)?,
        None => {
            let source = args.source.as_deref().unwrap();
            let repository = source
                .strip_prefix("github:")
                .ok_or("source must be github:owner/repository")?;
            let name = args.name.unwrap_or_else(|| {
                repository
                    .rsplit('/')
                    .next()
                    .unwrap_or(repository)
                    .to_ascii_lowercase()
            });
            if args.bins.is_empty() {
                return Err(
                    "specify at least one --bin; exported commands must be explicit".into(),
                );
            }
            let mut upstream = GitHubUpstream::new(name, repository.into(), args.bins);
            upstream.aliases = args.aliases;
            upstream.tag_prefix = args.tag_prefix;
            upstream.description = args.description;
            upstream.homepage = args.homepage;
            if !args.systems.is_empty() {
                upstream.systems = args.systems;
            }
            if !args.checks.is_empty() {
                upstream.checks = args
                    .checks
                    .iter()
                    .map(|check| {
                        serde_json::from_str(check).map_err(|e| format!("invalid --check: {e}"))
                    })
                    .collect::<Result<_, _>>()?;
            }
            let mut assets = BTreeMap::new();
            for asset in args.assets {
                let (system, pattern) =
                    asset.split_once('=').ok_or("--asset must be SYSTEM=NAME")?;
                if assets
                    .insert(system.to_string(), pattern.to_string())
                    .is_some()
                {
                    return Err(format!("duplicate asset rule for {system}"));
                }
            }
            upstream.assets = assets;
            vec![upstream]
        }
    };
    import_github_packages(catalog, &definitions, &args.output, args.max_pages)
}
