use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, io};

const STARTER_MANIFEST: &str = r#"local rb = require("@rootbeer")

-- Write files, create symlinks, and configure your system here.
-- See https://rootbeer.dev for documentation.

-- Example: write a simple file
-- rb.file("~/.config/example.txt", "hello from rootbeer!\n")

-- Example: symlink a file from this source directory
-- rb.link_file("config/gitconfig", "~/.gitconfig")
"#;

enum Source {
    GitHub(String),
    GitUrl(String),
    Local(PathBuf),
}

fn parse_source(s: &str) -> Source {
    // Explicit git URLs
    if s.starts_with("https://")
        || s.starts_with("http://")
        || s.starts_with("git://")
        || s.starts_with("ssh://")
        || s.starts_with("git@")
    {
        return Source::GitUrl(s.to_string());
    }

    // Local path (exists on disk)
    if Path::new(s).exists() {
        return Source::Local(PathBuf::from(s));
    }

    // GitHub shorthand: exactly one slash, no path separators that suggest a local path
    if s.matches('/').count() == 1 && !s.starts_with('/') && !s.starts_with('.') {
        return Source::GitHub(s.to_string());
    }

    eprintln!("error: could not determine source type for '{s}'");
    eprintln!("  expected: user/repo, git URL, or local path");
    std::process::exit(1);
}

pub fn run(source: Option<String>, force: bool) {
    let dest = super::source_dir();

    if force && dest.exists() {
        // Remove symlink or directory
        if dest.is_symlink() {
            fs::remove_file(&dest).unwrap_or_else(|e| {
                eprintln!("error: failed to remove {}: {e}", dest.display());
                std::process::exit(1);
            });
        } else {
            fs::remove_dir_all(&dest).unwrap_or_else(|e| {
                eprintln!("error: failed to remove {}: {e}", dest.display());
                std::process::exit(1);
            });
        }
    }

    match source {
        None => {
            if dest.exists() {
                eprintln!("error: source directory already exists: {}", dest.display());
                eprintln!("  use --force to replace it");
                std::process::exit(1);
            }

            fs::create_dir_all(&dest).unwrap_or_else(|e| {
                eprintln!("error: failed to create {}: {e}", dest.display());
                std::process::exit(1);
            });

            let manifest = dest.join("rootbeer.lua");
            fs::write(&manifest, STARTER_MANIFEST).unwrap_or_else(|e| {
                eprintln!("error: failed to write {}: {e}", manifest.display());
                std::process::exit(1);
            });

            println!("initialized new source at {}", dest.display());
            println!("  edit {} to get started", manifest.display());
        }

        Some(ref s) => {
            let url = match parse_source(s) {
                Source::GitHub(shorthand) => {
                    format!("https://github.com/{shorthand}.git")
                }
                Source::GitUrl(url) => url,
                Source::Local(path) => {
                    init_from_local(&path, &dest);
                    setup_lsp(&dest);
                    return;
                }
            };

            if dest.exists() {
                eprintln!("error: source directory already exists: {}", dest.display());
                eprintln!("  use --force to replace it");
                std::process::exit(1);
            }

            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).unwrap_or_else(|e| {
                    eprintln!("error: failed to create {}: {e}", parent.display());
                    std::process::exit(1);
                });
            }

            println!("cloning {url} ...");
            let status = Command::new("git")
                .args(["clone", &url, &dest.to_string_lossy()])
                .status()
                .unwrap_or_else(|e| {
                    eprintln!("error: failed to run git: {e}");
                    eprintln!("  is git installed?");
                    std::process::exit(1);
                });

            if !status.success() {
                eprintln!("error: git clone failed");
                std::process::exit(1);
            }

            let manifest = dest.join("rootbeer.lua");
            if !manifest.exists() {
                eprintln!(
                    "warning: no rootbeer.lua found in cloned repo at {}",
                    manifest.display()
                );
            }

            println!("initialized from {s} at {}", dest.display());
        }
    }

    setup_lsp(&dest);
}

fn setup_lsp(source_dir: &Path) {
    if let Err(e) = super::lsp::ensure_luaurc(source_dir) {
        eprintln!("warning: failed to set up LSP type definitions: {e}");
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

    let manifest = canonical.join("rootbeer.lua");
    if !manifest.exists() {
        eprintln!("warning: no rootbeer.lua found in {}", canonical.display());
    }

    if dest.exists() {
        eprintln!("error: source directory already exists: {}", dest.display());
        eprintln!("  use --force to replace it");
        std::process::exit(1);
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

fn symlink(src: &Path, dst: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(src, dst)
}
