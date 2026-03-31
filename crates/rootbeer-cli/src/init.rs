use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Source to initialize from: GitHub shorthand (user/repo), git URL, or local path.
    /// When omitted, creates a fresh config directory with a starter init.lua.
    pub source: Option<String>,

    /// Overwrite a pre-existing config directory (use with caution!)
    #[arg(short, long)]
    pub force: bool,

    /// Clone GitHub shorthand repos via SSH instead of HTTPS
    #[arg(long)]
    pub ssh: bool,
}

const STARTER_MANIFEST: &str = r#"local rb = require("rootbeer")

-- Write files, create symlinks, and configure your system here.
-- See https://rootbeer.tale.me for documentation.

-- Example: write a simple file
-- rb.file("~/.config/example.txt", "hello from rootbeer!\n")

-- Example: symlink a file from this source directory
-- rb.link_file("config/gitconfig", "~/.gitconfig")
"#;

enum InitSource {
    Fresh,
    GitHub(String),
    GitUrl(String),
    Local(PathBuf),
}

/// Parses the user-provided source string into an InitSource:
/// 1. If missing, treat as Fresh (create starter manifest)
/// 2. If it looks like a git URL, treat it as one
/// 3. If it exists on disk, treat it as a local path
/// 4. If it looks like a GitHub user/repo, treat it as shorthand
/// 5. If it is a single word, assume <user>/dotfiles on GitHub
/// 6. Otherwise, error out
fn parse_source(s: Option<String>) -> InitSource {
    let s = match s {
        None => return InitSource::Fresh,
        Some(s) => s,
    };

    if s.starts_with("https://")
        || s.starts_with("http://")
        || s.starts_with("git://")
        || s.starts_with("ssh://")
        || s.starts_with("git@")
    {
        return InitSource::GitUrl(s);
    }

    if Path::new(&s).exists() {
        return InitSource::Local(PathBuf::from(s));
    }

    if s.matches('/').count() == 1 && !s.starts_with('/') && !s.starts_with('.') {
        return InitSource::GitHub(s);
    }

    if !s.contains('/') && !s.contains('\\') {
        return InitSource::GitHub(format!("{s}/dotfiles"));
    }

    eprintln!("error: could not determine source type for '{s}'");
    eprintln!("hint: expected user/repo, git URL, or local path");
    std::process::exit(1);
}

pub fn run(args: Args) {
    let dest = rootbeer_core::config_dir();

    if dest.exists() {
        if !args.force {
            eprintln!("error: config directory already exists: {}", dest.display());
            eprintln!("hint: use --force to replace it");
            std::process::exit(1);
        }

        if dest.is_symlink() {
            fs::remove_file(&dest).unwrap_or_else(|e| {
                eprintln!("error: failed to remove existing symlink: {e}");
                std::process::exit(1);
            });
        } else {
            fs::remove_dir_all(&dest).unwrap_or_else(|e| {
                eprintln!("error: failed to remove existing directory: {e}");
                std::process::exit(1);
            });
        }
    }

    match parse_source(args.source) {
        InitSource::Fresh => init_fresh(&dest),
        InitSource::Local(path) => init_from_local(&path, &dest),

        InitSource::GitHub(shorthand) => {
            let url = if args.ssh {
                format!("git@github.com:{shorthand}.git")
            } else {
                format!("https://github.com/{shorthand}.git")
            };
            clone_git(&url, &dest);
            println!("initialized from {shorthand} at {}", dest.display());
        }

        InitSource::GitUrl(url) => {
            clone_git(&url, &dest);
            println!("initialized from {url} at {}", dest.display());
        }
    }

    setup_lsp(&dest);
    println!("hint: run `rb apply` to apply your configuration");
}

fn init_fresh(dest: &Path) {
    fs::create_dir_all(dest).unwrap_or_else(|e| {
        eprintln!("error: failed to create {}: {e}", dest.display());
        std::process::exit(1);
    });

    let manifest = dest.join("init.lua");
    fs::write(&manifest, STARTER_MANIFEST).unwrap_or_else(|e| {
        eprintln!("error: failed to write {}: {e}", manifest.display());
        std::process::exit(1);
    });

    println!("initialized new config at {}", dest.display());
    println!("hint: edit {} to get started", manifest.display());
}

fn clone_git(url: &str, dest: &Path) {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| {
            eprintln!("error: failed to create {}: {e}", parent.display());
            std::process::exit(1);
        });
    }

    println!("cloning {url} ...");
    let status = Command::new("git")
        .args(["clone", url, &dest.to_string_lossy()])
        .status()
        .unwrap_or_else(|e| {
            eprintln!("error: failed to run git: {e}");
            eprintln!("hint: is git installed?");
            std::process::exit(1);
        });

    if !status.success() {
        eprintln!("error: git clone failed");
        eprintln!("hint: if this is a private repo, try one of:");
        eprintln!("  rb init --ssh user/repo");
        eprintln!("  rb init git@github.com:user/repo.git");
        std::process::exit(1);
    }

    let manifest = dest.join("init.lua");
    if !manifest.exists() {
        eprintln!(
            "warning: no init.lua found in cloned repo at {}",
            dest.display()
        );
    }
}

fn init_from_local(path: &Path, dest: &Path) {
    let canonical = path.canonicalize().unwrap_or_else(|e| {
        eprintln!("error: invalid path {}: {e}", path.display());
        std::process::exit(1);
    });

    if !canonical.is_dir() {
        eprintln!("error: {} is not a directory", canonical.display());
        std::process::exit(1);
    }

    let manifest = canonical.join("init.lua");
    if !manifest.exists() {
        eprintln!("warning: no init.lua found in {}", canonical.display());
    }

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| {
            eprintln!("error: failed to create {}: {e}", parent.display());
            std::process::exit(1);
        });
    }

    symlink(&canonical, dest).unwrap_or_else(|e| {
        eprintln!(
            "error: failed to symlink {} -> {}: {e}",
            dest.display(),
            canonical.display()
        );

        std::process::exit(1);
    });

    println!(
        "initialized from {} (symlinked to {})",
        canonical.display(),
        dest.display()
    );
}

fn setup_lsp(config_dir: &Path) {
    super::typegen::setup(config_dir);
}
