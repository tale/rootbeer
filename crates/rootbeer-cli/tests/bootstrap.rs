use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::process::{Command, Output, Stdio};

use rootbeer_core::store::Store;

fn rb(root: &Path, opt: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rb"))
        .env("XDG_STATE_HOME", root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("ROOTBEER_ROOT", opt)
        .args(["unuse", "absent"])
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn seed(store: &Path, name: &str) -> std::path::PathBuf {
    let source = tempfile::tempdir().unwrap();
    fs::write(source.path().join(name), name).unwrap();
    Store::new(store)
        .add_tree(name, "1", source.path())
        .unwrap()
        .path
}

#[test]
fn migrates_the_legacy_store_and_keeps_existing_links_resolving() {
    let root = tempfile::tempdir().unwrap();
    let legacy = root.path().join("rootbeer/store");
    let entry = seed(&legacy, "tool");
    let invalid = legacy.join("sha256-broken-tool-1");
    fs::create_dir_all(&invalid).unwrap();
    let profile = root.path().join("profile");
    symlink(entry.join("tool"), &profile).unwrap();

    let opt = root.path().join("opt");
    let output = rb(root.path(), &opt);
    assert!(opt.join("layout.json").exists(), "{output:?}");

    let moved = opt.join("store").join(entry.file_name().unwrap());
    Store::new(opt.join("store")).verify_entry(&moved).unwrap();
    assert_eq!(fs::read_link(&entry).unwrap(), moved);
    assert_eq!(fs::read_to_string(&profile).unwrap(), "tool");
    assert!(invalid.is_dir());
    assert!(legacy.join(".migrated").exists());
}

#[test]
fn finishes_an_interrupted_migration() {
    let root = tempfile::tempdir().unwrap();
    let legacy = root.path().join("rootbeer/store");
    let opt = root.path().join("opt");
    let moved = seed(&opt.join("store"), "tool");
    let name = moved.file_name().unwrap().to_string_lossy().into_owned();
    fs::create_dir_all(&legacy).unwrap();
    symlink(&moved, legacy.join(format!(".{name}.link"))).unwrap();

    rb(root.path(), &opt);

    assert_eq!(fs::read_link(legacy.join(&name)).unwrap(), moved);
    assert!(!legacy.join(format!(".{name}.link")).exists());
}

#[test]
fn prints_the_exact_sudo_command_without_a_terminal() {
    let root = tempfile::tempdir().unwrap();
    let parent = root.path().join("locked");
    fs::create_dir(&parent).unwrap();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o555)).unwrap();

    let output = rb(root.path(), &parent.join("rootbeer"));

    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sudo install -d -o"), "{stderr}");
}

#[test]
fn refuses_a_newer_layout() {
    let root = tempfile::tempdir().unwrap();
    let opt = root.path().join("opt");
    fs::create_dir_all(opt.join("store")).unwrap();
    fs::write(opt.join("layout.json"), r#"{"version":999}"#).unwrap();

    let output = rb(root.path(), &opt);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("layout v999"));
}
