use std::fs;

use rootbeer_core::package::{profile, standalone};
use rootbeer_core::store::Store;

pub fn run() {
    if let Err(error) = collect() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn collect() -> Result<(), String> {
    let store = Store::default();
    standalone::write_user_root(&store).map_err(|error| error.to_string())?;

    // Configurations applied before roots existed would otherwise lose their packages.
    let has_configuration_bins = fs::read_dir(profile::bin_dir())
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false);
    if has_configuration_bins && !store.has_root("configuration") {
        return Err("run `rb apply` once so the configuration's packages are kept".into());
    }

    let removed = store
        .collect_garbage_as_owner()
        .map_err(|error| error.to_string())?;
    for name in &removed {
        println!("{name}");
    }
    eprintln!("removed {} store entries", removed.len());
    Ok(())
}
