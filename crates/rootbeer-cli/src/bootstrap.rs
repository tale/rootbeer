use std::fs;
use std::io::{self, IsTerminal};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use rootbeer_core::package::{profile, standalone, PackageRequest};
use rootbeer_core::store::{layout, root_dir, state_dir, Store, StoreManifest};

use crate::update::{self, Installation};

/// Brings the machine-wide root up to date before a command touches the store.
/// The common case is a few `stat`s and no output.
pub fn ensure() -> Result<(), String> {
    let root = root_dir();
    let store = root.join("store");
    if !store.is_dir() {
        create_root(&root, &store)?;
    }

    // A store owned by another user stays usable for reading; writes fail on use.
    let lock = match fs::File::create(root.join(".lock")) {
        Ok(lock) => lock,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return Ok(()),
        Err(error) => return Err(format!("{}: {error}", root.display())),
    };
    lock.lock().map_err(|error| error.to_string())?;

    match layout::read(&root).map_err(|error| error.to_string())? {
        Some(version) if version > layout::VERSION => {
            return Err(format!(
                "{} uses layout v{version}, newer than this rootbeer understands (v{}); update rootbeer",
                root.display(),
                layout::VERSION
            ));
        }
        Some(layout::VERSION) => {}
        _ => layout::write(&root).map_err(|error| error.to_string())?,
    }

    migrate_legacy_store(&store).map_err(|error| format!("migrating the legacy store: {error}"))
}

/// Installs the published `rootbeer` into the user profile when running from a
/// downloaded copy, so `rb` owns and updates itself from then on.
pub fn adopt() {
    if cfg!(debug_assertions) {
        return;
    }
    let Ok((executable, Installation::Standalone)) = update::detect() else {
        return;
    };

    let bin = profile::user_dir().join("bin");
    let is_installed = bin.join("rb").exists();
    if !is_installed {
        eprintln!("installing rootbeer into your profile...");
        let request = PackageRequest::parse("rootbeer");
        let result = standalone::prepare_with_resolver(
            &[request],
            true,
            false,
            false,
            rootbeer_build::consumer::self_update_resolver_stack,
        );
        if let Err(error) = result {
            eprintln!("warning: could not install rootbeer into your profile: {error}");
            return;
        }
    }

    let home = std::env::var_os("HOME").map(PathBuf::from);
    let is_linked = match home.map(|home| link_legacy_bin(&home, &executable, &bin)) {
        Some(Ok(is_linked)) => is_linked,
        Some(Err(error)) => {
            eprintln!("warning: could not link ~/.rootbeer/bin to your profile: {error}");
            false
        }
        None => false,
    };
    if !is_installed && !is_linked && !is_on_path(&bin) {
        eprintln!(
            "Run eval \"$({}/rb env)\" to add installed commands to this shell.",
            bin.display()
        );
    }
}

fn is_on_path(directory: &Path) -> bool {
    let Ok(directory) = fs::canonicalize(directory) else {
        return false;
    };
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .any(|entry| fs::canonicalize(entry).is_ok_and(|entry| entry == directory))
}

// TODO(legacy-install): remove once `rb.sh` installs into ~/.rootbeer/bin are gone.
/// Replaces the old `rb.sh` install directory with a link to the profile's `bin`,
/// so existing PATH lines keep working and run the profile's `rb`.
fn link_legacy_bin(home: &Path, executable: &Path, bin: &Path) -> io::Result<bool> {
    let legacy = home.join(".rootbeer/bin");
    let Ok(metadata) = fs::symlink_metadata(&legacy) else {
        return Ok(false);
    };
    if !metadata.is_dir()
        || fs::canonicalize(legacy.join("rb")).ok().as_deref() != Some(executable)
        || fs::read_dir(&legacy)?.count() != 1
    {
        return Ok(false);
    }

    let link = home.join(".rootbeer/.bin.link");
    let _ = fs::remove_file(&link);
    symlink(bin, &link)?;
    fs::remove_file(legacy.join("rb"))?;
    fs::remove_dir(&legacy)?;
    fs::rename(link, legacy)?;
    Ok(true)
}

