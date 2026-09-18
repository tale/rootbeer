use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

type Exports = BTreeMap<String, PathBuf>;

#[derive(Default, Serialize, Deserialize)]
struct Ownership {
    owners: BTreeMap<String, Exports>,
}

pub(crate) struct Applications {
    state: PathBuf,
    directory: PathBuf,
}

impl Default for Applications {
    fn default() -> Self {
        Self::new(
            crate::state_dir().join("applications"),
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join("Applications"))
                .unwrap_or_default(),
        )
    }
}

impl Applications {
    pub(crate) fn new(state: PathBuf, directory: PathBuf) -> Self {
        Self { state, directory }
    }

    pub(crate) fn contains_exports(&self, owner: &str, desired: &Exports) -> io::Result<bool> {
        if desired.is_empty() {
            return Ok(true);
        }
        let manifest = self.state.join("owners.json");
        let ownership: Ownership = match fs::read(&manifest) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "invalid application ownership {}: {error}",
                        manifest.display()
                    ),
                )
            })?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        let Some(owned) = ownership.owners.get(owner) else {
            return Ok(false);
        };
        Ok(desired.iter().all(|(name, target)| {
            owned.get(name) == Some(target)
                && fs::read_link(self.directory.join(name)).ok().as_ref() == Some(target)
        }))
    }

    pub(crate) fn synchronize(&self, owner: &str, desired: &Exports) -> io::Result<()> {
        self.synchronize_with(owner, desired, || Ok(()))
    }

    pub(crate) fn synchronize_with(
        &self,
        owner: &str,
        desired: &Exports,
        activate: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<()> {
        if desired.is_empty() && !self.state.exists() {
            return activate();
        }
        if !self.directory.is_absolute() {
            return Err(io::Error::other(
                "HOME must be an absolute path to install applications",
            ));
        }
        for (name, target) in desired {
            validate_name(name)?;
            if !target.is_absolute()
                || !target.is_dir()
                || target.extension().is_none_or(|value| value != "app")
            {
                return Err(io::Error::other(format!(
                    "invalid application bundle {}",
                    target.display()
                )));
            }
        }
        fs::create_dir_all(&self.state)?;
        let guard = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.state.join("lock"))?;
        guard.lock()?;
        let manifest = self.state.join("owners.json");
        let mut ownership: Ownership = match fs::read(&manifest) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "invalid application ownership {}: {error}",
                        manifest.display()
                    ),
                )
            })?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ownership::default(),
            Err(error) => return Err(error),
        };
        let previous = serde_json::to_vec(&ownership).map_err(io::Error::other)?;
        let old = exports(&ownership)?;
        if desired.is_empty() {
            ownership.owners.remove(owner);
        } else {
            ownership.owners.insert(owner.into(), desired.clone());
        }
        let next = exports(&ownership)?;
        let mut changes = Vec::new();
        for name in old
            .keys()
            .chain(next.keys())
            .collect::<std::collections::BTreeSet<_>>()
        {
            validate_name(name)?;
            let path = self.directory.join(name);
            let current = match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_symlink() => Some(fs::read_link(&path)?),
                Ok(_) if next.contains_key(name) => return Err(conflict(&path)),
                Ok(_) => continue,
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            if current.is_some() && current.as_ref() != old.get(name) {
                if next.contains_key(name) {
                    return Err(conflict(&path));
                }
                continue;
            }
            let target = next.get(name).cloned();
            if current != target {
                changes.push((path, current, target));
            }
        }
        if !changes.is_empty() {
            fs::create_dir_all(&self.directory)?;
        }
        let mut applied = Vec::new();
        let result = (|| {
            for (path, before, after) in &changes {
                replace(path, after.as_deref())?;
                applied.push((path, before, after));
            }
            write_manifest(
                &manifest,
                &serde_json::to_vec(&ownership).map_err(io::Error::other)?,
            )?;
            activate()
        })();
        if let Err(error) = result {
            for (path, before, after) in applied.into_iter().rev() {
                let is_owned = match fs::symlink_metadata(path) {
                    Ok(metadata) if metadata.is_symlink() => {
                        fs::read_link(path).ok().as_ref() == after.as_ref()
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => after.is_none(),
                    _ => false,
                };
                if is_owned {
                    replace(path, before.as_deref())?;
                }
            }
            write_manifest(&manifest, &previous)?;
            return Err(error);
        }
        Ok(())
    }
}

fn validate_name(name: &str) -> io::Result<()> {
    let path = Path::new(name);
    if path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
        || path.extension().is_none_or(|value| value != "app")
    {
        return Err(io::Error::other(format!(
            "invalid application export '{name}'"
        )));
    }
    Ok(())
}

