use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    #[cfg(not(unix))]
    compile_error!("rootbeer only supports unix-like systems (macOS, Linux)");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let lua_dir = manifest_dir.join("../../lua/rootbeer");
    let lua_dir = lua_dir
        .canonicalize()
        .expect("lua/rootbeer directory not found");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // For dynamic-lua, embed the path so runtime can find it
    println!("cargo:rustc-env=ROOTBEER_LUA_DIR={}", lua_dir.display());

    let mut modules = Vec::new();
    walk_lua_dir(&lua_dir, "rootbeer", &mut modules);
    // Sort deepest modules first so dependencies are registered before dependents
    // (e.g. rootbeer.shells.zsh before rootbeer.shells before rootbeer)
    modules.sort_by(|a, b| {
        let depth_a = a.0.matches('.').count();
        let depth_b = b.0.matches('.').count();
        depth_b.cmp(&depth_a).then_with(|| a.0.cmp(&b.0))
    });

    let mut out = fs::File::create(out_dir.join("lua_stdlib.rs")).unwrap();
    writeln!(out, "pub(crate) const LUA_MODULES: &[(&str, &str)] = &[").unwrap();
    for (name, path) in &modules {
        writeln!(
            out,
            "    (\"{name}\", include_str!(\"{}\")),",
            path.display()
        )
        .unwrap();
    }
    writeln!(out, "];").unwrap();
}

fn walk_lua_dir(dir: &Path, prefix: &str, modules: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("failed to read lua directory")
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            println!("cargo:rerun-if-changed={}", path.display());
            walk_lua_dir(&path, &format!("{prefix}.{name}"), modules);
            continue;
        }

        if !name.ends_with(".lua") {
            continue;
        }

        println!("cargo:rerun-if-changed={}", path.display());

        // Skip files marked as type-stub-only (@meta)
        if let Ok(content) = fs::read_to_string(&path) {
            if content.starts_with("--- @meta") {
                continue;
            }
        }

        // init.lua maps to the directory module (e.g. @rootbeer.shells),
        // but skip top-level init.lua since @rootbeer is the native module
        let module_name = if name == "init.lua" {
            if prefix == "rootbeer" {
                continue;
            }
            format!("@{prefix}")
        } else {
            format!("@{prefix}.{}", name.strip_suffix(".lua").unwrap())
        };

        modules.push((module_name, path));
    }
}
