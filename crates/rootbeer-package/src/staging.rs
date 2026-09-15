use std::path::Path;

pub fn staging(output: &Path) -> Result<tempfile::TempDir, String> {
    if output.try_exists().map_err(|e| e.to_string())? {
        return Err(format!("output already exists: {}", output.display()));
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    tempfile::tempdir_in(parent).map_err(|e| e.to_string())
}
