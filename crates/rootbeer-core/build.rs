use std::env;
use std::path::PathBuf;

fn main() {
    #[cfg(not(unix))]
    compile_error!("rootbeer only supports unix-like systems (macOS, Linux)");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let lua_dir = manifest_dir.join("../../lua");
    let lua_dir = lua_dir.canonicalize().expect("lua/ directory not found");

    println!("cargo:rustc-env=ROOTBEER_LUA_DIR={}", lua_dir.display());
}
