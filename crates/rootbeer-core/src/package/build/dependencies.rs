use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

pub(in crate::package) fn validate(root: &Path, libraries: &[PathBuf]) -> Result<(), String> {
    for library in libraries {
        let path = contained_file(root, library)?;
        let mut magic = [0; 8];
        fs::File::open(&path)
            .and_then(|mut file| file.read_exact(&mut magic))
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if &magic != b"!<arch>\n" {
            return Err(format!(
                "{} is not a self-contained static archive",
                path.display()
            ));
        }
    }
    Ok(())
}

pub(super) fn stage(root: &Path, libraries: &[PathBuf], output: &Path) -> Result<(), String> {
    if libraries.is_empty() {
        return Ok(());
    }
    validate(root, libraries)?;
    for library in libraries {
        link(root, library, output)?;
    }
    for directory in [
        "include",
        "lib/pkgconfig",
        "lib64/pkgconfig",
        "share/pkgconfig",
    ] {
        let path = root.join(directory);
        if path.try_exists().map_err(|error| error.to_string())? {
            tree(root, Path::new(directory), output)?;
        }
    }
    Ok(())
}

fn contained_file(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|error| format!("missing dependency export {}: {error}", relative.display()))?;
    if !path.starts_with(root) || !path.is_file() {
        return Err(format!(
            "dependency export {} escapes its package or is not a file",
            relative.display()
        ));
    }
    Ok(path)
}

fn link(root: &Path, relative: &Path, output: &Path) -> Result<(), String> {
    let source = contained_file(root, relative)?;
    let target = output.join(relative);
    if target.symlink_metadata().is_ok() {
        if fs::canonicalize(&target).ok().as_ref() == Some(&source) {
            return Ok(());
        }
        return Err(format!(
            "dependency export collision at {}",
            relative.display()
        ));
    }
    fs::create_dir_all(target.parent().unwrap()).map_err(|error| error.to_string())?;
    symlink(source, target).map_err(|error| error.to_string())
}

fn tree(root: &Path, relative: &Path, output: &Path) -> Result<(), String> {
    let directory = root.join(relative);
    let metadata = directory
        .symlink_metadata()
        .map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "dependency directory {} must be a real directory",
            relative.display()
        ));
    }
    let mut entries = fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = relative.join(entry.file_name());
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            tree(root, &path, output)?;
        } else {
            link(root, &path, output)?;
        }
    }
    Ok(())
}

pub(super) fn environment(
    prefix: &Path,
    environment: &mut BTreeMap<&str, String>,
) -> Result<(), String> {
    let quoted = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"));
    environment.insert("PKG_CONFIG_PATH", String::new());
    environment.insert(
        "PKG_CONFIG_LIBDIR",
        ["lib/pkgconfig", "lib64/pkgconfig", "share/pkgconfig"]
            .map(|directory| prefix.join(directory).to_string_lossy().into_owned())
            .join(":"),
    );
    environment.insert(
        "PKG_CONFIG_SYSROOT_DIR",
        prefix.to_string_lossy().into_owned(),
    );
    if !prefix.try_exists().map_err(|error| error.to_string())? {
        return Ok(());
    }
    environment.insert("CPPFLAGS", format!("-I{}", quoted(&prefix.join("include"))));
    environment.insert(
        "LDFLAGS",
        format!(
            "-L{} -L{}",
            quoted(&prefix.join("lib")),
            quoted(&prefix.join("lib64"))
        ),
    );
    Ok(())
}
