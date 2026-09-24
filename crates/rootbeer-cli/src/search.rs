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
        let rank = rank(&query, name, &package.aliases);
        matches.push((rank, serde_json::json!({
            "name": name,
            "description": package.description,
            "homepage": package.homepage,
            "license": package.license,
            "version": package.platforms.get(&system).map(|platform| &platform.version),
            "platforms": versions,
            "needs_newer_rb": package.min_engine_level.is_some_and(|level| level > ENGINE_LEVEL),
        })));
    }
    for (name, package) in &root.unreadable {
        let text = format!(
            "{name} {} {}",
            package.aliases.join(" "),
            package.description
        )
        .to_lowercase();
        if !query.split_whitespace().all(|term| text.contains(term)) {
            continue;
        }
        let rank = rank(&query, name, &package.aliases);
        matches.push((
            rank,
            serde_json::json!({
                "name": name,
                "description": package.description,
                "needs_newer_rb": true,
            }),
        ));
    }
    matches.sort_by(|(a_rank, a), (b_rank, b)| {
        a_rank
            .cmp(b_rank)
            .then_with(|| a["name"].as_str().cmp(&b["name"].as_str()))
    });
    let matches: Vec<_> = matches.into_iter().map(|(_, package)| package).collect();
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

/// Orders an exact name or alias match first, then a name or alias prefix, then any other hit.
fn rank(query: &str, name: &str, aliases: &[String]) -> u8 {
    let query = query.trim();
    let names: Vec<_> = std::iter::once(name)
        .chain(aliases.iter().map(String::as_str))
        .map(str::to_lowercase)
        .collect();
    if names.iter().any(|candidate| *candidate == query) {
        return 0;
    }
    if names.iter().any(|candidate| candidate.starts_with(query)) {
        return 1;
    }
    2
}

#[cfg(test)]
mod tests {
    use super::rank;

    #[test]
    fn ranks_exact_then_prefix_then_substring() {
        let aliases = vec!["rg".to_string()];
        assert_eq!(rank("rg", "ripgrep", &aliases), 0);
        assert_eq!(rank("ripgrep", "ripgrep", &[]), 0);
        assert_eq!(rank("rip", "ripgrep", &[]), 1);
        assert_eq!(rank("rg", "argc", &[]), 2);
        assert_eq!(rank("rg", "RipGrep", &[]), 2);
        assert_eq!(rank("ripgrep", "RipGrep", &[]), 0);
    }
}
