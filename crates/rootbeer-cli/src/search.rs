use rootbeer_core::package::{
    discovery::DiscoveryResolver, official, ResolveContext, ResolverInput,
};

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
    let selection = official::select_default(false)?;
    if let Some(notice) = selection.notice {
        eprintln!("{notice}");
    }
    let ResolverInput::Discovery(pin) = selection.input else {
        return Err("the selected registry has not published package discovery yet".into());
    };
    let resolver = DiscoveryResolver::new(&pin);
    let manifest = resolver.manifest()?;
    let system = ResolveContext::current().system;
    let query = args.query.to_lowercase();
    let mut matches = Vec::new();
    for package in manifest.catalog.packages.values() {
        let versions: Vec<_> = package
            .versions
            .iter()
            .filter(|(version, _)| {
                manifest
                    .records
                    .get(&format!("{}@{version}", package.name))
                    .is_some_and(|platforms| args.all_platforms || platforms.contains_key(&system))
            })
            .map(|(version, _)| version.as_str())
            .collect();
        let text = format!(
            "{} {} {} {}",
            package.name,
            package.aliases.join(" "),
            package.description,
            versions
                .iter()
                .flat_map(|version| package.versions[*version].for_system(&system).bins)
                .collect::<Vec<_>>()
                .join(" ")
        )
        .to_lowercase();
        if versions.is_empty() || !query.split_whitespace().all(|term| text.contains(term)) {
            continue;
        }
        matches.push(serde_json::json!({
            "name": package.name, "description": package.description, "versions": versions,
            "default_version": package.default_version_for(&system), "homepage": package.homepage,
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
        println!(
            "{}\t{}\t{}",
            package["name"].as_str().unwrap(),
            package["default_version"].as_str().unwrap(),
            package["description"].as_str().unwrap()
        );
    }
    Ok(())
}
