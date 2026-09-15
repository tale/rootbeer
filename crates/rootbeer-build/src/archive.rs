use flate2::{write::GzEncoder, Compression};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn pack(root: &Path, output: &Path) -> io::Result<()> {
    fn paths(root: &Path, directory: &Path, entries: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if directory == root && entry.file_name() == ".rootbeer" {
                continue;
            }
            entries.push(entry.path().strip_prefix(root).unwrap().to_path_buf());
            if entry.file_type()?.is_dir() {
                paths(root, &entry.path(), entries)?;
            }
        }
        Ok(())
    }
    let mut entries = Vec::new();
    paths(root, root, &mut entries)?;
    entries.sort();
    let mut archive = tar::Builder::new(GzEncoder::new(
        fs::File::create(output)?,
        Compression::default(),
    ));
    archive.mode(tar::HeaderMode::Deterministic);
    archive.follow_symlinks(false);
    for path in entries {
        archive.append_path_with_name(root.join(&path), &path)?;
    }
    archive.into_inner()?.finish()?.sync_all()
}
