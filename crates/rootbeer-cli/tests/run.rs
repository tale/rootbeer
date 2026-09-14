use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use rootbeer_core::package::lockfile::RootbeerLock;
use rootbeer_core::package::{
    LockedInstall, LockedPackage, LockedSource, PackageRealizer, PackageRequest, Provides,
    ResolveContext,
};
use rootbeer_core::store::{hash_bytes, Store};

fn seed(root: &Path, name: &str, bin: &str, contents: &str) {
    let state = root.join("rootbeer");
    let source = root.join(name);
    fs::write(&source, contents).unwrap();
    let mut package = LockedPackage {
        name: name.into(),
        version: "1".into(),
        source: LockedSource::File {
            path: source,
            sha256: hash_bytes(contents.as_bytes()),
        },
        install: LockedInstall::Binary {
            path: "payload".into(),
        },
        provides: Provides {
            bins: BTreeMap::from([(bin.into(), "payload".into())]),
        },
        output_sha256: None,
    };
    let realizer = PackageRealizer::with_dirs(
        Store::new(state.join("store")),
        state.join("downloads"),
        state.join("tmp"),
    );
    package.output_sha256 = Some(
        realizer
            .realize(&package)
            .unwrap()
            .store_entry
            .output_sha256,
    );
    let request = PackageRequest::parse(&format!("{name}@1"));
    let identity = hash_bytes(&serde_json::to_vec(&(ResolveContext::current(), request)).unwrap());
    RootbeerLock::from_packages([package])
        .unwrap()
        .write(state.join("standalone/requests").join(identity))
        .unwrap();
    fs::remove_file(root.join(name)).unwrap();
}

fn rb(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rb"));
    command
        .env("XDG_STATE_HOME", root)
        .env("XDG_CONFIG_HOME", root.join("config"));
    command
}

#[test]
fn run_reuses_offline_store_and_forwards_arguments_exit_status_and_extra_tools() {
    let root = tempfile::tempdir().unwrap();
    seed(
        root.path(),
        "tool",
        "actual",
        "#!/bin/sh\nprintf '%s\\n' \"${0##*/}\" \"$@\"\nhelper\nexit 37\n",
    );
    seed(
        root.path(),
        "helper",
        "helper",
        "#!/bin/sh\nprintf 'helper ran\\n'\n",
    );

    let output = rb(root.path())
        .args([
            "run",
            "tool@1",
            "--offline",
            "-p",
            "helper@1",
            "--",
            "--literal",
            "two words",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(37),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"actual\n--literal\ntwo words\nhelper ran\n");
    assert!(!root.path().join("rootbeer/profiles/user/current").exists());
    assert!(!root.path().join("config").exists());
}

#[test]
fn use_adds_packages_without_a_configuration_and_env_exposes_them() {
    let root = tempfile::tempdir().unwrap();
    seed(
        root.path(),
        "first",
        "first",
        "#!/bin/sh\nprintf 'first\\n'\n",
    );
    seed(
        root.path(),
        "second",
        "second",
        "#!/bin/sh\nprintf 'second\\n'\n",
    );
    for name in ["first@1", "second@1"] {
        let output = rb(root.path())
            .args(["use", name, "--offline"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let env = rb(root.path()).arg("env").output().unwrap();
    assert!(env.status.success());
    let script = format!(
        "{}\nfirst\nsecond\n",
        String::from_utf8(env.stdout).unwrap()
    );
    let output = Command::new("/bin/sh")
        .args(["-c", &script])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"first\nsecond\n");
    assert!(!root.path().join("config").exists());
}

#[test]
fn offline_miss_does_not_install_or_run_a_host_command() {
    let root = tempfile::tempdir().unwrap();
    let output = rb(root.path())
        .args(["run", "sh@1", "--offline", "--", "-c", "echo wrong"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not cached"));
}
