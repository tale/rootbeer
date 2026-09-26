use rootbeer_core::package::{roots, standalone};
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
    roots::seed(&store).map_err(|error| error.to_string())?;

    let removed = store
        .collect_garbage_as_owner()
        .map_err(|error| error.to_string())?;
    for name in &removed {
        println!("{name}");
    }
    eprintln!("removed {} store entries", removed.len());
    Ok(())
}
