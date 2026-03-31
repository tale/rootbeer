use std::process::Command;

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Target protocol: "ssh" or "https". Omit to show current remote.
    pub protocol: Option<String>,
}

pub fn run(args: Args) {
    let dir = rootbeer_core::config_dir();
    if !dir.exists() {
        eprintln!("error: config directory does not exist: {}", dir.display());
        eprintln!("hint: run `rb init` first");
        std::process::exit(1);
    }

    let current_url = get_origin_url(&dir);

    let protocol = match args.protocol {
        None => {
            println!("{current_url}");
            return;
        }
        Some(p) => p,
    };

    match protocol.as_str() {
        "ssh" => switch_protocol(&dir, &current_url, Protocol::Ssh),
        "https" => switch_protocol(&dir, &current_url, Protocol::Https),
        other => {
            eprintln!("error: unknown protocol '{other}'");
            eprintln!("hint: expected 'ssh' or 'https'");
            std::process::exit(1);
        }
    }
}

enum Protocol {
    Ssh,
    Https,
}

fn get_origin_url(dir: &std::path::Path) -> String {
    let output = Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "remote", "get-url", "origin"])
        .output()
        .unwrap_or_else(|e| {
            eprintln!("error: failed to run git: {e}");
            std::process::exit(1);
        });

    if !output.status.success() {
        eprintln!("error: failed to get origin URL");
        eprintln!("hint: is the config directory a git repository?");
        std::process::exit(1);
    }

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Extract "user/repo" from a GitHub URL, returning None for non-GitHub URLs.
fn parse_github_shorthand(url: &str) -> Option<String> {
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        return Some(rest.trim_end_matches(".git").to_string());
    }

    if let Some(rest) = url.strip_prefix("git@github.com:") {
        return Some(rest.trim_end_matches(".git").to_string());
    }

    None
}

fn switch_protocol(dir: &std::path::Path, current_url: &str, target: Protocol) {
    let shorthand = match parse_github_shorthand(current_url) {
        Some(s) => s,
        None => {
            eprintln!("error: origin is not a GitHub URL: {current_url}");
            eprintln!("hint: use `git remote set-url origin <url>` directly");
            std::process::exit(1);
        }
    };

    let (new_url, label) = match target {
        Protocol::Ssh => (format!("git@github.com:{shorthand}.git"), "SSH"),
        Protocol::Https => (format!("https://github.com/{shorthand}.git"), "HTTPS"),
    };

    if new_url == current_url {
        println!("origin is already using {label}");
        return;
    }

    let status = Command::new("git")
        .args([
            "-C",
            &dir.to_string_lossy(),
            "remote",
            "set-url",
            "origin",
            &new_url,
        ])
        .status()
        .unwrap_or_else(|e| {
            eprintln!("error: failed to run git: {e}");
            std::process::exit(1);
        });

    if !status.success() {
        eprintln!("error: failed to set origin URL");
        std::process::exit(1);
    }

    println!("origin updated to {label}: {new_url}");
}
