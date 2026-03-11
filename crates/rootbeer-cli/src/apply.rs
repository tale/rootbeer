use std::path::PathBuf;

pub fn run(
    script: PathBuf,
    opts: rootbeer_core::Options,
    lua_dir: Option<&PathBuf>,
    profile: Option<String>,
) {
    if !script.exists() {
        eprintln!("error: script not found: {}", script.display());
        std::process::exit(1);
    }

    let mut runtime = rootbeer_core::Runtime::from_script(&script).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });

    if let Some(lua_dir) = lua_dir {
        runtime.lua_dir = lua_dir.clone();
    }

    runtime.profile = profile;

    eprintln!(
        "applying ({}){}",
        opts.mode,
        if opts.force { " [force]" } else { "" }
    );
    match rootbeer_core::execute_with(runtime, opts) {
        Ok(report) => {
            eprintln!("done ({} operations)", report.results.len());
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
