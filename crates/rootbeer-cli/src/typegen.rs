use std::fs;
use std::path::Path;

/// Sets up type definitions so lua-language-server provides autocomplete.
///
/// With embedded-stdlib, extracts modules to `<data_dir>/typedefs/` and
/// points `.luarc.json` there. Without it, points `.luarc.json` directly
/// at the system lua directory — no copying needed.
pub fn setup(config_dir: &Path) {
    #[cfg(feature = "embedded-stdlib")]
    {
        let typedefs_dir = rootbeer_core::data_dir().join("typedefs");
        let rootbeer_dir = typedefs_dir.join("rootbeer");
        extract_embedded(&rootbeer_dir);
        write_luarc(config_dir, &typedefs_dir);
    }

    #[cfg(not(feature = "embedded-stdlib"))]
    {
        let lua_dir = rootbeer_core::lua_dir();
        write_luarc(config_dir, &lua_dir);
    }
}

#[cfg(feature = "embedded-stdlib")]
fn extract_embedded(dest: &Path) {
    fs::create_dir_all(dest).unwrap_or_else(|e| {
        eprintln!("error: failed to create {}: {e}", dest.display());
        std::process::exit(1);
    });

    for (name, source) in rootbeer_core::embedded_modules() {
        let path = dest.join(format!("{name}.lua"));
        fs::write(&path, source).unwrap_or_else(|e| {
            eprintln!("error: failed to write {}: {e}", path.display());
            std::process::exit(1);
        });
    }

    let init = dest.join("init.lua");
    fs::write(
        &init,
        "--- @meta _\n--- @type rootbeer\nlocal rootbeer\nreturn rootbeer\n",
    )
    .unwrap_or_else(|e| {
        eprintln!("error: failed to write {}: {e}", init.display());
        std::process::exit(1);
    });
}

fn write_luarc(config_dir: &Path, typedefs_dir: &Path) {
    let luarc_path = config_dir.join(".luarc.json");
    let typedefs_str = typedefs_dir.display().to_string();

    if luarc_path.exists() {
        if let Ok(content) = fs::read_to_string(&luarc_path) {
            if content.contains(&typedefs_str) {
                return;
            }
        }

        eprintln!("hint: existing .luarc.json found, add this to workspace.library:");
        eprintln!("hint: \"{typedefs_str}\"");
        return;
    }

    let content = format!("{{\n  \"workspace.library\": [\n    \"{typedefs_str}\"\n  ]\n}}\n",);

    fs::write(&luarc_path, content).unwrap_or_else(|e| {
        eprintln!("error: failed to write {}: {e}", luarc_path.display());
        std::process::exit(1);
    });
}
