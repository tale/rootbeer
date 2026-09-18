use std::fs;
use std::path::{Path, PathBuf};

use rootbeer_core::package::{profile, standalone, PackageRequest};

#[derive(Debug, PartialEq, Eq)]
enum Installation {
    Standalone,
    UserProfile,
    Configuration,
    Store,
}

pub fn run() {
    if let Err(error) = update() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn update() -> Result<(), String> {
    let executable = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| error.to_string())?;
    let invocation = invocation_path()?;
    let user_profile = profile::user_dir();
    let installation = installation(&executable, &invocation, &user_profile, &profile::dir());
    match installation {
        Installation::Configuration => {
            return Err("Rootbeer is managed by your Lua configuration; run `rb apply --update` from that configuration to update its lock and profile. Explicit version pins remain unchanged.".into());
        }
        Installation::Store => {
            return Err("this executable belongs to a package store or an unrecognized symlink installation; run rb through its owning profile to update it. Store contents cannot be overwritten.".into());
        }
        _ => {}
    }

    let is_persistent = installation == Installation::UserProfile;
    let request = if is_persistent {
        standalone::installed_request(&user_profile, "rootbeer")
            .map_err(|error| error.to_string())?
            .ok_or("this profile predates saved package requests; explicitly select your desired version with `rb use rootbeer --update` (or rootbeer@VERSION) before using `rb self-update`")?
    } else {
        PackageRequest::parse("rootbeer")
    };
    let environment = if is_persistent {
        standalone::prepare_with_resolver(
            &[request],
            true,
            false,
            true,
            rootbeer_build::consumer::resolver_stack_for_inputs,
        )?
    } else {
        // Standalone updates require a published artifact from the signed index.
        standalone::prepare(&[request], false, false, true)?
    };
    let package = &environment.packages[0];
    if !is_persistent {
        let binary = package
            .bins
            .get("rb")
            .ok_or("Rootbeer package does not export rb")?;
        replace_binary(&executable, binary).map_err(|error| error.to_string())?;
    }
    println!("rootbeer updated to {}", package.package.version);
    Ok(())
}

fn invocation_path() -> Result<PathBuf, String> {
    let argument = std::env::args_os()
        .next()
        .ok_or("missing executable name")?;
    let path = PathBuf::from(&argument);
    if path.components().count() > 1 || path.is_absolute() {
        return std::path::absolute(path).map_err(|error| error.to_string());
    }
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join(&argument))
        .find(|path| path.is_file())
        .ok_or_else(|| "cannot locate the invoked rb command in PATH".into())
}

fn installation(
    executable: &Path,
    invocation: &Path,
    user: &Path,
    configuration: &Path,
) -> Installation {
    let parent = invocation
        .parent()
        .and_then(|path| path.canonicalize().ok());
    let is_running = invocation.canonicalize().ok().as_deref() == Some(executable);
    for (profile, owner) in [
        (user, Installation::UserProfile),
        (configuration, Installation::Configuration),
    ] {
        if is_running && parent.is_some() && parent == profile.join("bin").canonicalize().ok() {
            return owner;
        }
    }
    if executable
        .ancestors()
        .any(|path| path.join(".rootbeer/manifest.json").exists())
        || executable.starts_with(rootbeer_core::store::Store::default().root())
        || fs::symlink_metadata(invocation).is_ok_and(|metadata| metadata.file_type().is_symlink())
        || !is_running
    {
        return Installation::Store;
    }
    Installation::Standalone
}

fn replace_binary(destination: &Path, source: &Path) -> std::io::Result<()> {
    let directory = destination
        .parent()
        .ok_or_else(|| std::io::Error::other("executable has no parent directory"))?;
    let temporary = tempfile::NamedTempFile::new_in(directory)?;
    fs::copy(source, temporary.path())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn ownership_uses_the_invoked_profile_even_when_both_share_a_store_entry() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let entry = root.join("store/entry");
        fs::create_dir_all(entry.join(".rootbeer")).unwrap();
        fs::write(entry.join(".rootbeer/manifest.json"), "{}").unwrap();
        let executable = entry.join("rb");
        fs::write(&executable, "old").unwrap();
        let user = root.join("user");
        let configuration = root.join("configuration");
        for profile in [&user, &configuration] {
            fs::create_dir_all(profile.join("bin")).unwrap();
            symlink(&executable, profile.join("bin/rb")).unwrap();
        }
        assert_eq!(
            installation(&executable, &user.join("bin/rb"), &user, &configuration),
            Installation::UserProfile
        );
        assert_eq!(
            installation(
                &executable,
                &configuration.join("bin/rb"),
                &user,
                &configuration
            ),
            Installation::Configuration
        );
        assert_eq!(
            installation(&executable, &executable, &user, &configuration),
            Installation::Store
        );
        let alias = root.join("alias");
        symlink(&executable, &alias).unwrap();
        assert_eq!(
            installation(&executable, &alias, &user, &configuration),
            Installation::Store
        );
        assert_eq!(fs::read(&executable).unwrap(), b"old");
    }

    #[test]
    fn standalone_replacement_preserves_old_binary_on_failure_and_replaces_symlinks_safely() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let executable = root.join("rb");
        let source = root.join("new");
        fs::write(&executable, "old").unwrap();
        assert_eq!(
            installation(
                &executable,
                &executable,
                &root.join("user"),
                &root.join("configuration")
            ),
            Installation::Standalone
        );
        assert!(replace_binary(&executable, &source).is_err());
        assert_eq!(fs::read(&executable).unwrap(), b"old");
        fs::write(&source, "new").unwrap();
        replace_binary(&executable, &source).unwrap();
        assert_eq!(fs::read(&executable).unwrap(), b"new");
        fs::remove_file(&executable).unwrap();
        symlink(&source, &executable).unwrap();
        let replacement = root.join("replacement");
        fs::write(&replacement, "replacement").unwrap();
        replace_binary(&executable, &replacement).unwrap();
        assert_eq!(fs::read(&source).unwrap(), b"new");
        assert_eq!(fs::read(&executable).unwrap(), b"replacement");
    }
}
