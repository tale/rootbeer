use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

fn collect(directory: &Path, paths: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, paths)?;
        } else {
            paths.push(path);
        }
    }
    Ok(())
}

const BACKENDS: &[&str] = &["autotools", "custom", "rust", "zig"];

fn is_backend(path: &Path) -> bool {
    BACKENDS
        .iter()
        .any(|name| path == Path::new(&format!("crates/rootbeer-build/src/backend/{name}.rs")))
}

pub fn fingerprint(workspace: &Path, crates: &[&str]) -> std::io::Result<String> {
    let mut paths = vec![
        workspace.join("Cargo.lock"),
        workspace.join("Cargo.toml"),
        workspace.join("scripts/cache_inputs.rs"),
    ];
    for name in crates {
        let directory = workspace.join("crates").join(name);
        paths.push(directory.join("Cargo.toml"));
        if directory.join("build.rs").exists() {
            paths.push(directory.join("build.rs"));
        }
        collect(&directory.join("src"), &mut paths)?;
    }
    paths.retain(|path| !is_backend(path.strip_prefix(workspace).unwrap()));
    fingerprint_paths(workspace, paths)
}

fn fingerprint_paths(workspace: &Path, mut paths: Vec<PathBuf>) -> std::io::Result<String> {
    paths.sort();
    let mut hash = Sha256::new();
    for path in paths {
        let name = path.strip_prefix(workspace).unwrap().to_string_lossy();
        let bytes = fs::read(&path)?;
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(not(test))]
pub fn emit(crates: &[&str]) {
    let directory = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace = directory.parent().unwrap().parent().unwrap();
    for name in crates {
        println!(
            "cargo:rerun-if-changed={}",
            workspace.join("crates").join(name).display()
        );
    }
    for path in ["Cargo.lock", "Cargo.toml", "scripts/cache_inputs.rs"] {
        println!("cargo:rerun-if-changed={}", workspace.join(path).display());
    }
    let identity = fingerprint(workspace, crates).expect("cannot fingerprint engine inputs");
    println!("cargo:rustc-env=ROOTBEER_ENGINE_IDENTITY={identity}");
    if crates.contains(&"rootbeer-build") {
        for name in BACKENDS {
            let path = workspace.join(format!("crates/rootbeer-build/src/backend/{name}.rs"));
            let identity = fingerprint_paths(workspace, vec![path])
                .expect("cannot fingerprint backend inputs");
            println!(
                "cargo:rustc-env=ROOTBEER_BACKEND_{}={identity}",
                name.to_uppercase()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_edits_are_scoped_and_unknown_build_files_invalidate_every_backend() {
        let root = tempfile::tempdir().unwrap();
        for name in [
            "Cargo.lock",
            "Cargo.toml",
            "scripts/cache_inputs.rs",
            "crates/rootbeer-build/Cargo.toml",
            "crates/rootbeer-build/build.rs",
            "crates/rootbeer-build/src/lib.rs",
            "crates/rootbeer-build/src/backend/mod.rs",
        ] {
            let path = root.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, name).unwrap();
        }
        for backend in BACKENDS {
            fs::write(
                root.path()
                    .join(format!("crates/rootbeer-build/src/backend/{backend}.rs")),
                backend,
            )
            .unwrap();
        }
        let identities = || {
            let shared = fingerprint(root.path(), &["rootbeer-build"]).unwrap();
            BACKENDS
                .iter()
                .map(|backend| {
                    let path = root
                        .path()
                        .join(format!("crates/rootbeer-build/src/backend/{backend}.rs"));
                    (
                        backend.to_string(),
                        (
                            shared.clone(),
                            fingerprint_paths(root.path(), vec![path]).unwrap(),
                        ),
                    )
                })
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        let original = identities();
        fs::write(
            root.path()
                .join("crates/rootbeer-build/src/backend/rust.rs"),
            "changed Rust backend",
        )
        .unwrap();
        let changed = identities();
        assert_ne!(original["rust"], changed["rust"]);
        for backend in ["autotools", "custom", "zig"] {
            assert_eq!(original[backend], changed[backend]);
        }
        for name in ["lib.rs", "backend/mod.rs", "backend/new.rs", "runner.rs"] {
            let before = identities();
            fs::write(
                root.path().join("crates/rootbeer-build/src").join(name),
                "shared or unclassified behavior",
            )
            .unwrap();
            let after = identities();
            for backend in BACKENDS {
                assert_ne!(before[*backend], after[*backend], "{name}: {backend}");
            }
        }
    }

    #[test]
    fn fingerprints_track_engine_inputs_but_ignore_cli_and_checkout_location() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        for root in [first.path(), second.path()] {
            for file in [
                "Cargo.lock",
                "Cargo.toml",
                "scripts/cache_inputs.rs",
                "crates/engine/Cargo.toml",
                "crates/engine/build.rs",
                "crates/engine/src/backend.rs",
                "crates/cli/src/main.rs",
            ] {
                let path = root.join(file);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, file).unwrap();
            }
        }
        let digest = |root| fingerprint(root, &["engine"]).unwrap();
        let original = digest(first.path());
        assert_eq!(original, digest(second.path()));
        fs::write(
            first.path().join("crates/cli/src/main.rs"),
            "unrelated edit",
        )
        .unwrap();
        assert_eq!(original, digest(first.path()));
        for file in [
            "Cargo.lock",
            "Cargo.toml",
            "scripts/cache_inputs.rs",
            "crates/engine/Cargo.toml",
            "crates/engine/build.rs",
            "crates/engine/src/backend.rs",
        ] {
            fs::write(first.path().join(file), "changed input").unwrap();
            assert_ne!(original, digest(first.path()), "{file}");
            fs::write(first.path().join(file), file).unwrap();
        }
        fs::write(
            first.path().join("crates/engine/src/new_backend.rs"),
            "new input",
        )
        .unwrap();
        assert_ne!(original, digest(first.path()));
    }
}
