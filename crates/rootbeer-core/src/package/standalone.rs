use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use super::applications::Applications;
use super::lockfile::RootbeerLock;
use super::{
    resolver_stack_for_inputs, LockedPackage, PackageIntent, PackageLockBuilder, PackageLockInput,
    PackageRealizer, PackageRequest, PackageResolverInputs, RealizedPackage, ResolveContext,
};
use crate::store::{hash_bytes, Store};

/// Packages realized independently of a Lua configuration.
pub struct Environment {
    pub packages: Vec<RealizedPackage>,
    pub bin_dir: PathBuf,
}

/// Resolves and caches an isolated command environment, or adds packages to the user profile.
pub fn prepare(
    requests: &[PackageRequest],
    is_persistent: bool,
    is_offline: bool,
    should_update: bool,
) -> Result<Environment, String> {
    prepare_with_resolver(
        requests,
        is_persistent,
        is_offline,
        should_update,
        resolver_stack_for_inputs,
    )
}

/// Prepares a command environment using the caller's package resolver.
pub fn prepare_with_resolver(
    requests: &[PackageRequest],
    is_persistent: bool,
    is_offline: bool,
    should_update: bool,
    resolver: fn(&PackageResolverInputs) -> super::ResolverStack,
) -> Result<Environment, String> {
    if requests.is_empty() {
        return Err("at least one package is required".into());
    }
    if is_offline && should_update {
        return Err("--offline cannot be combined with --update".into());
    }

    let state = crate::state_dir();
    let root = state.join("standalone");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let guard = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("lock"))
        .map_err(|e| e.to_string())?;
    guard.lock().map_err(|e| e.to_string())?;

    let realizer = if is_offline {
        PackageRealizer::offline(Store::default())
    } else {
        PackageRealizer::default()
    };
    let mut packages = Vec::new();
    for request in requests {
        let identity =
            serde_json::to_vec(&(ResolveContext::current(), request)).map_err(|e| e.to_string())?;
        let lock_path = root.join("requests").join(hash_bytes(&identity));
        let cached = read_lock(&lock_path).map_err(|e| e.to_string())?;
        let lock = match cached {
            Some(lock) if !should_update => lock,
            _ if is_offline => {
                return Err(format!(
                    "{request} is not cached; run without --offline first"
                ))
            }
            _ => {
                let mut inputs = PackageResolverInputs::default();
                if request
                    .resolver
                    .as_deref()
                    .is_none_or(|name| name == "rootbeer")
                {
                    let selection = super::official::select_default(should_update)?;
                    if let Some(notice) = selection.notice {
                        eprintln!("{notice}");
                    }
                    inputs.resolvers.insert("rootbeer".into(), selection.input);
                }
                let builder = PackageLockBuilder::new_with_inputs(
                    resolver(&inputs),
                    realizer.clone(),
                    ResolveContext::current(),
                    inputs.clone(),
                );
                let input = PackageLockInput::with_resolver_inputs(
                    ResolveContext::current(),
                    inputs,
                    vec![PackageIntent::request(request.clone())],
                );
                let lock = builder.build(&input).map_err(|e| e.to_string())?;
                write_lock(&lock, &lock_path).map_err(|e| e.to_string())?;
                lock
            }
        };
        let package = lock
            .package_for_request(request, &ResolveContext::current())
            .map_err(|e| e.to_string())?;
        packages.push(realizer.realize(package).map_err(|e| e.to_string())?);
    }

    if is_persistent {
        let profile = super::profile::user_dir();
        install_profile(
            &root,
            &profile,
            &packages,
            requests,
            &realizer,
            &Applications::default(),
        )
        .map_err(|e| e.to_string())?;
        return Ok(Environment {
            packages,
            bin_dir: profile.join("bin"),
        });
    }

    let generation = write_environment(&root, &packages).map_err(|e| e.to_string())?;
    Ok(Environment {
        packages,
        bin_dir: generation.join("bin"),
    })
}

fn install_profile(
    root: &Path,
    profile: &Path,
    packages: &[RealizedPackage],
    requests: &[PackageRequest],
    realizer: &PackageRealizer,
    applications: &Applications,
) -> io::Result<()> {
    let mut selections = read_requests(profile)?;
    for (package, request) in packages.iter().zip(requests) {
        selections.insert(package.package.name.clone(), request.clone());
    }
    let current = read_lock(&profile.join("packages.json"))?;
    let mut installed: BTreeMap<String, LockedPackage> = current
        .into_iter()
        .flat_map(|lock| lock.packages.into_values())
        .map(|package| (package.name.clone(), package))
        .collect();
    for package in packages {
        installed.insert(package.package.name.clone(), package.package.clone());
    }
    let realized = installed
        .values()
        .map(|package| realizer.realize(package))
        .collect::<Result<Vec<_>, _>>()?;
    let generation = write_environment_with_requests(root, &realized, &selections)?;
    applications.synchronize_with("user", &app_exports(&realized)?, || {
        activate(profile, &generation)
    })
}

