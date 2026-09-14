use super::*;
use crate::package::{LockedInstall, LockedSource, Provides};

fn realizer(root: &Path) -> PackageRealizer {
    PackageRealizer::with_dirs_and_offline(
        Store::new(root.join("store")),
        root.join("downloads"),
        root.join("tmp"),
        true,
    )
}

fn package(root: &Path, name: &str, version: &str, bin: &str) -> RealizedPackage {
    let source = root.join(format!("{name}-{version}"));
    let contents = format!("#!/bin/sh\nprintf '%s\\n' '{name} {version}'\n");
    fs::write(&source, &contents).unwrap();
    let package = LockedPackage {
        name: name.into(),
        version: version.into(),
        source: LockedSource::File {
            path: source,
            sha256: hash_bytes(contents.as_bytes()),
        },
        install: LockedInstall::Binary {
            path: PathBuf::from(bin),
        },
        provides: Provides {
            bins: BTreeMap::from([(bin.into(), PathBuf::from(bin))]),
        },
        output_sha256: None,
    };
    let mut realized = realizer(root).realize(&package).unwrap();
    realized.package.output_sha256 = Some(realized.store_entry.output_sha256.clone());
    realized
}

#[test]
fn conflicting_commands_preserve_the_active_environment() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let first = package(root, "first", "1.0.0", "tool");
    let second = package(root, "second", "1.0.0", "tool");
    let profile = root.join("profiles/user/current");
    let generation = write_environment(root, std::slice::from_ref(&first)).unwrap();
    activate(&profile, &generation).unwrap();
    let previous_lock = fs::read(profile.join("packages.json")).unwrap();

    let error = write_environment(root, &[first.clone(), second]).unwrap_err();

    assert!(error.to_string().contains("conflicting command 'tool'"));
    assert_eq!(fs::read_link(&profile).unwrap(), generation);
    assert_eq!(
        fs::read_link(profile.join("bin/tool")).unwrap(),
        first.bins["tool"]
    );
    assert_eq!(
        fs::read(profile.join("packages.json")).unwrap(),
        previous_lock
    );
    assert_eq!(fs::read_dir(root.join("environments")).unwrap().count(), 1);
}

#[test]
fn activation_switches_commands_and_lock_together_and_retains_previous_generation() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let first = package(root, "tool", "1.0.0", "tool");
    let second = package(root, "tool", "2.0.0", "tool");
    let profile = root.join("profiles/user/current");
    let previous = write_environment(root, std::slice::from_ref(&first)).unwrap();
    activate(&profile, &previous).unwrap();
    let replacement = write_environment(root, std::slice::from_ref(&second)).unwrap();

    activate(&profile, &replacement).unwrap();

    assert_eq!(fs::read_link(&profile).unwrap(), replacement);
    assert_eq!(
        fs::read_link(profile.join("bin/tool")).unwrap(),
        second.bins["tool"]
    );
    let lock = read_lock(&profile.join("packages.json")).unwrap().unwrap();
    assert_eq!(lock.packages.len(), 1);
    assert_eq!(lock.packages["tool@2.0.0"], second.package);
    assert_eq!(
        fs::read_link(previous.join("bin/tool")).unwrap(),
        first.bins["tool"]
    );
    assert!(previous.join("packages.json").is_file());
}

#[test]
fn failed_activation_preserves_an_existing_directory() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let first = package(root, "tool", "1.0.0", "tool");
    let generation = write_environment(root, &[first]).unwrap();
    let profile = root.join("profiles/user/current");
    fs::create_dir_all(&profile).unwrap();
    fs::write(profile.join("keep"), "existing profile").unwrap();

    assert!(activate(&profile, &generation).is_err());

    assert_eq!(
        fs::read_to_string(profile.join("keep")).unwrap(),
        "existing profile"
    );
    assert_eq!(fs::read_dir(profile.parent().unwrap()).unwrap().count(), 1);
    assert!(generation.join("packages.json").is_file());
}

#[test]
fn profile_updates_replace_one_package_and_retain_unrelated_packages() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let first = package(root, "tool", "1.0.0", "tool");
    let unrelated = package(root, "other", "3.0.0", "other");
    let replacement = package(root, "tool", "2.0.0", "new-tool");
    let profile = root.join("profiles/user/current");
    let realizer = realizer(root);
    install_profile(root, &profile, &[first, unrelated.clone()], &realizer).unwrap();
    let previous = fs::read_link(&profile).unwrap();

    install_profile(
        root,
        &profile,
        std::slice::from_ref(&replacement),
        &realizer,
    )
    .unwrap();

    let lock = read_lock(&profile.join("packages.json")).unwrap().unwrap();
    assert_eq!(lock.packages.len(), 2);
    assert_eq!(lock.packages["other@3.0.0"], unrelated.package);
    assert_eq!(lock.packages["tool@2.0.0"], replacement.package);
    assert!(!profile.join("bin/tool").exists());
    assert_eq!(
        fs::read_link(profile.join("bin/other")).unwrap(),
        unrelated.bins["other"]
    );
    assert_eq!(
        fs::read_link(profile.join("bin/new-tool")).unwrap(),
        replacement.bins["new-tool"]
    );
    assert_ne!(fs::read_link(&profile).unwrap(), previous);
    assert!(previous.join("bin/tool").is_file());
}

#[test]
fn profile_rejects_a_conflicting_addition_without_changing_installed_packages() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let first = package(root, "tool", "1.0.0", "tool");
    let conflict = package(root, "other", "1.0.0", "tool");
    let profile = root.join("profiles/user/current");
    let realizer = realizer(root);
    install_profile(root, &profile, std::slice::from_ref(&first), &realizer).unwrap();
    let previous = fs::read_link(&profile).unwrap();
    let previous_lock = fs::read(profile.join("packages.json")).unwrap();

    let error = install_profile(root, &profile, &[conflict], &realizer).unwrap_err();

    assert!(error.to_string().contains("conflicting command 'tool'"));
    assert_eq!(fs::read_link(&profile).unwrap(), previous);
    assert_eq!(
        fs::read(profile.join("packages.json")).unwrap(),
        previous_lock
    );
    assert_eq!(
        fs::read_link(profile.join("bin/tool")).unwrap(),
        first.bins["tool"]
    );
}

#[test]
fn cached_environments_reject_changed_commands_and_lock() {
    for change in ["redirect", "missing", "extra", "lock"] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let package = package(root, "tool", "1.0.0", "tool");
        let packages = [package];
        let generation = write_environment(root, &packages).unwrap();
        assert_eq!(write_environment(root, &packages).unwrap(), generation);

        match change {
            "redirect" => {
                fs::remove_file(generation.join("bin/tool")).unwrap();
                symlink("/bin/sh", generation.join("bin/tool")).unwrap();
            }
            "missing" => fs::remove_file(generation.join("bin/tool")).unwrap(),
            "extra" => symlink("/bin/sh", generation.join("bin/unexpected")).unwrap(),
            "lock" => {
                let mut lock = read_lock(&generation.join("packages.json"))
                    .unwrap()
                    .unwrap();
                lock.packages.clear();
                lock.write(generation.join("packages.json")).unwrap();
            }
            _ => unreachable!(),
        }

        assert!(
            write_environment(root, &packages).is_err(),
            "accepted {change}"
        );
    }
}
