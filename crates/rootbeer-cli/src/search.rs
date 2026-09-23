use rootbeer_core::package::repository::ENGINE_LEVEL;
use rootbeer_core::package::{Repository, RepositoryResolver, ResolveContext};

#[derive(clap::Args, Debug)]
pub struct Args {
    #[arg(default_value = "")]
    query: String,
    /// Include packages published for other platforms
    #[arg(long)]
    all_platforms: bool,
    /// Print matching package metadata as JSON
    #[arg(long)]
    json: bool,
}

pub fn run(args: Args) {
    if let Err(error) = search(args) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn search(args: Args) -> Result<(), String> {
    let selection =
        Repository::chosen(None)?.select(&rootbeer_core::package::state_dir(), false)?;
    if let Some(notice) = selection.notice {
        eprintln!("{notice}");
    }
    let resolver = RepositoryResolver::new(&selection.pin);
    let root = resolver.root()?;
    let system = ResolveContext::current().system;
    let query = args.query.to_lowercase();
    let mut matches = Vec::new();
    for (name, package) in &root.packages {
        let platforms: Vec<_> = package
            .platforms
            .iter()
            .filter(|(platform, _)| args.all_platforms || **platform == system)
            .collect();
        if platforms.is_empty() {
            continue;
        }
        let text = format!(
            "{name} {} {} {}",
            package.aliases.join(" "),
            package.description,
            platforms
                .iter()
                .flat_map(|(_, platform)| &platform.commands)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
        )
        .to_lowercase();
        if !query.split_whitespace().all(|term| text.contains(term)) {
            continue;
        }
        let versions: std::collections::BTreeMap<_, _> = platforms
            .iter()
            .map(|(platform, entry)| (platform.as_str(), entry.version.as_str()))
            .collect();
        matches.push(serde_json::json!({
            "name": name,
            "description": package.description,
            "homepage": package.homepage,
            "license": package.license,
            "version": package.platforms.get(&system).map(|platform| &platform.version),
            "platforms": versions,
            "needs_newer_rb": package.min_engine_level.is_some_and(|level| level > ENGINE_LEVEL),
        }));
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&matches).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    for package in matches {
        let marker = if package["needs_newer_rb"] == true {
            "\t(needs newer rb)"
        } else {
            ""
        };
        println!(
            "{}\t{}\t{}{marker}",
            package["name"].as_str().unwrap(),
            package["version"].as_str().unwrap_or("-"),
            package["description"].as_str().unwrap()
        );
    }
    Ok(())
}
