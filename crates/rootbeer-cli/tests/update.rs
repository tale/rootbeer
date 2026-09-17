use std::fs;
use std::os::unix::fs::symlink;
use std::process::Command;

#[test]
fn managed_update_refuses_before_network_or_store_mutation() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let state = root.join("rootbeer");
    let entry = state.join("store/test-rootbeer");
    fs::create_dir_all(entry.join(".rootbeer")).unwrap();
    fs::write(entry.join(".rootbeer/manifest.json"), "{}").unwrap();
    let binary = entry.join("rb");
    fs::copy(env!("CARGO_BIN_EXE_rb"), &binary).unwrap();
    let before = fs::read(&binary).unwrap();
    let lock = root.join("rootbeer.lock");
    fs::write(&lock, "unchanged lock").unwrap();
    for (owner, expected) in [
        ("default", "rb apply --update"),
        ("user", "predates saved package requests"),
    ] {
        let generation = state.join("generations").join(owner);
        fs::create_dir_all(generation.join("bin")).unwrap();
        symlink(&binary, generation.join("bin/rb")).unwrap();
        let profile = state.join("profiles").join(owner);
        fs::create_dir_all(&profile).unwrap();
        symlink(&generation, profile.join("current")).unwrap();
        let output = Command::new(profile.join("current/bin/rb"))
            .arg("update")
            .env("XDG_STATE_HOME", root)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{:?}",
            output
        );
        assert_eq!(fs::read_link(profile.join("current")).unwrap(), generation);
    }
    let output = Command::new(&binary)
        .arg("update")
        .env("XDG_STATE_HOME", root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Store contents cannot be overwritten")
    );
    assert_eq!(fs::read(&binary).unwrap(), before);
    assert_eq!(fs::read_to_string(lock).unwrap(), "unchanged lock");
    assert!(!state.join("indexes").exists());
}
