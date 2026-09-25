//! Setuid-root helper that inserts trees into the shared store. It reads a
//! [`rootbeer_store::stream`] on stdin, never a path, and hashes what it wrote,
//! so it needs no trust in the caller.

use std::io;
use std::path::PathBuf;

use rootbeer_store::Store;

fn main() {
    if let Err(error) = run() {
        eprintln!("rb-store: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [command, name, version] = arguments.as_slice() else {
        return Err("usage: rb-store add <name> <version> < tree".into());
    };
    if command != "add" {
        return Err(format!("unknown command {command}"));
    }

    unsafe { libc::umask(0o022) };
    let entry = Store::new(root().join("store"))
        .add_stream(name, version, &mut io::stdin().lock())
        .map_err(|error| error.to_string())?;
    println!("{}", entry.path.display());
    Ok(())
}

// Elevated, the root is fixed; unprivileged, the caller's own override is harmless.
fn root() -> PathBuf {
    let is_elevated = unsafe { libc::geteuid() != libc::getuid() };
    if is_elevated {
        return PathBuf::from(rootbeer_store::DEFAULT_ROOT);
    }
    rootbeer_store::root_dir()
}
