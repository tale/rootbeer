use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;

mod parse;
mod policy;
#[cfg(test)]
mod tests;

/// Static inspection of the native binaries in an installed package tree.
#[derive(Debug, Serialize)]
pub struct AuditReport {
    pub schema: u32,
    pub binaries: Vec<Binary>,
    pub violations: Vec<Violation>,
}

/// Loader facts for one architecture of an ELF or Mach-O file.
#[derive(Debug, Serialize)]
pub struct Binary {
    pub path: PathBuf,
    pub format: Format,
    pub architecture: u32,
    pub bits: u8,
    pub little_endian: bool,
    pub is_executable: bool,
    pub interpreter: Option<String>,
    pub identity: Option<String>,
    pub libraries: Vec<String>,
    pub rpaths: Vec<String>,
    pub runpaths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Elf,
    MachO,
}

/// A reference that cannot be justified by this package or the OS-runtime baseline.
#[derive(Debug, Serialize)]
pub struct Violation {
    pub path: PathBuf,
    pub architecture: u32,
    pub reference: String,
    pub reason: String,
}

impl AuditReport {
    /// Rejects a package containing unresolved or non-relocatable native references.
    pub fn validate(&self) -> Result<(), String> {
        if self.violations.is_empty() {
            return Ok(());
        }
        Err(format!(
            "runtime audit failed:\n{}",
            self.violations
                .iter()
                .map(|issue| {
                    format!(
                        "{} (architecture {}): {}: {}",
                        issue.path.display(),
                        issue.architecture,
                        issue.reference,
                        issue.reason
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ))
    }
}

/// Reads loader metadata without executing binaries or consulting host library paths.
pub fn audit(root: &Path) -> Result<AuditReport, String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    if !root.is_dir() {
        return Err("runtime audit requires an installed package directory".into());
    }
    let mut binaries = Vec::new();
    visit(&root, &root, &mut binaries)?;
    binaries.sort_by(|a, b| (&a.path, a.architecture).cmp(&(&b.path, b.architecture)));
    let violations = policy::check(&root, &binaries);
    Ok(AuditReport {
        schema: 1,
        binaries,
        violations,
    })
}

fn visit(root: &Path, directory: &Path, binaries: &mut Vec<Binary>) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        let path = entry.path();
        if kind.is_dir() {
            visit(root, &path, binaries)?;
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        let mut magic = [0; 4];
        let read = fs::File::open(&path)
            .and_then(|file| file.take(4).read(&mut magic))
            .map_err(|error| error.to_string())?;
        if read != 4 || !parse::is_native(magic) {
            continue;
        }
        let relative = path.strip_prefix(root).unwrap();
        let bytes = fs::read(&path).map_err(|error| error.to_string())?;
        binaries.extend(parse::parse(&bytes, relative).map_err(|error| {
            format!("{}: malformed native binary: {error}", relative.display())
        })?);
    }
    Ok(())
}
