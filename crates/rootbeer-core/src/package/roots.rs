//! Seeds garbage collection roots for installs made before roots existed.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::lockfile::RootbeerLock;
use super::{profile, runtime, standalone};
use crate::store::Store;

/// Writes whichever of this user's roots are missing from what is installed:
/// the user profile's lock, and for the configuration its lock plus every
/// store entry the profile still links to.
pub fn seed(store: &Store) -> io::Result<()> {
    if !store.has_root("user") {
        standalone::write_user_root(store)?;
    }
    if store.has_root("configuration") {
        return Ok(());
    }

    let lock_path = crate::config_dir().join("rootbeer.lock");
    let mut live = match RootbeerLock::read_compatible(&lock_path) {
        Ok((lock, _)) => lock_paths(store, &lock),
        Err(_) => BTreeSet::new(),
    };
    live.extend(linked_entries(store, &profile::bin_dir())?);
    store.write_root("configuration", &live)
}

fn lock_paths(store: &Store, lock: &RootbeerLock) -> BTreeSet<PathBuf> {
    let mut paths = BTreeSet::new();
    for package in lock.packages.values() {
        let Ok(dependencies) = runtime::closure(package) else {
            continue;
        };
        for entry in dependencies.into_iter().chain(std::iter::once(package)) {
            if let Ok(directory) = runtime::store_directory(entry) {
                paths.insert(store.root().join(directory));
            }
        }
    }
    paths
}

fn linked_entries(store: &Store, bin_dir: &Path) -> io::Result<BTreeSet<PathBuf>> {
    let links = match fs::read_dir(bin_dir) {
        Ok(links) => links,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => return Err(error),
    };
    // TODO(legacy-store): links made before the move still point at the old store.
    let legacy = crate::state_dir().join("store");

    let mut entries = BTreeSet::new();
    for link in links {
        let Ok(target) = fs::read_link(link?.path()) else {
            continue;
        };
        let relative = target
            .strip_prefix(store.root())
            .or_else(|_| target.strip_prefix(&legacy));
        if let Some(entry) = relative.ok().and_then(|path| path.iter().next()) {
            entries.insert(store.root().join(entry));
        }
    }
    Ok(entries)
}
