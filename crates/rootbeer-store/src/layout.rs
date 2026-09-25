//! Version marker for the on-disk layout under [`crate::root_dir`], so a newer
//! rootbeer migrates an older layout instead of guessing at it.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// v2: owned by root and written through the setuid `rb-store` helper.
pub const VERSION: u32 = 2;
const FILE: &str = "layout.json";

#[derive(Serialize, Deserialize)]
struct Layout {
    version: u32,
}

/// Returns the recorded layout version, or `None` when no marker exists yet.
pub fn read(root: &Path) -> io::Result<Option<u32>> {
    let bytes = match fs::read(root.join(FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };

    let layout: Layout = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    Ok(Some(layout.version))
}

/// Records [`VERSION`] atomically.
pub fn write(root: &Path) -> io::Result<()> {
    let json = serde_json::to_string(&Layout { version: VERSION }).map_err(io::Error::other)?;
    let tmp = root.join(format!(".{FILE}.{}", std::process::id()));
    fs::write(&tmp, format!("{json}\n"))?;
    fs::rename(tmp, root.join(FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_the_current_version() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(read(root.path()).unwrap(), None);

        write(root.path()).unwrap();
        assert_eq!(read(root.path()).unwrap(), Some(VERSION));
    }
}