fn create_root(root: &Path, store: &Path) -> Result<(), String> {
    match fs::create_dir_all(store) {
        Ok(()) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {}
        Err(error) => return Err(format!("{}: {error}", store.display())),
    }

    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    let arguments = [
        "install".to_string(),
        "-d".into(),
        "-o".into(),
        uid.to_string(),
        "-g".into(),
        gid.to_string(),
        root.display().to_string(),
        store.display().to_string(),
    ];
    let command = format!("sudo {}", arguments.join(" "));
    if !io::stdin().is_terminal() {
        return Err(format!(
            "{} does not exist; create it once for this machine with:\n  {command}",
            store.display()
        ));
    }

    eprintln!(
        "rootbeer needs to create {} once for this machine:",
        root.display()
    );
    eprintln!("  {command}");
    let status = Command::new("sudo")
        .args(&arguments)
        .status()
        .map_err(|error| format!("running sudo: {error}"))?;
    if !status.success() {
        return Err(format!("`{command}` failed"));
    }
    Ok(())
}

fn migrate_legacy_store(store: &Path) -> io::Result<()> {
    let legacy = state_dir().join("store");
    let marker = legacy.join(".migrated");
    if !legacy.is_dir() || marker.exists() || fs::canonicalize(&legacy)? == fs::canonicalize(store)?
    {
        return Ok(());
    }

    eprintln!(
        "moving packages from {} to {}...",
        legacy.display(),
        store.display()
    );
    let source = Store::new(&legacy);
    let target = Store::new(store);
    for entry in fs::read_dir(&legacy)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if let Some(pending) = name
            .strip_prefix('.')
            .and_then(|name| name.strip_suffix(".link"))
        {
            finish_interrupted(&path, &legacy.join(pending))?;
            continue;
        }
        if name.starts_with('.') || entry.file_type()?.is_symlink() {
            continue;
        }

        let manifest = match source.verify_entry(&path) {
            Ok(manifest) => manifest,
            Err(error) => {
                eprintln!("warning: leaving {} in place: {error}", path.display());
                continue;
            }
        };

        // The link is staged first so an interrupted move can still be finished.
        let destination = store.join(&name);
        let link = legacy.join(format!(".{name}.link"));
        let _ = fs::remove_file(&link);
        symlink(&destination, &link)?;
        if destination.exists() {
            target.verify_entry(&destination)?;
            fs::remove_dir_all(&path)?;
        } else {
            move_entry(&path, &destination, &target, manifest)?;
        }
        fs::rename(&link, &path)?;
    }

    fs::write(marker, "")
}

fn finish_interrupted(link: &Path, entry: &Path) -> io::Result<()> {
    if fs::symlink_metadata(entry).is_ok() {
        return fs::remove_file(link);
    }
    fs::rename(link, entry)
}

fn move_entry(
    path: &Path,
    destination: &Path,
    target: &Store,
    manifest: StoreManifest,
) -> io::Result<()> {
    match fs::rename(path, destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            target.add_tree(manifest.name, manifest.version, path)?;
            fs::remove_dir_all(path)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_install(home: &Path) -> PathBuf {
        let legacy = home.join(".rootbeer/bin");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("rb"), "rb").unwrap();
        fs::canonicalize(legacy.join("rb")).unwrap()
    }

    #[test]
    fn links_the_legacy_install_directory_to_the_profile() {
        let home = tempfile::tempdir().unwrap();
        let executable = legacy_install(home.path());
        let bin = home.path().join("profile/bin");
        fs::create_dir_all(&bin).unwrap();

        assert!(link_legacy_bin(home.path(), &executable, &bin).unwrap());
        assert_eq!(
            fs::read_link(home.path().join(".rootbeer/bin")).unwrap(),
            bin
        );
    }

    #[test]
    fn leaves_legacy_directories_it_does_not_own() {
        let home = tempfile::tempdir().unwrap();
        let executable = legacy_install(home.path());
        let bin = home.path().join("profile/bin");
        fs::write(home.path().join(".rootbeer/bin/other"), "").unwrap();

        assert!(!link_legacy_bin(home.path(), &executable, &bin).unwrap());
        assert!(!link_legacy_bin(home.path(), &home.path().join("elsewhere"), &bin).unwrap());
        assert!(home.path().join(".rootbeer/bin/rb").is_file());
    }
}