fn exports(ownership: &Ownership) -> io::Result<Exports> {
    let mut result = BTreeMap::new();
    let mut names = BTreeMap::new();
    for apps in ownership.owners.values() {
        for (name, target) in apps {
            if names
                .insert(name.to_lowercase(), name)
                .is_some_and(|previous| previous != name)
            {
                return Err(io::Error::other(format!(
                    "profiles export conflicting application names near '{name}'"
                )));
            }
            if result
                .insert(name.clone(), target.clone())
                .is_some_and(|previous| previous != *target)
            {
                return Err(io::Error::other(format!(
                    "profiles export conflicting application '{name}'"
                )));
            }
        }
    }
    Ok(result)
}

fn conflict(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "application {} already exists and is not owned by Rootbeer",
            path.display()
        ),
    )
}

fn replace(path: &Path, target: Option<&Path>) -> io::Result<()> {
    let Some(target) = target else {
        return fs::remove_file(path);
    };
    let temporary = tempfile::tempdir_in(path.parent().unwrap())?;
    let link = temporary.path().join("application");
    symlink(target, &link)?;
    fs::rename(link, path)
}

fn write_manifest(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    fs::write(temporary.path(), bytes)?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(root: &Path, version: &str) -> Exports {
        let target = root.join(version).join("Example.app");
        fs::create_dir_all(&target).unwrap();
        BTreeMap::from([("Example.app".into(), target)])
    }

    #[test]
    fn installs_updates_shares_and_removes_owned_links() {
        let root = tempfile::tempdir().unwrap();
        let manager =
            Applications::new(root.path().join("state"), root.path().join("Applications"));
        let first = fixture(root.path(), "first");
        let second = fixture(root.path(), "second");
        let link = manager.directory.join("Example.app");
        manager.synchronize("user", &first).unwrap();
        manager.synchronize("configuration", &first).unwrap();
        assert!(manager.synchronize("user", &second).is_err());
        assert_eq!(fs::read_link(&link).unwrap(), first["Example.app"]);
        manager
            .synchronize("configuration", &Exports::new())
            .unwrap();
        manager.synchronize("user", &second).unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), second["Example.app"]);
        manager.synchronize("user", &Exports::new()).unwrap();
        assert!(!link.is_symlink());
        assert!(second["Example.app"].is_dir());
    }

    #[test]
    fn refuses_unowned_apps_and_preserves_replaced_links_on_removal() {
        let root = tempfile::tempdir().unwrap();
        let manager =
            Applications::new(root.path().join("state"), root.path().join("Applications"));
        let desired = fixture(root.path(), "store");
        let link = manager.directory.join("Example.app");
        fs::create_dir_all(&link).unwrap();
        assert!(manager.synchronize("user", &desired).is_err());
        fs::remove_dir(&link).unwrap();
        symlink(&desired["Example.app"], &link).unwrap();
        assert!(manager.synchronize("user", &desired).is_err());
        fs::remove_file(&link).unwrap();
        manager.synchronize("user", &desired).unwrap();
        fs::remove_file(&link).unwrap();
        symlink("/user/replacement.app", &link).unwrap();
        manager.synchronize("user", &Exports::new()).unwrap();
        assert_eq!(
            fs::read_link(link).unwrap(),
            Path::new("/user/replacement.app")
        );
    }

    #[test]
    fn activation_failure_rolls_back_links_and_ownership() {
        let root = tempfile::tempdir().unwrap();
        let manager =
            Applications::new(root.path().join("state"), root.path().join("Applications"));
        let first = fixture(root.path(), "first");
        let second = fixture(root.path(), "second");
        manager.synchronize("user", &first).unwrap();
        let before = fs::read(manager.state.join("owners.json")).unwrap();
        assert!(manager
            .synchronize_with("user", &second, || Err(io::Error::other(
                "activation failed"
            )))
            .is_err());
        assert_eq!(fs::read(manager.state.join("owners.json")).unwrap(), before);
        assert_eq!(
            fs::read_link(manager.directory.join("Example.app")).unwrap(),
            first["Example.app"]
        );
    }
    #[test]
    fn rollback_does_not_overwrite_a_user_replacement() {
        let root = tempfile::tempdir().unwrap();
        let manager =
            Applications::new(root.path().join("state"), root.path().join("Applications"));
        let first = fixture(root.path(), "first");
        manager.synchronize("user", &first).unwrap();
        let link = manager.directory.join("Example.app");
        assert!(manager
            .synchronize_with("user", &Exports::new(), || {
                fs::write(&link, "user replacement")?;
                Err(io::Error::other("activation failed"))
            })
            .is_err());
        assert_eq!(fs::read_to_string(link).unwrap(), "user replacement");
    }

    #[test]
    fn rejects_case_collisions_across_profiles() {
        let root = tempfile::tempdir().unwrap();
        let manager =
            Applications::new(root.path().join("state"), root.path().join("Applications"));
        let first = fixture(root.path(), "first");
        manager.synchronize("user", &first).unwrap();
        let other = BTreeMap::from([("example.app".into(), first["Example.app"].clone())]);
        assert!(manager.synchronize("configuration", &other).is_err());
    }
}
