use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;

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

const BACKENDS: &[&str] = &["autotools", "custom", "go", "rust", "zig"];

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
    paths.retain(|path| {
        let relative = path.strip_prefix(workspace).unwrap();
        !is_backend(relative)
            && !matches!(
                relative.to_str(),
                Some(
                    "crates/rootbeer-build/src/scheduler.rs"
                        | "crates/rootbeer-package/src/execution.rs"
                )
            )
    });
    fingerprint_paths(workspace, paths)
}

fn fingerprint_paths(workspace: &Path, mut paths: Vec<PathBuf>) -> std::io::Result<String> {
    paths.sort();
    let mut inputs = Vec::new();
    let mut tests = BTreeSet::new();
    let mut production = BTreeSet::new();
    for path in paths {
        let bytes = fs::read(&path)?;
        let bytes = if path.extension().is_some_and(|extension| extension == "rs") {
            let source =
                syn::parse_file(std::str::from_utf8(&bytes).map_err(std::io::Error::other)?)
                    .map_err(|error| {
                        std::io::Error::other(format!("{}: {error}", path.display()))
                    })?;
            let directory = path.parent().unwrap();
            let modules = if matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("lib.rs" | "main.rs" | "mod.rs")
            ) {
                directory.to_path_buf()
            } else {
                directory.join(path.file_stem().unwrap())
            };
            let mut removed = Vec::new();
            production_items(
                &source.items,
                directory,
                &modules,
                &mut tests,
                &mut production,
                &mut removed,
            );
            let mut projected = bytes.clone();
            for span in removed.into_iter().rev() {
                projected.drain(span.byte_range());
            }
            projected
        } else {
            bytes
        };
        inputs.push((path.canonicalize()?, bytes));
    }
    let workspace = workspace.canonicalize()?;
    let mut hash = Sha256::new();
    for (path, bytes) in inputs {
        if tests.contains(&path) && !production.contains(&path) {
            continue;
        }
        let name = path.strip_prefix(&workspace).unwrap().to_string_lossy();
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn production_items(
    items: &[syn::Item],
    directory: &Path,
    modules: &Path,
    tests: &mut BTreeSet<PathBuf>,
    production: &mut BTreeSet<PathBuf>,
    removed: &mut Vec<proc_macro2::Span>,
) {
    for (index, item) in items.iter().enumerate() {
        let syn::Item::Mod(module) = item else {
            continue;
        };
        let is_test = is_test_module(item);
        if let Some((_, contents)) = &module.content {
            // Nontrailing test modules can change line!() in later production code.
            if is_test && items[index + 1..].iter().all(is_test_module) {
                removed.push(module.span());
            } else if !is_test {
                production_items(
                    contents,
                    directory,
                    &modules.join(module.ident.to_string()),
                    tests,
                    production,
                    removed,
                );
            }
            continue;
        }
        let explicit = module.attrs.iter().find_map(|attribute| {
            if !attribute.path().is_ident("path") {
                return None;
            }
            let syn::Meta::NameValue(value) = &attribute.meta else {
                return None;
            };
            let syn::Expr::Lit(value) = &value.value else {
                return None;
            };
            let syn::Lit::Str(value) = &value.lit else {
                return None;
            };
            Some(directory.join(value.value()))
        });
        let path = explicit.unwrap_or_else(|| {
            let file = modules.join(format!("{}.rs", module.ident));
            if file.exists() {
                file
            } else {
                modules.join(module.ident.to_string()).join("mod.rs")
            }
        });
        // Canonical paths make #[path] aliases agree with recursively discovered files.
        if let Ok(path) = path.canonicalize() {
            if is_test {
                tests.insert(path);
            } else {
                production.insert(path);
            }
        }
    }
}

fn is_test_module(item: &syn::Item) -> bool {
    matches!(item, syn::Item::Mod(module) if module.attrs.iter().any(|attribute| {
        attribute.path().is_ident("cfg") && attribute.parse_args::<syn::Ident>().is_ok_and(|name| name == "test")
    }))
}

fn compatible_identity(workspace: &Path, identity: &str) -> std::io::Result<String> {
    let path = workspace.join("scripts/cache-compatibility");
    let records = match fs::read_to_string(path) {
        Ok(records) => records,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(error),
    };
    let mut result = String::new();
    let mut seen = BTreeSet::new();
    for line in records
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
    {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 2
            || fields.iter().any(|field| {
                field.len() != 64
                    || !field
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
            || !seen.insert(fields[0])
        {
            return Err(std::io::Error::other("invalid cache compatibility record"));
        }
        if fields[0] == identity {
            result = fields[1].into();
        }
    }
    Ok(result)
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
    println!(
        "cargo:rerun-if-changed={}",
        workspace.join("scripts/cache-compatibility").display()
    );
    let compatible =
        compatible_identity(workspace, &identity).expect("cannot read cache compatibility");
    println!("cargo:rustc-env=ROOTBEER_COMPATIBLE_ENGINE_IDENTITY={compatible}");
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
    fn compatibility_requires_an_exact_reviewed_source_digest() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("scripts")).unwrap();
        let current = "a".repeat(64);
        let previous = "b".repeat(64);
        let path = root.path().join("scripts/cache-compatibility");
        fs::write(&path, format!("{current} {previous}\n")).unwrap();
        assert_eq!(
            compatible_identity(root.path(), &current).unwrap(),
            previous
        );
        assert!(compatible_identity(root.path(), &"c".repeat(64))
            .unwrap()
            .is_empty());
        fs::write(
            &path,
            format!("{current} {previous}\n{current} {previous}\n"),
        )
        .unwrap();
        assert!(compatible_identity(root.path(), &current).is_err());
        fs::write(&path, "invalid record").unwrap();
        assert!(compatible_identity(root.path(), &current).is_err());
    }

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
            fs::write(
                &path,
                if name.ends_with(".rs") {
                    format!("pub const INPUT: &str = {name:?};")
                } else {
                    name.to_string()
                },
            )
            .unwrap();
        }
        for backend in BACKENDS {
            fs::write(
                root.path()
                    .join(format!("crates/rootbeer-build/src/backend/{backend}.rs")),
                format!("pub const BACKEND: &str = {backend:?};"),
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
        for selected in BACKENDS {
            let original = identities();
            fs::write(
                root.path()
                    .join(format!("crates/rootbeer-build/src/backend/{selected}.rs")),
                format!("pub const CHANGE: &str = {selected:?};"),
            )
            .unwrap();
            let changed = identities();
            for backend in BACKENDS {
                if backend == selected {
                    assert_ne!(original[*backend], changed[*backend]);
                } else {
                    assert_eq!(original[*backend], changed[*backend]);
                }
            }
        }
        for name in ["lib.rs", "backend/mod.rs", "backend/new.rs", "runner.rs"] {
            let before = identities();
            fs::write(
                root.path().join("crates/rootbeer-build/src").join(name),
                "pub const CHANGE: &str = \"shared or unclassified behavior\";",
            )
            .unwrap();
            let after = identities();
            for backend in BACKENDS {
                assert_ne!(before[*backend], after[*backend], "{name}: {backend}");
            }
        }
    }

    #[test]
    fn test_code_and_scheduling_do_not_change_build_identity() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("crates/rootbeer-build/src");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir(root.path().join("scripts")).unwrap();
        for name in [
            "Cargo.lock",
            "Cargo.toml",
            "crates/rootbeer-build/Cargo.toml",
        ] {
            fs::write(root.path().join(name), "manifest").unwrap();
        }
        fs::write(
            root.path().join("scripts/cache_inputs.rs"),
            "fn fingerprint() {}",
        )
        .unwrap();
        let original =
            "pub fn build() {} #[cfg(test)] mod tests; #[cfg(test)] mod inline { fn test() {} }";
        fs::write(source.join("lib.rs"), original).unwrap();
        fs::write(source.join("tests.rs"), "fn original_test() {}").unwrap();
        let digest = fingerprint(root.path(), &["rootbeer-build"]).unwrap();
        fs::write(
            source.join("lib.rs"),
            original.replace("fn test() {}", "fn different_test() {}"),
        )
        .unwrap();
        fs::write(source.join("tests.rs"), "fn another_test() {}").unwrap();
        fs::write(source.join("scheduler.rs"), "fn scheduling_policy() {}").unwrap();
        assert_eq!(
            digest,
            fingerprint(root.path(), &["rootbeer-build"]).unwrap()
        );
        fs::write(
            source.join("lib.rs"),
            original.replace("pub fn build() {}", "pub fn build() { panic!(); }"),
        )
        .unwrap();
        assert_ne!(
            digest,
            fingerprint(root.path(), &["rootbeer-build"]).unwrap()
        );
        // A test-named module compiled in production must still be fingerprinted.
        fs::write(source.join("lib.rs"), "pub fn build() {} mod tests;").unwrap();
        let production = fingerprint(root.path(), &["rootbeer-build"]).unwrap();
        fs::write(source.join("tests.rs"), "fn changed_production() {}").unwrap();
        assert_ne!(
            production,
            fingerprint(root.path(), &["rootbeer-build"]).unwrap()
        );
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
                fs::write(
                    &path,
                    if file.ends_with(".rs") {
                        format!("pub const INPUT: &str = {file:?};")
                    } else {
                        file.to_string()
                    },
                )
                .unwrap();
            }
        }
        let digest = |root| fingerprint(root, &["engine"]).unwrap();
        let original = digest(first.path());
        assert_eq!(original, digest(second.path()));
        fs::write(
            first.path().join("crates/cli/src/main.rs"),
            "pub const CHANGE: &str = \"unrelated edit\";",
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
            fs::write(
                first.path().join(file),
                "pub const CHANGE: &str = \"changed\";",
            )
            .unwrap();
            assert_ne!(original, digest(first.path()), "{file}");
            fs::write(
                first.path().join(file),
                if file.ends_with(".rs") {
                    format!("pub const INPUT: &str = {file:?};")
                } else {
                    file.to_string()
                },
            )
            .unwrap();
        }
        fs::write(
            first.path().join("crates/engine/src/new_backend.rs"),
            "pub const CHANGE: &str = \"new input\";",
        )
        .unwrap();
        assert_ne!(original, digest(first.path()));
    }
}
