use std::path::Path;
use std::{fs, io};

/// Extracts the `@rootbeer` type definitions to `<data_dir>/typedefs/rootbeer/`
/// and writes a `.luarc.json` in `source_dir` pointing to them.
pub fn run(source_dir: &Path) {
    let typedefs_dir = rootbeer_core::Runtime::default_dir().join("typedefs");
    let rootbeer_dir = typedefs_dir.join("rootbeer");

    write_typedefs(&rootbeer_dir);
    write_luarc(source_dir, &typedefs_dir);

    println!("type definitions extracted to {}", rootbeer_dir.display());
    println!("wrote .luarc.json to {}", source_dir.display());
}

fn write_typedefs(rootbeer_dir: &Path) {
    fs::create_dir_all(rootbeer_dir).unwrap_or_else(|e| {
        eprintln!("error: failed to create {}: {e}", rootbeer_dir.display());
        std::process::exit(1);
    });

    let modules = get_modules();

    for (name, source) in &modules {
        let path = rootbeer_dir.join(format!("{name}.lua"));
        fs::write(&path, source).unwrap_or_else(|e| {
            eprintln!("error: failed to write {}: {e}", path.display());
            std::process::exit(1);
        });
    }

    write_init_lua(rootbeer_dir);
}

/// Generates `init.lua` — a requireable module that returns the `rootbeer` type.
/// When lua-language-server resolves `require("rootbeer")`, it finds this file
/// via `workspace.library` and returns a value typed as the `rootbeer` class
/// (defined in `core.lua`).
fn write_init_lua(rootbeer_dir: &Path) {
    let content = "\
--- @meta _
--- @type rootbeer
local rootbeer
return rootbeer
";

    let path = rootbeer_dir.join("init.lua");
    fs::write(&path, content).unwrap_or_else(|e| {
        eprintln!("error: failed to write {}: {e}", path.display());
        std::process::exit(1);
    });
}

fn get_modules() -> Vec<(String, String)> {
    // With embedded-stdlib, extract from the binary
    #[cfg(feature = "embedded-stdlib")]
    {
        rootbeer_core::embedded_modules()
            .iter()
            .map(|(name, src)| (name.to_string(), src.to_string()))
            .collect()
    }

    // Without embedded-stdlib, copy from the filesystem
    #[cfg(not(feature = "embedded-stdlib"))]
    {
        let lua_dir = rootbeer_core::lua_dir().join("rootbeer");
        let mut modules = Vec::new();

        let entries = fs::read_dir(&lua_dir).unwrap_or_else(|e| {
            eprintln!("error: failed to read {}: {e}", lua_dir.display());
            std::process::exit(1);
        });

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "lua") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    let source = fs::read_to_string(&path).unwrap_or_else(|e| {
                        eprintln!("error: failed to read {}: {e}", path.display());
                        std::process::exit(1);
                    });
                    modules.push((name.to_string(), source));
                }
            }
        }

        modules
    }
}

fn write_luarc(source_dir: &Path, typedefs_dir: &Path) {
    let luarc_path = source_dir.join(".luarc.json");

    // Don't overwrite if it already has our library path
    if luarc_path.exists() {
        if let Ok(content) = fs::read_to_string(&luarc_path) {
            if content.contains("rootbeer") {
                return;
            }
        }
    }

    let content = format!(
        "{{\n  \"workspace.library\": [\n    \"{}\"\n  ]\n}}\n",
        typedefs_dir.display()
    );

    fs::write(&luarc_path, content).unwrap_or_else(|e| {
        eprintln!("error: failed to write {}: {e}", luarc_path.display());
        std::process::exit(1);
    });
}

/// Writes type definitions and `.luarc.json`. Used by `rb init`.
pub fn ensure_luaurc(source_dir: &Path) -> io::Result<()> {
    let typedefs_dir = rootbeer_core::Runtime::default_dir().join("typedefs");
    let rootbeer_dir = typedefs_dir.join("rootbeer");

    write_typedefs(&rootbeer_dir);
    write_luarc(source_dir, &typedefs_dir);
    Ok(())
}
