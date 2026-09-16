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
}

#[cfg(test)]
mod tests {
    use super::*;

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