/// Removes packages from the persistent user profile while retaining cached downloads.
pub fn remove(names: &[String]) -> Result<(), String> {
    if names.is_empty() {
        return Err("at least one installed package name is required".into());
    }
    let root = crate::state_dir().join("standalone");
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let guard = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("lock"))
        .map_err(|error| error.to_string())?;
    guard.lock().map_err(|error| error.to_string())?;
    remove_from_profile(
        &root,
        &super::profile::user_dir(),
        names,
        &PackageRealizer::offline(Store::default()),
        &Applications::default(),
    )
    .map_err(|error| error.to_string())
}

fn remove_from_profile(
    root: &Path,
    profile: &Path,
    names: &[String],
    realizer: &PackageRealizer,
    applications: &Applications,
) -> io::Result<()> {
    let current = read_lock(&profile.join("packages.json"))?
        .ok_or_else(|| io::Error::other("no packages are installed in the user profile"))?;
    let mut installed: BTreeMap<_, _> = current
        .packages
        .into_values()
        .map(|package| (package.name.clone(), package))
        .collect();
    for name in names {
        if !installed.contains_key(name) {
            return Err(io::Error::other(format!(
                "package '{name}' is not installed in the user profile"
            )));
        }
    }
    let mut selections = read_requests(profile)?;
    for name in names {
        installed.remove(name);
        selections.remove(name);
    }
    let realized = installed
        .values()
        .map(|package| realizer.realize(package))
        .collect::<Result<Vec<_>, _>>()?;
    let generation = write_environment_with_requests(root, &realized, &selections)?;
    applications.synchronize_with("user", &app_exports(&realized)?, || {
        activate(profile, &generation)
    })
}

fn app_exports(packages: &[RealizedPackage]) -> io::Result<BTreeMap<String, PathBuf>> {
    let mut apps = BTreeMap::new();
    for package in packages {
        for (name, path) in &package.apps {
            if apps
                .insert(name.clone(), path.clone())
                .is_some_and(|previous| previous != *path)
            {
                return Err(io::Error::other(format!(
                    "packages export conflicting application '{name}'"
                )));
            }
        }
    }
    Ok(apps)
}

fn read_lock(path: &Path) -> io::Result<Option<RootbeerLock>> {
    match RootbeerLock::read(path) {
        Ok(lock) => Ok(Some(lock)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn write_lock(lock: &RootbeerLock, path: &Path) -> io::Result<()> {
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    lock.write(temporary.path())?;
    temporary.persist(path).map_err(|e| e.error)?;
    Ok(())
}

fn write_environment(root: &Path, packages: &[RealizedPackage]) -> io::Result<PathBuf> {
    write_environment_with_requests(root, packages, &BTreeMap::new())
}

fn write_environment_with_requests(
    root: &Path,
    packages: &[RealizedPackage],
    requests: &BTreeMap<String, PackageRequest>,
) -> io::Result<PathBuf> {
    let mut bins = BTreeMap::new();
    for package in packages {
        for (name, path) in &package.bins {
            if let Some(previous) = bins.insert(name.clone(), path.clone()) {
                if previous != *path {
                    return Err(io::Error::other(format!(
                        "packages export conflicting command '{name}'"
                    )));
                }
            }
        }
    }
    let lock = RootbeerLock::from_packages(packages.iter().map(|package| package.package.clone()))
        .map_err(io::Error::other)?;
    let identity = hash_bytes(&serde_json::to_vec(&(&lock, requests)).map_err(io::Error::other)?);
    let generations = root.join("environments");
    fs::create_dir_all(&generations)?;
    let destination = generations.join(identity);
    if destination.is_dir() {
        if RootbeerLock::read(destination.join("packages.json"))? != lock {
            return Err(io::Error::other(
                "cached package environment lock has changed",
            ));
        }
        if read_requests(&destination)? != *requests {
            return Err(io::Error::other("cached package requests have changed"));
        }
        for (name, path) in &bins {
            if fs::read_link(destination.join("bin").join(name))? != *path {
                return Err(io::Error::other(format!(
                    "cached command '{name}' has changed"
                )));
            }
        }
        if fs::read_dir(destination.join("bin"))?.count() != bins.len() {
            return Err(io::Error::other(
                "cached package environment has unexpected commands",
            ));
        }
        return Ok(destination);
    }

    let temporary = tempfile::tempdir_in(&generations)?;
    let bin_dir = temporary.path().join("bin");
    fs::create_dir(&bin_dir)?;
    for (name, path) in bins {
        symlink(path, bin_dir.join(name))?;
    }
    lock.write(temporary.path().join("packages.json"))?;
    fs::write(
        temporary.path().join("requests.json"),
        serde_json::to_vec(requests)?,
    )?;
    fs::rename(temporary.path(), &destination)?;
    Ok(destination)
}

fn activate(profile: &Path, generation: &Path) -> io::Result<()> {
    let parent = profile.parent().unwrap();
    fs::create_dir_all(parent)?;
    let temporary = tempfile::tempdir_in(parent)?;
    let link = temporary.path().join("current");
    symlink(generation, &link)?;
    fs::rename(link, profile)
}

/// Returns the original request recorded in a persistent profile generation.
pub fn installed_request(profile: &Path, name: &str) -> io::Result<Option<PackageRequest>> {
    Ok(read_requests(profile)?.remove(name))
}

fn read_requests(profile: &Path) -> io::Result<BTreeMap<String, PackageRequest>> {
    match fs::read(profile.join("requests.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests;
