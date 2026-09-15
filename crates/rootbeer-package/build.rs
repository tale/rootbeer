use std::path::PathBuf;
use std::{env, fs};
fn main() {
    println!("cargo:rerun-if-env-changed=ROOTBEER_INDEX_URL");
    println!("cargo:rerun-if-env-changed=ROOTBEER_INDEX_PUBLIC_KEY");
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let package_dir = manifest_dir.join("../../packages");
    println!("cargo:rerun-if-changed={}", package_dir.display());
    let mut recipes: Vec<_> = fs::read_dir(&package_dir)
        .expect("packages/ directory not found")
        .map(|entry| entry.expect("cannot read package definition").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "lua"))
        .collect();
    recipes.sort();
    let body = recipes
        .iter()
        .map(|path| {
            let name = path.file_stem().unwrap().to_str().unwrap();
            let path = path.canonicalize().unwrap();
            format!("({name:?}, include_str!({:?})),", path.to_str().unwrap())
        })
        .collect::<Vec<_>>()
        .join("\n");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(
        out_dir.join("package_catalog.rs"),
        format!("const EMBEDDED_RECIPES: &[(&str, &str)] = &[\n{body}\n];\n"),
    )
    .expect("cannot embed package catalog");
}
